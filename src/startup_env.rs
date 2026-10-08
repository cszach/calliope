//! Environment variables set before GTK and WebKit start.
//!
//! WebKit and GStreamer read these once, so they are applied in `main`
//! before anything else runs. Defaults work around problems seen on real
//! hardware; `[webkit.env]` in the config overrides them.

use std::collections::BTreeMap;

/// Workarounds applied unless the environment or the config sets the key.
///
/// `WEBKIT_GST_DMABUF_SINK_DISABLED`: with WebKit's DMA-BUF video sink,
/// muse.ai's H.264 avatar video renders as a solid block (green on screen,
/// black in snapshots) on WebKitGTK 2.54.1 with an AMD iGPU (#10).
pub const DEFAULTS: &[(&str, &str)] = &[("WEBKIT_GST_DMABUF_SINK_DISABLED", "1")];

/// What to do with one variable: set it, or remove it (`None`).
pub type Change = (String, Option<String>);

/// Changes to make, given the config's `[webkit.env]` and a check for
/// variables already in the process environment. A config value that is
/// empty removes the variable, which is how a default is turned off.
pub fn changes(
    config_env: &BTreeMap<String, String>,
    is_set: impl Fn(&str) -> bool,
) -> Vec<Change> {
    let mut out: Vec<Change> = DEFAULTS
        .iter()
        .filter(|(key, _)| !is_set(key) && !config_env.contains_key(*key))
        .map(|(key, value)| ((*key).to_owned(), Some((*value).to_owned())))
        .collect();
    out.extend(config_env.iter().map(|(key, value)| {
        let value = (!value.is_empty()).then(|| value.clone());
        (key.clone(), value)
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "WEBKIT_GST_DMABUF_SINK_DISABLED";

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn defaults_apply_when_nothing_sets_them() {
        let out = changes(&env(&[]), |_| false);
        assert!(out.contains(&(KEY.to_owned(), Some("1".to_owned()))));
    }

    #[test]
    fn process_environment_wins_over_defaults() {
        let out = changes(&env(&[]), |k| k == KEY);
        assert!(out.iter().all(|(k, _)| k != KEY));
    }

    #[test]
    fn config_overrides_defaults() {
        let out = changes(&env(&[(KEY, "0")]), |_| false);
        assert_eq!(out, vec![(KEY.to_owned(), Some("0".to_owned()))]);
    }

    #[test]
    fn empty_config_value_removes_the_variable() {
        let out = changes(&env(&[(KEY, "")]), |_| true);
        assert_eq!(out, vec![(KEY.to_owned(), None)]);
    }

    #[test]
    fn other_config_entries_pass_through() {
        let out = changes(&env(&[("GSK_RENDERER", "ngl")]), |_| false);
        assert!(out.contains(&("GSK_RENDERER".to_owned(), Some("ngl".to_owned()))));
    }
}
