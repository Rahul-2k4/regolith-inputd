use crate::traits::InputHandler;
use cosmic_config::ConfigGet;
use log::{debug, error};
use notify::RecommendedWatcher;
use serde::Deserialize;
use std::error::Error;
use swayipc::{Connection as SwayConnection, Input};

const COSMIC_COMP_CONFIG: &str = "com.system76.CosmicComp";
const COSMIC_COMP_CONFIG_VERSION: u64 = 1;

#[derive(Debug, Default, Deserialize)]
struct CosmicInputConfig {
    acceleration: Option<CosmicAccelConfig>,
    left_handed: Option<bool>,
    scroll_config: Option<CosmicScrollConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct CosmicAccelConfig {
    speed: f64,
}

#[derive(Debug, Default, Deserialize)]
struct CosmicScrollConfig {
    natural_scroll: Option<bool>,
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
        Ok(config.get("input_default").unwrap_or_default())
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
            if !keys.iter().any(|key| key == "input_default") {
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

pub struct CosmicInputHandler {
    name: &'static str,
    sway_connection: SwayConnection,
}

impl CosmicInputHandler {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            sway_connection: SwayConnection::new().unwrap(),
        }
    }
}

impl InputHandler for CosmicInputHandler {
    fn sway_connection(&mut self) -> &mut SwayConnection {
        &mut self.sway_connection
    }

    fn apply_changes(&mut self, key: &str) -> Result<(), Box<dyn Error>> {
        debug!(
            "COSMIC input handler '{}' has no settings apply path for key '{}' yet",
            self.name, key
        );
        Ok(())
    }

    fn apply_all(&mut self) -> Result<(), Box<dyn Error>> {
        debug!(
            "COSMIC input handler '{}' has no settings apply path yet",
            self.name
        );
        Ok(())
    }

    fn sync_from_sway_input(&mut self, input: &Input) -> Result<(), Box<dyn Error>> {
        debug!(
            "COSMIC input handler '{}' has no sync path for sway input type '{}' yet",
            self.name, input.input_type
        );
        Ok(())
    }

    fn monitor_settings_change(&mut self) {}
}

unsafe impl Send for CosmicInputHandler {}
