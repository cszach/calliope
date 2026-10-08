//! The GNOME Shell search provider: type a question in the Activities
//! overview and send it to Muse. Shell talks to it over D-Bus on the app's
//! own bus name, starting Muse as a D-Bus service if it is not running.

use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;

use crate::app::App;
use crate::consts::APP_ID;
use crate::{prompt, window};

const ID_PREFIX: &str = "ask:";
const INTERFACE: &str = "org.gnome.Shell.SearchProvider2";
/// Must match `ObjectPath` in the provider's `.ini`.
const OBJECT_PATH: &str = "/io/github/cszach/Muse/SearchProvider";
const XML: &str = include_str!("../data/dbus/org.gnome.Shell.SearchProvider2.xml");

/// Exports the provider on the app's D-Bus connection. Called from
/// `startup`, before the main loop dispatches any call to it.
pub fn register(app: &Rc<App>) {
    let Some(connection) = app.gtk.dbus_connection() else {
        log::warn!("search provider: no D-Bus connection");
        return;
    };
    let interface = match gio::DBusNodeInfo::for_xml(XML) {
        Ok(node) => node.lookup_interface(INTERFACE),
        Err(e) => {
            log::warn!("search provider: bad interface XML: {e}");
            return;
        }
    };
    let Some(interface) = interface else {
        log::warn!("search provider: {INTERFACE} missing from its XML");
        return;
    };
    let a = Rc::clone(app);
    let result = connection
        .register_object(OBJECT_PATH, &interface)
        .method_call(move |_, _, _, _, method, params, invocation| {
            // Each call restarts the idle timer of a D-Bus-started Muse.
            let _hold = a.gtk.hold();
            handle(&a, method, &params, invocation);
        })
        .build();
    if let Err(e) = result {
        log::warn!("search provider: cannot export {OBJECT_PATH}: {e}");
    }
}

fn handle(
    app: &Rc<App>,
    method: &str,
    params: &glib::Variant,
    invocation: gio::DBusMethodInvocation,
) {
    let (min_chars, prefix) = {
        let config = app.config();
        (
            config.search_provider.min_chars,
            config.search_provider.prefix.clone(),
        )
    };
    let results = |terms: &[String]| -> Vec<String> {
        query(terms, min_chars, &prefix)
            .map(|q| result_id(&q))
            .into_iter()
            .collect()
    };
    match method {
        "GetInitialResultSet" => match params.get::<(Vec<String>,)>() {
            Some((terms,)) => invocation.return_value(Some(&(results(&terms),).to_variant())),
            None => bad_arguments(invocation),
        },
        "GetSubsearchResultSet" => match params.get::<(Vec<String>, Vec<String>)>() {
            Some((_, terms)) => invocation.return_value(Some(&(results(&terms),).to_variant())),
            None => bad_arguments(invocation),
        },
        "GetResultMetas" => match params.get::<(Vec<String>,)>() {
            Some((ids,)) => {
                let metas: Vec<HashMap<String, glib::Variant>> =
                    ids.iter().filter_map(|id| meta(id)).collect();
                invocation.return_value(Some(&(metas,).to_variant()));
            }
            None => bad_arguments(invocation),
        },
        "ActivateResult" => match params.get::<(String, Vec<String>, u32)>() {
            Some((id, _, _)) => {
                invocation.return_value(None);
                if let Some(question) = query_of(&id) {
                    ask(app, question);
                }
            }
            None => bad_arguments(invocation),
        },
        "LaunchSearch" => match params.get::<(Vec<String>, u32)>() {
            Some((terms, _)) => {
                invocation.return_value(None);
                match query(&terms, 1, &prefix) {
                    Some(question) => ask(app, &question),
                    None => app.gtk.activate(),
                }
            }
            None => bad_arguments(invocation),
        },
        _ => invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.UnknownMethod",
            &format!("{INTERFACE} has no method {method}"),
        ),
    }
}

fn bad_arguments(invocation: gio::DBusMethodInvocation) {
    invocation.return_dbus_error(
        "org.freedesktop.DBus.Error.InvalidArgs",
        "unexpected argument types",
    );
}

fn meta(id: &str) -> Option<HashMap<String, glib::Variant>> {
    let question = query_of(id)?;
    Some(HashMap::from([
        ("id".to_owned(), id.to_variant()),
        ("name".to_owned(), "Ask Muse".to_variant()),
        ("description".to_owned(), question.to_variant()),
        ("gicon".to_owned(), APP_ID.to_variant()),
    ]))
}

/// Opens Muse in a new tab with the question typed in.
fn ask(app: &Rc<App>, question: &str) {
    let start = app.config().start_url.clone();
    let Some(view) = window::show_in_tab(app, &start) else {
        return;
    };
    let config = app.config();
    prompt::fill(
        &view,
        question,
        config.quick_ask.submit,
        &config.composer_selector,
    );
}

/// The question to offer for the overview's search terms, or `None` when
/// the terms are too short or lack the configured prefix.
pub fn query(terms: &[String], min_chars: usize, prefix: &str) -> Option<String> {
    let joined = terms.join(" ");
    let question = joined.trim().strip_prefix(prefix)?.trim();
    (!question.is_empty() && question.chars().count() >= min_chars).then(|| question.to_owned())
}

/// The result id Shell hands back on activation.
pub fn result_id(query: &str) -> String {
    format!("{ID_PREFIX}{query}")
}

/// The question inside a result id.
pub fn query_of(id: &str) -> Option<&str> {
    id.strip_prefix(ID_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn terms_join_into_one_question() {
        assert_eq!(
            query(&terms("how do I  boil eggs"), 3, ""),
            Some("how do I boil eggs".to_owned())
        );
    }

    #[test]
    fn short_queries_are_ignored() {
        assert_eq!(query(&terms("hi"), 3, ""), None);
        assert_eq!(query(&terms("hey"), 3, ""), Some("hey".to_owned()));
        assert_eq!(query(&[], 0, ""), None);
        // Characters, not bytes.
        assert_eq!(query(&terms("été"), 3, ""), Some("été".to_owned()));
    }

    #[test]
    fn prefix_is_required_and_stripped() {
        assert_eq!(query(&terms("how do I"), 3, "?"), None);
        assert_eq!(
            query(&terms("?how do I"), 3, "?"),
            Some("how do I".to_owned())
        );
        assert_eq!(
            query(&terms("? how do I"), 3, "?"),
            Some("how do I".to_owned())
        );
        // The length rule applies to the question, not the prefix.
        assert_eq!(query(&terms("?hi"), 3, "?"), None);
    }

    #[test]
    fn ids_round_trip() {
        let id = result_id("what's 2 + 2?");
        assert_eq!(id, "ask:what's 2 + 2?");
        assert_eq!(query_of(&id), Some("what's 2 + 2?"));
        assert_eq!(query_of("other:thing"), None);
    }
}
