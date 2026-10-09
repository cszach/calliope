//! The Preferences dialog: the settings people change, saved to the config
//! file as they change.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;

use crate::app::App;
use crate::background;
use crate::policy;
use crate::tray;

/// How long a spin row must stay still before its value is applied, so
/// holding a button does not rewrite the config file on every step.
const SETTLE: Duration = Duration::from_millis(400);

/// Runs `apply` once `row` has stopped changing for [`SETTLE`].
fn on_settled(row: &adw::SpinRow, apply: impl Fn(f64) + 'static) {
    let pending: Rc<Cell<Option<glib::SourceId>>> = Rc::default();
    let apply = Rc::new(apply);
    row.connect_value_notify(move |row| {
        if let Some(source) = pending.take() {
            source.remove();
        }
        let (value, apply, done) = (row.value(), Rc::clone(&apply), Rc::clone(&pending));
        pending.set(Some(glib::timeout_add_local_once(SETTLE, move || {
            done.set(None);
            apply(value);
        })));
    });
}

/// Zoom levels offered, as percentages.
const ZOOM_RANGE: (f64, f64, f64) = (30.0, 300.0, 10.0);

pub fn dialog(app: &Rc<App>) -> adw::PreferencesDialog {
    let dialog = adw::PreferencesDialog::new();
    let (general, watch) = general_page(app);
    dialog.add(&general);
    dialog.add(&privacy_page());
    if let Some((action, handler)) = watch {
        let handler = Cell::new(Some(handler));
        dialog.connect_closed(move |_| {
            if let Some(handler) = handler.take() {
                action.disconnect(handler);
            }
        });
    }
    dialog
}

