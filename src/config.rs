//! User configuration, stored as TOML in `~/.config/muse-client/config.toml`.
//!
//! Every key has a default, so a missing or partial file is fine. The file is
//! also where the app remembers window size and permission answers.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::consts::{DEFAULT_ALLOWED_HOSTS, DEFAULT_START_URL};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub start_url: String,
    pub background_mode: bool,
    pub debug: bool,
    /// Empty means WebKitGTK's default user agent.
    pub user_agent: String,
    pub zoom_level: f64,
    pub allowed_hosts: Vec<String>,
    /// Empty means auto-detect.
    pub composer_selector: String,
    pub window: WindowConfig,
    pub quick_ask: QuickAskConfig,
    pub search_provider: SearchProviderConfig,
    pub webkit: WebkitConfig,
    /// Remembered permission answers, keyed by origin (`https://muse.ai`).
    pub permissions: BTreeMap<String, OriginPermissions>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            start_url: DEFAULT_START_URL.to_owned(),
            background_mode: false,
            debug: false,
            user_agent: String::new(),
            zoom_level: 1.0,
            allowed_hosts: DEFAULT_ALLOWED_HOSTS
                .iter()
                .map(|h| (*h).to_owned())
                .collect(),
            composer_selector: String::new(),
            window: WindowConfig::default(),
            quick_ask: QuickAskConfig::default(),
            search_provider: SearchProviderConfig::default(),
            webkit: WebkitConfig::default(),
            permissions: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1100,
            height: 800,
            maximized: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickAskConfig {
    pub width: i32,
    pub height: i32,
    pub submit: bool,
}

