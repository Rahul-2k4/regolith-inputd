use crate::traits::InputHandler;
use cosmic_config::ConfigGet;
use log::{debug, error};
use notify::RecommendedWatcher;
use serde::Deserialize;
use std::error::Error;
use swayipc::{Connection as SwayConnection, Input};

const COSMIC_COMP_CONFIG: &str = "com.system76.CosmicComp";
const COSMIC_COMP_CONFIG_VERSION: u64 = 1;
const COSMIC_MOUSE_CONFIG_KEY: &str = "input_default";
const COSMIC_TOUCHPAD_CONFIG_KEY: &str = "input_touchpad";
const COSMIC_TOUCHPAD_OVERRIDE_KEY: &str = "input_touchpad_override";
const COSMIC_XKB_CONFIG_KEY: &str = "xkb_config";

#[derive(Debug, Default, Deserialize)]
struct CosmicInputConfig {
    acceleration: Option<CosmicAccelConfig>,
    click_method: Option<CosmicClickMethod>,
    disable_while_typing: Option<bool>,
    left_handed: Option<bool>,
    middle_button_emulation: Option<bool>,
    scroll_config: Option<CosmicScrollConfig>,
    tap_config: Option<CosmicTapConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct CosmicAccelConfig {
    profile: Option<CosmicAccelProfile>,
    speed: f64,
}

#[derive(Debug, Default, Deserialize)]
struct CosmicScrollConfig {
    method: Option<CosmicScrollMethod>,
    natural_scroll: Option<bool>,
    scroll_factor: Option<f64>,
}

#[derive(Debug, Deserialize)]
enum CosmicAccelProfile {
    Flat,
    Adaptive,
}

#[derive(Debug, Deserialize)]
enum CosmicClickMethod {
    ButtonAreas,
    Clickfinger,
}

#[derive(Debug, Deserialize)]
enum CosmicScrollMethod {
    NoScroll,
    TwoFinger,
    Edge,
    OnButtonDown,
}

#[derive(Debug, Deserialize)]
enum CosmicTouchpadOverride {
    None,
    ForceDisable,
}

#[derive(Debug, Deserialize)]
struct CosmicTapConfig {
    enabled: bool,
    drag: bool,
    drag_lock: bool,
}

#[derive(Debug, Deserialize)]
struct CosmicXkbConfig {
    layout: String,
    variant: String,
    #[serde(default = "default_repeat_delay")]
    repeat_delay: u32,
    #[serde(default = "default_repeat_rate")]
    repeat_rate: u32,
}

fn default_repeat_delay() -> u32 {
    600
}

fn default_repeat_rate() -> u32 {
    25
}

impl Default for CosmicXkbConfig {
    fn default() -> Self {
        Self {
            layout: String::new(),
            variant: String::new(),
            repeat_delay: default_repeat_delay(),
            repeat_rate: default_repeat_rate(),
        }
    }
}

pub struct CosmicMouseHandler {
    sway_connection: SwayConnection,
    _watcher: Option<RecommendedWatcher>,
}

impl CosmicMouseHandler {
    pub fn new() -> Self {
        Self {
            sway_connection: SwayConnection::new().unwrap(),
            _watcher: None,
        }
    }

    fn input_config() -> Result<CosmicInputConfig, Box<dyn Error>> {
        let config = cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION)?;
        Self::input_config_from(&config)
    }

    fn input_config_from(
        config: &cosmic_config::Config,
    ) -> Result<CosmicInputConfig, Box<dyn Error>> {
        Ok(config.get(COSMIC_MOUSE_CONFIG_KEY).unwrap_or_default())
    }

    fn set_bool(
        sway_connection: &mut SwayConnection,
        option: &str,
        value: Option<bool>,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(value) = value {
            let sway_value = if value { "enabled" } else { "disabled" };
            sway_connection.run_command(format!("input type:pointer {option} {sway_value}"))?;
        }
        Ok(())
    }

    fn apply_input_config(
        sway_connection: &mut SwayConnection,
        input_config: CosmicInputConfig,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(acceleration) = input_config.acceleration {
            sway_connection.run_command(format!(
                "input type:pointer pointer_accel {}",
                acceleration.speed
            ))?;
        }

        Self::set_bool(sway_connection, "left_handed", input_config.left_handed)?;

        if let Some(scroll_config) = input_config.scroll_config {
            Self::set_bool(
                sway_connection,
                "natural_scroll",
                scroll_config.natural_scroll,
            )?;
        }

        Ok(())
    }
}

impl InputHandler for CosmicMouseHandler {
    fn sway_connection(&mut self) -> &mut SwayConnection {
        &mut self.sway_connection
    }

    fn apply_changes(&mut self, _: &str) -> Result<(), Box<dyn Error>> {
        self.apply_all()
    }

