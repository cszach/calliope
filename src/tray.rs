//! The top bar icon: a StatusNotifierItem with a small menu, shown by the
//! AppIndicator extension (GNOME has no tray of its own). It carries a dot
//! while a Muse notification has arrived that no Calliope window has been
//! focused for since.
//!
//! The item and its `com.canonical.dbusmenu` menu are exported on the app's
//! own connection and registered under the app's bus name, so the Flatpak
//! needs only to talk to the watcher. The extension sends an activation token
//! before each click (`ProvideXdgActivationToken`), which lets Wayland focus
//! the window the click opens.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;

use crate::app::App;
use crate::consts::APP_ID;
use crate::{quick_ask, window};

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/io/github/cszach/Calliope/TopBarMenu";
const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const ITEM_XML: &str = include_str!("../data/dbus/org.kde.StatusNotifierItem.xml");
const MENU_XML: &str = include_str!("../data/dbus/com.canonical.dbusmenu.xml");
/// The menu never changes, so its layout has one revision.
const MENU_REVISION: u32 = 1;

thread_local! {
    static ATTENTION: Cell<bool> = const { Cell::new(false) };
    /// The token from the last `ProvideXdgActivationToken` and when it came
    /// (monotonic µs), for the click that follows it.
    static TOKEN: RefCell<Option<(String, i64)>> = const { RefCell::new(None) };
}

/// The menu, in order. Ids are dbusmenu item ids; 0 is the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Open,
    QuickAsk,
    NewWindow,
    Separator,
    Quit,
}

const MENU: [Entry; 5] = [
    Entry::Open,
    Entry::QuickAsk,
    Entry::NewWindow,
    Entry::Separator,
    Entry::Quit,
];

impl Entry {
    fn id(self) -> i32 {
        MENU.iter()
            .position(|e| *e == self)
            .map_or(0, |i| i as i32 + 1)
    }

    fn from_id(id: i32) -> Option<Self> {
        usize::try_from(id - 1)
            .ok()
            .and_then(|i| MENU.get(i))
            .copied()
    }

    /// dbusmenu labels mark the access key with an underscore, like GTK.
    fn label(self) -> &'static str {
        match self {
            Entry::Open => "_Open Calliope",
            Entry::QuickAsk => "_Quick Ask",
            Entry::NewWindow => "_New Window",
            Entry::Separator => "",
            Entry::Quit => "Q_uit",
        }
    }
}

/// The item's `Status`: hidden when the user turned the icon off.
fn status(shown: bool, attention: bool) -> &'static str {
    match (shown, attention) {
        (false, _) => "Passive",
        (true, false) => "Active",
        (true, true) => "NeedsAttention",
    }
}

/// Called from `startup`. A Calliope that GNOME Shell started only to answer
/// searches shows no icon until it opens a window, so typing in the overview
/// never flashes one into the top bar.
pub fn init(app: &Rc<App>) {
    if !app.gtk.flags().contains(gio::ApplicationFlags::IS_SERVICE) {
        start(app);
        return;
    }
    let started = Cell::new(false);
    let a = Rc::clone(app);
    app.gtk.connect_window_added(move |_, _| {
        if !started.replace(true) {
            start(&a);
        }
    });
}

/// Exports the item and its menu and registers with the watcher whenever
/// one is on the bus.
fn start(app: &Rc<App>) {
    let Some(connection) = app.gtk.dbus_connection() else {
        log::warn!("top bar icon: no D-Bus connection");
        return;
    };
    if let Err(e) = export(app, &connection) {
        log::warn!("top bar icon: {e}");
        return;
    }
    let Some(name) = app.gtk.application_id().map(|id| id.to_string()) else {
        log::warn!("top bar icon: the app has no id");
        return;
    };
    // The watcher comes and goes with the extension and with GNOME Shell.
    let _ = gio::bus_watch_name_on_connection(
        &connection,
        WATCHER,
        gio::BusNameWatcherFlags::NONE,
        move |connection, _, _| register(&connection, &name),
        |_, _| log::debug!("top bar icon: no StatusNotifierWatcher"),
    );

    // Any Calliope window coming to the front clears the dot.
    for window in app.gtk.windows() {
        clear_on_focus(app, &window);
    }
    let a = Rc::clone(app);
    app.gtk
        .connect_window_added(move |_, window| clear_on_focus(&a, window));
}

fn clear_on_focus(app: &Rc<App>, window: &gtk::Window) {
    let a = Rc::clone(app);
    window.connect_is_active_notify(move |window| {
        if window.is_active() {
            set_attention(&a, false);
        }
    });
}