impl Default for QuickAskConfig {
    fn default() -> Self {
        Self {
            width: 480,
            height: 640,
            submit: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchProviderConfig {
    pub min_chars: usize,
    pub prefix: String,
}

impl Default for SearchProviderConfig {
    fn default() -> Self {
        Self {
            min_chars: 3,
            prefix: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebkitConfig {
    /// Environment variables set before WebKit starts.
    pub env: BTreeMap<String, String>,
}

/// A remembered answer per capability; `None` means ask.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OriginPermissions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub microphone: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notifications: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Microphone,
    Camera,
    Notifications,
}

impl OriginPermissions {
    pub fn get(&self, cap: Capability) -> Option<bool> {
        match cap {
            Capability::Microphone => self.microphone,
            Capability::Camera => self.camera,
            Capability::Notifications => self.notifications,
        }
    }

    pub fn set(&mut self, cap: Capability, allowed: bool) {
        let slot = match cap {
            Capability::Microphone => &mut self.microphone,
            Capability::Camera => &mut self.camera,
            Capability::Notifications => &mut self.notifications,
        };
        *slot = Some(allowed);
    }
}

const SAVED_HEADER: &str = "# Muse settings that differ from the defaults. Muse rewrites this file;\n\
     # see config.example.toml in the repository for every key.\n\n";

/// Removes from `value` every key whose value equals the one in `defaults`,
/// recursing into tables and dropping tables left empty.
fn prune_defaults(value: &mut toml::Value, defaults: &toml::Value) {
    let (Some(table), Some(defaults)) = (value.as_table_mut(), defaults.as_table()) else {
        return;
    };
    table.retain(|key, v| match defaults.get(key) {
        Some(d) if v == d => false,
        Some(d) if v.is_table() => {
            prune_defaults(v, d);
            v.as_table().is_some_and(|t| !t.is_empty())
        }
        _ => true,
    });
}

impl Config {
    /// Reads the config, falling back to defaults when the file is missing or
    /// broken. The flag says whether saving may overwrite the file: it is
    /// false when a file exists but could not be used, so a typo in a
    /// hand-edited config is never replaced by defaults.
    pub fn load(path: &Path) -> (Self, bool) {
        match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(config) => (config, true),
                Err(e) => {
                    log::warn!(
                        "ignoring invalid config {} and leaving it untouched: {e}",
                        path.display()
                    );
                    (Self::default(), false)
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => (Self::default(), true),
            Err(e) => {
                log::warn!("cannot read config {}: {e}", path.display());
                (Self::default(), false)
            }
        }
    }

    /// Writes the values that differ from the defaults, atomically: a crash
    /// mid-write leaves the old file. Leaving defaults out lets a changed
    /// default reach existing installs.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut value = toml::Value::try_from(self).map_err(io::Error::other)?;
        let defaults = toml::Value::try_from(Self::default()).map_err(io::Error::other)?;
        prune_defaults(&mut value, &defaults);
        let body = toml::to_string_pretty(&value).map_err(io::Error::other)?;
        let text = format!("{SAVED_HEADER}{body}");
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    /// Remembered answer for `origin`, or `None` to ask.
    pub fn permission(&self, origin: &str, cap: Capability) -> Option<bool> {
        self.permissions.get(origin).and_then(|p| p.get(cap))
    }

    pub fn remember_permission(&mut self, origin: &str, cap: Capability, allowed: bool) {
        self.permissions
            .entry(origin.to_owned())
            .or_default()
            .set(cap, allowed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("muse-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("config.toml")
    }

    #[test]
    fn example_config_matches_defaults() {
        // The shipped example documents the defaults; keep them in step.
        let example: Config = toml::from_str(include_str!("../data/config.example.toml")).unwrap();
        assert_eq!(example, Config::default());
    }

    #[test]
    fn partial_file_fills_in_defaults() {
        let cfg: Config =
            toml::from_str("background_mode = true\n[window]\nwidth = 640\n").unwrap();
        assert!(cfg.background_mode);
        assert_eq!(cfg.window.width, 640);
        assert_eq!(cfg.window.height, WindowConfig::default().height);
        assert_eq!(cfg.start_url, DEFAULT_START_URL);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let path = temp_path("missing").with_file_name("nope.toml");
        assert_eq!(Config::load(&path), (Config::default(), true));
    }

    #[test]
    fn invalid_file_gives_defaults() {
        let path = temp_path("invalid");
        std::fs::write(&path, "this is = = not toml").unwrap();
        // Defaults, and the broken file must not be overwritten.
        assert_eq!(Config::load(&path), (Config::default(), false));
    }

    #[test]
    fn prompts_are_sent_by_default() {
        assert!(Config::default().quick_ask.submit);
    }

    #[test]
    fn saving_defaults_writes_no_settings() {
        let path = temp_path("defaults");
        Config::default().save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let settings: Vec<&str> = text
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .collect();
        assert!(settings.is_empty(), "unexpected settings: {settings:?}");
    }

    #[test]
    fn saving_keeps_only_changed_values() {
        let path = temp_path("changed");
        let mut cfg = Config::default();
        cfg.window.width = 640;
        cfg.quick_ask.submit = false;
        cfg.remember_permission("https://muse.ai", Capability::Microphone, true);
        cfg.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        for wanted in ["width = 640", "submit = false", "microphone = true"] {
            assert!(text.contains(wanted), "missing {wanted:?} in:\n{text}");
        }
        for unwanted in ["start_url", "height", "zoom_level", "min_chars", "[webkit"] {
            assert!(
                !text.contains(unwanted),
                "unexpected {unwanted:?} in:\n{text}"
            );
        }
        assert_eq!(Config::load(&path), (cfg, true));
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path("roundtrip");
        let mut cfg = Config {
            zoom_level: 1.25,
            ..Config::default()
        };
        cfg.remember_permission("https://muse.ai", Capability::Microphone, true);
        cfg.remember_permission("https://muse.ai", Capability::Notifications, false);
        cfg.save(&path).unwrap();
        assert_eq!(Config::load(&path), (cfg, true));
        assert!(!path.with_extension("toml.tmp").exists());
    }

    #[test]
    fn permissions_are_per_origin_and_per_capability() {
        let mut cfg = Config::default();
        cfg.remember_permission("https://muse.ai", Capability::Microphone, true);
        assert_eq!(
            cfg.permission("https://muse.ai", Capability::Microphone),
            Some(true)
        );
        assert_eq!(cfg.permission("https://muse.ai", Capability::Camera), None);
        assert_eq!(
            cfg.permission("https://evil.example", Capability::Microphone),
            None
        );
    }
}
