use crate::traits::{GnomeInputHandler, InputHandler};
use crate::utils;
use gio::{prelude::SettingsExtManual, Settings};
use log::info;
use std::error::Error;
use swayipc::Connection as SwayConnection;

pub struct InputSourcesHandler {
    settings: Settings,
    sway_connection: SwayConnection,
}

fn first_layout_name(layouts: &[String]) -> Option<&str> {
    layouts.first().map(String::as_str)
}

fn input_source_commands(
    sources: Vec<(String, String)>,
) -> Result<(String, String), Box<dyn Error>> {
    let sources = if sources.is_empty() {
        vec![(String::from("xkb"), String::from("us"))]
    } else {
        sources
    };
    let (layouts, variants) = sources
        .into_iter()
        .map(|(_, layout)| {
            if layout.contains('+') {
                let (layout, variant) = layout.split_once('+').unwrap();
                (String::from(layout), String::from(variant))
            } else {
                (layout, String::from(""))
            }
        })
        .reduce(|(layout, variant), (curr_layout, curr_variant)| {
            (layout + "," + &curr_layout, variant + "," + &curr_variant)
        })
        .ok_or("Invalid keyboard layout or variant")?;
    Ok((layouts, variants))
}

impl InputSourcesHandler {
    pub fn new() -> Result<InputSourcesHandler, Box<dyn Error>> {
        let settings = Settings::new("org.gnome.desktop.input-sources");
        let sway_connection = utils::new_sway_connection()?;
        Ok(InputSourcesHandler {
            settings,
            sway_connection,
        })
    }
    fn apply_input_sources(&mut self) -> Result<(), Box<dyn Error>> {
        let sources: Vec<(String, String)> = self.settings().get("sources");
        // Layout is of form code+variant
        let (layouts, variants) = input_source_commands(sources)?;
        let layout_cmd = format!("input type:keyboard xkb_layout '{layouts}'");
        let vairants_cmd = format!("input type:keyboard xkb_variant '{variants}'");
        info!("{vairants_cmd}");
        info!("{layout_cmd}");
        self.sway_connection().run_command(vairants_cmd)?;
        self.sway_connection().run_command(layout_cmd)?;
        Ok(())
    }
}

impl InputHandler for InputSourcesHandler {
    fn apply_changes(&mut self, key: &str) -> Result<(), Box<dyn Error>> {
        info!("org.gnome.desktop.input-sources -> Key: {key} chaged");
        if key == "sources" {
            self.apply_input_sources()?
        };
        Ok(())
    }
    fn apply_all(&mut self) -> Result<(), Box<dyn Error>> {
        self.apply_input_sources()
    }
    fn sync_from_sway_input(&mut self, input: &swayipc::Input) -> Result<(), Box<dyn Error>> {
        if let Some(layout) = first_layout_name(&input.xkb_layout_names) {
            info!("xkb_layout: {layout}");
        }
        Ok(())
    }
    fn sway_connection(&mut self) -> &mut swayipc::Connection {
        &mut self.sway_connection
    }
    fn monitor_settings_change(&mut self) {
        self.monitor_gnome_settings_change();
    }
}

impl GnomeInputHandler for InputSourcesHandler {
    fn settings(&self) -> &Settings {
        &self.settings
    }
}
unsafe impl Send for InputSourcesHandler {}

#[cfg(test)]
mod tests {
    use super::{first_layout_name, input_source_commands};

    #[test]
    fn missing_sway_layout_is_ignored_without_panicking() {
        assert_eq!(first_layout_name(&[]), None);
    }

    #[test]
    fn first_sway_layout_is_preserved_for_logging() {
        let layouts = vec!["English (US)".to_string(), "Arabic".to_string()];

        assert_eq!(first_layout_name(&layouts), Some("English (US)"));
    }

    #[test]
    fn empty_sources_use_the_default_us_layout() {
        assert_eq!(
            input_source_commands(Vec::new()).unwrap(),
            ("us".into(), "".into())
        );
    }
}
