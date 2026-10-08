//! Muse: an unofficial GNOME desktop client for Meta's Muse (muse.ai).

mod app;
mod config;
mod consts;
mod downloads;
mod engine;
mod hotkey;
mod notifications;
mod paths;
mod permissions;
mod policy;
mod popup;
mod prompt;
mod quick_ask;
mod search_provider;
mod shortcuts;
mod startup_env;
mod tab;
mod webview;
mod window;
mod zoom;

use config::Config;

fn main() -> glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("muse=info")).init();

    let config_path = paths::config_file();
    let (config, config_writable) = Config::load(&config_path);

    // WebKit, GStreamer and GTK read these once, when they start.
    // Read here rather than through GApplication: it must act before GTK
    // starts, and a second instance only forwards to the first.
    let safe_graphics = std::env::args_os().any(|a| a == "--safe-graphics");
    let changes = startup_env::changes(&config.webkit.env, safe_graphics, |key| {
        std::env::var_os(key).is_some()
    });
    for (key, value) in changes {
        // SAFETY: no other threads exist yet.
        unsafe {
            match value {
                Some(value) => std::env::set_var(&key, value),
                None => std::env::remove_var(&key),
            }
        }
    }

    app::App::new(config, config_path, config_writable).run()
}
