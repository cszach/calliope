//! User configuration, stored as TOML in `~/.config/calliope/config.toml`.
//!
//! Every key has a default, so a missing or partial file is fine. The file is
//! also where the app remembers window size and permission answers.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::consts::{DEFAULT_ALLOWED_HOSTS, DEFAULT_START_URL};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub start_url: String,
    pub background_mode: bool,
    /// Start in the background at login; applies while background mode is on.
    pub start_at_login: bool,
    /// Show an icon in the top bar (needs the AppIndicator extension).
    pub top_bar_icon: bool,
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
            start_at_login: false,
            top_bar_icon: true,
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
    /// The user has bound the global shortcut, so it is bound again at
    /// startup. Inside Flatpak this is the only record of it Calliope can read.
    pub shortcut_bound: bool,
}

impl Default for QuickAskConfig {
    fn default() -> Self {
        Self {
            width: 480,
            height: 640,
            submit: true,
            shortcut_bound: false,
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

const SAVED_HEADER: &str = "# Calliope settings that differ from the defaults. Calliope rewrites this file\n\
     # when a setting changes, keeping your edits but not comments. See\n\
     # config.example.toml in the repository for every key.\n\n";

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

/// Applies to `target` the changes from `base` to `ours`: a key whose value
/// differs gets `ours`'s value, a key `ours` dropped is removed, and tables
/// are compared key by key. Every other key keeps `target`'s value.
fn apply_changes(target: &mut toml::Table, base: &toml::Table, ours: &toml::Table) {
    let keys: BTreeSet<&String> = base.keys().chain(ours.keys()).collect();
    for key in keys {
        match (base.get(key), ours.get(key)) {
            (b, o) if b == o => {}
            (Some(toml::Value::Table(b)), Some(toml::Value::Table(o))) => {
                match target
                    .entry(key.as_str())
                    .or_insert_with(|| toml::Value::Table(toml::Table::new()))
                {
                    toml::Value::Table(t) => apply_changes(t, b, o),
                    other => *other = toml::Value::Table(o.clone()),
                }
            }
            (_, Some(o)) => {
                target.insert(key.clone(), o.clone());
            }
            (_, None) => {
                target.remove(key.as_str());
            }
        }
    }
}

fn to_table(config: &Config) -> io::Result<toml::Table> {
    match toml::Value::try_from(config).map_err(io::Error::other)? {
        toml::Value::Table(table) => Ok(table),
        _ => Err(io::Error::other("config is not a table")),
    }
}

impl Config {
    /// Reads the config, falling back to defaults when the file is missing or
    /// broken.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                log::warn!("ignoring invalid config {}: {e}", path.display());
                Self::default()
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                log::warn!("cannot read config {}: {e}", path.display());
                Self::default()
            }
        }
    }

    /// Writes the settings Calliope changed since `saved`, the config as it
    /// last read or wrote the file, into the file as it is now: keys edited
    /// by hand meanwhile, and keys Calliope does not know, stay. A file that
    /// cannot be read, or would not load, is left untouched and is an error.
    /// With nothing changed, the file is not touched at all.
    ///
    /// Values equal to the defaults are left out, so a changed default
    /// reaches existing installs. The write is atomic: a crash mid-write
    /// leaves the old file.
    pub fn save(&self, path: &Path, saved: &Self) -> io::Result<()> {
        if self == saved {
            return Ok(());
        }
        let mut file = match std::fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => toml::Table::new(),
            Err(e) => return Err(e),
        };
        apply_changes(&mut file, &to_table(saved)?, &to_table(self)?);
        let mut value = toml::Value::Table(file);
        value
            .clone()
            .try_into::<Self>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        prune_defaults(&mut value, &toml::Value::Table(to_table(&Self::default())?));
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
        let dir = std::env::temp_dir().join(format!("calliope-test-{}-{name}", std::process::id()));
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
        assert_eq!(Config::load(&path), Config::default());
    }

    #[test]
    fn invalid_file_gives_defaults() {
        let path = temp_path("invalid");
        std::fs::write(&path, "this is = = not toml").unwrap();
        assert_eq!(Config::load(&path), Config::default());
    }

    #[test]
    fn prompts_are_sent_by_default() {
        assert!(Config::default().quick_ask.submit);
    }

    #[test]
    fn saving_defaults_writes_no_settings() {
        let path = temp_path("defaults");
        let saved = Config {
            background_mode: true,
            ..Config::default()
        };
        Config::default().save(&path, &saved).unwrap();
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
        cfg.save(&path, &Config::default()).unwrap();
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
        assert_eq!(Config::load(&path), cfg);
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
        cfg.save(&path, &Config::default()).unwrap();
        assert_eq!(Config::load(&path), cfg);
        assert!(!path.with_extension("toml.tmp").exists());
    }

    #[test]
    fn saving_keeps_keys_edited_since_loading() {
        let path = temp_path("edited");
        let saved = Config::default();
        std::fs::write(
            &path,
            "start_url = \"https://muse.ai/edited\"\nfuture_key = 1\n\
             [webkit.env]\nGST_DEBUG = \"2\"\n",
        )
        .unwrap();
        let mut cfg = saved.clone();
        cfg.window.width = 640;
        cfg.save(&path, &saved).unwrap();
        let loaded = Config::load(&path);
        assert_eq!(loaded.window.width, 640);
        assert_eq!(loaded.start_url, "https://muse.ai/edited");
        assert_eq!(loaded.webkit.env["GST_DEBUG"], "2");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("future_key = 1"), "unknown key lost:\n{text}");
    }

    #[test]
    fn saving_nothing_changed_leaves_the_file_alone() {
        let path = temp_path("unchanged");
        let text = "# pinned\nzoom_level = 1.0\n";
        std::fs::write(&path, text).unwrap();
        let cfg = Config::default();
        cfg.save(&path, &cfg).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn saving_overrides_a_key_calliope_changed() {
        let path = temp_path("override");
        let saved = Config::default();
        std::fs::write(&path, "zoom_level = 1.5\n").unwrap();
        let cfg = Config {
            zoom_level: 2.0,
            ..saved.clone()
        };
        cfg.save(&path, &saved).unwrap();
        assert_eq!(Config::load(&path).zoom_level, 2.0);
    }

    #[test]
    fn saving_removes_only_what_calliope_removed() {
        let path = temp_path("removed");
        let mut saved = Config::default();
        saved.remember_permission("https://muse.ai", Capability::Microphone, true);
        saved.save(&path, &Config::default()).unwrap();
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("\n[permissions.\"https://meta.ai\"]\ncamera = false\n");
        std::fs::write(&path, text).unwrap();
        let mut cfg = saved.clone();
        cfg.permissions.clear();
        cfg.save(&path, &saved).unwrap();
        let loaded = Config::load(&path);
        assert_eq!(
            loaded.permission("https://muse.ai", Capability::Microphone),
            None
        );
        assert_eq!(
            loaded.permission("https://meta.ai", Capability::Camera),
            Some(false)
        );
    }

    #[test]
    fn saving_leaves_a_broken_file_untouched() {
        for (name, broken) in [
            ("syntax", "this is = = not toml"),
            ("type", "zoom_level = \"big\"\n"),
        ] {
            let path = temp_path(name);
            std::fs::write(&path, broken).unwrap();
            let cfg = Config {
                background_mode: true,
                ..Config::default()
            };
            assert!(cfg.save(&path, &Config::default()).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        }
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
