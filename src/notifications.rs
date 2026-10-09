//! Web notifications from the page, shown as GNOME notifications. Clicking
//! one brings its window and tab forward and tells the page.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;

use crate::app::App;
use crate::consts::APP_ID;
use crate::tray;

/// Notifications the page has shown and not yet closed, by WebKit id.
#[derive(Default)]
pub struct Live(RefCell<HashMap<u64, (glib::WeakRef<webkit::WebView>, webkit::Notification)>>);

fn gio_id(id: u64) -> String {
    format!("web:{id}")
}

/// Handles `show-notification`; returns true so WebKit does not also show
/// its own, which could not raise our window.
pub fn show(app: &Rc<App>, view: &webkit::WebView, notification: &webkit::Notification) -> bool {
    let id = notification.id();
    let title = notification
        .title()
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .unwrap_or_else(|| "Muse".to_owned());
    let note = gio::Notification::new(&title);
    if let Some(body) = notification.body().filter(|b| !b.is_empty()) {
        note.set_body(Some(&body));
    }
    note.set_icon(&gio::ThemedIcon::new(APP_ID));
    note.set_default_action_and_target_value("app.web-notification", Some(&id.to_variant()));
    app.gtk.send_notification(Some(&gio_id(id)), &note);
    tray::notify(app);

    let live = app.web_notifications();
    live.0
        .borrow_mut()
        .retain(|_, (view, _)| view.upgrade().is_some());
    live.0
        .borrow_mut()
        .insert(id, (view.downgrade(), notification.clone()));

    // The page closed it, or replaced it with another of the same tag.
    let a = Rc::clone(app);
    notification.connect_closed(move |n| {
        a.gtk.withdraw_notification(&gio_id(n.id()));
        a.web_notifications().0.borrow_mut().remove(&n.id());
    });
    true
}

/// The `app.web-notification` action: raise the view's window, select its
/// tab, and forward the click to the page.
pub fn clicked(app: &App, id: u64) {
    let entry = app.web_notifications().0.borrow_mut().remove(&id);
    let Some((view, notification)) = entry else {
        app.gtk.activate();
        return;
    };
    match view.upgrade() {
        Some(view) => {
            select_tab_of(&view);
            if let Some(window) = view.root().and_downcast::<gtk::Window>() {
                window.present();
            }
        }
        None => app.gtk.activate(),
    }
    notification.clicked();
}

fn select_tab_of(view: &webkit::WebView) {
    let Some(tabs) = view
        .ancestor(adw::TabView::static_type())
        .and_downcast::<adw::TabView>()
    else {
        return;
    };
    for i in 0..tabs.n_pages() {
        let page = tabs.nth_page(i);
        if view.is_ancestor(&page.child()) {
            tabs.set_selected_page(&page);
            return;
        }
    }
}
