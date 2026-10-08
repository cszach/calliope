//! Keyboard shortcuts: the accelerator table and the dialog listing it.

use adw::prelude::*;

/// (action, accelerators, title, section). One table drives both the
/// accelerators and the dialog, so they cannot drift apart.
const SHORTCUTS: &[(&str, &[&str], &str, &str)] = &[
    (
        "app.new-window",
        &["<Control>n"],
        "New window",
        "Tabs and Windows",
    ),
    (
        "win.new-tab",
        &["<Control>t"],
        "New tab",
        "Tabs and Windows",
    ),
    (
        "win.close-tab",
        &["<Control>w"],
        "Close tab",
        "Tabs and Windows",
    ),
    ("app.quit", &["<Control>q"], "Quit", "Tabs and Windows"),
    ("win.reload", &["<Control>r", "F5"], "Reload", "Navigation"),
    (
        "win.reload-bypass-cache",
        &["<Control><Shift>r", "<Shift>F5"],
        "Reload, ignoring the cache",
        "Navigation",
    ),
    ("win.back", &["<Alt>Left"], "Back", "Navigation"),
    ("win.forward", &["<Alt>Right"], "Forward", "Navigation"),
    (
        "win.zoom-in",
        &["<Control>plus", "<Control>equal", "<Control>KP_Add"],
        "Zoom in",
        "View",
    ),
    (
        "win.zoom-out",
        &["<Control>minus", "<Control>KP_Subtract"],
        "Zoom out",
        "View",
    ),
    (
        "win.zoom-reset",
        &["<Control>0", "<Control>KP_0"],
        "Reset zoom",
        "View",
    ),
    ("win.fullscreen", &["F11"], "Fullscreen", "View"),
    (
        "app.preferences",
        &["<Control>comma"],
        "Preferences",
        "General",
    ),
    (
        "win.show-shortcuts",
        &["<Control>question"],
        "Keyboard shortcuts",
        "General",
    ),
];

const DEBUG_SHORTCUTS: &[(&str, &[&str])] = &[("win.inspector", &["<Control><Shift>i"])];

pub fn install(app: &adw::Application, debug: bool) {
    for (action, accels, _, _) in SHORTCUTS {
        app.set_accels_for_action(action, accels);
    }
    if debug {
        for (action, accels) in DEBUG_SHORTCUTS {
            app.set_accels_for_action(action, accels);
        }
    }
}

pub fn dialog() -> adw::ShortcutsDialog {
    let dialog = adw::ShortcutsDialog::new();
    let mut current: Option<(&str, adw::ShortcutsSection)> = None;
    for (_, accels, title, section) in SHORTCUTS {
        if current.as_ref().is_none_or(|(name, _)| name != section) {
            if let Some((_, done)) = current.take() {
                dialog.add(done);
            }
            current = Some((section, adw::ShortcutsSection::new(Some(section))));
        }
        if let Some((_, s)) = &current {
            // The first accelerator only; alternates such as Ctrl+= for zoom
            // still work but would crowd the list.
            s.add(adw::ShortcutsItem::new(title, accels[0]));
        }
    }
    if let Some((_, done)) = current {
        dialog.add(done);
    }
    dialog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_are_contiguous() {
        // dialog() starts a new section whenever the name changes, so a
        // section split across the table would appear twice.
        let mut seen: Vec<&str> = Vec::new();
        for (_, _, _, section) in SHORTCUTS {
            if seen.last() != Some(section) {
                assert!(!seen.contains(section), "section {section} is split");
                seen.push(section);
            }
        }
    }

    #[test]
    fn every_action_is_namespaced_and_unique() {
        let mut actions: Vec<&str> = SHORTCUTS.iter().map(|s| s.0).collect();
        assert!(
            actions
                .iter()
                .all(|a| a.starts_with("app.") || a.starts_with("win."))
        );
        actions.sort_unstable();
        actions.dedup();
        assert_eq!(actions.len(), SHORTCUTS.len());
    }
}
