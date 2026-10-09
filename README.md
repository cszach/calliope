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

Fedora 44 or later:

```sh
sudo dnf install git make gcc cargo gtk4-devel libadwaita-devel webkitgtk6.0-devel
```

Ubuntu 26.04 or later:

```sh
sudo apt install git make gcc cargo pkg-config libgtk-4-dev libadwaita-1-dev libwebkitgtk-6.0-dev
```

Then:

```sh
git clone https://github.com/cszach/calliope.git
cd calliope
make build
make run
```

Other distributions need GTK 4.20, libadwaita 1.8, WebKitGTK 2.52 (the
`webkitgtk-6.0` API) and Rust 1.85, or later; Ubuntu 24.04 and Debian 13 are
too old. CI builds on Fedora 44 and Ubuntu 26.04.

Each [release](https://github.com/cszach/calliope/releases) also has a
source tarball with every Rust crate included, which builds the same way
without network access.

`make install` installs into `~/.local` instead of Flatpak. To build the
Flatpak, see `build-aux/flatpak/`; CI builds it on every pull request.

## Troubleshooting

**The window is blank, flickers, shows glitches or crashes.** Quit Calliope
(Ctrl+Q, or Quit in the top bar menu when it runs in the background), then
start it with safer graphics settings:

```sh
flatpak run io.github.cszach.Calliope --safe-graphics
```

(`calliope --safe-graphics` for a build from source.) This draws with GTK's
OpenGL renderer instead of Vulkan, turns off NVIDIA's explicit sync, and
has WebKit share frames through memory instead of DMA-BUF. It is slower.

If that helps, or to go further, set variables in the `[webkit.env]` table
of the config file, which Calliope applies at every start:
`~/.var/app/io.github.cszach.Calliope/config/calliope/config.toml` for the
Flatpak, `~/.config/calliope/config.toml` otherwise. Try them one at a time,
in this order, cheapest first:

```toml
[webkit.env]
GSK_RENDERER = "ngl"
__NV_DISABLE_EXPLICIT_SYNC = "1"
WEBKIT_DISABLE_DMABUF_RENDERER = "1"
WEBKIT_SKIA_ENABLE_CPU_RENDERING = "1"
WEBKIT_DISABLE_COMPOSITING_MODE = "1"
GDK_BACKEND = "x11"
```

On laptops with NVIDIA PRIME, Calliope ignores the offload variables
(`__NV_PRIME_RENDER_OFFLOAD` and others) and renders on the integrated GPU,
where Muse's video works; set them in `[webkit.env]` to opt back in.
`data/config.example.toml` lists every setting.
