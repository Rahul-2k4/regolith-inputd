use crate::traits::InputHandler;
use log::debug;
use std::error::Error;
use swayipc::{Connection as SwayConnection, Input};

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