fn export(app: &Rc<App>, connection: &gio::DBusConnection) -> Result<(), String> {
    let interface = |xml: &str, name: &str| {
        gio::DBusNodeInfo::for_xml(xml)
            .map_err(|e| format!("bad interface XML: {e}"))?
            .lookup_interface(name)
            .ok_or_else(|| format!("{name} missing from its XML"))
    };
    let item = interface(ITEM_XML, ITEM_INTERFACE)?;
    let menu = interface(MENU_XML, MENU_INTERFACE)?;

    let a = Rc::clone(app);
    let b = Rc::clone(app);
    connection
        .register_object(ITEM_PATH, &item)
        .method_call(move |_, _, _, _, method, params, invocation| {
            item_call(&a, method, &params);
            invocation.return_value(None);
        })
        .property(move |_, _, _, _, name| item_property(&b, name))
        .build()
        .map_err(|e| format!("cannot export the item: {e}"))?;

    let a = Rc::clone(app);
    connection
        .register_object(MENU_PATH, &menu)
        .method_call(move |_, _, _, _, method, params, invocation| {
            invocation.return_value(menu_call(&a, method, &params).as_ref());
        })
        .property(|_, _, _, _, name| menu_property(name))
        .build()
        .map_err(|e| format!("cannot export the menu: {e}"))?;
    Ok(())
}

