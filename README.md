# Calliope

An unofficial desktop client for [Muse](https://muse.ai), Meta's AI agent,
for GNOME on Linux. It runs the real muse.ai web app in the system WebKitGTK
engine, so it is light (no bundled Chromium) and has the full interface, and
it keeps you logged in across restarts.

Calliope is not affiliated with or endorsed by Meta. "Muse" is Meta's name
for its product; Calliope only hosts the public website.

Work in progress. See the [issues](https://github.com/cszach/calliope/issues).

## Install

Calliope is a Flatpak from its own signed repository: open
[zachnguyen.com/calliope](https://zachnguyen.com/calliope/) and choose
**Install with GNOME Software**, or run

```sh
flatpak install --user https://zachnguyen.com/calliope/calliope.flatpakref
```

Updates arrive with your other Flatpak apps. Flatpak installs every app's
overview search switched off: to ask Muse from the Activities overview, turn
Calliope on in **Settings → Search** (Calliope's Preferences has a button for
it).

## Build

Fedora:

```sh
sudo dnf install gtk4-devel libadwaita-devel webkitgtk6.0-devel
make build
make run
```

`make install` installs into `~/.local` instead of Flatpak. To build the
Flatpak, see `build-aux/flatpak/`; CI builds it on every pull request.
