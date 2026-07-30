#[cfg(feature = "gnome")]
use gio::prelude::SettingsExtManual;
#[cfg(feature = "gnome")]
use gio::{traits::SettingsExt, Settings};
#[cfg(feature = "gnome")]
use log::error;
use std::error::Error;
use std::thread;
use std::time::Duration;
use swayipc::{Connection as SwayConnection, EnabledOrDisabled, Input, SendEvents};

use crate::{ALLOW_SETTINGS_APPLY, ALLOW_SWAYINPUT_APPLY};

struct GateRestore<'a> {
    gate: &'a crate::GateState,
}

impl Drop for GateRestore<'_> {
    fn drop(&mut self) {
        self.gate.end_suppression();
    }
}

fn with_gate_suppressed<T, E, F>(gate: &crate::GateState, action: F) -> Result<T, E>
where
    F: FnOnce() -> Result<T, E>,
{
    gate.begin_suppression();
    let restore = GateRestore { gate };
    let result = action();
    drop(restore);
    result
}

pub trait InputHandler {
    fn sway_connection(&mut self) -> &mut SwayConnection;
    fn apply_changes(&mut self, _: &str) -> Result<(), Box<dyn Error>>;
    fn apply_all(&mut self) -> Result<(), Box<dyn Error>>;
    fn sync_from_sway_input(&mut self, _: &Input) -> Result<(), Box<dyn Error>>;
    fn monitor_settings_change(&mut self);

    fn apply_changes_sync(&mut self, key: &str) -> Result<(), Box<dyn Error>> {
        with_gate_suppressed(&ALLOW_SWAYINPUT_APPLY, || {
            let result = self.apply_changes(key);
            thread::sleep(Duration::from_millis(100));
            result
        })
    }

    fn apply_all_sync(&mut self) -> Result<(), Box<dyn Error>> {
        if !ALLOW_SETTINGS_APPLY.is_enabled() {
            return Ok(());
        }
        with_gate_suppressed(&ALLOW_SWAYINPUT_APPLY, || {
            let result = self.apply_all();
            thread::sleep(Duration::from_millis(100));
            result
        })
    }

    fn sync_from_sway_input_sync(&mut self, input: &Input) -> Result<(), Box<dyn Error>> {
        if !ALLOW_SWAYINPUT_APPLY.is_enabled() {
            return Ok(());
        }
        with_gate_suppressed(&ALLOW_SETTINGS_APPLY, || {
            let result = self.sync_from_sway_input(input);
            thread::sleep(Duration::from_millis(100));
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::with_gate_suppressed;
    use crate::GateState;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn restores_gate_after_failed_operation() {
        let gate = GateState::new(true);
        let result: Result<(), &str> = with_gate_suppressed(&gate, || Err("failed"));

        assert_eq!(result, Err("failed"));
        assert!(gate.is_enabled());
    }

    #[test]
    fn preserves_existing_suppression_after_operation() {
        let gate = GateState::new(false);
        let result: Result<(), &str> = with_gate_suppressed(&gate, || Ok(()));

        assert_eq!(result, Ok(()));
        assert!(!gate.is_enabled());
    }

    #[test]
    fn overlapping_suppressions_stay_disabled_until_last_finishes() {
        let gate = Arc::new(GateState::new(true));
        gate.begin_suppression();
        let other = Arc::clone(&gate);
        let worker = thread::spawn(move || {
            other.begin_suppression();
            assert!(!other.is_enabled());
            other.end_suppression();
        });
        worker.join().unwrap();
        assert!(!gate.is_enabled());
        gate.end_suppression();
        assert!(gate.is_enabled());
    }

    #[test]
    fn unmatched_suppression_release_does_not_wrap_counter() {
        let gate = GateState::new(true);

        gate.end_suppression();

        assert!(gate.is_enabled());
    }
}

#[cfg(feature = "gnome")]
pub trait GnomeInputHandler: InputHandler {
    fn settings(&self) -> &Settings;

    fn monitor_gnome_settings_change(&mut self)
    where
        Self: 'static,
    {
        let ptr: *mut Self = self;
        self.settings().connect_changed(None, move |_, key| unsafe {
            if !ptr.is_null() {
                if let Err(e) = (*ptr).apply_changes_sync(key) {
                    error!("{e}");
                };
            }
        });
    }
}

#[cfg(feature = "gnome")]
pub trait PointerMethods: GnomeInputHandler {
    fn pointer_type(&self) -> &str;
    fn apply_left_handed(&mut self) -> Result<(), Box<dyn Error>>;
    fn apply_speed(&mut self) -> Result<(), Box<dyn Error>> {
        let new_val: f64 = self.settings().get("speed");
        let pointer_type = self.pointer_type();
        let cmd = format!("input type:{pointer_type} pointer_accel {new_val}");
        self.sway_connection().run_command(cmd)?;
        Ok(())
    }
    fn apply_natural_scroll(&mut self) -> Result<(), Box<dyn Error>> {
        let new_val: &str = if self.settings().get("natural-scroll") {
            "enabled"
        } else {
            "disabled"
        };
        let pointer_type = self.pointer_type();
        let cmd = format!("input type:{pointer_type} natural_scroll {new_val}");
        self.sway_connection().run_command(cmd)?;
        Ok(())
    }
    fn sync_pointer_gsettings(&self, input: &Input) -> Result<(), Box<dyn Error>> {
        if input.libinput.is_none() {
            return Ok(());
        }
        let libinput = input.libinput.as_ref().unwrap();
        if let Some(speed) = libinput.accel_speed {
            self.settings().set_double("speed", speed)?;
        }
        if let Some(natural) = libinput.natural_scroll.as_ref() {
            self.settings()
                .set_boolean("natural-scroll", natural.to_primitive())?;
        }
        if let Some(accel) = libinput.accel_speed {
            self.settings().set_double("speed", accel)?;
        }
        Ok(())
    }
}

pub trait SwayTypeToPrimitive<T> {
    fn to_primitive(&self) -> T;
}

pub trait PrimitiveToSwayType<T> {
    fn to_sway_type(self) -> T;
}

impl SwayTypeToPrimitive<bool> for EnabledOrDisabled {
    fn to_primitive(&self) -> bool {
        match self {
            EnabledOrDisabled::Enabled => true,
            EnabledOrDisabled::Disabled => false,
        }
    }
}

impl SwayTypeToPrimitive<&str> for EnabledOrDisabled {
    fn to_primitive(&self) -> &'static str {
        match self {
            EnabledOrDisabled::Enabled => "enabled",
            EnabledOrDisabled::Disabled => "disabled",
        }
    }
}

impl PrimitiveToSwayType<EnabledOrDisabled> for bool {
    fn to_sway_type(self) -> EnabledOrDisabled {
        if self {
            EnabledOrDisabled::Enabled
        } else {
            EnabledOrDisabled::Disabled
        }
    }
}

impl SwayTypeToPrimitive<bool> for SendEvents {
    fn to_primitive(&self) -> bool {
        matches!(self, SendEvents::Enabled)
    }
}

impl SwayTypeToPrimitive<&str> for SendEvents {
    fn to_primitive(&self) -> &'static str {
        match self {
            SendEvents::Enabled => "enabled",
            _ => "disabled",
        }
    }
}