/// The General page, and the handler watching the background-mode action,
/// which the dialog disconnects when it closes.
fn general_page(
    app: &Rc<App>,
) -> (
    adw::PreferencesPage,
    Option<(gio::SimpleAction, glib::SignalHandlerId)>,
) {
    let config = app.config().clone();
    let page = adw::PreferencesPage::builder()
        .title("General")
        .icon_name("preferences-system-symbolic")
        .build();

    let background = adw::PreferencesGroup::new();
    let run_in_background = adw::SwitchRow::builder()
        .title("Run in _Background")
        .use_underline(true)
        .subtitle("Keep Calliope running after its window closes, for notifications and quick ask")
        .active(config.background_mode)
        .build();
    let a = Rc::clone(app);
    run_in_background.connect_active_notify(move |row| {
        a.gtk
            .change_action_state("background-mode", &row.is_active().to_variant());
    });
    // `calliope --background` can turn it on while the dialog is open.
    let watch = app
        .gtk
        .lookup_action("background-mode")
        .and_downcast::<gio::SimpleAction>()
        .map(|action| {
            let handler = action.connect_state_notify(glib::clone!(
                #[weak]
                run_in_background,
                move |action| {
                    let on = action.state().and_then(|s| s.get::<bool>()) == Some(true);
                    if run_in_background.is_active() != on {
                        run_in_background.set_active(on);
                    }
                }
            ));
            (action, handler)
        });
    background.add(&run_in_background);
    let start_at_login = adw::SwitchRow::builder()
        .title("Start at _Login")
        .use_underline(true)
        .subtitle("Start Calliope in the background when you log in")
        .active(config.start_at_login)
        .build();
    run_in_background
        .bind_property("active", &start_at_login, "sensitive")
        .sync_create()
        .build();
    let a = Rc::clone(app);
    start_at_login.connect_active_notify(move |row| {
        a.config_mut().start_at_login = row.is_active();
        a.save_config();
        background::sync(&a, row.root().and_downcast());
    });
    background.add(&start_at_login);
    let top_bar = adw::SwitchRow::builder()
        .title("Show in _Top Bar")
        .use_underline(true)
        .subtitle("An icon with a dot for new notifications; needs the AppIndicator extension")
        .active(config.top_bar_icon)
        .build();
    let a = Rc::clone(app);
    top_bar.connect_active_notify(move |row| tray::set_shown(&a, row.is_active()));
    background.add(&top_bar);
    page.add(&background);

    let quick_ask = adw::PreferencesGroup::builder()
        .title("Quick Ask")
        .description("A small window for questions, opened from any app")
        .build();
    let send = adw::SwitchRow::builder()
        .title("_Send Prompts Automatically")
        .use_underline(true)
        .subtitle(
            "Send questions from the overview and “calliope --ask” as soon as they are typed in",
        )
        .active(config.quick_ask.submit)
        .build();
    let a = Rc::clone(app);
    send.connect_active_notify(move |row| {
        a.config_mut().quick_ask.submit = row.is_active();
        a.save_config();
    });
    quick_ask.add(&send);
    let shortcut = adw::ButtonRow::builder()
        .title("Set Up _Keyboard Shortcut…")
        .use_underline(true)
        .action_name("app.quick-ask-shortcut")
        .build();
    quick_ask.add(&shortcut);
    page.add(&quick_ask);

    let search = adw::PreferencesGroup::builder()
        .title("Search")
        .description(
            "Type a question in the Activities overview to ask Muse. With a prefix such as \
             “?”, only questions that start with it are offered.",
        )
        .build();
    let min_chars = adw::SpinRow::with_range(1.0, 20.0, 1.0);
    min_chars.set_title("_Minimum Length");
    min_chars.set_use_underline(true);
    min_chars.set_subtitle("Characters typed before “Ask Muse” appears");
    min_chars.set_value(config.search_provider.min_chars as f64);
    let a = Rc::clone(app);
    on_settled(&min_chars, move |value| {
        a.config_mut().search_provider.min_chars = value as usize;
        a.save_config();
    });
    search.add(&min_chars);
    let prefix = adw::EntryRow::builder()
        .title("_Prefix")
        .use_underline(true)
        .text(config.search_provider.prefix.as_str())
        .show_apply_button(true)
        .build();
    let a = Rc::clone(app);
    prefix.connect_apply(move |row| {
        a.config_mut().search_provider.prefix = row.text().trim().to_owned();
        a.save_config();
    });
    search.add(&prefix);
    page.add(&search);

    let pages = adw::PreferencesGroup::builder().title("Pages").build();
    let start = adw::EntryRow::builder()
        .title("S_tart Page")
        .use_underline(true)
        .text(config.start_url.as_str())
        .show_apply_button(true)
        .input_purpose(gtk::InputPurpose::Url)
        .build();
    let a = Rc::clone(app);
    start.connect_apply(move |row| {
        let text = row.text().trim().to_owned();
        if policy::openable(&text) {
            row.remove_css_class("error");
            a.config_mut().start_url = text;
            a.save_config();
        } else {
            row.add_css_class("error");
        }
    });
    pages.add(&start);
    let (min, max, step) = ZOOM_RANGE;
    let zoom = adw::SpinRow::with_range(min, max, step);
    zoom.set_title("Default _Zoom");
    zoom.set_use_underline(true);
    zoom.set_subtitle("Percent; Ctrl+plus and Ctrl+minus change it too");
    zoom.set_value((config.zoom_level * 100.0).round());
    let a = Rc::clone(app);
    on_settled(&zoom, move |value| a.set_zoom(value / 100.0));
    pages.add(&zoom);
    page.add(&pages);

    (page, watch)
}

fn privacy_page() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Privacy")
        .icon_name("security-medium-symbolic")
        .build();
    let data = adw::PreferencesGroup::builder()
        .title("Website Data")
        .description("Cookies, storage and the permissions you granted to sites")
        .build();
    let clear = adw::ButtonRow::builder()
        .title("_Clear Site Data…")
        .use_underline(true)
        .action_name("app.clear-site-data")
        .build();
    clear.add_css_class("destructive-action");
    data.add(&clear);
    page.add(&data);
    page
}
