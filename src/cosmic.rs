use crate::traits::{InputHandler, SwayTypeToPrimitive};
use crate::utils;
use cosmic_config::{ConfigGet, ConfigSet};
use log::{debug, error};
use notify::RecommendedWatcher;
use serde::{Deserialize, Serialize};
use std::error::Error;
use swayipc::{Connection as SwayConnection, Input};

const COSMIC_COMP_CONFIG: &str = "com.system76.CosmicComp";
const COSMIC_COMP_CONFIG_VERSION: u64 = 1;
const COSMIC_MOUSE_CONFIG_KEY: &str = "input_default";
const COSMIC_TOUCHPAD_CONFIG_KEY: &str = "input_touchpad";
const COSMIC_TOUCHPAD_OVERRIDE_KEY: &str = "input_touchpad_override";
const COSMIC_XKB_CONFIG_KEY: &str = "xkb_config";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WatchAction {
    Ignore,
    Apply,
}

fn has_key(keys: &[String], expected: &str) -> bool {
    keys.iter().any(|key| key == expected)
}
fn watch_action(keys: &[String], expected: &str) -> WatchAction {
    if has_key(keys, expected) {
        WatchAction::Apply
    } else {
        WatchAction::Ignore
    }
}

fn touchpad_watch_triggers(keys: &[String]) -> bool {
    has_key(keys, COSMIC_TOUCHPAD_CONFIG_KEY) || has_key(keys, COSMIC_TOUCHPAD_OVERRIDE_KEY)
}
fn touchpad_watch_applies(keys: &[String]) -> bool {
    has_key(keys, COSMIC_TOUCHPAD_CONFIG_KEY)
}
fn touchpad_watch_applies_override(keys: &[String]) -> bool {
    has_key(keys, COSMIC_TOUCHPAD_OVERRIDE_KEY)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TouchpadWatchAction {
    Ignore,
    ApplyConfig,
    ApplyOverride,
    ApplyConfigAndOverride,
}

fn touchpad_watch_action(keys: &[String]) -> TouchpadWatchAction {
    if !touchpad_watch_triggers(keys) {
        return TouchpadWatchAction::Ignore;
    }
    match (
        touchpad_watch_applies(keys),
        touchpad_watch_applies_override(keys),
    ) {
        (false, false) => TouchpadWatchAction::Ignore,
        (true, false) => TouchpadWatchAction::ApplyConfig,
        (false, true) => TouchpadWatchAction::ApplyOverride,
        (true, true) => TouchpadWatchAction::ApplyConfigAndOverride,
    }
}

fn should_apply_touchpad_watch(settings_apply_enabled: bool, keys: &[String]) -> bool {
    settings_apply_enabled && touchpad_watch_action(keys) != TouchpadWatchAction::Ignore
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct CosmicInputConfig {
    acceleration: Option<CosmicAccelConfig>,
    click_method: Option<CosmicClickMethod>,
    disable_while_typing: Option<bool>,
    left_handed: Option<bool>,
    middle_button_emulation: Option<bool>,
    scroll_config: Option<CosmicScrollConfig>,
    tap_config: Option<CosmicTapConfig>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct CosmicAccelConfig {
    profile: Option<CosmicAccelProfile>,
    speed: f64,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct CosmicScrollConfig {
    method: Option<CosmicScrollMethod>,
    natural_scroll: Option<bool>,
    scroll_factor: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
enum CosmicAccelProfile {
    Flat,
    Adaptive,
}

#[derive(Debug, Deserialize, Serialize)]
enum CosmicClickMethod {
    ButtonAreas,
    Clickfinger,
}

#[derive(Debug, Deserialize, Serialize)]
enum CosmicScrollMethod {
    NoScroll,
    TwoFinger,
    Edge,
    OnButtonDown,
}

#[derive(Debug, Deserialize, Serialize)]
enum CosmicTouchpadOverride {
    None,
    ForceDisable,
}

#[derive(Debug, Deserialize, Serialize)]
struct CosmicTapConfig {
    enabled: bool,
    drag: bool,
    drag_lock: bool,
}

fn config_with_sway_values(
    mut input_config: CosmicInputConfig,
    accel_speed: Option<f64>,
    natural_scroll: Option<bool>,
    left_handed: Option<bool>,
) -> CosmicInputConfig {
    if let Some(speed) = accel_speed {
        input_config
            .acceleration
            .get_or_insert_with(Default::default)
            .speed = speed;
    }
    if let Some(natural_scroll) = natural_scroll {
        input_config
            .scroll_config
            .get_or_insert_with(Default::default)
            .natural_scroll = Some(natural_scroll);
    }
    if let Some(left_handed) = left_handed {
        input_config.left_handed = Some(left_handed);
    }
    input_config
}

fn should_apply_mouse_watch(settings_apply_enabled: bool, keys: &[String]) -> bool {
    settings_apply_enabled && watch_action(keys, COSMIC_MOUSE_CONFIG_KEY) == WatchAction::Apply
}

#[derive(Debug, Deserialize, Serialize)]
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
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            sway_connection: utils::new_sway_connection()?,
            _watcher: None,
        })
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

    fn commands_for_config(input_config: CosmicInputConfig) -> Vec<String> {
        let mut commands = Vec::new();
        if let Some(acceleration) = input_config.acceleration {
            commands.push(format!(
                "input type:pointer pointer_accel {}",
                acceleration.speed
            ));
        }
        for (option, value) in [
            ("left_handed", input_config.left_handed),
            (
                "natural_scroll",
                input_config
                    .scroll_config
                    .and_then(|scroll| scroll.natural_scroll),
            ),
        ] {
            if let Some(value) = value {
                let sway_value = if value { "enabled" } else { "disabled" };
                commands.push(format!("input type:pointer {option} {sway_value}"));
            }
        }
        commands
    }

    fn apply_input_config(
        sway_connection: &mut SwayConnection,
        input_config: CosmicInputConfig,
    ) -> Result<(), Box<dyn Error>> {
        for command in Self::commands_for_config(input_config) {
            sway_connection.run_command(command)?;
        }
        Ok(())
    }
}

fn apply_commands<F>(commands: Vec<String>, mut apply: F) -> Result<(), Box<dyn Error>>
where
    F: FnMut(String) -> Result<(), Box<dyn Error>>,
{
    for command in commands {
        apply(command)?;
    }
    Ok(())
}

fn mouse_watch_callback<F>(
    config: &cosmic_config::Config,
    keys: &[String],
    apply: F,
) -> Result<(), Box<dyn Error>>
where
    F: FnMut(String) -> Result<(), Box<dyn Error>>,
{
    if watch_action(keys, COSMIC_MOUSE_CONFIG_KEY) == WatchAction::Ignore {
        return Ok(());
    }

    let input_config = CosmicMouseHandler::input_config_from(config)?;
    apply_commands(CosmicMouseHandler::commands_for_config(input_config), apply)
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
        let Some(libinput) = input.libinput.as_ref() else {
            return Ok(());
        };
        let accel_speed = libinput.accel_speed;
        let natural_scroll = libinput
            .natural_scroll
            .as_ref()
            .map(|value| value.to_primitive());
        let left_handed = libinput
            .left_handed
            .as_ref()
            .map(|value| value.to_primitive());
        if accel_speed.is_none() && natural_scroll.is_none() && left_handed.is_none() {
            return Ok(());
        }

        let config = cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION)?;
        config.set(
            COSMIC_MOUSE_CONFIG_KEY,
            config_with_sway_values(
                Self::input_config_from(&config)?,
                accel_speed,
                natural_scroll,
                left_handed,
            ),
        )?;
        Ok(())
    }

    fn monitor_settings_change(&mut self) {
        let Ok(config) = cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION)
        else {
            error!("Failed to create COSMIC config watcher for mouse settings");
            return;
        };

        match config.watch(|config, keys| {
            if !should_apply_mouse_watch(crate::ALLOW_SETTINGS_APPLY.is_enabled(), keys) {
                return;
            }

            let result = SwayConnection::new()
                .map_err(|err| -> Box<dyn Error> { Box::new(err) })
                .and_then(|mut sway_connection| {
                    mouse_watch_callback(config, keys, |command| {
                        sway_connection.run_command(command)?;
                        Ok(())
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
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            sway_connection: utils::new_sway_connection()?,
            _watcher: None,
        })
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

    fn commands_for_config(input_config: CosmicInputConfig) -> Vec<String> {
        let mut commands = Vec::new();
        if let Some(acceleration) = input_config.acceleration {
            commands.push(format!(
                "input type:touchpad pointer_accel {}",
                acceleration.speed
            ));
            if let Some(profile) = acceleration.profile {
                commands.push(format!(
                    "input type:touchpad accel_profile {}",
                    match profile {
                        CosmicAccelProfile::Flat => "flat",
                        CosmicAccelProfile::Adaptive => "adaptive",
                    }
                ));
            }
        }
        if let Some(click_method) = input_config.click_method {
            commands.push(format!(
                "input type:touchpad click_method {}",
                match click_method {
                    CosmicClickMethod::ButtonAreas => "button_areas",
                    CosmicClickMethod::Clickfinger => "clickfinger",
                }
            ));
        }
        for (option, value) in [
            ("dwt", input_config.disable_while_typing),
            ("left_handed", input_config.left_handed),
            ("middle_emulation", input_config.middle_button_emulation),
        ] {
            if let Some(value) = value {
                commands.push(format!(
                    "input type:touchpad {option} {}",
                    if value { "enabled" } else { "disabled" }
                ));
            }
        }
        if let Some(scroll_config) = input_config.scroll_config {
            if let Some(method) = scroll_config.method {
                commands.push(format!(
                    "input type:touchpad scroll_method {}",
                    match method {
                        CosmicScrollMethod::NoScroll => "none",
                        CosmicScrollMethod::TwoFinger => "two_finger",
                        CosmicScrollMethod::Edge => "edge",
                        CosmicScrollMethod::OnButtonDown => "on_button_down",
                    }
                ));
            }
            if let Some(value) = scroll_config.natural_scroll {
                commands.push(format!(
                    "input type:touchpad natural_scroll {}",
                    if value { "enabled" } else { "disabled" }
                ));
            }
            if let Some(factor) = scroll_config.scroll_factor {
                commands.push(format!("input type:touchpad scroll_factor {factor}"));
            }
        }
        if let Some(tap_config) = input_config.tap_config {
            commands.push(format!(
                "input type:touchpad tap {}",
                if tap_config.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            ));
            commands.push(format!(
                "input type:touchpad drag {}",
                if tap_config.drag {
                    "enabled"
                } else {
                    "disabled"
                }
            ));
            commands.push(format!(
                "input type:touchpad drag_lock {}",
                if tap_config.drag_lock {
                    "enabled"
                } else {
                    "disabled"
                }
            ));
        }
        commands
    }
    fn commands_for_override(override_: Option<CosmicTouchpadOverride>) -> Vec<String> {
        match override_ {
            Some(CosmicTouchpadOverride::ForceDisable) => {
                vec!["input type:touchpad events disabled".to_string()]
            }
            Some(CosmicTouchpadOverride::None) | None => {
                vec!["input type:touchpad events enabled".to_string()]
            }
        }
    }

    fn commands_for_apply(
        input_config: CosmicInputConfig,
        override_: Option<CosmicTouchpadOverride>,
    ) -> Vec<String> {
        let mut commands = Self::commands_for_config(input_config);
        commands.extend(Self::commands_for_override(override_));
        commands
    }
}

fn touchpad_watch_callback<F>(
    config: &cosmic_config::Config,
    keys: &[String],
    apply: F,
) -> Result<(), Box<dyn Error>>
where
    F: FnMut(String) -> Result<(), Box<dyn Error>>,
{
    let action = touchpad_watch_action(keys);
    let commands = match action {
        TouchpadWatchAction::Ignore => Vec::new(),
        TouchpadWatchAction::ApplyConfig => CosmicTouchpadHandler::commands_for_config(
            CosmicTouchpadHandler::input_config_from(config)?,
        ),
        TouchpadWatchAction::ApplyOverride => CosmicTouchpadHandler::commands_for_override(
            CosmicTouchpadHandler::touchpad_override(config)?,
        ),
        TouchpadWatchAction::ApplyConfigAndOverride => CosmicTouchpadHandler::commands_for_apply(
            CosmicTouchpadHandler::input_config_from(config)?,
            CosmicTouchpadHandler::touchpad_override(config)?,
        ),
    };

    apply_commands(commands, apply)
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
        let commands = Self::commands_for_apply(
            Self::input_config_from(&config)?,
            Self::touchpad_override(&config)?,
        );
        for command in commands {
            self.sway_connection.run_command(command)?;
        }
        Ok(())
    }

    fn sync_from_sway_input(&mut self, input: &Input) -> Result<(), Box<dyn Error>> {
        let Some(libinput) = input.libinput.as_ref() else {
            return Ok(());
        };
        let accel_speed = libinput.accel_speed;
        let natural_scroll = libinput
            .natural_scroll
            .as_ref()
            .map(|value| value.to_primitive());
        if accel_speed.is_none() && natural_scroll.is_none() {
            return Ok(());
        }

        let config = cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION)?;
        config.set(
            COSMIC_TOUCHPAD_CONFIG_KEY,
            config_with_sway_values(
                Self::input_config_from(&config)?,
                accel_speed,
                natural_scroll,
                None,
            ),
        )?;
        Ok(())
    }

    fn monitor_settings_change(&mut self) {
        let Ok(config) = cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION)
        else {
            error!("Failed to create COSMIC config watcher for touchpad settings");
            return;
        };

        match config.watch(|config, keys| {
            if !should_apply_touchpad_watch(crate::ALLOW_SETTINGS_APPLY.is_enabled(), keys) {
                return;
            }

            let result = SwayConnection::new()
                .map_err(|err| -> Box<dyn Error> { Box::new(err) })
                .and_then(|mut sway_connection| {
                    touchpad_watch_callback(config, keys, |command| {
                        sway_connection.run_command(command)?;
                        Ok(())
                    })
                });

            if let Err(err) = result {
                error!("Failed to apply COSMIC touchpad settings change: {err}");
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
    pub fn new(name: &'static str) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            name,
            sway_connection: utils::new_sway_connection()?,
            _watcher: None,
        })
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

fn xkb_watch_callback<F>(
    config: &cosmic_config::Config,
    keys: &[String],
    name: &str,
    apply: F,
) -> Result<(), Box<dyn Error>>
where
    F: FnMut(String) -> Result<(), Box<dyn Error>>,
{
    if watch_action(keys, COSMIC_XKB_CONFIG_KEY) == WatchAction::Ignore {
        return Ok(());
    }

    let xkb_config = CosmicInputHandler::xkb_config_from(config)?;
    apply_commands(
        CosmicInputHandler::commands_for_config(name, &xkb_config),
        apply,
    )
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
            if watch_action(keys, COSMIC_XKB_CONFIG_KEY) == WatchAction::Ignore {
                return;
            }

            let result = SwayConnection::new()
                .map_err(|err| -> Box<dyn Error> { Box::new(err) })
                .and_then(|mut sway_connection| {
                    xkb_watch_callback(config, keys, name, |command| {
                        sway_connection.run_command(command)?;
                        Ok(())
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
    use super::{CosmicInputConfig, CosmicInputHandler, CosmicXkbConfig};
    use crate::traits::InputHandler;

    #[test]
    fn watcher_filters_match_supported_keys_only() {
        assert!(super::has_key(
            &["xkb_config".into()],
            super::COSMIC_XKB_CONFIG_KEY
        ));
        assert!(super::has_key(
            &["input_default".into()],
            super::COSMIC_MOUSE_CONFIG_KEY
        ));
        assert!(!super::has_key(
            &["other".into()],
            super::COSMIC_XKB_CONFIG_KEY
        ));
    }

    #[test]
    fn watcher_routes_default_touchpad_and_xkb_keys() {
        assert_eq!(
            super::watch_action(&["input_default".into()], super::COSMIC_MOUSE_CONFIG_KEY),
            super::WatchAction::Apply
        );
        assert_eq!(
            super::watch_action(
                &["input_touchpad".into()],
                super::COSMIC_TOUCHPAD_CONFIG_KEY
            ),
            super::WatchAction::Apply
        );
        assert_eq!(
            super::watch_action(&["xkb_config".into()], super::COSMIC_XKB_CONFIG_KEY),
            super::WatchAction::Apply
        );
        assert_eq!(
            super::watch_action(&["input_default".into()], super::COSMIC_XKB_CONFIG_KEY),
            super::WatchAction::Ignore
        );
    }

    #[test]
    fn mouse_watcher_requires_settings_apply_gate_and_default_key() {
        assert!(!super::should_apply_mouse_watch(
            false,
            &[super::COSMIC_MOUSE_CONFIG_KEY.into()]
        ));
        assert!(!super::should_apply_mouse_watch(
            true,
            &[super::COSMIC_TOUCHPAD_CONFIG_KEY.into()]
        ));
        assert!(super::should_apply_mouse_watch(
            true,
            &[super::COSMIC_MOUSE_CONFIG_KEY.into()]
        ));
    }

    #[test]
    fn mouse_commands_emit_default_input_settings() {
        let config = CosmicInputConfig {
            acceleration: Some(super::CosmicAccelConfig {
                profile: None,
                speed: 0.25,
            }),
            left_handed: Some(true),
            scroll_config: Some(super::CosmicScrollConfig {
                natural_scroll: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            super::CosmicMouseHandler::commands_for_config(config),
            vec![
                "input type:pointer pointer_accel 0.25",
                "input type:pointer left_handed enabled",
                "input type:pointer natural_scroll disabled",
            ]
        );
    }

    fn test_config(name: &str) -> (cosmic_config::Config, std::path::PathBuf) {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "regolith-inputd-cosmic-watch-{name}-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let config = cosmic_config::Config::with_custom_path(
            super::COSMIC_COMP_CONFIG,
            super::COSMIC_COMP_CONFIG_VERSION,
            root.clone(),
        )
        .unwrap();
        (config, root)
    }

    struct TestConfigHomeGuard {
        previous_config_home: Option<std::ffi::OsString>,
        root: std::path::PathBuf,
        restored: bool,
        cleaned: bool,
    }

    impl TestConfigHomeGuard {
        fn new(root: std::path::PathBuf) -> Self {
            let previous_config_home = std::env::var_os("XDG_CONFIG_HOME");
            std::env::set_var("XDG_CONFIG_HOME", &root);
            Self {
                previous_config_home,
                root,
                restored: false,
                cleaned: false,
            }
        }

        fn restore_config_home(&mut self) {
            if self.restored {
                return;
            }
            match &self.previous_config_home {
                Some(path) => std::env::set_var("XDG_CONFIG_HOME", path),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
            self.restored = true;
        }

        fn cleanup(&mut self) -> std::io::Result<()> {
            self.restore_config_home();
            if !self.cleaned && self.root.exists() {
                std::fs::remove_dir_all(&self.root)?;
                self.cleaned = true;
            }
            Ok(())
        }
    }

    impl Drop for TestConfigHomeGuard {
        fn drop(&mut self) {
            self.restore_config_home();
            // Panic cleanup is best effort; the normal test path checks cleanup().
            if !self.cleaned {
                let _ = std::fs::remove_dir_all(&self.root);
            }
        }
    }

    static CONFIG_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn touchpad_reverse_sync_persists_sway_values_without_overwriting_other_config() {
        use cosmic_config::{ConfigGet, ConfigSet};
        use std::os::unix::net::UnixStream;
        let _lock = CONFIG_HOME_LOCK.lock().unwrap();
        let (config, root) = test_config("touchpad-reverse-sync");
        let mut config_home = TestConfigHomeGuard::new(root);

        let original = CosmicInputConfig {
            acceleration: Some(super::CosmicAccelConfig {
                profile: Some(super::CosmicAccelProfile::Flat),
                speed: 0.3,
            }),
            click_method: Some(super::CosmicClickMethod::Clickfinger),
            disable_while_typing: Some(true),
            left_handed: Some(false),
            middle_button_emulation: Some(true),
            scroll_config: Some(super::CosmicScrollConfig {
                method: Some(super::CosmicScrollMethod::Edge),
                natural_scroll: Some(false),
                scroll_factor: Some(2.0),
            }),
            tap_config: Some(super::CosmicTapConfig {
                enabled: true,
                drag: true,
                drag_lock: false,
            }),
        };
        config
            .set(super::COSMIC_TOUCHPAD_CONFIG_KEY, &original)
            .unwrap();
        config
            .set(
                super::COSMIC_TOUCHPAD_OVERRIDE_KEY,
                super::CosmicTouchpadOverride::ForceDisable,
            )
            .unwrap();

        let input = serde_json::from_value(serde_json::json!({
            "identifier": "test-touchpad",
            "name": "Test Touchpad",
            "vendor": 1,
            "product": 2,
            "type": "touchpad",
            "libinput": {
                "accel_speed": -0.4,
                "natural_scroll": "enabled"
            }
        }))
        .unwrap();
        let (connection, _peer) = UnixStream::pair().unwrap();
        let mut handler = super::CosmicTouchpadHandler {
            sway_connection: connection.into(),
            _watcher: None,
        };

        handler.sync_from_sway_input(&input).unwrap();

        let synced: CosmicInputConfig = config.get(super::COSMIC_TOUCHPAD_CONFIG_KEY).unwrap();
        let CosmicInputConfig {
            acceleration,
            click_method,
            disable_while_typing,
            left_handed,
            middle_button_emulation,
            scroll_config,
            tap_config,
        } = synced;
        let acceleration = acceleration.unwrap();
        let scroll_config = scroll_config.unwrap();
        let tap_config = tap_config.unwrap();
        assert!(matches!(
            acceleration.profile,
            Some(super::CosmicAccelProfile::Flat)
        ));
        assert_eq!(acceleration.speed, -0.4);
        assert_eq!(scroll_config.natural_scroll, Some(true));
        assert!(matches!(
            config
                .get::<super::CosmicTouchpadOverride>(super::COSMIC_TOUCHPAD_OVERRIDE_KEY)
                .unwrap(),
            super::CosmicTouchpadOverride::ForceDisable
        ));
        assert!(matches!(
            click_method,
            Some(super::CosmicClickMethod::Clickfinger)
        ));
        assert_eq!(disable_while_typing, Some(true));
        assert_eq!(left_handed, Some(false));
        assert_eq!(middle_button_emulation, Some(true));
        assert!(matches!(
            scroll_config.method,
            Some(super::CosmicScrollMethod::Edge)
        ));
        assert_eq!(scroll_config.scroll_factor, Some(2.0));
        assert!(tap_config.enabled);
        assert!(tap_config.drag);
        assert!(!tap_config.drag_lock);

        drop(config);
        config_home.cleanup().unwrap();
    }

    #[test]
    fn mouse_reverse_sync_persists_sway_values_without_overwriting_other_config() {
        use cosmic_config::{ConfigGet, ConfigSet};
        use std::os::unix::net::UnixStream;
        let _lock = CONFIG_HOME_LOCK.lock().unwrap();
        let (config, root) = test_config("mouse-reverse-sync");
        let mut config_home = TestConfigHomeGuard::new(root);

        let original = CosmicInputConfig {
            acceleration: Some(super::CosmicAccelConfig {
                profile: Some(super::CosmicAccelProfile::Adaptive),
                speed: 0.3,
            }),
            click_method: Some(super::CosmicClickMethod::Clickfinger),
            disable_while_typing: Some(true),
            left_handed: Some(false),
            middle_button_emulation: Some(true),
            scroll_config: Some(super::CosmicScrollConfig {
                method: Some(super::CosmicScrollMethod::Edge),
                natural_scroll: Some(false),
                scroll_factor: Some(2.0),
            }),
            tap_config: Some(super::CosmicTapConfig {
                enabled: true,
                drag: true,
                drag_lock: false,
            }),
        };
        config
            .set(super::COSMIC_MOUSE_CONFIG_KEY, &original)
            .unwrap();

        let input = serde_json::from_value(serde_json::json!({
            "identifier": "test-mouse",
            "name": "Test Mouse",
            "vendor": 1,
            "product": 2,
            "type": "pointer",
            "libinput": {
                "accel_speed": -0.4,
                "natural_scroll": "enabled",
                "left_handed": "enabled"
            }
        }))
        .unwrap();
        let (connection, _peer) = UnixStream::pair().unwrap();
        let mut handler = super::CosmicMouseHandler {
            sway_connection: connection.into(),
            _watcher: None,
        };

        handler.sync_from_sway_input(&input).unwrap();

        let synced: CosmicInputConfig = config.get(super::COSMIC_MOUSE_CONFIG_KEY).unwrap();
        let CosmicInputConfig {
            acceleration,
            click_method,
            disable_while_typing,
            left_handed,
            middle_button_emulation,
            scroll_config,
            tap_config,
        } = synced;
        let acceleration = acceleration.unwrap();
        let scroll_config = scroll_config.unwrap();
        let tap_config = tap_config.unwrap();
        assert!(matches!(
            acceleration.profile,
            Some(super::CosmicAccelProfile::Adaptive)
        ));
        assert_eq!(acceleration.speed, -0.4);
        assert_eq!(scroll_config.natural_scroll, Some(true));
        assert!(matches!(
            click_method,
            Some(super::CosmicClickMethod::Clickfinger)
        ));
        assert_eq!(disable_while_typing, Some(true));
        assert_eq!(left_handed, Some(true));
        assert_eq!(middle_button_emulation, Some(true));
        assert!(matches!(
            scroll_config.method,
            Some(super::CosmicScrollMethod::Edge)
        ));
        assert_eq!(scroll_config.scroll_factor, Some(2.0));
        assert!(tap_config.enabled);
        assert!(tap_config.drag);
        assert!(!tap_config.drag_lock);

        drop(config);
        config_home.cleanup().unwrap();
    }

    #[test]
    fn config_watch_callback_routes_default_input_and_emits_mouse_commands() {
        use cosmic_config::ConfigSet;
        use std::sync::mpsc::channel;
        use std::time::Duration;

        let (config, root) = test_config("mouse");
        let (sender, receiver) = channel();
        let watcher = config
            .watch(move |config, keys| {
                let mut commands = Vec::new();
                let result = super::mouse_watch_callback(config, keys, |command| {
                    commands.push(command);
                    Ok(())
                });
                if result.is_ok() && !commands.is_empty() {
                    sender.send(commands).unwrap();
                }
            })
            .unwrap();

        config
            .set(
                super::COSMIC_MOUSE_CONFIG_KEY,
                CosmicInputConfig {
                    acceleration: Some(super::CosmicAccelConfig {
                        profile: None,
                        speed: 0.25,
                    }),
                    left_handed: Some(true),
                    scroll_config: Some(super::CosmicScrollConfig {
                        natural_scroll: Some(false),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            vec![
                "input type:pointer pointer_accel 0.25",
                "input type:pointer left_handed enabled",
                "input type:pointer natural_scroll disabled",
            ]
        );
        drop(watcher);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn config_watch_callback_routes_touchpad_config_and_override_callbacks() {
        use cosmic_config::ConfigSet;
        use std::sync::mpsc::channel;
        use std::time::Duration;

        let (config, root) = test_config("touchpad");
        let (sender, receiver) = channel();
        let watcher = config
            .watch(move |config, keys| {
                let mut commands = Vec::new();
                let result = super::touchpad_watch_callback(config, keys, |command| {
                    commands.push(command);
                    Ok(())
                });
                if result.is_ok() && !commands.is_empty() {
                    sender.send(commands).unwrap();
                }
            })
            .unwrap();

        let transaction = config.transaction();
        transaction
            .set(
                super::COSMIC_TOUCHPAD_CONFIG_KEY,
                CosmicInputConfig {
                    tap_config: Some(super::CosmicTapConfig {
                        enabled: true,
                        drag: false,
                        drag_lock: true,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
        transaction
            .set(
                super::COSMIC_TOUCHPAD_OVERRIDE_KEY,
                super::CosmicTouchpadOverride::ForceDisable,
            )
            .unwrap();
        transaction.commit().unwrap();

        let callback_results = [
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
        ];
        assert!(callback_results.contains(&vec![
            "input type:touchpad tap enabled".to_string(),
            "input type:touchpad drag disabled".to_string(),
            "input type:touchpad drag_lock enabled".to_string(),
        ]));
        assert!(
            callback_results.contains(&vec!["input type:touchpad events disabled".to_string(),])
        );
        drop(watcher);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn config_watch_callback_routes_xkb_to_keyboard_and_input_sources() {
        use cosmic_config::ConfigSet;
        use std::sync::mpsc::channel;
        use std::time::Duration;

        let (config, root) = test_config("xkb");
        let (sender, receiver) = channel();
        let watcher = config
            .watch(move |config, keys| {
                for name in ["keyboard", "input-sources"] {
                    let mut commands = Vec::new();
                    let result = super::xkb_watch_callback(config, keys, name, |command| {
                        commands.push(command);
                        Ok(())
                    });
                    if result.is_ok() && !commands.is_empty() {
                        sender.send((name, commands)).unwrap();
                    }
                }
            })
            .unwrap();

        config
            .set(
                super::COSMIC_XKB_CONFIG_KEY,
                super::CosmicXkbConfig {
                    layout: "us".into(),
                    variant: "altgr-intl".into(),
                    repeat_delay: 450,
                    repeat_rate: 35,
                },
            )
            .unwrap();

        let mut results = [
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
        ];
        results.sort_by_key(|(name, _)| *name);
        assert_eq!(
            results,
            [
                (
                    "input-sources",
                    vec![
                        String::from("input type:keyboard xkb_variant 'altgr-intl'"),
                        String::from("input type:keyboard xkb_layout 'us'"),
                    ],
                ),
                (
                    "keyboard",
                    vec![
                        String::from("input type:keyboard repeat_delay 450"),
                        String::from("input type:keyboard repeat_rate 35"),
                    ],
                ),
            ]
        );
        drop(watcher);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn touchpad_watcher_triggers_for_config_or_override() {
        assert!(super::touchpad_watch_triggers(&["input_touchpad".into()]));
        assert!(super::touchpad_watch_triggers(&[
            "input_touchpad_override".into()
        ]));
        assert!(!super::touchpad_watch_triggers(&["other".into()]));
    }

    #[test]
    fn touchpad_watcher_applies_only_when_input_config_changes() {
        assert!(super::touchpad_watch_applies(&["input_touchpad".into()]));
        assert!(!super::touchpad_watch_applies(&[
            "input_touchpad_override".into()
        ]));
        assert!(super::touchpad_watch_applies(&[
            "input_touchpad_override".into(),
            "input_touchpad".into(),
        ]));
        assert!(!super::touchpad_watch_applies(&["other".into()]));
    }

    #[test]
    fn touchpad_watcher_decides_config_and_override_side_effects() {
        assert_eq!(
            super::touchpad_watch_action(&["input_touchpad".into()]),
            super::TouchpadWatchAction::ApplyConfig
        );
        assert_eq!(
            super::touchpad_watch_action(&["input_touchpad_override".into()]),
            super::TouchpadWatchAction::ApplyOverride
        );
        assert_eq!(
            super::touchpad_watch_action(&[
                "input_touchpad_override".into(),
                "input_touchpad".into(),
            ]),
            super::TouchpadWatchAction::ApplyConfigAndOverride
        );
        assert_eq!(
            super::touchpad_watch_action(&["other".into()]),
            super::TouchpadWatchAction::Ignore
        );
    }

    #[test]
    fn touchpad_watcher_skips_reverse_sync_writes() {
        let keys = ["input_touchpad".into()];

        assert!(super::should_apply_touchpad_watch(true, &keys));
        assert!(!super::should_apply_touchpad_watch(false, &keys));
    }

    #[test]
    fn touchpad_watcher_applies_override_changes_without_config_changes() {
        assert!(super::touchpad_watch_applies_override(&[
            "input_touchpad_override".into()
        ]));
        assert!(super::touchpad_watch_applies_override(&[
            "input_touchpad_override".into(),
            "input_touchpad".into(),
        ]));
        assert!(!super::touchpad_watch_applies_override(&[
            "input_touchpad".into()
        ]));
    }

    #[test]
    fn touchpad_override_commands_map_force_disable_and_clear() {
        assert_eq!(
            super::CosmicTouchpadHandler::commands_for_override(Some(
                super::CosmicTouchpadOverride::ForceDisable
            )),
            vec!["input type:touchpad events disabled"]
        );
        assert_eq!(
            super::CosmicTouchpadHandler::commands_for_override(Some(
                super::CosmicTouchpadOverride::None
            )),
            vec!["input type:touchpad events enabled"]
        );
        assert_eq!(
            super::CosmicTouchpadHandler::commands_for_override(None),
            vec!["input type:touchpad events enabled"]
        );
    }

    #[test]
    fn touchpad_full_apply_orders_override_after_normal_config() {
        assert_eq!(
            super::CosmicTouchpadHandler::commands_for_apply(
                CosmicInputConfig {
                    tap_config: Some(super::CosmicTapConfig {
                        enabled: true,
                        drag: false,
                        drag_lock: true,
                    }),
                    ..Default::default()
                },
                Some(super::CosmicTouchpadOverride::ForceDisable),
            ),
            vec![
                "input type:touchpad tap enabled",
                "input type:touchpad drag disabled",
                "input type:touchpad drag_lock enabled",
                "input type:touchpad events disabled",
            ]
        );
    }

    #[test]
    fn touchpad_reverse_sync_updates_only_acceleration_speed_and_natural_scroll() {
        let config = CosmicInputConfig {
            acceleration: Some(super::CosmicAccelConfig {
                profile: Some(super::CosmicAccelProfile::Adaptive),
                speed: 0.4,
            }),
            click_method: Some(super::CosmicClickMethod::Clickfinger),
            disable_while_typing: Some(true),
            left_handed: Some(false),
            middle_button_emulation: Some(true),
            scroll_config: Some(super::CosmicScrollConfig {
                method: Some(super::CosmicScrollMethod::TwoFinger),
                natural_scroll: Some(false),
                scroll_factor: Some(1.25),
            }),
            tap_config: Some(super::CosmicTapConfig {
                enabled: true,
                drag: true,
                drag_lock: false,
            }),
        };

        let synced = super::config_with_sway_values(config, Some(-0.2), Some(true), None);

        let acceleration = synced.acceleration.unwrap();
        assert!(matches!(
            acceleration.profile,
            Some(super::CosmicAccelProfile::Adaptive)
        ));
        assert_eq!(acceleration.speed, -0.2);
        assert!(matches!(
            synced.click_method,
            Some(super::CosmicClickMethod::Clickfinger)
        ));
        assert_eq!(synced.disable_while_typing, Some(true));
        assert_eq!(synced.left_handed, Some(false));
        assert_eq!(synced.middle_button_emulation, Some(true));
        let scroll = synced.scroll_config.unwrap();
        assert!(matches!(
            scroll.method,
            Some(super::CosmicScrollMethod::TwoFinger)
        ));
        assert_eq!(scroll.natural_scroll, Some(true));
        assert_eq!(scroll.scroll_factor, Some(1.25));
        let tap = synced.tap_config.unwrap();
        assert!(tap.enabled);
        assert!(tap.drag);
        assert!(!tap.drag_lock);
    }

    #[test]
    fn touchpad_reverse_sync_leaves_missing_sway_values_and_override_independent() {
        let config = CosmicInputConfig {
            acceleration: Some(super::CosmicAccelConfig {
                profile: Some(super::CosmicAccelProfile::Flat),
                speed: 0.3,
            }),
            scroll_config: Some(super::CosmicScrollConfig {
                method: Some(super::CosmicScrollMethod::Edge),
                natural_scroll: Some(false),
                scroll_factor: Some(2.0),
            }),
            ..Default::default()
        };

        let synced = super::config_with_sway_values(config, None, None, None);

        let acceleration = synced.acceleration.unwrap();
        assert!(matches!(
            acceleration.profile,
            Some(super::CosmicAccelProfile::Flat)
        ));
        assert_eq!(acceleration.speed, 0.3);
        let scroll = synced.scroll_config.unwrap();
        assert!(matches!(
            scroll.method,
            Some(super::CosmicScrollMethod::Edge)
        ));
        assert_eq!(scroll.natural_scroll, Some(false));
        assert_eq!(scroll.scroll_factor, Some(2.0));
        assert_eq!(
            super::CosmicTouchpadHandler::commands_for_override(Some(
                super::CosmicTouchpadOverride::ForceDisable
            )),
            vec!["input type:touchpad events disabled"]
        );
    }

    #[test]
    fn touchpad_commands_map_all_supported_boolean_and_numeric_options() {
        let config = CosmicInputConfig {
            acceleration: Some(super::CosmicAccelConfig {
                profile: Some(super::CosmicAccelProfile::Flat),
                speed: 0.0,
            }),
            disable_while_typing: Some(false),
            left_handed: Some(true),
            middle_button_emulation: Some(false),
            scroll_config: Some(super::CosmicScrollConfig {
                method: Some(super::CosmicScrollMethod::OnButtonDown),
                natural_scroll: Some(false),
                scroll_factor: Some(2.0),
            }),
            tap_config: Some(super::CosmicTapConfig {
                enabled: false,
                drag: true,
                drag_lock: false,
            }),
            ..Default::default()
        };

        assert_eq!(
            super::CosmicTouchpadHandler::commands_for_config(config),
            vec![
                "input type:touchpad pointer_accel 0",
                "input type:touchpad accel_profile flat",
                "input type:touchpad dwt disabled",
                "input type:touchpad left_handed enabled",
                "input type:touchpad middle_emulation disabled",
                "input type:touchpad scroll_method on_button_down",
                "input type:touchpad natural_scroll disabled",
                "input type:touchpad scroll_factor 2",
                "input type:touchpad tap disabled",
                "input type:touchpad drag enabled",
                "input type:touchpad drag_lock disabled",
            ]
        );
    }

    #[test]
    fn keyboard_and_input_source_mapping_stays_split() {
        let config = CosmicXkbConfig {
            layout: "us".into(),
            variant: String::new(),
            repeat_delay: 500,
            repeat_rate: 30,
        };
        assert_eq!(
            CosmicInputHandler::commands_for_config("keyboard", &config).len(),
            2
        );
        assert_eq!(
            CosmicInputHandler::commands_for_config("input-sources", &config),
            vec![String::from("input type:keyboard xkb_layout 'us'").to_string()]
        );
    }

    #[test]
    fn touchpad_commands_map_acceleration_profile_click_and_scroll() {
        let config = CosmicInputConfig {
            acceleration: Some(super::CosmicAccelConfig {
                profile: Some(super::CosmicAccelProfile::Adaptive),
                speed: 0.4,
            }),
            click_method: Some(super::CosmicClickMethod::Clickfinger),
            scroll_config: Some(super::CosmicScrollConfig {
                method: Some(super::CosmicScrollMethod::TwoFinger),
                natural_scroll: Some(true),
                scroll_factor: Some(1.25),
            }),
            ..Default::default()
        };
        assert_eq!(
            super::CosmicTouchpadHandler::commands_for_config(config),
            vec![
                "input type:touchpad pointer_accel 0.4",
                "input type:touchpad accel_profile adaptive",
                "input type:touchpad click_method clickfinger",
                "input type:touchpad scroll_method two_finger",
                "input type:touchpad natural_scroll enabled",
                "input type:touchpad scroll_factor 1.25"
            ]
        );
    }

    #[test]
    fn touchpad_commands_map_tap_drag_and_drag_lock() {
        let config = CosmicInputConfig {
            tap_config: Some(super::CosmicTapConfig {
                enabled: true,
                drag: false,
                drag_lock: true,
            }),
            ..Default::default()
        };
        assert_eq!(
            super::CosmicTouchpadHandler::commands_for_config(config),
            vec![
                "input type:touchpad tap enabled",
                "input type:touchpad drag disabled",
                "input type:touchpad drag_lock enabled"
            ]
        );
    }

    #[test]
    fn touchpad_commands_skip_partial_optional_configs() {
        let config = CosmicInputConfig {
            acceleration: Some(super::CosmicAccelConfig {
                profile: None,
                speed: -0.2,
            }),
            scroll_config: Some(super::CosmicScrollConfig {
                method: Some(super::CosmicScrollMethod::NoScroll),
                natural_scroll: None,
                scroll_factor: None,
            }),
            ..Default::default()
        };
        assert_eq!(
            super::CosmicTouchpadHandler::commands_for_config(config),
            vec![
                "input type:touchpad pointer_accel -0.2",
                "input type:touchpad scroll_method none"
            ]
        );
    }

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
                String::from("input type:keyboard repeat_delay 450").to_string(),
                String::from("input type:keyboard repeat_rate 35").to_string(),
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
                String::from("input type:keyboard repeat_delay 450").to_string(),
                String::from("input type:keyboard repeat_rate 35").to_string(),
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
