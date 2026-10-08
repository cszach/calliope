//! Windows for popups a page opens with `window.open()`, such as sign-in
//! flows for apps Muse connects to. They share the opener's session, so a
//! sign-in completed here is seen by the main window.

use std::rc::Rc;

use adw::prelude::*;
use webkit::prelude::*;

use crate::app::App;
use crate::webview;

const DEFAULT_SIZE: (i32, i32) = (600, 720);

/// Builds the window for `view`, shown once WebKit says the popup is ready.
pub fn attach(app: &Rc<App>, opener: &webkit::WebView, view: &webkit::WebView) {
    let title = adw::WindowTitle::new("", "");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));

    let open_button = gtk::Button::from_icon_name("external-link-symbolic");
    open_button.set_tooltip_text(Some("Open in Browser"));
    open_button.connect_clicked(glib::clone!(
        #[weak]
        view,
        move |button| {
            if let Some(uri) = view.uri() {
                webview::open_external(button, &uri);
            }
        }
    ));
    header.pack_end(&open_button);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(view));

    let window = adw::Window::builder()
        .application(&app.gtk)
        .default_width(DEFAULT_SIZE.0)
        .default_height(DEFAULT_SIZE.1)
        .content(&toolbar)
        .build();
    if let Some(parent) = opener.root().and_downcast::<gtk::Window>() {
        window.set_transient_for(Some(&parent));
    }

    view.connect_title_notify(glib::clone!(
        #[weak]
        title,
        #[weak]
        window,
        move |view| {
            let text = view.title().map(|t| t.to_string()).unwrap_or_default();
            title.set_title(&text);
            window.set_title(Some(&text));
        }
    ));
    view.connect_uri_notify(glib::clone!(
        #[weak]
        title,
        move |view| {
            let host = view
                .uri()
                .and_then(|u| url::Url::parse(&u).ok())
                .and_then(|u| u.host_str().map(str::to_owned))
                .unwrap_or_default();
            title.set_subtitle(&host);
        }
    ));
    view.connect_ready_to_show(glib::clone!(
        #[weak]
        window,
        move |_| window.present()
    ));
    // The page called window.close().
    view.connect_close(glib::clone!(
        #[weak]
        window,
        move |_| window.destroy()
    ));
}
