//! Answers WebKit permission requests: asks once for microphone, camera and
//! notifications and remembers the answer per origin; decides the rest
//! silently.

use std::rc::Rc;

use adw::prelude::*;
use webkit::prelude::*;

use crate::app::App;
use crate::config::Capability;
use crate::policy::{self, host_allowed};

/// Returns true when the request was handled (now or after a prompt).
pub fn handle(app: &Rc<App>, view: &webkit::WebView, request: &webkit::PermissionRequest) -> bool {
    let uri = view.uri().map(|u| u.to_string()).unwrap_or_default();
    let Some(origin) = policy::origin_of(&uri) else {
        request.deny();
        return true;
    };

    if let Some(media) = request.downcast_ref::<webkit::UserMediaPermissionRequest>() {
        let mut caps = Vec::new();
        if media.is_for_audio_device() {
            caps.push(Capability::Microphone);
        }
        if media.is_for_video_device() {
            caps.push(Capability::Camera);
        }
        if caps.is_empty() {
            // Screen sharing: ask every time, never remember.
            prompt(
                app,
                view,
                request,
                &origin,
                &[],
                "Share Your Screen?",
                "share your screen",
            );
        } else {
            decide_or_prompt(app, view, request, &origin, &caps);
        }
        return true;
    }

    if request.is::<webkit::NotificationPermissionRequest>() {
        decide_or_prompt(app, view, request, &origin, &[Capability::Notifications]);
        return true;
    }

    // Needed by the site itself (copy buttons, device lists for voice,
    // protected media, storage access in Meta's login iframes): allow for the
    // hosts the app is for, refuse elsewhere.
    if request.is::<webkit::ClipboardPermissionRequest>()
        || request.is::<webkit::DeviceInfoPermissionRequest>()
        || request.is::<webkit::MediaKeySystemPermissionRequest>()
        || request.is::<webkit::WebsiteDataAccessPermissionRequest>()
    {
        let host = url::Url::parse(&uri)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        if host_allowed(&host, &app.config().allowed_hosts) {
            request.allow();
        } else {
            request.deny();
        }
        return true;
    }

    // Location, pointer lock, XR: nothing in Muse needs them.
    request.deny();
    true
}

fn decide_or_prompt(
    app: &Rc<App>,
    view: &webkit::WebView,
    request: &webkit::PermissionRequest,
    origin: &str,
    caps: &[Capability],
) {
    let remembered: Vec<Option<bool>> = {
        let config = app.config();
        caps.iter().map(|c| config.permission(origin, *c)).collect()
    };
    if remembered.contains(&Some(false)) {
        request.deny();
    } else if remembered.iter().all(|r| *r == Some(true)) {
        request.allow();
    } else {
        let (heading, what) = describe(caps);
        prompt(app, view, request, origin, caps, heading, what);
    }
}

fn describe(caps: &[Capability]) -> (&'static str, &'static str) {
    let mic = caps.contains(&Capability::Microphone);
    let cam = caps.contains(&Capability::Camera);
    match (mic, cam) {
        (true, true) => (
            "Allow Microphone and Camera?",
            "use your microphone and camera",
        ),
        (true, false) => ("Allow Microphone?", "use your microphone"),
        (false, true) => ("Allow Camera?", "use your camera"),
        (false, false) => ("Allow Notifications?", "show notifications"),
    }
}

fn prompt(
    app: &Rc<App>,
    view: &webkit::WebView,
    request: &webkit::PermissionRequest,
    origin: &str,
    caps: &[Capability],
    heading: &str,
    what: &str,
) {
    let site = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .unwrap_or(origin);
    let dialog = adw::AlertDialog::new(Some(heading), Some(&format!("{site} wants to {what}.")));
    dialog.add_response("deny", "_Deny");
    dialog.add_response("allow", "_Allow");
    dialog.set_response_appearance("allow", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("allow"));
    dialog.set_close_response("deny");

    // WebKit waits for an answer as long as we hold the request.
    let request = request.clone();
    let app = Rc::clone(app);
    let origin = origin.to_owned();
    let caps = caps.to_vec();
    dialog.choose(Some(view), gio::Cancellable::NONE, move |response| {
        let allowed = response == "allow";
        if !caps.is_empty() {
            {
                let mut config = app.config_mut();
                for cap in &caps {
                    config.remember_permission(&origin, *cap, allowed);
                }
            }
            app.save_config();
        }
        if allowed {
            request.allow();
        } else {
            request.deny();
        }
    });
}