    fn apply_all(&mut self) -> Result<(), Box<dyn Error>> {
        Self::apply_input_config(&mut self.sway_connection, Self::input_config()?)
    }

    fn sync_from_sway_input(&mut self, input: &Input) -> Result<(), Box<dyn Error>> {
        debug!(
            "COSMIC mouse handler does not sync sway input type '{}' back to cosmic-config yet",
            input.input_type
        );
        Ok(())
    }

    fn monitor_settings_change(&mut self) {
        let Ok(config) = cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION)
        else {
            error!("Failed to create COSMIC config watcher for mouse settings");
            return;
        };

        match config.watch(|config, keys| {
            if !keys.iter().any(|key| key == COSMIC_MOUSE_CONFIG_KEY) {
                return;
            }

            let result = SwayConnection::new()
                .map_err(|err| -> Box<dyn Error> { Box::new(err) })
                .and_then(|mut sway_connection| {
                    Self::input_config_from(config).and_then(|input_config| {
                        Self::apply_input_config(&mut sway_connection, input_config)
                    })
                });

            if let Err(err) = result {
                error!("Failed to apply COSMIC mouse settings change: {err}");
            }
        }) {
            Ok(watcher) => self._watcher = Some(watcher),
            Err(err) => error!("Failed to watch COSMIC mouse settings: {err}"),
        }
    }
}

unsafe impl Send for CosmicMouseHandler {}

pub struct CosmicTouchpadHandler {
    sway_connection: SwayConnection,
    _watcher: Option<RecommendedWatcher>,
}

impl CosmicTouchpadHandler {
    pub fn new() -> Self {
        Self {
            sway_connection: SwayConnection::new().unwrap(),
            _watcher: None,
        }
    }

    fn input_config_from(
        config: &cosmic_config::Config,
    ) -> Result<CosmicInputConfig, Box<dyn Error>> {
        Ok(config.get(COSMIC_TOUCHPAD_CONFIG_KEY).unwrap_or_default())
    }

    fn touchpad_override(
        config: &cosmic_config::Config,
    ) -> Result<Option<CosmicTouchpadOverride>, Box<dyn Error>> {
        Ok(config.get(COSMIC_TOUCHPAD_OVERRIDE_KEY).ok())
    }

    fn set_bool(
        sway_connection: &mut SwayConnection,
        option: &str,
        value: Option<bool>,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(value) = value {
            let sway_value = if value { "enabled" } else { "disabled" };
            sway_connection.run_command(format!("input type:touchpad {option} {sway_value}"))?;
        }
        Ok(())
    }

    fn apply_input_config(
        sway_connection: &mut SwayConnection,
        input_config: CosmicInputConfig,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(acceleration) = input_config.acceleration {
            sway_connection.run_command(format!(
                "input type:touchpad pointer_accel {}",
                acceleration.speed
            ))?;
            if let Some(profile) = acceleration.profile {
                let profile = match profile {
                    CosmicAccelProfile::Flat => "flat",
                    CosmicAccelProfile::Adaptive => "adaptive",
                };
                sway_connection
                    .run_command(format!("input type:touchpad accel_profile {profile}"))?;
            }
        }

        if let Some(click_method) = input_config.click_method {
            let method = match click_method {
                CosmicClickMethod::ButtonAreas => "button_areas",
                CosmicClickMethod::Clickfinger => "clickfinger",
            };
            sway_connection.run_command(format!("input type:touchpad click_method {method}"))?;
        }

        Self::set_bool(sway_connection, "dwt", input_config.disable_while_typing)?;
        Self::set_bool(sway_connection, "left_handed", input_config.left_handed)?;
        Self::set_bool(
            sway_connection,
            "middle_emulation",
            input_config.middle_button_emulation,
        )?;

        if let Some(scroll_config) = input_config.scroll_config {
            if let Some(method) = scroll_config.method {
                let method = match method {
                    CosmicScrollMethod::NoScroll => "none",
                    CosmicScrollMethod::TwoFinger => "two_finger",
                    CosmicScrollMethod::Edge => "edge",
                    CosmicScrollMethod::OnButtonDown => "on_button",
                };
                sway_connection
                    .run_command(format!("input type:touchpad scroll_method {method}"))?;
            }
            Self::set_bool(
                sway_connection,
                "natural_scroll",
                scroll_config.natural_scroll,
            )?;
            if let Some(factor) = scroll_config.scroll_factor {
                sway_connection
                    .run_command(format!("input type:touchpad scroll_factor {factor}"))?;
            }
        }

        if let Some(tap_config) = input_config.tap_config {
            Self::set_bool(sway_connection, "tap", Some(tap_config.enabled))?;
            Self::set_bool(sway_connection, "tap_and_drag", Some(tap_config.drag))?;
            Self::set_bool(sway_connection, "drag_lock", Some(tap_config.drag_lock))?;
        }

        Ok(())
    }

