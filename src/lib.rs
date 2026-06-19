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
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use swayipc::{Event, TickEvent};

static ALLOW_SWAYINPUT_APPLY: AtomicBool = AtomicBool::new(true);
static ALLOW_SETTINGS_APPLY: AtomicBool = AtomicBool::new(true);

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
                Ok(Event::Input(event)) if ALLOW_SWAYINPUT_APPLY.load(Ordering::Relaxed) => {
                    utils::sync_input_settings(&mut handlers_sref, &event.input).unwrap();
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
                            ALLOW_SWAYINPUT_APPLY.store(false, Ordering::Relaxed);
                            info!(
                                "Recieved tick, allow_sync = {}",
                                ALLOW_SWAYINPUT_APPLY.load(Ordering::Relaxed)
                            );
                        }
                        Ok(SwayReloadTick { status: ReloadDone }) => {
                            thread::sleep(Duration::from_millis(100));
                            ALLOW_SWAYINPUT_APPLY.store(true, Ordering::Relaxed);
                            info!(
                                "Recieved tick, allow_sync = {}",
                                ALLOW_SWAYINPUT_APPLY.load(Ordering::Relaxed)
                            );
                            info!("Sway reload done - Reapplying configurations from settings");
                            let mut handlers_lock = handlers_sref
                                .lock()
                                .expect("Acquired lock for handers_sref");
                            for handle in handlers_lock.iter_mut() {
                                handle
                                    .apply_all_sync()
                                    .expect("Failed to re-apply configs from gsettings");
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
