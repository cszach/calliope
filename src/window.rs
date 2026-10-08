//! A main window: header bar, tab bar (shown with two or more tabs) and the
//! tabs themselves. Every window and tab shares the one web engine.

use std::rc::Rc;

use adw::prelude::*;
use webkit::prelude::*;

use crate::app::App;
use crate::{shortcuts, tab, webview, zoom};

/// Opens a window with one tab per URI, or one tab on the start page.
pub fn open(app: &Rc<App>, uris: &[String]) -> adw::ApplicationWindow {
    let window = create(app, uris);
    window.present();
    window
}

/// Like [`open`], but leaves the window hidden.
pub fn create(app: &Rc<App>, uris: &[String]) -> adw::ApplicationWindow {
    let (window, tabs) = build(app);
    if uris.is_empty() {
        let start = app.config().start_url.clone();
        add_tab(app, &tabs, &start);
    } else {
        for uri in uris {
            add_tab(app, &tabs, uri);
        }
    }
    window
}

/// Opens `uri` in a new tab of the current window, or of a new window, shows
/// the window, and returns the tab's view.
pub fn show_in_tab(app: &Rc<App>, uri: &str) -> Option<webkit::WebView> {
    let (window, tabs) = app.target_window().unwrap_or_else(|| build(app));
    let page = add_tab(app, &tabs, uri);
    window.present();
    tab::view_of(&page.child())
}

/// Adds a tab loading `uri` and selects it.
pub fn add_tab(app: &Rc<App>, tabs: &adw::TabView, uri: &str) -> adw::TabPage {
    let child = tab::new(app, None, Some(uri));
    let page = tabs.append(&child);
    page.set_title("Muse");
    tabs.set_selected_page(&page);
    page
}

fn build(app: &Rc<App>) -> (adw::ApplicationWindow, adw::TabView) {
    let tabs = adw::TabView::new();
    let tab_bar = adw::TabBar::new();
    tab_bar.set_view(Some(&tabs));
    tab_bar.set_autohide(true);

    let title = adw::WindowTitle::new("Calliope", "");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));

    let new_tab = gtk::Button::from_icon_name("tab-new-symbolic");
    new_tab.set_tooltip_text(Some("New Tab"));
    new_tab.set_action_name(Some("win.new-tab"));
    header.pack_start(&new_tab);

    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button.set_tooltip_text(Some("Main Menu"));
    menu_button.set_primary(true);
    menu_button.set_menu_model(Some(&main_menu()));
    header.pack_end(&menu_button);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.add_top_bar(&tab_bar);
    toolbar.set_content(Some(&tabs));

    let window = {
        let config = app.config();
        let window = adw::ApplicationWindow::builder()
            .application(&app.gtk)
            .title("Calliope")
            .default_width(config.window.width)
            .default_height(config.window.height)
            .content(&toolbar)
            .build();
        if config.window.maximized {
            window.maximize();
        }
        window
    };
    window
        .bind_property("title", &title, "title")
        .sync_create()
        .build();

    // Page or F11 fullscreen hides the bars; video and the VM view want the
    // whole screen.
    window.connect_fullscreened_notify(glib::clone!(
        #[weak]
        toolbar,
        move |window| toolbar.set_reveal_top_bars(!window.is_fullscreen())
    ));

    tabs.connect_selected_page_notify(glib::clone!(
        #[weak]
        window,
        move |tabs| {
            if let Some(view) = tabs.selected_page().and_then(|p| tab::view_of(&p.child())) {
                window.set_title(Some(&tab::title_of(&view)));
            }
        }
    ));
    // A window whose last tab closed or was dragged away has no purpose.
    tabs.connect_page_detached(glib::clone!(
        #[weak]
        window,
        move |tabs, _, _| {
            if tabs.n_pages() == 0 {
                window.close();
            }
        }
    ));
    // Dragging a tab out of the window.
    let a = Rc::clone(app);
    tabs.connect_create_window(move |_| {
        let (window, tabs) = build(&a);
        window.present();
        Some(tabs)
    });

    add_actions(app, &window, &tabs);

    let a = Rc::clone(app);
    window.connect_close_request(glib::clone!(
        #[weak]
        tabs,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |window| {
            {
                let mut config = a.config_mut();
                config.window.maximized = window.is_maximized();
                if !window.is_maximized() && !window.is_fullscreen() {
                    let (width, height) = window.default_size();
                    config.window.width = width;
                    config.window.height = height;
                }
            }
            a.save_config();
            if a.hides_on_close(&tabs) {
                window.set_visible(false);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }
    ));

    app.register_window(&window, &tabs);
    (window, tabs)
}

