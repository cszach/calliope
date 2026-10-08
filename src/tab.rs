//! One tab: a web view, a load progress bar, the page shown instead of the
//! view when the web process crashes or muse.ai cannot be reached, and a
//! banner for pages that need WebRTC, which this WebKitGTK lacks.

use std::rc::Rc;

use adw::prelude::*;
use webkit::prelude::*;

use crate::app::App;
use crate::webview;

const WEB: &str = "web";
const ERROR: &str = "error";

/// Builds a tab around `view`, or a new view, and starts loading `uri`.
pub fn new(app: &Rc<App>, view: Option<webkit::WebView>, uri: Option<&str>) -> gtk::Widget {
    let view = view.unwrap_or_else(|| webview::new_view(app, None));

    let status = adw::StatusPage::new();
    let reload = gtk::Button::with_mnemonic("_Reload");
    reload.add_css_class("pill");
    reload.add_css_class("suggested-action");
    reload.set_halign(gtk::Align::Center);
    status.set_child(Some(&reload));

    let stack = gtk::Stack::new();
    stack.add_named(&view, Some(WEB));
    stack.add_named(&status, Some(ERROR));
    stack.set_visible_child_name(WEB);

    let progress = gtk::ProgressBar::new();
    progress.add_css_class("osd");
    progress.set_valign(gtk::Align::Start);
    progress.set_can_target(false);
    progress.set_visible(false);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&stack));
    overlay.add_overlay(&progress);

    let banner = webview::webrtc_banner(&view);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&banner);
    root.append(&overlay);

    reload.connect_clicked(glib::clone!(
        #[weak]
        view,
        #[weak]
        stack,
        move |_| {
            stack.set_visible_child_name(WEB);
            view.reload();
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
        #[weak]
        root,
        move |view| {
            if !view.is_loading() {
                progress.set_visible(false);
            }
            if let Some(page) = page_of(&root) {
                page.set_loading(view.is_loading());
            }
        }
    ));
    view.connect_title_notify(glib::clone!(
        #[weak]
        root,
        move |view| {
            let title = title_of(view);
            if let Some(page) = page_of(&root) {
                page.set_title(&title);
                if page.is_selected() {
                    if let Some(window) = root.root().and_downcast::<gtk::Window>() {
                        window.set_title(Some(&title));
                    }
                }
            }
        }
    ));
    view.connect_favicon_notify(glib::clone!(
        #[weak]
        root,
        move |view| {
            if let Some(page) = page_of(&root) {
                page.set_icon(view.favicon().as_ref());
            }
        }
    ));

    view.connect_load_changed(glib::clone!(
        #[weak]
        stack,
        move |view, event| {
            if event == webkit::LoadEvent::Committed {
                stack.set_visible_child_name(WEB);
            }
            if matches!(
                event,
                webkit::LoadEvent::Committed | webkit::LoadEvent::Finished
            ) {
                log::info!("{event:?} {}", view.uri().unwrap_or_default());
            }
        }
    ));
    view.connect_load_failed(glib::clone!(
        #[weak]
        stack,
        #[weak]
        status,
        #[upgrade_or]
        false,
        move |_, _, uri, error| {
            // Cancelled loads and our own policy decisions (a download, a
            // link sent to the browser) are not failures the user should see.
            if error.matches(webkit::NetworkError::Cancelled)
                || error.kind::<webkit::PolicyError>().is_some()
            {
                return false;
            }
            log::warn!("load failed {uri}: {error}");
            status.set_icon_name(Some("network-offline-symbolic"));
            status.set_title("Can’t Reach Muse");
            status.set_description(Some(error.message()));
            stack.set_visible_child_name(ERROR);
            true
        }
    ));
    view.connect_web_process_terminated(glib::clone!(
        #[weak]
        stack,
        #[weak]
        status,
        move |_, reason| {
            log::warn!("web process terminated: {reason:?}");
            if reason == webkit::WebProcessTerminationReason::TerminatedByApi {
                return;
            }
            status.set_icon_name(Some("computer-fail-symbolic"));
            status.set_title("Muse Stopped Responding");
            status.set_description(Some(match reason {
                webkit::WebProcessTerminationReason::ExceededMemoryLimit => {
                    "The page used too much memory and was stopped."
                }
                _ => "The page crashed. Reloading usually brings it back.",
            }));
            stack.set_visible_child_name(ERROR);
        }
    ));

    if let Some(uri) = uri {
        view.load_uri(uri);
    }
    root.upcast()
}

/// The web view inside a tab built by [`new`].
pub fn view_of(tab: &gtk::Widget) -> Option<webkit::WebView> {
    tab.downcast_ref::<gtk::Box>()?
        .last_child()?
        .downcast::<gtk::Overlay>()
        .ok()?
        .child()?
        .downcast::<gtk::Stack>()
        .ok()?
        .child_by_name(WEB)?
        .downcast::<webkit::WebView>()
        .ok()
}

pub fn title_of(view: &webkit::WebView) -> String {
    view.title()
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .unwrap_or_else(|| "Muse".to_owned())
}

/// The tab page holding `root`, wherever it currently lives: tabs move
/// between windows when dragged.
fn page_of(root: &gtk::Box) -> Option<adw::TabPage> {
    let tab_view = root
        .ancestor(adw::TabView::static_type())?
        .downcast::<adw::TabView>()
        .ok()?;
    Some(tab_view.page(root))
}
