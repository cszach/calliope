//! The main window: a header bar and one web view.

use std::rc::Rc;

use adw::prelude::*;
use webkit::prelude::*;

use crate::app::App;
use crate::webview;

pub fn open(app: &Rc<App>, uri: &str) -> adw::ApplicationWindow {
    let view = webview::new_view(app, None);

    let title = adw::WindowTitle::new("Muse", "");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));

    let progress = gtk::ProgressBar::new();
    progress.add_css_class("osd");
    progress.set_valign(gtk::Align::Start);
    progress.set_can_target(false);
    progress.set_visible(false);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&view));
    overlay.add_overlay(&progress);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&overlay));

    let window = {
        let config = app.config();
        let window = adw::ApplicationWindow::builder()
            .application(&app.gtk)
            .title("Muse")
            .default_width(config.window.width)
            .default_height(config.window.height)
            .content(&toolbar)
            .build();
        if config.window.maximized {
            window.maximize();
        }
        window
    };

    view.connect_title_notify(glib::clone!(
        #[weak]
        title,
        #[weak]
        window,
        move |view| {
            let text = view
                .title()
                .filter(|t| !t.is_empty())
                .map(|t| t.to_string())
                .unwrap_or_else(|| "Muse".to_owned());
            title.set_title(&text);
            window.set_title(Some(&text));
        }
    ));
    view.connect_estimated_load_progress_notify(glib::clone!(
        #[weak]
        progress,
        move |view| {
            let fraction = view.estimated_load_progress();
            progress.set_fraction(fraction);
            progress.set_visible(view.is_loading() && fraction < 1.0);
        }
    ));
    view.connect_is_loading_notify(glib::clone!(
        #[weak]
        progress,
        move |view| {
            if !view.is_loading() {
                progress.set_visible(false);
            }
        }
    ));

    view.connect_load_changed(|view, event| {
        if matches!(
            event,
            webkit::LoadEvent::Committed | webkit::LoadEvent::Finished
        ) {
            log::info!("{event:?} {}", view.uri().unwrap_or_default());
        }
    });
    view.connect_load_failed(|_, _, uri, error| {
        log::warn!("load failed {uri}: {error}");
        false
    });
    view.connect_web_process_terminated(|_, reason| {
        log::warn!("web process terminated: {reason:?}");
    });

    let inspector = gio::ActionEntry::builder("inspector")
        .activate(glib::clone!(
            #[weak]
            view,
            move |_: &adw::ApplicationWindow, _, _| {
                if let Some(inspector) = view.inspector() {
                    inspector.show();
                }
            }
        ))
        .build();
    if app.debug() {
        window.add_action_entries([inspector]);
    }

    let a = Rc::clone(app);
    window.connect_close_request(move |window| {
        {
            let mut config = a.config_mut();
            config.window.maximized = window.is_maximized();
            if !window.is_maximized() {
                let (width, height) = window.default_size();
                config.window.width = width;
                config.window.height = height;
            }
        }
        a.save_config();
        glib::Propagation::Proceed
    });

    view.load_uri(uri);
    window.present();
    window
}
