use crate::traits::InputHandler;
use crate::utils::retry_action;
use std::error::Error;
use std::time::Duration;

#[cfg(feature = "cosmic")]
use crate::cosmic::{CosmicInputHandler, CosmicMouseHandler, CosmicTouchpadHandler};

#[cfg(feature = "gnome")]
use crate::{
    input_sources::InputSourcesHandler, keyboard::KeyboardHandler, mouse::MouseHandler,
    touchpad::TouchpadHandler,
};

pub type HandlerSet = [Box<dyn InputHandler + Send>; 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Gnome,
    Cosmic,
}

impl BackendKind {
    pub fn from_current_desktop() -> Self {
        std::env::var("XDG_CURRENT_DESKTOP")
            .map(|desktop| Self::from_desktop_value(&desktop))
            .unwrap_or(Self::Gnome)
    }

    pub fn from_desktop_value(desktop: &str) -> Self {
        if desktop
            .split(':')
            .any(|part| part.eq_ignore_ascii_case("cosmic"))
        {
            Self::Cosmic
        } else {
            Self::Gnome
        }
    }

    pub fn create_handlers(self) -> Result<HandlerSet, Box<dyn Error>> {
        match self {
            Self::Gnome => create_gnome_handlers(),
            Self::Cosmic => create_cosmic_handlers(),
        }
    }
}

pub fn create_handlers_with_retry<F, T, E>(
    action: F,
    max_retry: usize,
    duration_before_retry: Duration,
) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
    E: std::fmt::Display,
{
    retry_action(action, max_retry, duration_before_retry)
}

#[cfg(feature = "gnome")]
fn create_gnome_handlers() -> Result<HandlerSet, Box<dyn Error>> {
    Ok([
        Box::new(MouseHandler::new()?),
        Box::new(KeyboardHandler::new()?),
        Box::new(TouchpadHandler::new()?),
        Box::new(InputSourcesHandler::new()?),
    ])
}

#[cfg(not(feature = "gnome"))]
fn create_gnome_handlers() -> Result<HandlerSet, Box<dyn Error>> {
    Err(
        "GNOME input backend selected, but regolith-inputd was built without the gnome feature"
            .into(),
    )
}

#[cfg(feature = "cosmic")]
fn create_cosmic_handlers() -> Result<HandlerSet, Box<dyn Error>> {
    Ok([
        Box::new(CosmicMouseHandler::new()?),
        Box::new(CosmicInputHandler::new("keyboard")?),
        Box::new(CosmicTouchpadHandler::new()?),
        Box::new(CosmicInputHandler::new("input-sources")?),
    ])
}

#[cfg(not(feature = "cosmic"))]
fn create_cosmic_handlers() -> Result<HandlerSet, Box<dyn Error>> {
    Err("COSMIC desktop detected, but regolith-inputd was built without the cosmic feature".into())
}

#[cfg(test)]
mod tests {
    use super::{create_handlers_with_retry, BackendKind};
    use std::time::Duration;

    #[test]
    fn selects_cosmic_when_desktop_contains_cosmic() {
        assert_eq!(
            BackendKind::from_desktop_value("Regolith-Wayland:COSMIC:sway"),
            BackendKind::Cosmic
        );
    }

    #[test]
    fn selects_gnome_for_non_cosmic_desktop() {
        assert_eq!(
            BackendKind::from_desktop_value("Regolith-Wayland:GNOME:sway"),
            BackendKind::Gnome
        );
    }

    #[test]
    fn matches_cosmic_case_insensitively() {
        assert_eq!(
            BackendKind::from_desktop_value("regolith-wayland:cosmic:sway"),
            BackendKind::Cosmic
        );
    }

    #[test]
    fn defaults_to_gnome_when_desktop_is_unset_or_unrecognized() {
        assert_eq!(BackendKind::from_desktop_value(""), BackendKind::Gnome);
        assert_eq!(
            BackendKind::from_desktop_value("Regolith-Wayland:sway"),
            BackendKind::Gnome
        );
    }

    #[test]
    fn does_not_select_cosmic_for_partial_desktop_name() {
        assert_eq!(
            BackendKind::from_desktop_value("Regolith-Wayland:cosmic-like:sway"),
            BackendKind::Gnome
        );
    }

    #[test]
    fn handler_startup_retries_the_constructor_as_one_operation() {
        let mut attempts = 0;
        let result = create_handlers_with_retry(
            || {
                attempts += 1;
                if attempts == 2 {
                    Ok(())
                } else {
                    Err("Sway IPC unavailable")
                }
            },
            2,
            Duration::ZERO,
        );

        assert_eq!(result, Ok(()));
        assert_eq!(attempts, 2);
    }

    #[test]
    fn handler_startup_returns_constructor_error_after_budget() {
        let mut attempts = 0;
        let result = create_handlers_with_retry(
            || {
                attempts += 1;
                Err::<(), _>("Sway IPC unavailable")
            },
            2,
            Duration::ZERO,
        );

        assert_eq!(result, Err("Sway IPC unavailable"));
        assert_eq!(attempts, 3);
    }
}
