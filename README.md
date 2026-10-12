# Calliope

A desktop client for [Muse](https://muse.ai), Meta's AI agent, on GNOME.
Calliope runs the real muse.ai web app, so every Muse feature is there. It uses
the system's WebKitGTK engine instead of a bundled Chromium, so it stays light.
It keeps you logged in across restarts, and adds what a browser tab cannot:
Quick Ask from any app, Ask Muse from the Activities overview, notifications
while its window is closed, and tabs and windows that share one login.

Calliope is unofficial: it is not affiliated with or endorsed by Meta. "Muse" is
Meta's name for its product, and Calliope only hosts the public website. You
need a Muse account.

## Install

Calliope is a Flatpak from its own signed repository: open
[zachnguyen.com/calliope](https://zachnguyen.com/calliope/) and choose **Install
with GNOME Software**, or run

```sh
flatpak install --user https://zachnguyen.com/calliope/calliope.flatpakref
```

Updates arrive with your other Flatpak apps. To build it yourself, see
[Build](#build).

## Features

### Quick Ask

A small window for a quick question, opened from any app with a keyboard
shortcut. In Preferences (Ctrl+comma), choose **Set Up Keyboard Shortcut…**.
GNOME asks you to confirm a key, and suggests Ctrl+Alt+M. Press the shortcut
again or Escape to hide the window. It keeps its page between uses, so you can
come back to the answer. Change the key later in **Settings → Keyboard → View
and Customize Shortcuts**.

From a terminal, `calliope --quick-ask` shows or hides it, and
`calliope --ask "TEXT"` opens it with TEXT typed in. With the Flatpak, use
`flatpak run io.github.cszach.Calliope` in place of `calliope`.

### Ask Muse from the overview

Type a question in the Activities overview and choose **Ask Muse**: Calliope
opens with the question sent. Flatpak installs every app's search switched off,
so first turn Calliope on in **Settings → Search**. Preferences has an **Open
Search Settings** button for this.

In Preferences, a prefix such as `?` limits Ask Muse to questions that start
with it, and **Send Prompts Automatically** chooses whether questions are sent
or only typed in. The same choice applies to `--ask`.

GNOME Shell does not let a search result raise a window. When Calliope is
already open, GNOME may say "Calliope is ready" instead of showing it; click the
notification.

### Notifications and background mode

When Muse sends a notification, it appears as a GNOME notification, and clicking
it brings back the tab it came from. Calliope asks once whether muse.ai may show
notifications, and remembers the answer.

With **Run in Background** on (Preferences, or the main menu), closing the
window keeps Calliope running, so notifications and Quick Ask keep working and
the page is there when you come back. **Start at Login** starts it in the
background when you log in; the Flatpak then appears under **Background Apps**
in the system menu. With the
[AppIndicator extension](https://extensions.gnome.org/extension/615/appindicator-support/),
Calliope also shows an icon in the top bar, with a dot for new notifications. To
quit for real, press Ctrl+Q or choose Quit from the top bar menu.

### Tabs and windows

Every tab and window shares one login. Links to other websites open in your web
browser. Muse's own sign-in steps and the sites it connects to stay in the app.
Downloads go to your Downloads folder, with a notification to open them. Uploads
use the GNOME file chooser. Voice mode uses your microphone once you allow it.

### Keyboard shortcuts

Ctrl+? shows these in the app.

| Keys                          | Action                     |
| ----------------------------- | -------------------------- |
| Ctrl+N                        | New window                 |
| Ctrl+T                        | New tab                    |
| Ctrl+W                        | Close tab                  |
| Ctrl+Q                        | Quit                       |
| Ctrl+R or F5                  | Reload                     |
| Ctrl+Shift+R or Shift+F5      | Reload, ignoring the cache |
| Alt+Left, Alt+Right           | Back, forward              |
| Ctrl+plus, Ctrl+minus, Ctrl+0 | Zoom in, out, reset        |
| F11                           | Fullscreen                 |
| Ctrl+comma                    | Preferences                |
| Ctrl+?                        | Keyboard shortcuts         |
| Ctrl+Alt+M (your choice)      | Quick Ask, from any app    |

## Privacy

Calliope talks to no server of its own and collects nothing. The pages it shows
are muse.ai and Meta's login, which follow
[Meta's privacy policy](https://www.facebook.com/privacy/policy/).

Your login, cookies and site storage stay on your computer, like a browser's.
For the Flatpak they live in `~/.var/app/io.github.cszach.Calliope/`. For a
build from source they live in `~/.local/share/calliope`, `~/.cache/calliope`
and `~/.config/calliope/config.toml`. **Clear Site Data…** in Preferences logs
you out of Muse, deletes the site data and forgets the permissions you granted.

Calliope asks before a page uses the microphone, camera or notifications, and
remembers each answer in the config file. It asks every time before sharing your
screen. Location and pointer lock are always refused.

Calliope adds three small scripts to pages, in an isolated script world that the
page cannot see or change. They are in [`data/js`](data/js):

- `detect-webrtc.js` watches for errors that name WebRTC, so the tab can offer
  to open the page in a browser (see below). It does not change WebRTC or
  anything else on the page.
- `blob-media.js` works around a WebKitGTK 2.54 bug that corrupts audio and
  video played from `blob:` URLs, such as the videos Muse generates. It reloads
  such media from a `data:` URL, in the page.
- `fill-prompt.js`, on muse.ai only, types a question into Muse's message box
  for Quick Ask, `--ask` and Ask Muse, and presses Enter when you have asked for
  that.

## Limitations

WebKitGTK 2.54, which Fedora 44 and the GNOME 51 runtime ship, has no WebRTC.
Its developers turned WebRTC off in 2.54 while they move it to a new backend.
Voice mode and the Secure VM view work without it. If a page fails because it
needs WebRTC, Calliope shows a banner with **Open in Browser**.

## Troubleshooting

**The window is blank, flickers, shows glitches or crashes.** Quit Calliope
(Ctrl+Q, or Quit in the top bar menu when it runs in the background), then start
it with safer graphics settings:

```sh
flatpak run io.github.cszach.Calliope --safe-graphics
```

(`calliope --safe-graphics` for a build from source.) This draws with GTK's
OpenGL renderer instead of Vulkan, turns off NVIDIA's explicit sync, and has
WebKit share frames through memory instead of DMA-BUF. It is slower.

If that helps, or to go further, set variables in the `[webkit.env]` table of
the config file, which Calliope applies at every start:
`~/.var/app/io.github.cszach.Calliope/config/calliope/config.toml` for the
Flatpak, `~/.config/calliope/config.toml` otherwise. Try them one at a time, in
this order, cheapest first:

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

**Muse says the browser is not supported.** Set `user_agent` in the config file;
`data/config.example.toml` has a macOS variant to try.

**Ask Muse does not appear in the overview.** Turn Calliope on in **Settings →
Search**, and type at least three characters (Preferences sets the minimum and
the prefix).

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
`webkitgtk-6.0` API) and Rust 1.85, or later; Ubuntu 24.04 and Debian 13 are too
old. CI builds on Fedora 44 and Ubuntu 26.04.

Each [release](https://github.com/cszach/calliope/releases) also has a source
tarball with every Rust crate included, which builds the same way without
network access.

`make install` installs into `~/.local` instead of Flatpak. Overview search then
also needs `make install-search-provider`, which uses `sudo`, because GNOME
Shell reads search providers only from system folders. To build the Flatpak, see
`build-aux/flatpak/`; CI builds it on every pull request.

## Development

`make check` runs formatting, Clippy and the tests, as CI does.
[`docs/notes.md`](docs/notes.md) records what was found out about WebKitGTK,
GNOME and muse.ai, with sources. [`docs/releasing.md`](docs/releasing.md)
explains releases.

Two scripts run Calliope in headless GNOME sessions of their own:
`scripts/smoke.py` checks that it starts and draws on Wayland and X11 at several
scales, and `scripts/demo.py` records the demo video and the screenshots by
script.

## License

[MIT](LICENSE).
