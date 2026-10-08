//! Building a view on the shared engine and wiring the behaviour every view
//! has: where links go, popups, and permission prompts.

use std::rc::Rc;

use webkit::prelude::*;

use crate::app::App;
use crate::notifications;
use crate::permissions;
use crate::policy::{self, Disposition, Navigation};
use crate::popup;

/// A view for a window or tab. `related` is set only for a popup: it makes
/// WebKit put the popup in the opener's session and process, which
/// `window.opener` needs.
pub fn new_view(app: &Rc<App>, related: Option<&webkit::WebView>) -> webkit::WebView {
    let engine = app.engine();
    let builder = webkit::WebView::builder()
        .web_context(&engine.context)
        .user_content_manager(&engine.content)
        .settings(&engine.settings)
        .website_policies(&engine.policies)
        .zoom_level(app.config().zoom_level)
        .hexpand(true)
        .vexpand(true);
    // Both are construct-only and a popup inherits the session from its
    // related view; setting both is a critical warning in WebKit.
    let view = match related {
        Some(opener) => builder.related_view(opener).build(),
        None => builder.network_session(&engine.session).build(),
    };
    wire(app, &view, related.is_some());
    view
}

fn wire(app: &Rc<App>, view: &webkit::WebView, in_popup: bool) {
    let a = Rc::clone(app);
    view.connect_decide_policy(move |view, decision, kind| {
        decide_policy(&a, view, decision, kind, in_popup)
    });

    let a = Rc::clone(app);
    view.connect_create(move |opener, _action| {
        let popup_view = new_view(&a, Some(opener));
        popup::attach(&a, opener, &popup_view);
        Some(popup_view.upcast())
    });

    let a = Rc::clone(app);
    view.connect_show_notification(move |view, n| notifications::show(&a, view, n));

    let a = Rc::clone(app);
    view.connect_permission_request(move |view, request| permissions::handle(&a, view, request));
}

fn decide_policy(
    app: &App,
    view: &webkit::WebView,
    decision: &webkit::PolicyDecision,
    kind: webkit::PolicyDecisionType,
    in_popup: bool,
) -> bool {
    match kind {
        webkit::PolicyDecisionType::NavigationAction
        | webkit::PolicyDecisionType::NewWindowAction => {
            let Some(action) = decision
                .downcast_ref::<webkit::NavigationPolicyDecision>()
                .and_then(|d| d.navigation_action())
            else {
                return false;
            };
            let Some(uri) = action.request().and_then(|r| r.uri()) else {
                return false;
            };
            let nav = Navigation {
                new_window: kind == webkit::PolicyDecisionType::NewWindowAction,
                link_clicked: action.navigation_type() == webkit::NavigationType::LinkClicked,
                user_gesture: action.is_user_gesture(),
                in_popup,
            };
            match policy::classify(&uri, nav, &app.config().allowed_hosts) {
                Disposition::InApp => decision.use_(),
                Disposition::External => {
                    decision.ignore();
                    open_external(view, &uri);
                }
                Disposition::Block => {
                    log::info!("blocked navigation without a user gesture: {uri}");
                    decision.ignore();
                }
            }
            true
        }
        webkit::PolicyDecisionType::Response => {
            let Some(response) = decision.downcast_ref::<webkit::ResponsePolicyDecision>() else {
                return false;
            };
            if response.is_mime_type_supported() {
                decision.use_();
            } else {
                decision.download();
            }
            true
        }
        _ => false,
    }
}

/// Hands a URI to the default browser or handler.
pub fn open_external(widget: &impl IsA<gtk::Widget>, uri: &str) {
    let parent = widget.root().and_downcast::<gtk::Window>();
    let uri_owned = uri.to_owned();
    gtk::UriLauncher::new(uri).launch(parent.as_ref(), gio::Cancellable::NONE, move |res| {
        if let Err(e) = res {
            log::warn!("cannot open {uri_owned}: {e}");
        }
    });
}
