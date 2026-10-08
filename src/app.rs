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
use crate::{downloads, policy, shortcuts, tab, window};

const REPO_URL: &str = "https://github.com/cszach/muse-gnome";

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
    /// `--background`: the first activation loads Muse without showing it.
    start_hidden: Cell<bool>,
    /// Set by Quit, so closing the last window really closes it.
    quitting: Cell<bool>,
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
            start_hidden: Cell::new(false),
            quitting: Cell::new(false),
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

    /// Live main windows, most recently registered last.
    fn main_windows(&self) -> Vec<(adw::ApplicationWindow, adw::TabView)> {
        let mut windows = self.windows.borrow_mut();
        windows.retain(|w| w.window.upgrade().is_some() && w.tabs.upgrade().is_some());
        windows
            .iter()
            .filter_map(|w| Some((w.window.upgrade()?, w.tabs.upgrade()?)))
            .collect()
    }

    /// Every web view in every main window.
    fn views(&self) -> Vec<webkit::WebView> {
        self.main_windows()
            .iter()
            .flat_map(|(_, tabs)| {
                (0..tabs.n_pages())
                    .filter_map(|i| tab::view_of(&tabs.nth_page(i).child()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The main window to act on: the focused one, else the newest one on
    /// screen, else the one background mode hid.
    fn target_window(&self) -> Option<(adw::ApplicationWindow, adw::TabView)> {
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

    /// Turns background mode on or off, saves it, and updates the menu.
    fn set_background_mode(&self, on: bool) {
        if self.config().background_mode != on {
            self.config_mut().background_mode = on;
            self.save_config();
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
            // A window hidden by background mode would otherwise keep Muse
            // running out of sight.
            for (window, _) in self.main_windows() {
                if !window.is_visible() {
                    window.destroy();
                }
            }
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

    /// Paints the view's background in the current light or dark style, so
    /// a new tab does not flash white in dark mode.
    pub fn style_view(&self, view: &webkit::WebView) {
        let dark = adw::StyleManager::default().is_dark();
        let rgba = if dark {
            gdk::RGBA::new(0.133, 0.133, 0.149, 1.0)
        } else {
            gdk::RGBA::WHITE
        };
        view.set_background_color(&rgba);
    }

    fn connect_signals(self: &Rc<Self>) {
        let app = Rc::clone(self);
        self.gtk.connect_handle_local_options(move |gtk, options| {
            use std::ops::ControlFlow;
            if options.contains("debug") {
                app.debug_flag.set(true);
            }
            let background = options.contains("background");
            let new_window = options.contains("new-window");
            if !background && !new_window {
                return ControlFlow::Continue(());
            }
            // Find out whether Muse is already running; registering runs
            // `startup` when this process is the first.
            if let Err(e) = gtk.register(gio::Cancellable::NONE) {
                log::error!("cannot register the application: {e}");
                return ControlFlow::Break(glib::ExitCode::FAILURE);
            }
            if gtk.is_remote() {
                if new_window {
                    gtk.activate_action("new-window", None);
                } else {
                    gtk.change_action_state("background-mode", &true.to_variant());
                }
                return ControlFlow::Break(glib::ExitCode::SUCCESS);
            }
            if background && !new_window {
                app.set_background_mode(true);
                app.start_hidden.set(true);
            }
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

            let styled = Rc::clone(&app);
            adw::StyleManager::default().connect_dark_notify(move |_| {
                for view in styled.views() {
                    styled.style_view(&view);
                }
            });
        });

        let app = Rc::clone(self);
        self.gtk.connect_activate(move |_| {
            if app.start_hidden.replace(false) {
                // Load Muse in a window that is never shown until asked for,
                // so its notifications arrive from the start.
                window::create(&app, &[]);
                return;
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
            app.start_hidden.set(false);
            let uris: Vec<String> = files
                .iter()
                .map(|f| f.uri().to_string())
                .filter(|uri| {
                    let ok = policy::openable(uri);
                    if !ok {
                        log::warn!("not opening {uri}: only http and https links open in Muse");
                    }
                    ok
                })
                .collect();
            if uris.is_empty() {
                gtk.activate();
                return;
            }
            match app.target_window() {
                Some((window, tabs)) => {
                    for uri in &uris {
                        window::add_tab(&app, &tabs, uri);
                    }
                    window.present();
                }
                None => {
                    window::open(&app, &uris);
                }
            }
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
                for (window, _) in app.main_windows() {
                    window.close();
                }
                gtk.quit();
            })
            .build();

        let app = Rc::clone(self);
        // Activating toggles it (GIO's default for a boolean state); a
        // second `muse --background` sets it over D-Bus.
        let background = gio::ActionEntry::builder("background-mode")
            .state(self.config().background_mode.to_variant())
            .change_state(move |_: &GtkApp, _, value| {
                if let Some(on) = value.and_then(|v| v.get::<bool>()) {
                    app.set_background_mode(on);
                }
            })
            .build();

        let app = Rc::clone(self);
        let about = gio::ActionEntry::builder("about")
            .activate(move |_: &GtkApp, _, _| {
                let dialog = adw::AboutDialog::builder()
                    .application_name("Muse")
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