/// Registers under the app's well-known name, not the unique one: when the
/// extension starts it also scans the bus for items and files any it finds
/// under the connection's well-known name. Registering under that same name
/// makes the two meet as one item whichever comes first; a unique name made
/// them two icons.
fn register(connection: &gio::DBusConnection, name: &str) {
    connection.call(
        Some(WATCHER),
        "/StatusNotifierWatcher",
        WATCHER,
        "RegisterStatusNotifierItem",
        Some(&(name,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        |result| match result {
            Ok(_) => log::info!("top bar icon registered"),
            Err(e) => log::warn!("top bar icon: the watcher refused it: {e}"),
        },
    );
}

/// Shows or hides the icon, for Preferences.
pub fn set_shown(app: &App, shown: bool) {
    app.config_mut().top_bar_icon = shown;
    app.save_config();
    emit_status(app);
}

/// Marks a notification the user has not seen in a Calliope window yet.
pub fn notify(app: &App) {
    let seen = app.gtk.windows().iter().any(|w| w.is_active());
    if !seen {
        set_attention(app, true);
    }
}

fn set_attention(app: &App, on: bool) {
    if ATTENTION.replace(on) != on {
        emit_status(app);
    }
}

fn emit_status(app: &App) {
    let Some(connection) = app.gtk.dbus_connection() else {
        return;
    };
    let status = status(app.config().top_bar_icon, ATTENTION.get());
    if let Err(e) = connection.emit_signal(
        None,
        ITEM_PATH,
        ITEM_INTERFACE,
        "NewStatus",
        Some(&(status,).to_variant()),
    ) {
        log::debug!("top bar icon: {e}");
    }
}

fn item_property(app: &App, name: &str) -> glib::Variant {
    let no_pixmaps: Vec<(i32, i32, Vec<u8>)> = Vec::new();
    match name {
        "Category" => "ApplicationStatus".to_variant(),
        "Id" => APP_ID.to_variant(),
        "Title" => "Calliope".to_variant(),
        "Status" => status(app.config().top_bar_icon, ATTENTION.get()).to_variant(),
        "WindowId" => 0i32.to_variant(),
        "IconName" => format!("{APP_ID}-symbolic").to_variant(),
        "AttentionIconName" => format!("{APP_ID}-attention-symbolic").to_variant(),
        "IconPixmap" | "OverlayIconPixmap" | "AttentionIconPixmap" => no_pixmaps.to_variant(),
        "ToolTip" => ("", no_pixmaps, "Calliope", "").to_variant(),
        "ItemIsMenu" => false.to_variant(),
        "Menu" => glib::variant::ObjectPath::try_from(MENU_PATH)
            .map(|p| p.to_variant())
            .unwrap_or_else(|_| "/".to_variant()),
        // IconThemePath, OverlayIconName, AttentionMovieName.
        _ => "".to_variant(),
    }
}

fn item_call(app: &Rc<App>, method: &str, params: &glib::Variant) {
    match method {
        "ProvideXdgActivationToken" => {
            if let Some((token,)) = params.get::<(String,)>() {
                TOKEN.replace(Some((token, glib::monotonic_time())));
            }
        }
        "Activate" => run(app, Entry::Open),
        // A middle click.
        "SecondaryActivate" => run(app, Entry::QuickAsk),
        _ => {}
    }
}

/// The extension sends the token just before the click it belongs to; an
/// older one belongs to a click that never arrived.
const TOKEN_LIFETIME_US: i64 = 5_000_000;

fn run(app: &Rc<App>, entry: Entry) {
    let token = TOKEN
        .take()
        .filter(|(_, at)| glib::monotonic_time() - at < TOKEN_LIFETIME_US)
        .map(|(token, _)| token);
    match entry {
        Entry::Open => app.show_main_window(token.as_deref()),
        Entry::QuickAsk => quick_ask::toggle(app, token.as_deref()),
        Entry::NewWindow => {
            let window = window::create(app, &[]);
            if let Some(token) = &token {
                window.set_startup_id(token);
            }
            window.present();
        }
        Entry::Quit => app.gtk.activate_action("quit", None),
        Entry::Separator => {}
    }
}

fn menu_property(name: &str) -> glib::Variant {
    match name {
        "Version" => 3u32.to_variant(),
        "TextDirection" => "ltr".to_variant(),
        "Status" => "normal".to_variant(),
        _ => Vec::<String>::new().to_variant(),
    }
}

/// The reply to a dbusmenu call, or `None` for methods with no reply.
fn menu_call(app: &Rc<App>, method: &str, params: &glib::Variant) -> Option<glib::Variant> {
    match method {
        "GetLayout" => Some(layout_reply()),
        "GetGroupProperties" => {
            let (ids, _) = params.get::<(Vec<i32>, Vec<String>)>()?;
            let found: Vec<_> = ids
                .into_iter()
                .filter_map(|id| Some((id, properties(Entry::from_id(id)?))))
                .collect();
            Some((found,).to_variant())
        }
        "GetProperty" => {
            let (id, name) = params.get::<(i32, String)>()?;
            let value = Entry::from_id(id)
                .and_then(|e| properties(e).remove(&name))
                .unwrap_or_else(|| "".to_variant());
            Some((value,).to_variant())
        }
        "Event" => {
            let (id, event, _, _) = params.get::<(i32, String, glib::Variant, u32)>()?;
            if event == "clicked" {
                if let Some(entry) = Entry::from_id(id) {
                    run(app, entry);
                }
            }
            None
        }
        "EventGroup" => {
            let (events,) = params.get::<(Vec<(i32, String, glib::Variant, u32)>,)>()?;
            let mut errors = Vec::new();
            for (id, event, _, _) in events {
                match Entry::from_id(id) {
                    Some(entry) if event == "clicked" => run(app, entry),
                    Some(_) => {}
                    None => errors.push(id),
                }
            }
            Some((errors,).to_variant())
        }
        "AboutToShow" => Some((false,).to_variant()),
        "AboutToShowGroup" => Some((Vec::<i32>::new(), Vec::<i32>::new()).to_variant()),
        _ => None,
    }
}

fn properties(entry: Entry) -> HashMap<String, glib::Variant> {
    let mut props = HashMap::new();
    if entry == Entry::Separator {
        props.insert("type".to_owned(), "separator".to_variant());
    } else {
        props.insert("label".to_owned(), entry.label().to_variant());
    }
    props
}

/// `GetLayout`'s reply, `(u(ia{sv}av))`. A tuple holding a `Variant` would
/// box it as `v`, so the reply is built from its parts.
fn layout_reply() -> glib::Variant {
    glib::Variant::tuple_from_iter([MENU_REVISION.to_variant(), layout()])
}

/// The whole menu as `(ia{sv}av)`: the root and its items.
fn layout() -> glib::Variant {
    let children: Vec<glib::Variant> = MENU
        .iter()
        .map(|e| (e.id(), properties(*e), Vec::<glib::Variant>::new()).to_variant())
        .collect();
    let mut root = HashMap::new();
    root.insert("children-display".to_owned(), "submenu".to_variant());
    (0i32, root, children).to_variant()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_round_trip_and_skip_the_root() {
        for entry in MENU {
            assert!(entry.id() > 0);
            assert_eq!(Entry::from_id(entry.id()), Some(entry));
        }
        assert_eq!(Entry::from_id(0), None);
        assert_eq!(Entry::from_id(MENU.len() as i32 + 1), None);
        assert_eq!(Entry::from_id(-1), None);
    }

    #[test]
    fn status_hides_the_icon_when_turned_off() {
        assert_eq!(status(false, true), "Passive");
        assert_eq!(status(true, false), "Active");
        assert_eq!(status(true, true), "NeedsAttention");
    }

    #[test]
    fn layout_has_the_dbusmenu_signature() {
        let layout = layout();
        assert_eq!(layout.type_().as_str(), "(ia{sv}av)");
        assert_eq!(layout.child_value(2).n_children(), MENU.len());
        assert_eq!(layout_reply().type_().as_str(), "(u(ia{sv}av))");
    }
}