    fn log_touchpad_override(config: &cosmic_config::Config) {
        match Self::touchpad_override(config) {
            Ok(Some(CosmicTouchpadOverride::ForceDisable)) => debug!(
                "COSMIC touchpad force-disable is set, but Sway has no type:touchpad enable/disable command"
            ),
            Ok(Some(CosmicTouchpadOverride::None)) | Ok(None) => {}
            Err(err) => error!("Failed to read COSMIC touchpad override: {err}"),
        }
    }
}

impl InputHandler for CosmicTouchpadHandler {
    fn sway_connection(&mut self) -> &mut SwayConnection {
        &mut self.sway_connection
    }

    fn apply_changes(&mut self, _: &str) -> Result<(), Box<dyn Error>> {
        self.apply_all()
    }

    fn apply_all(&mut self) -> Result<(), Box<dyn Error>> {
        let config = cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION)?;
        Self::log_touchpad_override(&config);
        Self::apply_input_config(&mut self.sway_connection, Self::input_config_from(&config)?)
    }

    fn sync_from_sway_input(&mut self, input: &Input) -> Result<(), Box<dyn Error>> {
        debug!(
            "COSMIC touchpad handler does not sync sway input type '{}' back to cosmic-config yet",
            input.input_type
        );
        Ok(())
    }

    fn monitor_settings_change(&mut self) {
        let Ok(config) = cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION)
        else {
            error!("Failed to create COSMIC config watcher for touchpad settings");
            return;
        };

        match config.watch(|config, keys| {
            let touchpad_changed = keys.iter().any(|key| key == COSMIC_TOUCHPAD_CONFIG_KEY);
            let override_changed = keys.iter().any(|key| key == COSMIC_TOUCHPAD_OVERRIDE_KEY);
            if !touchpad_changed && !override_changed {
                return;
            }

            Self::log_touchpad_override(config);

            if touchpad_changed {
                let result = SwayConnection::new()
                    .map_err(|err| -> Box<dyn Error> { Box::new(err) })
                    .and_then(|mut sway_connection| {
                        Self::input_config_from(config).and_then(|input_config| {
                            Self::apply_input_config(&mut sway_connection, input_config)
                        })
                    });

                if let Err(err) = result {
                    error!("Failed to apply COSMIC touchpad settings change: {err}");
                }
            }
        }) {
            Ok(watcher) => self._watcher = Some(watcher),
            Err(err) => error!("Failed to watch COSMIC touchpad settings: {err}"),
        }
    }
}

unsafe impl Send for CosmicTouchpadHandler {}

pub struct CosmicInputHandler {
    name: &'static str,
    sway_connection: SwayConnection,
    _watcher: Option<RecommendedWatcher>,
}

impl CosmicInputHandler {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            sway_connection: SwayConnection::new().unwrap(),
            _watcher: None,
        }
    }

    fn xkb_config() -> Result<CosmicXkbConfig, Box<dyn Error>> {
        let config = cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION)?;
        Self::xkb_config_from(&config)
    }

    fn xkb_config_from(config: &cosmic_config::Config) -> Result<CosmicXkbConfig, Box<dyn Error>> {
        Ok(config.get(COSMIC_XKB_CONFIG_KEY).unwrap_or_default())
    }

    fn commands_for_config(name: &str, xkb_config: &CosmicXkbConfig) -> Vec<String> {
        match name {
            "keyboard" => vec![
                format!(
                    "input type:keyboard repeat_delay {}",
                    xkb_config.repeat_delay
                ),
                format!("input type:keyboard repeat_rate {}", xkb_config.repeat_rate),
            ],
            "input-sources" if !xkb_config.layout.is_empty() => {
                let mut commands = Vec::new();
                if !xkb_config.variant.is_empty() {
                    commands.push(format!(
                        "input type:keyboard xkb_variant '{}'",
                        xkb_config.variant
                    ));
                }
                commands.push(format!(
                    "input type:keyboard xkb_layout '{}'",
                    xkb_config.layout
                ));
                commands
            }
            "input-sources" => Vec::new(),
            _ => Vec::new(),
        }
    }

    fn apply_xkb_config(
        sway_connection: &mut SwayConnection,
        name: &str,
        xkb_config: CosmicXkbConfig,
    ) -> Result<(), Box<dyn Error>> {
        for command in Self::commands_for_config(name, &xkb_config) {
            sway_connection.run_command(command)?;
        }
        Ok(())
    }
}

impl InputHandler for CosmicInputHandler {
    fn sway_connection(&mut self) -> &mut SwayConnection {
        &mut self.sway_connection
    }

    fn apply_changes(&mut self, key: &str) -> Result<(), Box<dyn Error>> {
        if key == COSMIC_XKB_CONFIG_KEY {
            self.apply_all()?;
        } else {
            debug!(
                "COSMIC input handler '{}' ignores unsupported settings key '{}'",
                self.name, key
            );
        }
        Ok(())
    }

