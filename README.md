# Muse for GNOME

An unofficial desktop client for [Muse](https://muse.ai), Meta's AI agent,
for GNOME on Linux. It runs the real muse.ai web app in the system WebKitGTK
engine, so it is light (no bundled Chromium) and has the full interface, and
it keeps you logged in across restarts.

Not affiliated with or endorsed by Meta. "Muse" is Meta's name for its
product; this project only wraps the public website.

Work in progress. See the [issues](https://github.com/cszach/muse-ai-client/issues).

## Build

Fedora:

```sh
sudo dnf install gtk4-devel libadwaita-devel webkitgtk6.0-devel
make build
make run
```