fn main_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let windows = gio::Menu::new();
    windows.append(Some("_New Window"), Some("app.new-window"));
    windows.append(Some("New _Tab"), Some("win.new-tab"));
    windows.append(Some("_Quick Ask"), Some("app.quick-ask"));
    menu.append_section(None, &windows);

    let zoom = gio::Menu::new();
    zoom.append(Some("Zoom _In"), Some("win.zoom-in"));
    zoom.append(Some("Zoom _Out"), Some("win.zoom-out"));
    zoom.append(Some("_Reset Zoom"), Some("win.zoom-reset"));
    menu.append_section(None, &zoom);

    let app = gio::Menu::new();
    app.append(Some("Run in _Background"), Some("app.background-mode"));
    app.append(Some("Quick Ask _Shortcut…"), Some("app.quick-ask-shortcut"));
    menu.append_section(None, &app);

    let page = gio::Menu::new();
    page.append(Some("Open in _Browser"), Some("win.open-in-browser"));
    page.append(Some("_Clear Site Data…"), Some("app.clear-site-data"));
    menu.append_section(None, &page);

    let about = gio::Menu::new();
    about.append(Some("_Keyboard Shortcuts"), Some("win.show-shortcuts"));
    about.append(Some("_About Calliope"), Some("app.about"));
    menu.append_section(None, &about);

    menu
}

fn current_view(tabs: &adw::TabView) -> Option<webkit::WebView> {
    tabs.selected_page().and_then(|p| tab::view_of(&p.child()))
}

fn add_actions(app: &Rc<App>, window: &adw::ApplicationWindow, tabs: &adw::TabView) {
    type Win = adw::ApplicationWindow;

    let on_view = |name: &str, f: fn(&webkit::WebView)| {
        gio::ActionEntry::builder(name)
            .activate(glib::clone!(
                #[weak]
                tabs,
                move |_: &Win, _, _| {
                    if let Some(view) = current_view(&tabs) {
                        f(&view);
                    }
                }
            ))
            .build()
    };

    let a = Rc::clone(app);
    let new_tab = gio::ActionEntry::builder("new-tab")
        .activate(glib::clone!(
            #[weak]
            tabs,
            move |_: &Win, _, _| {
                let start = a.config().start_url.clone();
                add_tab(&a, &tabs, &start);
            }
        ))
        .build();

    let close_tab = gio::ActionEntry::builder("close-tab")
        .activate(glib::clone!(
            #[weak]
            tabs,
            move |_: &Win, _, _| {
                if let Some(page) = tabs.selected_page() {
                    tabs.close_page(&page);
                }
            }
        ))
        .build();

    let zoom_action = |name: &str, step: fn(f64) -> f64| {
        let a = Rc::clone(app);
        gio::ActionEntry::builder(name)
            .activate(move |_: &Win, _, _| {
                let level = step(a.config().zoom_level);
                a.set_zoom(level);
            })
            .build()
    };

    let fullscreen = gio::ActionEntry::builder("fullscreen")
        .activate(|window: &Win, _, _| {
            if window.is_fullscreen() {
                window.unfullscreen();
            } else {
                window.fullscreen();
            }
        })
        .build();

    let open_in_browser = gio::ActionEntry::builder("open-in-browser")
        .activate(glib::clone!(
            #[weak]
            tabs,
            move |window: &Win, _, _| {
                if let Some(uri) = current_view(&tabs).and_then(|v| v.uri()) {
                    webview::open_external(window, &uri);
                }
            }
        ))
        .build();

    let show_shortcuts = gio::ActionEntry::builder("show-shortcuts")
        .activate(|window: &Win, _, _| shortcuts::dialog().present(Some(window)))
        .build();

    window.add_action_entries([
        new_tab,
        close_tab,
        on_view("reload", |v| v.reload()),
        on_view("reload-bypass-cache", |v| v.reload_bypass_cache()),
        on_view("back", |v| v.go_back()),
        on_view("forward", |v| v.go_forward()),
        zoom_action("zoom-in", zoom::zoom_in),
        zoom_action("zoom-out", zoom::zoom_out),
        zoom_action("zoom-reset", |_| 1.0),
        fullscreen,
        open_in_browser,
        show_shortcuts,
    ]);

    if app.debug() {
        add_debug_actions(
            window,
            glib::clone!(
                #[weak]
                tabs,
                #[upgrade_or]
                None,
                move || current_view(&tabs)
            ),
        );
    }
}

