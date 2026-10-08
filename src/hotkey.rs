//! The quick-ask shortcut, bound through the GlobalShortcuts portal: it works
//! from any app, and its activation token lets Wayland focus the window.
//!
//! GNOME asks the user to confirm a new binding in a dialog, so the app binds
//! at startup only when GNOME already has a binding stored for it; otherwise
//! it waits for the user to choose "Quick Ask Shortcut…" in the menu.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;

use crate::app::App;
use crate::quick_ask;

const SHORTCUT_ID: &str = "quick-ask";
/// Offered in GNOME's dialog; Ctrl+Alt+M is unbound in a default GNOME.
const PREFERRED_TRIGGER: &str = "CTRL+ALT+m";
/// Where GNOME Settings stores the bindings it has confirmed.
const GNOME_SCHEMA: &str = "org.gnome.settings-daemon.global-shortcuts";
/// At login the portal may not be up yet when autostart runs Muse.
const STARTUP_RETRIES: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum State {
    #[default]
    Unbound,
    Binding,
    /// Bound for this run; the trigger GNOME reports, empty if none.
    Bound(String),
}

thread_local! {
    /// The portal accepts one host-app registration per connection.
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn state() -> State {
    STATE.with_borrow(Clone::clone)
}

fn set_state(state: State) {
    STATE.set(state);
}

/// Binds the shortcut silently if GNOME already has one stored for this app.
pub fn init(app: &Rc<App>) {
    if stored_in_gnome(&app_id(app)) {
        start(app, None, STARTUP_RETRIES);
    }
}

/// The "Quick Ask Shortcut…" menu item: binds the shortcut (GNOME shows its
/// dialog the first time), or says which key it is.
pub fn set_up(app: &Rc<App>, parent: Option<gtk::Window>) {
    match state() {
        State::Binding => {}
        State::Bound(trigger) => explain(&trigger, parent.as_ref()),
        State::Unbound => start(app, parent, 0),
    }
}

fn app_id(app: &App) -> String {
    app.gtk
        .application_id()
        .map(|id| id.to_string())
        .unwrap_or_default()
}

fn stored_in_gnome(app_id: &str) -> bool {
    let Some(source) = gio::SettingsSchemaSource::default() else {
        return false;
    };
    if source.lookup(GNOME_SCHEMA, true).is_none() {
        return false;
    }
    gio::Settings::new(GNOME_SCHEMA)
        .strv("applications")
        .iter()
        .any(|id| id == app_id)
}

/// Binds in the background. `retries` applies to the silent startup bind.
fn start(app: &Rc<App>, parent: Option<gtk::Window>, retries: u32) {
    set_state(State::Binding);
    let app = Rc::clone(app);
    glib::spawn_future_local(async move {
        let result = listen(&app, parent.clone()).await;
        set_state(State::Unbound);
        let Err(e) = result else {
            log::warn!("quick-ask shortcut: the portal closed its signal stream");
            return;
        };
        log::warn!("quick-ask shortcut unavailable: {e}");
        if let Some(parent) = &parent {
            let cancelled = matches!(
                e,
                ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)
            );
            if !cancelled {
                failed(&e.to_string(), Some(parent));
            }
        } else if retries > 0 {
            glib::timeout_add_local_once(RETRY_DELAY, move || {
                if state() == State::Unbound {
                    start(&app, None, retries - 1);
                }
            });
        }
    });
}

/// Binds the shortcut, then toggles quick ask on every press. Runs for the
/// life of the app.
async fn listen(app: &Rc<App>, parent: Option<gtk::Window>) -> ashpd::Result<()> {
    if !REGISTERED.get() {
        let id = ashpd::AppID::try_from(app_id(app).as_str())?;
        ashpd::register_host_app(id).await?;
        REGISTERED.set(true);
    }
    let portal = GlobalShortcuts::new().await?;
    let session = portal.create_session(Default::default()).await?;
    let result = bind_and_listen(app, &portal, &session, parent).await;
    // A session can bind only once; close it so a retry starts clean.
    if let Err(e) = session.close().await {
        log::debug!("closing the shortcut session: {e}");
    }
    result
}

async fn bind_and_listen(
    app: &Rc<App>,
    portal: &GlobalShortcuts,
    session: &ashpd::desktop::Session<GlobalShortcuts>,
    parent: Option<gtk::Window>,
) -> ashpd::Result<()> {
    let mut activated = portal.receive_activated().await?;

    let identifier = match &parent {
        Some(window) => ashpd::WindowIdentifier::from_native(window).await,
        None => None,
    };
    let shortcut =
        NewShortcut::new(SHORTCUT_ID, "Open Quick Ask").preferred_trigger(PREFERRED_TRIGGER);
    let bound = portal
        .bind_shortcuts(
            session,
            &[shortcut],
            identifier.as_ref(),
            Default::default(),
        )
        .await?
        .response()?;
    let trigger = bound
        .shortcuts()
        .iter()
        .find(|s| s.id() == SHORTCUT_ID)
        .map(|s| s.trigger_description().to_owned())
        .unwrap_or_default();
    log::info!("quick-ask shortcut bound: {trigger:?}");
    set_state(State::Bound(trigger.clone()));
    if parent.is_some() {
        explain(&trigger, parent.as_ref());
    }

    while let Some(event) = activated.next().await {
        // One session per process, so the id alone identifies the press.
        if event.shortcut_id() != SHORTCUT_ID {
            continue;
        }
        let token = event
            .options()
            .get("activation_token")
            .and_then(|v| <&str>::try_from(v).ok())
            .map(str::to_owned);
        quick_ask::toggle(app, token.as_deref());
    }
    Ok(())
}

fn explain(trigger: &str, parent: Option<&gtk::Window>) {
    let body = if trigger.is_empty() {
        "No key is assigned yet. Assign one in Settings, under Keyboard → View and Customize \
         Shortcuts."
            .to_owned()
    } else {
        format!(
            "Press {trigger} in any app to open Quick Ask. To change it, open Settings, then \
             Keyboard → View and Customize Shortcuts."
        )
    };
    let dialog = adw::AlertDialog::new(Some("Quick Ask Shortcut"), Some(&body));
    dialog.add_response("ok", "_OK");
    dialog.present(parent);
}

fn failed(error: &str, parent: Option<&gtk::Window>) {
    // The portal only knows apps with an installed desktop file.
    let cause = if error.contains("App info not found") {
        "Global shortcuts need Muse to be installed. Run “make install”, then start Muse \
         from the app grid."
            .to_owned()
    } else {
        format!("The desktop’s shortcut service said: {error}")
    };
    let body = format!(
        "{cause}\n\nAlternatively, “make install-shortcut” binds Ctrl+Alt+M to \
         “muse --quick-ask” as a custom shortcut."
    );
    let dialog = adw::AlertDialog::new(Some("Can’t Set Up the Shortcut"), Some(&body));
    dialog.add_response("ok", "_OK");
    dialog.present(parent);
}
