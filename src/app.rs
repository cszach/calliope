//! Process-wide state: the GTK application, the config, the web engine, and
//! the app-level actions.

use std::cell::{Cell, OnceCell, Ref, RefCell, RefMut};
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use webkit::prelude::*;

use crate::config::Config;
use crate::consts::APP_ID;
use crate::engine::Engine;
use crate::notifications;
use crate::{
    background, downloads, hotkey, policy, preferences, quick_ask, search_provider, shortcuts, tab,
    window,
};

const REPO_URL: &str = "https://github.com/cszach/calliope";
/// How long a D-Bus-started Calliope with no window waits for the next call.
const SERVICE_IDLE_MS: u32 = 30_000;

/// What the first activation of this process should do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum Launch {
    #[default]
    Normal,
    /// `--background`: load Muse without showing it.
    Hidden,
    /// `--quick-ask`.
    QuickAsk,
    /// `--ask TEXT`.
    Ask(String),
}

/// A main window and its tabs, held weakly: GTK owns both.
struct WindowRef {
    window: glib::WeakRef<adw::ApplicationWindow>,
    tabs: glib::WeakRef<adw::TabView>,
}

pub struct App {
    pub gtk: adw::Application,
    config: RefCell<Config>,
    config_path: PathBuf,
    /// False when an existing config file could not be parsed.
    config_writable: bool,
    engine: OnceCell<Engine>,
    debug_flag: Cell<bool>,
    windows: RefCell<Vec<WindowRef>>,
    web_notifications: notifications::Live,
    /// Keeps the process alive with no window while background mode is on.
    background_hold: RefCell<Option<gio::ApplicationHoldGuard>>,
    /// Consumed by the first activation.
    launch: RefCell<Launch>,
    /// Set by Quit, so closing the last window really closes it.
    quitting: Cell<bool>,
    quick_ask: glib::WeakRef<adw::ApplicationWindow>,
}

impl App {
    pub fn new(config: Config, config_path: PathBuf, config_writable: bool) -> Rc<Self> {
        let gtk = adw::Application::builder()
            .application_id(APP_ID)
            .flags(gio::ApplicationFlags::HANDLES_OPEN | gio::ApplicationFlags::CAN_OVERRIDE_APP_ID)
            .build();
        gtk.add_main_option(
            "debug",
            glib::Char::from(b'd'),
            glib::OptionFlags::NONE,
            glib::OptionArg::None,
            "Enable the web inspector and verbose logging",
            None,
        );
        gtk.add_main_option(
            "background",
            glib::Char::from(b'b'),
            glib::OptionFlags::NONE,
            glib::OptionArg::None,
            "Start in the background, with no window, and turn on background mode",
            None,
        );
        gtk.add_main_option(
            "quick-ask",
            glib::Char::from(b'k'),
            glib::OptionFlags::NONE,
            glib::OptionArg::None,
            "Show or hide the quick-ask window",
            None,
        );
        gtk.add_main_option(
            "ask",
            glib::Char::from(b'a'),
            glib::OptionFlags::NONE,
            glib::OptionArg::String,
            "Open the quick-ask window with TEXT typed in",
            Some("TEXT"),
        );
        // Applied in `main` before GTK starts; declared so GApplication
        // accepts it and lists it in --help.
        gtk.add_main_option(
            "safe-graphics",
            glib::Char::from(0u8),
            glib::OptionFlags::NONE,
            glib::OptionArg::None,
            "Work around rendering glitches and crashes (slower)",
            None,
        );
        gtk.add_main_option(
            "new-window",
            glib::Char::from(b'n'),
            glib::OptionFlags::NONE,
            glib::OptionArg::None,
            "Open a new window",
            None,
        );
        let app = Rc::new(Self {
            gtk,
            config: RefCell::new(config),
            config_path,
            config_writable,
            engine: OnceCell::new(),
            debug_flag: Cell::new(false),
            windows: RefCell::new(Vec::new()),
            web_notifications: notifications::Live::default(),
            background_hold: RefCell::new(None),
            launch: RefCell::new(Launch::Normal),
            quitting: Cell::new(false),
            quick_ask: glib::WeakRef::new(),
        });
        app.connect_signals();
        app
    }