/// Debug-only actions for driving a window over D-Bus while testing
/// (`gdbus call ... org.gtk.Actions.Activate`). `current` gives the view
/// they act on.
pub fn add_debug_actions(
    window: &adw::ApplicationWindow,
    current: impl Fn() -> Option<webkit::WebView> + 'static,
) {
    type Win = adw::ApplicationWindow;
    let current = Rc::new(current);

    // Closes the window as its close button would.
    let close = gio::ActionEntry::builder("debug-close")
        .activate(|window: &Win, _, _| window.close())
        .build();
    let screenshot = gio::ActionEntry::builder("debug-screenshot")
        .activate(|window: &Win, _, _| save_screenshot(window))
        .build();
    // Runs JavaScript in the current view and logs the result.
    let view = Rc::clone(&current);
    let eval = gio::ActionEntry::builder("debug-eval")
        .parameter_type(Some(glib::VariantTy::STRING))
        .activate(move |_: &Win, _, param| {
            let (Some(view), Some(script)) = (view(), param.and_then(|p| p.get::<String>())) else {
                return;
            };
            view.evaluate_javascript(&script, None, None, gio::Cancellable::NONE, |result| {
                match result {
                    Ok(value) => log::info!("debug-eval: {}", value.to_str()),
                    Err(e) => log::warn!("debug-eval failed: {e}"),
                }
            });
        })
        .build();
    let inspector = gio::ActionEntry::builder("inspector")
        .activate(move |_: &Win, _, _| {
            if let Some(inspector) = current().and_then(|v| v.inspector()) {
                inspector.show();
            }
        })
        .build();
    window.add_action_entries([eval, inspector, screenshot, close]);
}

/// Debug only: writes the window as rendered to
/// `~/.cache/calliope/screenshot.png`, so the UI can be checked without a
/// screen capture (trigger with `gdbus call ... org.gtk.Actions.Activate`).
/// It shows the last frame GTK drew: a window that is hidden or covered is
/// not redrawn, so present it first for an up-to-date picture.
fn save_screenshot(window: &adw::ApplicationWindow) {
    let (width, height) = (window.width(), window.height());
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
    let Some(node) = snapshot.to_node() else {
        log::warn!("screenshot: nothing rendered");
        return;
    };
    let Some(renderer) = window.renderer() else {
        log::warn!("screenshot: window has no renderer");
        return;
    };
    let path = crate::paths::cache_dir().join("screenshot.png");
    let texture = renderer.render_texture(node, None);
    match texture.save_to_png(&path) {
        Ok(()) => log::info!("screenshot saved to {}", path.display()),
        Err(e) => log::warn!("screenshot failed: {e}"),
    }
}
