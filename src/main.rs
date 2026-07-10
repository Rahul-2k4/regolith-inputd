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
    let mut manager = SettingsManager::new();
    if let Err(e) = manager.start_monitoring() {
        error!("{e}");
        panic!();
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
