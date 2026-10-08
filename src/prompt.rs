//! Typing a prompt into the Muse composer. The composer appears only after
//! the page, and after a login the redirect back to Muse, has loaded, so the
//! fill is retried for a while; if it never appears the prompt goes to the
//! clipboard instead.

use std::time::Duration;

use adw::prelude::*;
use webkit::prelude::*;

use crate::consts::SCRIPT_WORLD;

const RETRY: Duration = Duration::from_millis(500);
const ATTEMPTS: u32 = 40;

/// The call into `fill-prompt.js`. Arguments are JSON literals, so any text
/// is safe to embed.
pub fn script(text: &str, submit: bool, selector: &str) -> String {
    format!(
        "window.__museFill ? window.__museFill({}, {submit}, {}) : false",
        json(text),
        json(selector)
    )
}

/// JavaScript that focuses the composer, if there is one.
pub fn focus_script(selector: &str) -> String {
    format!(
        "window.__museFocus ? window.__museFocus({}) : false",
        json(selector)
    )
}

fn json(s: &str) -> String {
    serde_json::Value::from(s).to_string()
}

/// Types `text` into the composer of `view`, retrying until it exists.
pub fn fill(view: &webkit::WebView, text: &str, submit: bool, selector: &str) {
    attempt(
        view.downgrade(),
        script(text, submit, selector),
        text.to_owned(),
        ATTEMPTS,
    );
}

fn attempt(view: glib::WeakRef<webkit::WebView>, script: String, text: String, left: u32) {
    let Some(strong) = view.upgrade() else {
        return;
    };
    strong.evaluate_javascript(
        &script.clone(),
        Some(SCRIPT_WORLD),
        None,
        gio::Cancellable::NONE,
        move |result| {
            // An error is normal while a page is mid-navigation.
            if result.is_ok_and(|value| value.to_boolean()) {
                return;
            }
            if left <= 1 {
                if let Some(view) = view.upgrade() {
                    give_up(&view, &text);
                }
                return;
            }
            glib::timeout_add_local_once(RETRY, move || attempt(view, script, text, left - 1));
        },
    );
}

fn give_up(view: &webkit::WebView, text: &str) {
    log::warn!("could not find the Muse composer; copied the prompt instead");
    view.clipboard().set_text(text);
    if let Some(overlay) = view
        .ancestor(adw::ToastOverlay::static_type())
        .and_downcast::<adw::ToastOverlay>()
    {
        overlay.add_toast(adw::Toast::new("Prompt copied. Paste it with Ctrl+V"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_embedded_as_a_string_literal() {
        let s = script("it's \"quoted\"\n</script>`${x}`", true, "");
        assert_eq!(
            s,
            r#"window.__museFill ? window.__museFill("it's \"quoted\"\n</script>`${x}`", true, "") : false"#
        );
    }

    #[test]
    fn selector_is_passed_through() {
        assert!(
            script("hi", false, "div[data-x='1']")
                .ends_with(r#""hi", false, "div[data-x='1']") : false"#)
        );
        assert_eq!(
            focus_script("#c"),
            r##"window.__museFocus ? window.__museFocus("#c") : false"##
        );
    }
}
