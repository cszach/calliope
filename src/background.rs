//! Starting at login: Calliope writes its own autostart entry.

use std::path::PathBuf;

use crate::app::App;
use crate::consts::APP_ID;

const AUTOSTART_TEMPLATE: &str =
    include_str!("../data/io.github.cszach.Calliope.autostart.desktop.in");

/// At startup: adopts an autostart entry written by an older version, and
/// leaves the entry alone otherwise, so a development build never repoints
/// it at itself.
pub fn init(app: &App) {
    if autostart_file().is_file() && !app.config().start_at_login {
        app.config_mut().start_at_login = true;
        app.save_config();
    }
}

/// Brings the autostart entry in line with the config: it exists while
/// background mode and "Start at Login" are both on.
pub fn sync(app: &App) {
    set_autostart_file(app.config().background_mode && app.config().start_at_login);
}

fn autostart_file() -> PathBuf {
    glib::user_config_dir()
        .join("autostart")
        .join(format!("{APP_ID}.desktop"))
}

/// The autostart entry for this binary, wherever it is installed.
fn autostart_entry() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_str()?;
    Some(AUTOSTART_TEMPLATE.replace("@BINDIR@", dir))
}

fn set_autostart_file(on: bool) {
    let path = autostart_file();
    let result = if on {
        let Some(entry) = autostart_entry() else {
            log::warn!("cannot find Calliope’s own path for the autostart entry");
            return;
        };
        path.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, entry))
    } else {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    };
    if let Err(e) = result {
        log::warn!("cannot update {}: {e}", path.display());
    }
}
