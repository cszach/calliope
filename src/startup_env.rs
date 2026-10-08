//! Environment variables set before GTK and WebKit start.
//!
//! WebKit and GStreamer read these once, so they are applied in `main`
//! before anything else runs. Defaults work around problems seen on real
//! hardware; `[webkit.env]` in the config overrides them.

use std::collections::BTreeMap;

/// Workarounds applied unless the config sets the key. `None` removes the
/// variable from the environment.
///
/// The PRIME offload variables put the whole app on a discrete NVIDIA GPU
/// when it is launched from a terminal that sets them. muse.ai's avatar
/// video then renders as a solid green circle, and the app draws more power
/// for nothing; the desktop composites on the integrated GPU anyway (#10).
/// Unlike the other defaults these are removed even when the process
/// environment sets them, since inheriting them is the problem.
pub const DEFAULTS: &[(&str, Option<&str>)] = &[
    ("__NV_PRIME_RENDER_OFFLOAD", None),
    ("__NV_PRIME_RENDER_OFFLOAD_PROVIDER", None),
    ("__GLX_VENDOR_LIBRARY_NAME", None),
    ("__VK_LAYER_NV_optimus", None),
    ("DRI_PRIME", None),
];

/// `--safe-graphics`: the cheap workarounds for rendering glitches and
/// crashes, from the README's troubleshooting ladder. GTK's GL renderer
/// instead of Vulkan, no explicit sync on NVIDIA, and WebKit drawing
/// through shared memory instead of DMA-BUF.
pub const SAFE_GRAPHICS: &[(&str, &str)] = &[
    ("GSK_RENDERER", "ngl"),
    ("__NV_DISABLE_EXPLICIT_SYNC", "1"),
    ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
];

/// What to do with one variable: set it, or remove it (`None`).
pub type Change = (String, Option<String>);

/// Changes to make, given the config's `[webkit.env]`, `--safe-graphics`,
/// and a check for variables already in the process environment. A config value that is
/// empty removes the variable, which is how a default is turned off.
pub fn changes(
    config_env: &BTreeMap<String, String>,
    safe_graphics: bool,
    is_set: impl Fn(&str) -> bool,
) -> Vec<Change> {
    let mut out: Vec<Change> = DEFAULTS
        .iter()
        .filter(|(key, value)| !config_env.contains_key(*key) && (value.is_none() || !is_set(key)))
        .map(|(key, value)| ((*key).to_owned(), value.map(str::to_owned)))
        .collect();
    if safe_graphics {
        out.extend(
            SAFE_GRAPHICS
                .iter()
                .filter(|(key, _)| !config_env.contains_key(*key))
                .map(|(key, value)| ((*key).to_owned(), Some((*value).to_owned()))),
        );
    }
    out.extend(config_env.iter().map(|(key, value)| {
        let value = (!value.is_empty()).then(|| value.clone());
        (key.clone(), value)
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFFLOAD: &str = "__NV_PRIME_RENDER_OFFLOAD";

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn safe_graphics_sets_its_workarounds() {
        let out = changes(&env(&[]), true, |_| true);
        for (key, value) in [
            ("GSK_RENDERER", "ngl"),
            ("__NV_DISABLE_EXPLICIT_SYNC", "1"),
            ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
        ] {
            assert!(
                out.contains(&(key.to_owned(), Some(value.to_owned()))),
                "{key}"
            );
        }
        let normal = changes(&env(&[]), false, |_| false);
        assert!(!normal.iter().any(|(k, _)| k == "GSK_RENDERER"));
    }

    #[test]
    fn config_overrides_safe_graphics() {
        let out = changes(&env(&[("GSK_RENDERER", "vulkan")]), true, |_| false);
        assert!(out.contains(&("GSK_RENDERER".to_owned(), Some("vulkan".to_owned()))));
        assert!(!out.contains(&("GSK_RENDERER".to_owned(), Some("ngl".to_owned()))));
    }

    #[test]
    fn offload_variables_are_removed_even_when_inherited() {
        let out = changes(&env(&[]), false, |_| true);
        assert!(out.contains(&(OFFLOAD.to_owned(), None)));
        assert!(out.contains(&("__GLX_VENDOR_LIBRARY_NAME".to_owned(), None)));
    }

    #[test]
    fn config_can_opt_back_into_offload() {
        let out = changes(&env(&[(OFFLOAD, "1")]), false, |_| true);
        assert!(out.contains(&(OFFLOAD.to_owned(), Some("1".to_owned()))));
        assert!(!out.contains(&(OFFLOAD.to_owned(), None)));
    }

    #[test]
    fn empty_config_value_removes_the_variable() {
        let out = changes(&env(&[("GSK_RENDERER", "")]), false, |_| true);
        assert!(out.contains(&("GSK_RENDERER".to_owned(), None)));
    }

    #[test]
    fn other_config_entries_pass_through() {
        let out = changes(&env(&[("GSK_RENDERER", "ngl")]), false, |_| false);
        assert!(out.contains(&("GSK_RENDERER".to_owned(), Some("ngl".to_owned()))));
    }
}
