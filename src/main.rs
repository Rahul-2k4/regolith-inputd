use log::error;
use regolith_inputd::SettingsManager;

fn main() {
    pretty_env_logger::init();

    #[cfg(feature = "gnome")]
    use gio::{
        prelude::ApplicationExtManual, traits::ApplicationExt, Application, ApplicationFlags,
    };

    #[cfg(feature = "gnome")]
    let app = Application::new(Some("org.regolith.inputd"), ApplicationFlags::IS_SERVICE);
    let mut manager = match SettingsManager::new() {
        Ok(manager) => manager,
        Err(error) => {
            error!("Failed to connect to Sway IPC during startup: {error}");
            std::process::exit(1);
        }
    };
    if let Err(e) = manager.start_monitoring() {
        error!("{e}");
        std::process::exit(1);
    }

    #[cfg(feature = "gnome")]
    app.hold();

    #[cfg(feature = "gnome")]
    app.run();

    #[cfg(not(feature = "gnome"))]
    loop {
        std::thread::park();
    }
}
