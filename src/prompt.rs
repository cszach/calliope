//! Typing a prompt into the Muse composer. The composer appears only after
//! the page, and after a login the redirect back to Muse, has loaded, so the
//! fill is retried for a while; if it never appears the prompt goes to the
//! clipboard instead.

use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use webkit::prelude::*;

use crate::consts::SCRIPT_WORLD;

const RETRY: Duration = Duration::from_millis(500);
const ATTEMPTS: u32 = 40;
/// Consecutive checks that must find the text in place before the fill
/// counts: the page may re-render and wipe an early fill.
const STABLE_CHECKS: u32 = 3;
/// Ten minutes of checks while the page is off screen.
const HIDDEN_CHECKS: u32 = 1200;

/// What `__museEnsure` reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    /// The page is off screen and may not lay out its composer.
    Hidden,
    Missing,
    Filled,
    Present,
}

impl Seen {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "hidden" => Some(Self::Hidden),
            "missing" => Some(Self::Missing),
            "filled" => Some(Self::Filled),
            "present" => Some(Self::Present),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Progress {
    attempts_left: u32,
    stable: u32,
    /// Checks left while the page is off screen, which do not spend
    /// attempts: a window behind others may only come forward when the user
    /// clicks GNOME's "Muse is ready".
    hidden_left: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    Retry(Progress),
    Done,
    GiveUp,
}

/// One step of the fill loop. `seen` is `None` when the script could not
/// run, which is normal while a page is mid-navigation.
fn advance(progress: Progress, seen: Option<Seen>) -> Next {
    if seen == Some(Seen::Hidden) {
        return match progress.hidden_left {
            0 | 1 => Next::GiveUp,
            n => Next::Retry(Progress {
                hidden_left: n - 1,
                stable: 0,
                ..progress
            }),
        };
    }
    let stable = if seen == Some(Seen::Present) {
        progress.stable + 1
    } else {
        0
    };
    if stable >= STABLE_CHECKS {
        return Next::Done;
    }
    if progress.attempts_left <= 1 {
        return Next::GiveUp;
    }
    Next::Retry(Progress {
        attempts_left: progress.attempts_left - 1,
        stable,
        ..progress
    })
}

/// The call into `fill-prompt.js`. Arguments are JSON literals, so any text
/// is safe to embed.
pub fn script(text: &str, selector: &str) -> String {
    format!(
        "window.__museEnsure ? window.__museEnsure({}, {}) : \"missing\"",
        json(text),
        json(selector)
    )
}

pub fn submit_script(selector: &str) -> String {
    format!(
        "window.__museSubmit ? window.__museSubmit({}) : false",
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

/// Types `text` into the composer of `view`, retrying until it exists and
/// the text stays put, then presses Enter if `submit`.
pub fn fill(view: &webkit::WebView, text: &str, submit: bool, selector: &str) {
    let job = Rc::new(Job {
        view: view.downgrade(),
        script: script(text, selector),
        submit: submit.then(|| submit_script(selector)),
        text: text.to_owned(),
    });
    step(
        job,
        Progress {
            attempts_left: ATTEMPTS,
            stable: 0,
            hidden_left: HIDDEN_CHECKS,
        },
    );
}

struct Job {
    view: glib::WeakRef<webkit::WebView>,
    script: String,
    submit: Option<String>,
    text: String,
}

fn step(job: Rc<Job>, progress: Progress) {
    let Some(view) = job.view.upgrade() else {
        return;
    };
    let script = job.script.clone();
    view.evaluate_javascript(
        &script,
        Some(SCRIPT_WORLD),
        None,
        gio::Cancellable::NONE,
        move |result| {
            let seen = result.ok().and_then(|v| Seen::parse(&v.to_str()));
            log::debug!("prompt fill: {seen:?}, {progress:?}");
            let Some(view) = job.view.upgrade() else {
                return;
            };
            match advance(progress, seen) {
                Next::Retry(next) => {
                    glib::timeout_add_local_once(RETRY, move || step(job, next));
                }
                Next::Done => {
                    if let Some(submit) = &job.submit {
                        view.evaluate_javascript(
                            submit,
                            Some(SCRIPT_WORLD),
                            None,
                            gio::Cancellable::NONE,
                            |_| {},
                        );
                    }
                }
                Next::GiveUp => give_up(&view, &job.text),
            }
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
        let s = script("it's \"quoted\"\n</script>`${x}`", "");
        assert_eq!(
            s,
            r#"window.__museEnsure ? window.__museEnsure("it's \"quoted\"\n</script>`${x}`", "") : "missing""#
        );
    }

    #[test]
    fn selector_is_passed_through() {
        assert!(
            script("hi", "div[data-x='1']").ends_with(r#"("hi", "div[data-x='1']") : "missing""#)
        );
        assert_eq!(
            submit_script("#c"),
            r##"window.__museSubmit ? window.__museSubmit("#c") : false"##
        );
        assert_eq!(
            focus_script("#c"),
            r##"window.__museFocus ? window.__museFocus("#c") : false"##
        );
    }

    fn at(attempts_left: u32, stable: u32) -> Progress {
        Progress {
            attempts_left,
            stable,
            hidden_left: 10,
        }
    }

    #[test]
    fn done_only_after_the_text_stays_put() {
        assert_eq!(
            advance(at(40, 0), Some(Seen::Filled)),
            Next::Retry(at(39, 0))
        );
        assert_eq!(
            advance(at(39, 0), Some(Seen::Present)),
            Next::Retry(at(38, 1))
        );
        assert_eq!(
            advance(at(38, 1), Some(Seen::Present)),
            Next::Retry(at(37, 2))
        );
        assert_eq!(advance(at(37, 2), Some(Seen::Present)), Next::Done);
    }

    #[test]
    fn a_wiped_fill_starts_the_count_again() {
        assert_eq!(
            advance(at(38, 2), Some(Seen::Filled)),
            Next::Retry(at(37, 0))
        );
        assert_eq!(
            advance(at(38, 2), Some(Seen::Missing)),
            Next::Retry(at(37, 0))
        );
        assert_eq!(advance(at(38, 2), None), Next::Retry(at(37, 0)));
    }

    #[test]
    fn gives_up_when_attempts_run_out() {
        assert_eq!(advance(at(1, 0), Some(Seen::Missing)), Next::GiveUp);
        // Stability on the last attempt still counts.
        assert_eq!(advance(at(1, 2), Some(Seen::Present)), Next::Done);
    }

    #[test]
    fn waiting_off_screen_spends_no_attempts() {
        let p = at(5, 2);
        let Next::Retry(next) = advance(p, Some(Seen::Hidden)) else {
            panic!("should keep waiting");
        };
        assert_eq!(
            (next.attempts_left, next.stable, next.hidden_left),
            (5, 0, 9)
        );
        let tired = Progress {
            hidden_left: 1,
            ..p
        };
        assert_eq!(advance(tired, Some(Seen::Hidden)), Next::GiveUp);
    }

    #[test]
    fn unknown_answers_are_ignored() {
        assert_eq!(Seen::parse("present"), Some(Seen::Present));
        assert_eq!(Seen::parse("true"), None);
    }
}
