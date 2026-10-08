//! Process-wide state: the GTK application, the config and the web engine.

use std::cell::{Cell, OnceCell, Ref, RefCell, RefMut};
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;

use crate::config::Config;
use crate::consts::APP_ID;
use crate::engine::Engine;
use crate::window;

pub struct App {
    pub gtk: adw::Application,
    config: RefCell<Config>,
    config_path: PathBuf,
    engine: OnceCell<Engine>,
    debug_flag: Cell<bool>,
}

impl App {
    pub fn new(config: Config, config_path: PathBuf) -> Rc<Self> {
        let gtk = adw::Application::builder()
            .application_id(APP_ID)
            .flags(gio::ApplicationFlags::HANDLES_OPEN)
            .build();
        gtk.add_main_option(
            "debug",
            glib::Char::from(b'd'),
            glib::OptionFlags::NONE,
            glib::OptionArg::None,
            "Enable the web inspector and verbose logging",
            None,
        );
        let app = Rc::new(Self {
            gtk,
            config: RefCell::new(config),
            config_path,
            engine: OnceCell::new(),
            debug_flag: Cell::new(false),
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

    fn connect_signals(self: &Rc<Self>) {
        let app = Rc::clone(self);
        self.gtk.connect_handle_local_options(move |_, options| {
            if options.contains("debug") {
                app.debug_flag.set(true);
            }
            std::ops::ControlFlow::Continue(())
        });

        let app = Rc::clone(self);
        self.gtk.connect_startup(move |_| {
            // WebKit objects need GTK initialised, and the network session
            // must exist before the first view.
            let engine = Engine::new(&app);
            if app.engine.set(engine).is_err() {
                unreachable!("startup runs once");
            }
            if app.debug() {
                app.gtk
                    .set_accels_for_action("win.inspector", &["<Control><Shift>i"]);
            }
        });

        let app = Rc::clone(self);
        self.gtk.connect_activate(move |gtk| {
            match gtk
                .active_window()
                .or_else(|| gtk.windows().into_iter().next())
            {
                Some(win) => win.present(),
                None => {
                    let uri = app.config().start_url.clone();
                    window::open(&app, &uri);
                }
            }
        });

        let app = Rc::clone(self);
        self.gtk.connect_open(move |_, files, _hint| {
            for file in files {
                window::open(&app, &file.uri());
            }
        });

        let app = Rc::clone(self);
        self.gtk.connect_shutdown(move |_| app.save_config());
    }
}
