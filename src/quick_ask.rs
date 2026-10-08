//! The quick-ask window: a small Muse window summoned from anywhere and
//! dismissed with Escape. It keeps its page between uses, so after the first
//! time it opens instantly, already logged in.

use std::rc::Rc;

use adw::prelude::*;
use webkit::prelude::*;

use crate::app::App;
use crate::consts::SCRIPT_WORLD;
use crate::{prompt, tab};

/// Shows the window, or hides it when it is already in front. `token` is
/// the activation token from the portal shortcut, which Wayland needs to
/// give the window focus.
pub fn toggle(app: &Rc<App>, token: Option<&str>) {
    match app.quick_ask_window() {
        Some(window) if window.is_visible() && window.is_active() => dismiss(app, &window),
        _ => {
            show(app, token);
        }
    }
}

/// Shows the window with `text` typed into the composer.
pub fn ask(app: &Rc<App>, text: &str) {
    let Some(view) = show(app, None) else {
        return;
    };
    let config = app.config();
    prompt::fill(
        &view,
        text,
        config.quick_ask.submit,
        &config.composer_selector,
    );
}

/// Destroys the window if it is hidden and nothing else keeps Muse running,
/// so a hidden quick-ask window never stops the app from quitting.
pub fn reap(app: &App) {
    if let Some(window) = app.quick_ask_window() {
        if !window.is_visible() && !app.keeps_quick_ask() {
            window.destroy();
        }
    }
}

fn show(app: &Rc<App>, token: Option<&str>) -> Option<webkit::WebView> {
    let window = app.quick_ask_window().unwrap_or_else(|| build(app));
    if let Some(token) = token {
        window.set_startup_id(token);
    }
    window.present();
    let view = view_of(&window);
    if let Some(view) = &view {
        view.grab_focus();
        let script = prompt::focus_script(&app.config().composer_selector);
        view.evaluate_javascript(
            &script,
            Some(SCRIPT_WORLD),
            None,
            gio::Cancellable::NONE,
            |_| {},
        );
    }
    view
}

/// Remembers the window's size for next launch.
pub fn remember_size(app: &App) {
    let Some(window) = app.quick_ask_window().filter(|w| w.is_visible()) else {
        return;
    };
    let (width, height) = window.default_size();
    {
        let mut config = app.config_mut();
        config.quick_ask.width = width;
        config.quick_ask.height = height;
    }
    app.save_config();
}

/// Hides the window, or destroys it when it is the last thing keeping Muse
/// running and background mode is off.
fn dismiss(app: &App, window: &adw::ApplicationWindow) {
    remember_size(app);
    if app.keeps_quick_ask() {
        window.set_visible(false);
    } else {
        window.destroy();
    }
}

fn build(app: &Rc<App>) -> adw::ApplicationWindow {
    let start = app.config().start_url.clone();
    let content = tab::new(app, None, Some(&start));

    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&content));

    let header = adw::HeaderBar::new();
    let move_to_window = gtk::Button::from_icon_name("window-new-symbolic");
    move_to_window.set_tooltip_text(Some("Continue in Main Window"));
    header.pack_start(&move_to_window);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&toasts));

    let window = {
        let config = app.config();
        adw::ApplicationWindow::builder()
            .application(&app.gtk)
            .title("Quick Ask")
            .default_width(config.quick_ask.width)
            .default_height(config.quick_ask.height)
            .content(&toolbar)
            .build()
    };
    window.add_css_class("quick-ask");

    // Bubble phase: the page sees Escape first, so it can close its own
    // menus and dialogs; only an Escape it leaves alone hides the window.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Bubble);
    let a = Rc::clone(app);
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        window,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, key, _, modifiers| {
            // Ignore lock keys such as Caps Lock; only real modifiers count.
            let modifiers = modifiers & gtk::accelerator_get_default_mod_mask();
            if key == gdk::Key::Escape && modifiers.is_empty() {
                dismiss(&a, &window);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    ));
    window.add_controller(keys);

    let a = Rc::clone(app);
    window.connect_close_request(move |window| {
        dismiss(&a, window);
        glib::Propagation::Stop
    });

    let a = Rc::clone(app);
    move_to_window.connect_clicked(glib::clone!(
        #[weak]
        window,
        move |_| {
            let Some(view) = view_of(&window) else {
                return;
            };
            let start = a.config().start_url.clone();
            let uri = view.uri().map_or_else(|| start.clone(), |u| u.to_string());
            a.open_uris(&[uri]);
            // The conversation lives on in the main window; the next quick
            // ask starts fresh.
            view.load_uri(&start);
            dismiss(&a, &window);
        }
    ));

    if app.debug() {
        crate::window::add_debug_actions(
            &window,
            glib::clone!(
                #[weak]
                window,
                #[upgrade_or]
                None,
                move || view_of(&window)
            ),
        );
    }

    app.set_quick_ask_window(&window);
    window
}

fn view_of(window: &adw::ApplicationWindow) -> Option<webkit::WebView> {
    let toolbar = window.content()?.downcast::<adw::ToolbarView>().ok()?;
    let toasts = toolbar.content()?.downcast::<adw::ToastOverlay>().ok()?;
    tab::view_of(&toasts.child()?)
}
