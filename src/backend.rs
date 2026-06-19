use crate::traits::InputHandler;

#[cfg(feature = "cosmic")]
use crate::cosmic::{CosmicInputHandler, CosmicMouseHandler};

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

    pub fn create_handlers(self) -> HandlerSet {
        match self {
            Self::Gnome => create_gnome_handlers(),
            Self::Cosmic => create_cosmic_handlers(),
        }
    }
}

#[cfg(feature = "gnome")]
fn create_gnome_handlers() -> HandlerSet {
    [
        Box::new(MouseHandler::new()),
        Box::new(KeyboardHandler::new()),
        Box::new(TouchpadHandler::new()),
        Box::new(InputSourcesHandler::new()),
    ]
}

#[cfg(not(feature = "gnome"))]
fn create_gnome_handlers() -> HandlerSet {
    panic!("GNOME input backend selected, but regolith-inputd was built without the gnome feature");
}

#[cfg(feature = "cosmic")]
fn create_cosmic_handlers() -> HandlerSet {
    [
        Box::new(CosmicMouseHandler::new()),
        Box::new(CosmicInputHandler::new("keyboard")),
        Box::new(CosmicInputHandler::new("touchpad")),
        Box::new(CosmicInputHandler::new("input-sources")),
    ]
}

#[cfg(not(feature = "cosmic"))]
fn create_cosmic_handlers() -> HandlerSet {
    panic!("COSMIC desktop detected, but regolith-inputd was built without the cosmic feature");
}

#[cfg(test)]
mod tests {
    use super::BackendKind;

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
}
