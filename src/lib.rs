mod backend;
#[cfg(feature = "cosmic")]
mod cosmic;
#[cfg(feature = "gnome")]
mod input_sources;
#[cfg(feature = "gnome")]
mod keyboard;
#[cfg(feature = "gnome")]
mod mouse;
#[cfg(feature = "gnome")]
mod touchpad;
mod traits;
mod utils;

use backend::BackendKind;
use backend::HandlerSet;
use log::info;
use log::{debug, warn};
use serde::Deserialize;
use std::error::Error;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use swayipc::{Event, TickEvent};

pub(crate) struct GateState {
    requested: AtomicBool,
    suppressions: AtomicUsize,
}

impl GateState {
    pub(crate) const fn new(enabled: bool) -> Self {
        Self {
            requested: AtomicBool::new(enabled),
            suppressions: AtomicUsize::new(0),
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.requested.load(Ordering::Relaxed) && self.suppressions.load(Ordering::Relaxed) == 0
    }

    pub(crate) fn set_requested(&self, enabled: bool) {
        self.requested.store(enabled, Ordering::Relaxed);
    }

    pub(crate) fn begin_suppression(&self) {
        self.suppressions.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn end_suppression(&self) {
        if self
            .suppressions
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                count.checked_sub(1)
            })
            .is_err()
        {
            warn!("Ignored unmatched input gate suppression release");
        }
    }
}

static ALLOW_SWAYINPUT_APPLY: GateState = GateState::new(true);
static ALLOW_SETTINGS_APPLY: GateState = GateState::new(true);

// Type Aliases
type SharedRef<T> = Arc<Mutex<T>>;
type HandlerList = SharedRef<HandlerSet>;

// Structs
pub struct SettingsManager {
    handlers: HandlerList,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
enum SwayReloadStatus {
    #[serde(rename = "reload_pending")]
    ReloadPending,
    #[serde(rename = "reload_done")]
    ReloadDone,
}

#[derive(Deserialize, Debug)]
struct SwayReloadTick {
    status: SwayReloadStatus,
}
// Method Implementations
impl SettingsManager {
    pub fn new() -> SettingsManager {
        Self::new_for_backend(BackendKind::from_current_desktop())
    }

    fn new_for_backend(backend: BackendKind) -> SettingsManager {
        utils::retry_action(swayipc::Connection::new, 5, Duration::from_millis(500));
        let handlers = Arc::new(Mutex::new(backend.create_handlers()));
        SettingsManager { handlers }
    }

    pub fn start_monitoring(&mut self) -> Result<(), Box<dyn Error + '_>> {
        let mut handlers_lock = self.handlers.lock()?;
        for handle in handlers_lock.iter_mut() {
            handle.apply_all_sync()?;
            handle.monitor_settings_change();
        }

        let handlers_sref = self.handlers.clone();
        thread::spawn(move || Self::monitor_swayinput_events(handlers_sref));
        Ok(())
    }

    fn monitor_swayinput_events(mut handlers_sref: HandlerList) {
        let event_stream = utils::retry_action(
            utils::get_new_inputevent_stream,
            5,
            Duration::from_millis(500),
        );
        for event in event_stream {
            match event {
                Ok(Event::Input(event)) if ALLOW_SWAYINPUT_APPLY.is_enabled() => {
                    if !utils::is_supported_input_type(&event.input.input_type) {
                        warn!(
                            "Ignoring unsupported Sway input type: {}",
                            event.input.input_type
                        );
                        continue;
                    }
                    if let Err(e) = utils::retry_fallible(
                        || utils::sync_input_settings(&mut handlers_sref, &event.input),
                        5,
                        Duration::from_millis(500),
                    ) {
                        warn!("Failed to sync input settings: {e}");
                    }
                }
                Ok(Event::Tick(TickEvent {
                    payload,
                    first: false,
                    ..
                })) => {
                    use SwayReloadStatus::{ReloadDone, ReloadPending};
                    match serde_json::from_str::<SwayReloadTick>(&payload) {
                        Ok(SwayReloadTick {
                            status: ReloadPending,
                        }) => {
                            ALLOW_SWAYINPUT_APPLY.set_requested(false);
                            info!(
                                "Recieved tick, allow_sync = {}",
                                ALLOW_SWAYINPUT_APPLY.is_enabled()
                            );
                        }
                        Ok(SwayReloadTick { status: ReloadDone }) => {
                            thread::sleep(Duration::from_millis(100));
                            ALLOW_SWAYINPUT_APPLY.set_requested(true);
                            info!(
                                "Recieved tick, allow_sync = {}",
                                ALLOW_SWAYINPUT_APPLY.is_enabled()
                            );
                            info!("Sway reload done - Reapplying configurations from settings");
                            let handler_count = utils::recover_lock(&handlers_sref).len();
                            for handler_index in 0..handler_count {
                                let result = utils::retry_fallible(
                                    || {
                                        let mut handlers_lock = utils::recover_lock(&handlers_sref);
                                        handlers_lock[handler_index].apply_all_sync()
                                    },
                                    5,
                                    Duration::from_millis(500),
                                );
                                if let Err(e) = result {
                                    warn!("Failed to re-apply input settings: {e}");
                                }
                            }
                        }
                        Err(e) => debug!("Invalid Payload Recieved: {e}"),
                    }
                }
                Err(e) => warn!("{e}"),
                _ => continue,
            }
        }
    }
}

impl Default for SettingsManager {
    fn default() -> Self {
        Self::new()
    }
}
