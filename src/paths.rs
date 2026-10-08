//! XDG locations, all under a `muse-client` subdirectory.

use std::path::PathBuf;

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
