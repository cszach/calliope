//! Running without a window and starting at login.
//!
//! Inside Flatpak both go through the Background portal: GNOME asks the user
//! once, lists Calliope under Background Apps, and writes the autostart entry
//! itself. Outside the sandbox Calliope writes its own autostart entry.

use std::path::PathBuf;
use std::rc::Rc;

use ashpd::desktop::background::Background;

use crate::app::App;
use crate::consts::APP_ID;

const REASON: &str = "Calliope keeps running to show Muse’s notifications and open Quick Ask";
const AUTOSTART_TEMPLATE: &str =
    include_str!("../data/io.github.cszach.Calliope.autostart.desktop.in");

/// At startup. Outside the sandbox: adopts an autostart entry written by an
/// older version, and leaves the entry alone otherwise, so a development
/// build never repoints it at itself. Inside: renews the background
/// permission and autostart if background mode is on.
pub fn init(app: &Rc<App>) {
    if !ashpd::is_sandboxed() {
        if autostart_file().is_file() && !app.config().start_at_login {
            app.config_mut().start_at_login = true;
            app.save_config();
        }
    } else if app.config().background_mode {
        sync(app, None);
    }
}

/// Brings the system in line with the config: background permission and
/// autostart follow background mode and "Start at Login". Turns background
/// mode off again if the user refuses it.
pub fn sync(app: &Rc<App>, parent: Option<gtk::Window>) {
    let running = app.config().background_mode;
    let autostart = running && app.config().start_at_login;
    if !ashpd::is_sandboxed() {
        set_autostart_file(autostart);
        return;
    }
    let app = Rc::clone(app);
    glib::spawn_future_local(async move {
        let identifier = match &parent {
            Some(window) => ashpd::WindowIdentifier::from_native(window).await,
            None => None,
        };
        let result = Background::request()
            .identifier(identifier)
            .reason(REASON)
            .auto_start(autostart)
            .command(["calliope", "--background"])
            .dbus_activatable(false)
            .send()
            .await
            .and_then(|request| request.response());
        match result {
            Ok(answer) => {
                log::info!(
                    "background portal: run in background {}, autostart {}",
                    answer.run_in_background(),
                    answer.auto_start()
                );
                if running && !answer.run_in_background() {
                    refused(&app);
                }
            }
            Err(ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)) if running => {
                refused(&app)
            }
            Err(e) => log::warn!("background portal: {e}"),
        }
    });
}

/// The user said no: GNOME would stop Calliope once its windows close.
fn refused(app: &App) {
    log::info!("running in the background was refused; turning background mode off");
    app.leave_background_mode();
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
