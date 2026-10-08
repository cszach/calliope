//! Downloads: saved to the Downloads folder under a name that never
//! overwrites a file, then announced with a notification.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gio::prelude::ApplicationExt;
use glib::prelude::ToVariant;

use crate::app::App;

/// Linux file names are limited to 255 bytes; this leaves room for the
/// " (N)" that `unique_path` may add.
pub const MAX_NAME_BYTES: usize = 240;

/// A file name safe to create in a directory: no path separators, no
/// control characters, not hidden, not empty, and short enough to create.
pub fn sanitize(suggested: &str) -> String {
    let base = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_control() { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    if cleaned.is_empty() {
        return "download".to_owned();
    }
    shorten(cleaned)
}

/// Cuts the stem, never the extension, to fit `MAX_NAME_BYTES`, at a
/// character boundary.
fn shorten(name: &str) -> String {
    if name.len() <= MAX_NAME_BYTES {
        return name.to_owned();
    }
    let (stem, ext) = split_extension(name);
    let ext = if ext.len() > 16 { "" } else { ext };
    let budget = MAX_NAME_BYTES - ext.len();
    let mut cut = budget.min(stem.len());
    while !stem.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{ext}", &stem[..cut])
}

/// `dir/name`, or `dir/stem (N).ext` for the first N that is free.
pub fn unique_path(dir: &Path, name: &str, taken: impl Fn(&Path) -> bool) -> PathBuf {
    let candidate = dir.join(name);
    if !taken(&candidate) {
        return candidate;
    }
    let (stem, ext) = split_extension(name);
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !taken(p))
        .expect("an unused name exists")
}

/// Splits `report.tar.gz` into `report` and `.tar.gz`, `notes.txt` into
/// `notes` and `.txt`; a name without a dot has no extension.
fn split_extension(name: &str) -> (&str, &str) {
    const DOUBLE: &[&str] = &[".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst"];
    let lower = name.to_ascii_lowercase();
    if let Some(ext) = DOUBLE
        .iter()
        .find(|e| lower.ends_with(*e) && lower.len() > e.len())
    {
        return name.split_at(name.len() - ext.len());
    }
    match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    }
}

/// Destinations handed out but not finished yet, so two downloads with the
/// same name started together do not pick the same path.
#[derive(Default)]
pub struct Reserved(RefCell<HashSet<PathBuf>>);

pub fn attach(app: &Rc<App>) {
    let reserved = Rc::new(Reserved::default());
    let a = Rc::clone(app);
    app.engine()
        .session
        .connect_download_started(move |_, download| {
            download.set_allow_overwrite(false);
            let failed = Rc::new(std::cell::Cell::new(false));

            let r = Rc::clone(&reserved);
            download.connect_decide_destination(move |download, suggested| {
                let dir = glib::user_special_dir(glib::UserDirectory::Downloads)
                    .unwrap_or_else(glib::home_dir);
                // The folder may have been deleted; WebKit will not create it.
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    log::warn!("cannot create {}: {e}", dir.display());
                }
                let path = unique_path(&dir, &sanitize(suggested), |p| {
                    p.exists() || r.0.borrow().contains(p)
                });
                match path.to_str() {
                    Some(dest) => {
                        r.0.borrow_mut().insert(path.clone());
                        download.set_destination(dest);
                    }
                    None => {
                        log::warn!("download path is not UTF-8: {}", path.display());
                        download.cancel();
                    }
                }
                true
            });

            let (a2, r2, f2) = (Rc::clone(&a), Rc::clone(&reserved), Rc::clone(&failed));
            download.connect_failed(move |download, error| {
                f2.set(true);
                let path = download.destination().map(PathBuf::from);
                if let Some(p) = &path {
                    r2.0.borrow_mut().remove(p);
                }
                if error.matches(webkit::DownloadError::CancelledByUser) {
                    return;
                }
                log::warn!("download failed: {error}");
                let name = path
                    .as_deref()
                    .and_then(Path::file_name)
                    .map(|n| n.to_string_lossy().into_owned())
                    .or_else(|| {
                        download
                            .response()
                            .and_then(|r| r.suggested_filename())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| "A file".to_owned());
                let note = gio::Notification::new("Download Failed");
                note.set_body(Some(&format!("{name}: {}", error.message())));
                a2.gtk.send_notification(None, &note);
            });

            let (a3, r3) = (Rc::clone(&a), Rc::clone(&reserved));
            // Emitted after `failed` too; only a success is announced here.
            download.connect_finished(move |download| {
                let Some(path) = download.destination().map(PathBuf::from) else {
                    return;
                };
                r3.0.borrow_mut().remove(&path);
                if failed.get() {
                    return;
                }
                notify_done(&a3, &path);
            });
        });
}

fn notify_done(app: &App, path: &Path) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let target = path.to_string_lossy().to_variant();
    let note = gio::Notification::new("Download Complete");
    note.set_body(Some(&name));
    note.set_default_action_and_target_value("app.open-download", Some(&target));
    note.add_button_with_target_value("Open", "app.open-download", Some(&target));
    note.add_button_with_target_value("Show in Folder", "app.show-download", Some(&target));
    app.gtk
        .send_notification(Some(&format!("download:{}", path.display())), &note);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none_taken(_: &Path) -> bool {
        false
    }

    #[test]
    fn sanitize_strips_paths_and_hidden_dots() {
        assert_eq!(sanitize("../../etc/passwd"), "passwd");
        assert_eq!(sanitize("C:\\Users\\x\\report.pdf"), "report.pdf");
        assert_eq!(sanitize(".bashrc"), "bashrc");
        assert_eq!(sanitize("  "), "download");
        assert_eq!(sanitize("a\u{7}b.txt"), "a_b.txt");
    }

    #[test]
    fn long_names_are_shortened_keeping_the_extension() {
        let name = sanitize(&format!("{}.pdf", "é".repeat(200)));
        assert!(name.len() <= MAX_NAME_BYTES, "{} bytes", name.len());
        assert!(name.ends_with(".pdf"));
        assert!(name.starts_with('é'));
    }

    #[test]
    fn free_name_is_used_as_is() {
        let p = unique_path(Path::new("/d"), "a.txt", none_taken);
        assert_eq!(p, Path::new("/d/a.txt"));
    }

    #[test]
    fn taken_name_gets_a_number_before_the_extension() {
        let taken = |p: &Path| p == Path::new("/d/a.txt") || p == Path::new("/d/a (1).txt");
        assert_eq!(
            unique_path(Path::new("/d"), "a.txt", taken),
            Path::new("/d/a (2).txt")
        );
    }

    #[test]
    fn double_extensions_stay_together() {
        let taken = |p: &Path| p == Path::new("/d/x.tar.gz");
        assert_eq!(
            unique_path(Path::new("/d"), "x.tar.gz", taken),
            Path::new("/d/x (1).tar.gz")
        );
    }

    #[test]
    fn names_without_extension_get_a_plain_suffix() {
        let taken = |p: &Path| p == Path::new("/d/README");
        assert_eq!(
            unique_path(Path::new("/d"), "README", taken),
            Path::new("/d/README (1)")
        );
    }
}
