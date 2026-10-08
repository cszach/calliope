//! Where a navigation goes: stay in the app, or hand off to the default
//! browser. Pure logic, so it is unit-tested apart from WebKit.
//!
//! The rule protects login flows. Meta's login, and the sign-in flows of apps
//! Muse connects to, chain redirects and popups across many hosts, so only a
//! deliberate link click to a host outside the allow-list leaves the app.
//! Redirects, iframes, form posts and script-opened popups stay in.

use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Load it in the app (for a new-window action: open an in-app popup).
    InApp,
    /// Open it in the default browser or handler and cancel it here.
    External,
}

/// What WebKit tells us about a navigation.
#[derive(Debug, Clone, Copy, Default)]
pub struct Navigation {
    /// A `NewWindowAction`: `target="_blank"` or `window.open()`.
    pub new_window: bool,
    /// The user clicked a link (as opposed to a redirect, script or form).
    pub link_clicked: bool,
    pub user_gesture: bool,
    /// The view is a popup opened by the page, typically a sign-in flow.
    pub in_popup: bool,
}

/// True when `host` is one of `allowed` or a subdomain of one.
pub fn host_allowed(host: &str, allowed: &[String]) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    allowed.iter().any(|a| {
        let a = a.trim_end_matches('.').to_ascii_lowercase();
        host == a || host.ends_with(&format!(".{a}"))
    })
}

pub fn classify(uri: &str, nav: Navigation, allowed: &[String]) -> Disposition {
    let Ok(url) = Url::parse(uri) else {
        return Disposition::InApp;
    };
    match url.scheme() {
        "http" | "https" => {}
        // Page-internal documents never leave the view.
        "about" | "blob" | "data" => return Disposition::InApp,
        // mailto:, tel:, app links and the like belong to other apps.
        _ => return Disposition::External,
    }
    if url.host_str().is_some_and(|h| host_allowed(h, allowed)) {
        return Disposition::InApp;
    }
    if nav.new_window {
        // `target="_blank"` link: the user wants to read it, in the browser.
        // `window.open()`: likely a sign-in popup that must share our cookies.
        return if nav.link_clicked {
            Disposition::External
        } else {
            Disposition::InApp
        };
    }
    if nav.in_popup {
        // Inside a sign-in popup, every step of the flow stays together.
        return Disposition::InApp;
    }
    if nav.link_clicked && nav.user_gesture {
        Disposition::External
    } else {
        Disposition::InApp
    }
}

/// `scheme://host[:port]` of an http(s) URI, the key for remembered
/// permissions.
pub fn origin_of(uri: &str) -> Option<String> {
    let url = Url::parse(uri).ok()?;
    match url.scheme() {
        "http" | "https" => Some(url.origin().ascii_serialization()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed() -> Vec<String> {
        crate::consts::DEFAULT_ALLOWED_HOSTS
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
    }

    const CLICK: Navigation = Navigation {
        new_window: false,
        link_clicked: true,
        user_gesture: true,
        in_popup: false,
    };
    const REDIRECT: Navigation = Navigation {
        new_window: false,
        link_clicked: false,
        user_gesture: false,
        in_popup: false,
    };

    #[test]
    fn hosts_match_exactly_or_as_subdomain() {
        let a = allowed();
        assert!(host_allowed("muse.ai", &a));
        assert!(host_allowed("auth.muse.ai", &a));
        assert!(host_allowed("WWW.Facebook.com.", &a));
        assert!(!host_allowed("notmuse.ai", &a));
        assert!(!host_allowed("muse.ai.evil.example", &a));
    }

    #[test]
    fn allowed_hosts_stay_in_app_even_on_click() {
        assert_eq!(
            classify("https://auth.muse.ai/x", CLICK, &allowed()),
            Disposition::InApp
        );
    }

    #[test]
    fn clicked_link_to_other_host_goes_external() {
        assert_eq!(
            classify("https://en.wikipedia.org/", CLICK, &allowed()),
            Disposition::External
        );
    }

    #[test]
    fn redirect_to_other_host_stays_in_app() {
        // e.g. an identity provider in the middle of Meta's login chain.
        assert_eq!(
            classify("https://accounts.example/login", REDIRECT, &allowed()),
            Disposition::InApp
        );
    }

    #[test]
    fn blank_target_link_goes_external() {
        let nav = Navigation {
            new_window: true,
            ..CLICK
        };
        assert_eq!(
            classify("https://news.example/a", nav, &allowed()),
            Disposition::External
        );
    }

    #[test]
    fn script_popup_stays_in_app() {
        let nav = Navigation {
            new_window: true,
            link_clicked: false,
            user_gesture: true,
            in_popup: false,
        };
        assert_eq!(
            classify("https://accounts.google.com/o/oauth2", nav, &allowed()),
            Disposition::InApp
        );
    }

    #[test]
    fn clicks_inside_a_popup_stay_in_it() {
        let nav = Navigation {
            in_popup: true,
            ..CLICK
        };
        assert_eq!(
            classify("https://accounts.google.com/choose", nav, &allowed()),
            Disposition::InApp
        );
    }

    #[test]
    fn other_schemes_go_to_their_apps() {
        assert_eq!(
            classify("mailto:a@b.example", REDIRECT, &allowed()),
            Disposition::External
        );
        assert_eq!(
            classify("about:blank", CLICK, &allowed()),
            Disposition::InApp
        );
        assert_eq!(
            classify("blob:https://muse.ai/1234", CLICK, &allowed()),
            Disposition::InApp
        );
    }

    #[test]
    fn origin_is_scheme_host_port() {
        assert_eq!(
            origin_of("https://muse.ai/chat/1?x=y").as_deref(),
            Some("https://muse.ai")
        );
        assert_eq!(
            origin_of("http://localhost:8080/").as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(origin_of("about:blank"), None);
    }
}