    pub fn run(&self) -> glib::ExitCode {
        self.gtk.run()
    }

    pub fn config(&self) -> Ref<'_, Config> {
        self.config.borrow()
    }

    pub fn config_mut(&self) -> RefMut<'_, Config> {
        self.config.borrow_mut()
    }

    pub fn save_config(&self) {
        if !self.config_writable {
            log::warn!(
                "not saving settings: fix {} and restart",
                self.config_path.display()
            );
            return;
        }
        if let Err(e) = self.config.borrow().save(&self.config_path) {
            log::warn!("cannot save config {}: {e}", self.config_path.display());
        }
    }

    pub fn debug(&self) -> bool {
        self.debug_flag.get() || self.config.borrow().debug
    }

    /// The shared web engine. Exists from `startup` on.
    pub fn engine(&self) -> &Engine {
        self.engine
            .get()
            .expect("engine is created in startup, before any window")
    }

    pub fn web_notifications(&self) -> &notifications::Live {
        &self.web_notifications
    }

    pub fn register_window(&self, window: &adw::ApplicationWindow, tabs: &adw::TabView) {
        self.windows.borrow_mut().push(WindowRef {
            window: window.downgrade(),
            tabs: tabs.downgrade(),
        });
    }

    /// Live main windows, most recently registered last. A window being
    /// destroyed can outlive its removal from the application for a moment;
    /// it no longer counts.
    fn main_windows(&self) -> Vec<(adw::ApplicationWindow, adw::TabView)> {
        let attached = self.gtk.windows();
        let mut windows = self.windows.borrow_mut();
        windows.retain(|w| w.window.upgrade().is_some() && w.tabs.upgrade().is_some());
        windows
            .iter()
            .filter_map(|w| Some((w.window.upgrade()?, w.tabs.upgrade()?)))
            .filter(|(w, _)| attached.iter().any(|a| a == w.upcast_ref::<gtk::Window>()))
            .collect()
    }

    /// Every web view in every main window, and the quick-ask view.
    fn views(&self) -> Vec<webkit::WebView> {
        let mut views: Vec<_> = self
            .main_windows()
            .iter()
            .flat_map(|(_, tabs)| {
                (0..tabs.n_pages())
                    .filter_map(|i| tab::view_of(&tabs.nth_page(i).child()))
                    .collect::<Vec<_>>()
            })
            .collect();
        views.extend(self.quick_ask_window().and_then(|w| quick_ask::view_of(&w)));
        views
    }

    /// The main window to act on: the focused one, else the newest one on
    /// screen, else the one background mode hid.
    pub fn target_window(&self) -> Option<(adw::ApplicationWindow, adw::TabView)> {
        let windows = self.main_windows();
        let active = self.gtk.active_window();
        windows
            .iter()
            .find(|(w, _)| active.as_ref() == Some(w.upcast_ref()))
            .or_else(|| windows.iter().rev().find(|(w, _)| w.is_visible()))
            .or(windows.last())
            .cloned()
    }

    /// Whether closing the window that holds `tabs` should hide it instead:
    /// in background mode the last window stays alive, hidden, so Muse can
    /// keep sending notifications. An empty window is closed regardless.
    pub fn hides_on_close(&self, tabs: &adw::TabView) -> bool {
        !self.quitting.get()
            && self.config().background_mode
            && tabs.n_pages() > 0
            && self.main_windows().iter().all(|(_, t)| t == tabs)
    }

    pub fn quick_ask_window(&self) -> Option<adw::ApplicationWindow> {
        self.quick_ask.upgrade()
    }

    pub fn set_quick_ask_window(&self, window: &adw::ApplicationWindow) {
        self.quick_ask.set(Some(window));
    }

    /// Whether a dismissed quick-ask window should stay alive, hidden, for
    /// next time: only while something else keeps Calliope running.
    pub fn keeps_quick_ask(&self) -> bool {
        self.config().background_mode || !self.main_windows().is_empty()
    }

    /// Opens links in tabs of the current window, or a new one.
    pub fn open_uris(self: &Rc<Self>, uris: &[String]) {
        match self.target_window() {
            Some((window, tabs)) => {
                for uri in uris {
                    window::add_tab(self, &tabs, uri);
                }
                window.present();
            }
            None => {
                window::open(self, uris);
            }
        }
    }

    /// Turns background mode on or off, saves it, and updates the menu.
    fn set_background_mode(self: &Rc<Self>, on: bool) {
        if self.config().background_mode != on {
            self.config_mut().background_mode = on;
            self.save_config();
            background::sync(self, self.gtk.active_window());
        }
        self.apply_background_mode();
    }

    fn apply_background_mode(&self) {
        let on = self.config().background_mode;
        let mut hold = self.background_hold.borrow_mut();
        if on && hold.is_none() {
            *hold = Some(self.gtk.hold());
        } else if !on {
            hold.take();
        }
        drop(hold);
        if !on {
            // A window hidden by background mode would otherwise keep Calliope
            // running out of sight.
            for (window, _) in self.main_windows() {
                if !window.is_visible() {
                    window.destroy();
                }
            }
            quick_ask::reap(self);
        }
        if let Some(action) = self
            .gtk
            .lookup_action("background-mode")
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_state(&on.to_variant());
        }
    }

    /// Applies a zoom level to every view and remembers it.
    pub fn set_zoom(&self, level: f64) {
        self.config_mut().zoom_level = level;
        self.save_config();
        for view in self.views() {
            view.set_zoom_level(level);
        }
    }

    fn connect_signals(self: &Rc<Self>) {
        let app = Rc::clone(self);
        self.gtk.connect_handle_local_options(move |gtk, options| {
            use std::ops::ControlFlow;
            if options.contains("debug") {
                app.debug_flag.set(true);
            }
            let safe_graphics = options.contains("safe-graphics");
            let background = options.contains("background");
            let new_window = options.contains("new-window");
            let quick_ask = options.contains("quick-ask");
            let ask = options
                .lookup::<String>("ask")
                .ok()
                .flatten()
                .filter(|t| !t.trim().is_empty());
            if !safe_graphics && !background && !new_window && !quick_ask && ask.is_none() {
                return ControlFlow::Continue(());
            }
            // Find out whether Calliope is already running; registering runs
            // `startup` when this process is the first.
            if let Err(e) = gtk.register(gio::Cancellable::NONE) {
                log::error!("cannot register the application: {e}");
                return ControlFlow::Break(glib::ExitCode::FAILURE);
            }
            if gtk.is_remote() {
                if safe_graphics {
                    log::warn!(
                        "--safe-graphics only applies when Calliope starts; quit the running \
                         Calliope (Ctrl+Q) and run this again"
                    );
                }
                if let Some(text) = &ask {
                    gtk.activate_action("ask", Some(&text.to_variant()));
                } else if quick_ask {
                    gtk.activate_action("quick-ask", None);
                } else if new_window {
                    gtk.activate_action("new-window", None);
                } else if background {
                    gtk.change_action_state("background-mode", &true.to_variant());
                } else {
                    gtk.activate();
                }
                return ControlFlow::Break(glib::ExitCode::SUCCESS);
            }
            if background {
                app.set_background_mode(true);
            }
            *app.launch.borrow_mut() = if let Some(text) = ask {
                Launch::Ask(text)
            } else if quick_ask {
                Launch::QuickAsk
            } else if background && !new_window {
                Launch::Hidden
            } else {
                Launch::Normal
            };
            ControlFlow::Continue(())
        });

        let app = Rc::clone(self);
        self.gtk.connect_startup(move |_| {
            // WebKit objects need GTK initialised, and the network session
            // must exist before the first view.
            let engine = Engine::new(&app);
            if app.engine.set(engine).is_err() {
                unreachable!("startup runs once");
            }
            downloads::attach(&app);
            app.add_actions();
            app.apply_background_mode();
            shortcuts::install(&app.gtk, app.debug());
            background::init(&app);
            hotkey::init(&app);
            search_provider::register(&app);
            // Started over D-Bus, possibly by GNOME Shell just to answer a
            // search: stay up between keystrokes instead of exiting as soon
            // as a call returns. Once a window opens, exit normally again.
            if app.gtk.flags().contains(gio::ApplicationFlags::IS_SERVICE) {
                app.gtk.set_inactivity_timeout(SERVICE_IDLE_MS);
                app.gtk
                    .connect_window_added(|gtk, _| gtk.set_inactivity_timeout(0));
            }

            // A hidden quick-ask window must not keep Calliope alive once the
            // last main window has gone.
            let reaper = Rc::clone(&app);
            app.gtk
                .connect_window_removed(move |_, _| quick_ask::reap(&reaper));
        });

        let app = Rc::clone(self);
        self.gtk.connect_activate(move |_| {
            match app.launch.take() {
                Launch::Normal => {}
                Launch::Hidden => {
                    // Load Muse in a window that is never shown until asked
                    // for, so its notifications arrive from the start.
                    window::create(&app, &[]);
                    return;
                }
                Launch::QuickAsk => {
                    quick_ask::toggle(&app, None);
                    return;
                }
                Launch::Ask(text) => {
                    quick_ask::ask(&app, &text);
                    return;
                }
            }
            match app.target_window() {
                Some((window, _)) => window.present(),
                None => {
                    window::open(&app, &[]);
                }
            }
        });

        let app = Rc::clone(self);
        self.gtk.connect_open(move |gtk, files, _hint| {
            // Links on the command line mean the user wants to see them.
            app.launch.take();
            let uris: Vec<String> = files
                .iter()
                .map(|f| f.uri().to_string())
                .filter(|uri| {
                    let ok = policy::openable(uri);
                    if !ok {
                        log::warn!("not opening {uri}: only http and https links open in Calliope");
                    }
                    ok
                })
                .collect();
            if uris.is_empty() {
                gtk.activate();
                return;
            }
            app.open_uris(&uris);
        });

        let app = Rc::clone(self);
        self.gtk.connect_shutdown(move |_| app.save_config());
    }

    fn add_actions(self: &Rc<Self>) {
        type GtkApp = adw::Application;

        let app = Rc::clone(self);
        let new_window = gio::ActionEntry::builder("new-window")
            .activate(move |_: &GtkApp, _, _| {
                window::open(&app, &[]);
            })
            .build();

        let app = Rc::clone(self);
        let quit = gio::ActionEntry::builder("quit")
            .activate(move |gtk: &GtkApp, _, _| {
                // Closing runs each window's close handler, which saves its
                // size; popups close with their opener.
                app.quitting.set(true);
                quick_ask::remember_size(&app);
                for (window, _) in app.main_windows() {
                    window.close();
                }
                gtk.quit();
            })
            .build();

        let app = Rc::clone(self);
        // Activating toggles it (GIO's default for a boolean state); a
        // second `calliope --background` sets it over D-Bus.
        let background = gio::ActionEntry::builder("background-mode")
            .state(self.config().background_mode.to_variant())
            .change_state(move |_: &GtkApp, _, value| {
                if let Some(on) = value.and_then(|v| v.get::<bool>()) {
                    app.set_background_mode(on);
                }
            })
            .build();

        let app = Rc::clone(self);
        let quick_ask = gio::ActionEntry::builder("quick-ask")
            .activate(move |_: &GtkApp, _, _| quick_ask::toggle(&app, None))
            .build();

        let app = Rc::clone(self);
        let ask = gio::ActionEntry::builder("ask")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(move |_: &GtkApp, _, param| {
                if let Some(text) = param.and_then(|p| p.get::<String>()) {
                    quick_ask::ask(&app, &text);
                }
            })
            .build();

        let app = Rc::clone(self);
        let set_up_shortcut = gio::ActionEntry::builder("quick-ask-shortcut")
            .activate(move |_: &GtkApp, _, _| {
                let parent = app.target_window().map(|(w, _)| w.upcast());
                hotkey::set_up(&app, parent);
            })
            .build();

        let app = Rc::clone(self);
        let preferences = gio::ActionEntry::builder("preferences")
            .activate(move |_: &GtkApp, _, _| {
                preferences::dialog(&app).present(app.target_window().map(|(w, _)| w).as_ref());
            })
            .build();

        let app = Rc::clone(self);
        let about = gio::ActionEntry::builder("about")
            .activate(move |_: &GtkApp, _, _| {
                let dialog = adw::AboutDialog::builder()
                    .application_name("Calliope")
                    .application_icon(APP_ID)
                    .developer_name("Zach")
                    .version(env!("CARGO_PKG_VERSION"))
                    .license_type(gtk::License::MitX11)
                    .website(REPO_URL)
                    .issue_url(format!("{REPO_URL}/issues"))
                    .comments(
                        "An unofficial desktop client for Muse, Meta’s AI agent. \
                         Not affiliated with or endorsed by Meta.",
                    )
                    .build();
                dialog.present(app.target_window().map(|(w, _)| w).as_ref());
            })
            .build();

        let app = Rc::clone(self);
        let clear = gio::ActionEntry::builder("clear-site-data")
            .activate(move |_: &GtkApp, _, _| app.confirm_clear_site_data())
            .build();

        let app = Rc::clone(self);
        let web_notification = gio::ActionEntry::builder("web-notification")
            .parameter_type(Some(glib::VariantTy::UINT64))
            .activate(move |_: &GtkApp, _, param| {
                if let Some(id) = param.and_then(|p| p.get::<u64>()) {
                    notifications::clicked(&app, id);
                }
            })
            .build();

        let app = Rc::clone(self);
        let open_download = gio::ActionEntry::builder("open-download")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(move |_: &GtkApp, _, param| {
                if let Some(path) = param.and_then(|p| p.get::<String>()) {
                    let file = gio::File::for_path(&path);
                    let parent = app.target_window().map(|(w, _)| w);
                    gtk::FileLauncher::new(Some(&file)).launch(
                        parent.as_ref(),
                        gio::Cancellable::NONE,
                        move |r| {
                            if let Err(e) = r {
                                log::warn!("cannot open {path}: {e}");
                            }
                        },
                    );
                }
            })
            .build();

        let app = Rc::clone(self);
        let show_download = gio::ActionEntry::builder("show-download")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(move |_: &GtkApp, _, param| {
                if let Some(path) = param.and_then(|p| p.get::<String>()) {
                    let file = gio::File::for_path(&path);
                    let parent = app.target_window().map(|(w, _)| w);
                    gtk::FileLauncher::new(Some(&file)).open_containing_folder(
                        parent.as_ref(),
                        gio::Cancellable::NONE,
                        move |r| {
                            if let Err(e) = r {
                                log::warn!("cannot show {path}: {e}");
                            }
                        },
                    );
                }
            })
            .build();

        self.gtk.add_action_entries([
            new_window,
            quit,
            background,
            quick_ask,
            ask,
            set_up_shortcut,
            preferences,
            about,
            clear,
            web_notification,
            open_download,
            show_download,
        ]);
    }

    fn confirm_clear_site_data(self: &Rc<Self>) {
        let dialog = adw::AlertDialog::new(
            Some("Clear Site Data?"),
            Some("This logs you out of Muse and forgets the permissions you granted."),
        );
        dialog.add_response("cancel", "_Cancel");
        dialog.add_response("clear", "C_lear");
        dialog.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let app = Rc::clone(self);
        let parent = self.target_window().map(|(w, _)| w);
        dialog.choose(parent.as_ref(), gio::Cancellable::NONE, move |response| {
            if response != "clear" {
                return;
            }
            app.config_mut().permissions.clear();
            app.save_config();
            let Some(data) = app.engine().session.website_data_manager() else {
                return;
            };
            // The callback must be Send; it runs on this thread, so the guard
            // lets it carry the app back.
            let guard = glib::thread_guard::ThreadGuard::new(Rc::clone(&app));
            data.clear(
                webkit::WebsiteDataTypes::ALL,
                glib::TimeSpan::from_seconds(0),
                gio::Cancellable::NONE,
                move |result| {
                    if let Err(e) = result {
                        log::warn!("clearing site data failed: {e}");
                    }
                    // WebKit hands notification grants to each web process
                    // when it launches, so restart them to drop the old ones.
                    for view in guard.get_ref().views() {
                        view.terminate_web_process();
                        view.reload();
                    }
                },
            );
        });
    }
}
