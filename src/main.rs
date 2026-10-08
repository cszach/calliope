//! Muse: an unofficial GNOME desktop client for Meta's Muse (muse.ai).

mod app;
mod config;
mod consts;
mod engine;
mod paths;
mod permissions;
mod policy;
mod popup;
mod webview;
mod window;

use config::Config;

fn main() -> glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("muse=info")).init();

    let config_path = paths::config_file();
    let (config, config_writable) = Config::load(&config_path);

    // WebKit and GTK read these once, when they start.
    for (key, value) in &config.webkit.env {
        // SAFETY: no other threads exist yet.
        unsafe { std::env::set_var(key, value) };
    }

    app::App::new(config, config_path, config_writable).run()
}
