//! The one WebKit stack every view shares: network session (cookies,
//! storage), web context, user scripts and settings.

use std::rc::Rc;

use crate::app::App;
use crate::config::Capability;
use crate::consts::SCRIPT_WORLD;
use crate::paths;

pub struct Engine {
    pub session: webkit::NetworkSession,
    pub context: webkit::WebContext,
    pub content: webkit::UserContentManager,
    pub settings: webkit::Settings,
    pub policies: webkit::WebsitePolicies,
}

const USER_SCRIPTS: &[&str] = &[
    include_str!("../data/js/detect-webrtc.js"),
    include_str!("../data/js/fill-prompt.js"),
];

const SCRIPT_ALLOW_LIST: &[&str] = &["https://muse.ai/*", "https://*.muse.ai/*"];

impl Engine {
    pub fn new(app: &Rc<App>) -> Self {
        let data_dir = paths::data_dir();
        let cache_dir = paths::cache_dir();
        for dir in [&data_dir, &cache_dir] {
            if let Err(e) = std::fs::create_dir_all(dir) {
                log::warn!("cannot create {}: {e}", dir.display());
            }
        }

        let session = webkit::NetworkSession::new(data_dir.to_str(), cache_dir.to_str());
        // WebKit keeps cookies in memory unless told otherwise; this is what
        // keeps the login across restarts. It must happen before any load.
        if let Some(cookies) = session.cookie_manager() {
            match paths::cookies_file().to_str() {
                Some(path) => {
                    cookies.set_persistent_storage(path, webkit::CookiePersistentStorage::Sqlite)
                }
                None => log::warn!("cookie path is not UTF-8; cookies will not persist"),
            }
            // A single-site client: third-party cookie blocking only risks
            // breaking Meta's cross-domain login.
            cookies.set_accept_policy(webkit::CookieAcceptPolicy::Always);
        }
        session.set_itp_enabled(false);

        let context = webkit::WebContext::new();
        let notif_app = Rc::clone(app);
        context.connect_initialize_notification_permissions(move |ctx| {
            let config = notif_app.config();
            let mut allowed = Vec::new();
            let mut denied = Vec::new();
            for origin in config.permissions.keys() {
                match config.permission(origin, Capability::Notifications) {
                    Some(true) => allowed.push(webkit::SecurityOrigin::for_uri(origin)),
                    Some(false) => denied.push(webkit::SecurityOrigin::for_uri(origin)),
                    None => {}
                }
            }
            let allowed: Vec<_> = allowed.iter().collect();
            let denied: Vec<_> = denied.iter().collect();
            ctx.initialize_notification_permissions(&allowed, &denied);
        });

        let content = webkit::UserContentManager::new();
        for source in USER_SCRIPTS {
            content.add_script(&webkit::UserScript::for_world(
                source,
                webkit::UserContentInjectedFrames::TopFrame,
                webkit::UserScriptInjectionTime::Start,
                SCRIPT_WORLD,
                SCRIPT_ALLOW_LIST,
                &[],
            ));
        }

        let config = app.config();
        let debug = app.debug();
        let settings = webkit::Settings::new();
        settings.set_enable_media_stream(true);
        settings.set_enable_mediasource(true);
        settings.set_enable_encrypted_media(true);
        // A no-op on WebKitGTK 2.54 (no WebRTC); takes effect from 2.56.
        settings.set_enable_webrtc(true);
        settings.set_hardware_acceleration_policy(webkit::HardwareAccelerationPolicy::Always);
        // Sign-in popups are often opened after an async step, outside the
        // click that started them.
        settings.set_javascript_can_open_windows_automatically(true);
        // Voice replies must be able to play without a fresh click.
        settings.set_media_playback_requires_user_gesture(false);
        settings.set_enable_back_forward_navigation_gestures(true);
        settings.set_enable_smooth_scrolling(true);
        settings.set_enable_developer_extras(debug);
        settings.set_enable_write_console_messages_to_stdout(debug);
        if !config.user_agent.trim().is_empty() {
            settings.set_user_agent(Some(config.user_agent.trim()));
        }

        let policies = webkit::WebsitePolicies::builder()
            .autoplay(webkit::AutoplayPolicy::Allow)
            .build();

        Self {
            session,
            context,
            content,
            settings,
            policies,
        }
    }
}