    fn apply_all(&mut self) -> Result<(), Box<dyn Error>> {
        Self::apply_xkb_config(&mut self.sway_connection, self.name, Self::xkb_config()?)
    }

    fn sync_from_sway_input(&mut self, input: &Input) -> Result<(), Box<dyn Error>> {
        // TODO: Map Sway keyboard state back into COSMIC xkb_config when reverse sync is in scope.
        debug!(
            "COSMIC input handler '{}' does not sync sway input type '{}' back to cosmic-config yet",
            self.name, input.input_type
        );
        Ok(())
    }

    fn monitor_settings_change(&mut self) {
        let Ok(config) = cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION)
        else {
            error!(
                "Failed to create COSMIC config watcher for {} settings",
                self.name
            );
            return;
        };

        let name = self.name;
        match config.watch(move |config, keys| {
            if !keys.iter().any(|key| key == COSMIC_XKB_CONFIG_KEY) {
                return;
            }

            let result = SwayConnection::new()
                .map_err(|err| -> Box<dyn Error> { Box::new(err) })
                .and_then(|mut sway_connection| {
                    Self::xkb_config_from(config).and_then(|xkb_config| {
                        Self::apply_xkb_config(&mut sway_connection, name, xkb_config)
                    })
                });

            if let Err(err) = result {
                error!("Failed to apply COSMIC {name} settings change: {err}");
            }
        }) {
            Ok(watcher) => self._watcher = Some(watcher),
            Err(err) => error!("Failed to watch COSMIC {} settings: {err}", self.name),
        }
    }
}

unsafe impl Send for CosmicInputHandler {}

#[cfg(test)]
mod tests {
    use super::{CosmicInputHandler, CosmicXkbConfig};

    #[test]
    fn keyboard_commands_ignore_layout_variant_and_only_emit_repeat() {
        let config = CosmicXkbConfig {
            layout: "us,ara".to_string(),
            variant: ",azerty".to_string(),
            repeat_delay: 450,
            repeat_rate: 35,
        };

        assert_eq!(
            CosmicInputHandler::commands_for_config("keyboard", &config),
            vec![
                "input type:keyboard repeat_delay 450".to_string(),
                "input type:keyboard repeat_rate 35".to_string(),
            ]
        );
    }

    #[test]
    fn input_sources_layout_without_variant_emits_only_layout() {
        let config = CosmicXkbConfig {
            layout: "us,ara".to_string(),
            variant: String::new(),
            ..Default::default()
        };

        assert_eq!(
            CosmicInputHandler::commands_for_config("input-sources", &config),
            vec!["input type:keyboard xkb_layout 'us,ara'".to_string()]
        );
    }

    #[test]
    fn default_xkb_config_maps_keyboard_repeat_and_skips_input_sources() {
        let config = CosmicXkbConfig::default();

        assert_eq!(
            CosmicInputHandler::commands_for_config("keyboard", &config),
            vec![
                "input type:keyboard repeat_delay 600".to_string(),
                "input type:keyboard repeat_rate 25".to_string(),
            ]
        );
        assert!(CosmicInputHandler::commands_for_config("input-sources", &config).is_empty());
    }

    #[test]
    fn maps_cosmic_keyboard_repeat_to_sway_commands() {
        let config = CosmicXkbConfig {
            repeat_delay: 450,
            repeat_rate: 35,
            ..Default::default()
        };

        assert_eq!(
            CosmicInputHandler::commands_for_config("keyboard", &config),
            vec![
                "input type:keyboard repeat_delay 450".to_string(),
                "input type:keyboard repeat_rate 35".to_string(),
            ]
        );
    }

    #[test]
    fn maps_cosmic_xkb_layout_and_variant_to_sway_commands() {
        let config = CosmicXkbConfig {
            layout: "us,ara".to_string(),
            variant: ",azerty".to_string(),
            ..Default::default()
        };

        assert_eq!(
            CosmicInputHandler::commands_for_config("input-sources", &config),
            vec![
                "input type:keyboard xkb_variant ',azerty'".to_string(),
                "input type:keyboard xkb_layout 'us,ara'".to_string(),
            ]
        );
    }

    #[test]
    fn skips_input_sources_commands_when_layout_is_empty() {
        let config = CosmicXkbConfig {
            variant: "azerty".to_string(),
            ..Default::default()
        };

        assert!(CosmicInputHandler::commands_for_config("input-sources", &config).is_empty());
    }

    #[test]
    fn leaves_unknown_cosmic_input_handler_without_commands() {
        let config = CosmicXkbConfig::default();

        assert!(CosmicInputHandler::commands_for_config("unknown", &config).is_empty());
    }
}
