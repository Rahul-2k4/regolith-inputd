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
