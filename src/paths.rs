//! XDG locations, all under a `calliope` subdirectory.

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

/// The directory name before the app was renamed to Calliope (#24).
const OLD_DIR_NAME: &str = "muse-client";

/// Moves the pre-rename config, data and cache directories to their new
/// names, so an existing install keeps its login and settings. Runs before
/// anything reads them.
pub fn migrate_from_old_name() {
    for base in [
        glib::user_config_dir(),
        glib::user_data_dir(),
        glib::user_cache_dir(),
    ] {
        let (old, new) = (base.join(OLD_DIR_NAME), base.join(DIR_NAME));
        match migrate_dir(&old, &new) {
            Ok(true) => log::info!("moved {} to {}", old.display(), new.display()),
            Ok(false) => {}
            Err(e) => log::warn!("cannot move {} to {}: {e}", old.display(), new.display()),
        }
    }
}

/// Moves `old` to `new` when only `old` exists. Returns whether it moved.
fn migrate_dir(old: &Path, new: &Path) -> io::Result<bool> {
    if !old.is_dir() || new.exists() {
        return Ok(false);
    }
    std::fs::rename(old, new)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("calliope-paths-{}-{name}", std::process::id()));
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
    fn nothing_to_move_is_fine() {
        let base = scratch("none");
        assert!(!migrate_dir(&base.join("old"), &base.join("new")).unwrap());
        assert!(!base.join("new").exists());
    }
}
