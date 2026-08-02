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

use backend::HandlerSet;
use backend::{create_handlers_with_retry, BackendKind};
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
use swayipc::{Event, EventStream, TickEvent};

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

pub(crate) const STARTUP_MAX_RETRIES: usize = 60;
pub(crate) const STARTUP_RETRY_DELAY: Duration = Duration::from_millis(500);

// Structs
pub struct SettingsManager {
    handlers: HandlerList,
}

fn get_inputevent_stream_with_retry<F, E>(
    action: F,
    max_retry: usize,
    duration_before_retry: Duration,
) -> Result<EventStream, E>
where
    F: FnMut() -> Result<EventStream, E>,
    E: std::fmt::Display,
{
    utils::retry_action(action, max_retry, duration_before_retry)
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
    pub fn new() -> Result<SettingsManager, Box<dyn Error>> {
        Self::new_for_backend(BackendKind::from_current_desktop())
    }

    fn new_for_backend(backend: BackendKind) -> Result<SettingsManager, Box<dyn Error>> {
        backend.validate_feature()?;
        let handlers = Arc::new(Mutex::new(create_handlers_with_retry(
            || backend.create_handlers(),
            STARTUP_MAX_RETRIES,
            STARTUP_RETRY_DELAY,
        )?));
        Ok(SettingsManager { handlers })
    }

    pub fn start_monitoring(&mut self) -> Result<(), Box<dyn Error + '_>> {
        let event_stream = get_inputevent_stream_with_retry(
            utils::get_new_inputevent_stream,
            STARTUP_MAX_RETRIES,
            STARTUP_RETRY_DELAY,
        )?;
        let mut handlers_lock = self.handlers.lock()?;
        for handle in handlers_lock.iter_mut() {
            handle.apply_all_sync()?;
            handle.monitor_settings_change();
        }

        let handlers_sref = self.handlers.clone();
        thread::spawn(move || Self::monitor_swayinput_events(event_stream, handlers_sref));
        Ok(())
    }

    fn monitor_swayinput_events(event_stream: EventStream, mut handlers_sref: HandlerList) {
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

#[cfg(test)]
mod tests {
    use super::{get_inputevent_stream_with_retry, STARTUP_MAX_RETRIES, STARTUP_RETRY_DELAY};
    use std::time::Duration;

    #[test]
    fn startup_retry_policy_has_sixty_retries_at_half_second_intervals() {
        assert_eq!(STARTUP_MAX_RETRIES, 60);
        assert_eq!(STARTUP_RETRY_DELAY, Duration::from_millis(500));
    }

    #[test]
    fn initial_event_stream_failure_is_returned_after_retries() {
        let mut attempts = 0;
        let result = get_inputevent_stream_with_retry(
            || {
                attempts += 1;
                Err::<swayipc::EventStream, _>("event subscription failed")
            },
            STARTUP_MAX_RETRIES,
            Duration::ZERO,
        );

        assert!(result.is_err());
        assert_eq!(
            result.err().map(|error| error.to_string()),
            Some("event subscription failed".to_string())
        );
        assert_eq!(attempts, STARTUP_MAX_RETRIES + 1);
    }
}
