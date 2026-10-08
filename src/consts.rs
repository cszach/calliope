//! Names and defaults shared across the app.

pub const APP_ID: &str = "io.github.cszach.Calliope";

/// Directory name under the XDG config, data and cache directories.
pub const DIR_NAME: &str = "calliope";

pub const DEFAULT_START_URL: &str = "https://muse.ai/";

/// Hosts that stay inside the app. Subdomains are included.
pub const DEFAULT_ALLOWED_HOSTS: &[&str] = &[
    "muse.ai",
    "meta.ai",
    "meta.com",
    "facebook.com",
    "fb.com",
    "fbcdn.net",
    "whatsapp.com",
];

/// Isolated JavaScript world for the app's own user scripts, so page scripts
/// cannot see or tamper with them.
pub const SCRIPT_WORLD: &str = "calliope";
