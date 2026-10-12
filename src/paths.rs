//! XDG locations, all under a `muse-gnome` subdirectory.

use std::io;
use std::path::{Path, PathBuf};

use crate::consts::DIR_NAME;

pub fn config_file() -> PathBuf {
    glib::user_config_dir().join(DIR_NAME).join("config.toml")
}

/// Cookies, local storage, IndexedDB: everything that keeps the login.
pub fn data_dir() -> PathBuf {
    glib::user_data_dir().join(DIR_NAME)
}

pub fn cache_dir() -> PathBuf {
    glib::user_cache_dir().join(DIR_NAME)
}

pub fn cookies_file() -> PathBuf {
    data_dir().join("cookies.sqlite")
}

/// The app's name before it was renamed to Muse: its directory name, and
/// the id its Flatpak kept its files under.
const OLD_DIR_NAME: &str = "calliope";
const OLD_FLATPAK_ID: &str = "io.github.cszach.Calliope";

/// Moves the config, data and cache directories of an install under the old
/// name to their new places, so it keeps its login and settings. Runs before
/// anything reads them.
pub fn migrate_from_old_name() {
    let home = glib::home_dir();
    for (base, kind, default) in [
        (glib::user_config_dir(), "config", ".config"),
        (glib::user_data_dir(), "data", ".local/share"),
        (glib::user_cache_dir(), "cache", ".cache"),
    ] {
        let new = base.join(DIR_NAME);
        for old in old_dirs(&home, &base, kind, default) {
            match migrate_dir(&old, &new) {
                Ok(true) => log::info!("moved {} to {}", old.display(), new.display()),
                Ok(false) => {}
                Err(e) => log::warn!("cannot move {} to {}: {e}", old.display(), new.display()),
            }
        }
    }
}

/// Where an install under the old name kept the directory that is now under
/// `base`, most recent first: an install into `~/.local`, then the Flatpak,
/// whose directories are named `kind`. The Flatpak's copy counts only when
/// `base` is the XDG default for `home`: a run with directories of its own (a
/// test, a benchmark) must not take the real profile.
fn old_dirs(home: &Path, base: &Path, kind: &str, default: &str) -> Vec<PathBuf> {
    let mut dirs = vec![base.join(OLD_DIR_NAME)];
    if base == home.join(default) {
        dirs.push(
            home.join(".var/app")
                .join(OLD_FLATPAK_ID)
                .join(kind)
                .join(OLD_DIR_NAME),
        );
    }
    dirs
}

/// Moves `old` to `new` when only `old` exists. Returns whether it moved.
fn migrate_dir(old: &Path, new: &Path) -> io::Result<bool> {
    if !old.is_dir() || new.exists() {
        return Ok(false);
    }
    if let Some(parent) = new.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(old, new)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("muse-paths-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn old_directory_moves_to_the_new_name() {
        let base = scratch("moves");
        std::fs::create_dir(base.join("old")).unwrap();
        std::fs::write(base.join("old/cookies.sqlite"), "c").unwrap();
        assert!(migrate_dir(&base.join("old"), &base.join("new")).unwrap());
        assert_eq!(
            std::fs::read_to_string(base.join("new/cookies.sqlite")).unwrap(),
            "c"
        );
        assert!(!base.join("old").exists());
    }

    #[test]
    fn existing_new_directory_is_left_alone() {
        let base = scratch("both");
        std::fs::create_dir(base.join("old")).unwrap();
        std::fs::create_dir(base.join("new")).unwrap();
        assert!(!migrate_dir(&base.join("old"), &base.join("new")).unwrap());
        assert!(base.join("old").exists());
    }

    #[test]
    fn missing_parent_of_the_new_directory_is_created() {
        let base = scratch("parent");
        std::fs::create_dir(base.join("old")).unwrap();
        assert!(migrate_dir(&base.join("old"), &base.join("share/new")).unwrap());
        assert!(base.join("share/new").is_dir());
    }

    #[test]
    fn flatpak_profile_counts_only_for_the_default_directories() {
        let home = Path::new("/home/u");
        assert_eq!(
            old_dirs(home, &home.join(".local/share"), "data", ".local/share"),
            [
                home.join(".local/share/calliope"),
                home.join(".var/app/io.github.cszach.Calliope/data/calliope"),
            ]
        );
        let own = Path::new("/tmp/bench/data");
        assert_eq!(
            old_dirs(home, own, "data", ".local/share"),
            [own.join("calliope")]
        );
    }

    #[test]
    fn nothing_to_move_is_fine() {
        let base = scratch("none");
        assert!(!migrate_dir(&base.join("old"), &base.join("new")).unwrap());
        assert!(!base.join("new").exists());
    }
}
