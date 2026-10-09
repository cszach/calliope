# Notes

Facts this project depends on that are not visible in the code. Each carries
its source and the date it was checked. When one changes, edit it in place.

## muse.ai

- muse.ai is Meta's "Muse" personal AI agent, launched 2026-09-09. Meta ships
  apps for iOS, Android, Mac and WhatsApp; there is no Linux client (the
  "Download apps" dialog on https://muse.ai, read 2026-10-08).
- An unauthenticated `https://muse.ai/` answers 307 to
  `https://auth.muse.ai/aymh/?origin=https://muse.ai`, which redirects through
  `https://www.facebook.com/aymh/redirect-cycle/`. Login is a mobile number or
  email plus a one-time code, with no password (fetched 2026-10-08).
- In the app, the logged-out chain is `muse.ai` → `auth.muse.ai/aymh` →
  `facebook.com/aymh/redirect-cycle/` → `https://muse.ai/?aymh_complete=1`,
  which shows the login form. It works with WebKitGTK's default user agent
  and default cookie handling plus `CookieAcceptPolicy::Always` (app run,
  2026-10-08). Before login, muse.ai already sets persistent
  `hatch_native_auth_*` cookies plus Meta's `datr`, `wd`, `dpr` and `_fbp`;
  `.facebook.com` sets `datr` and `fr`.
- muse.ai's Content-Security-Policy uses `manifest-src`, which WebKit does not
  recognise; the console warning it prints on every load is harmless.
- The PWA manifest at `https://muse.ai/manifest.json` names the app "Muse",
  `start_url` `/?__pwa=1`, and lists icons at
  `/images/pwa_icons/{any,maskable}/{512,310,256,192,180,144,96,72,64,48,32}.png?v=3`,
  fetchable without login (2026-10-08). Meta's trademarks page requires
  permission to use its marks, so the repo ships its own icon and
  `make fetch-icon` downloads the official one for personal use only.

## WebKitGTK 2.54.1 (Fedora 44 package `webkitgtk6.0-2.54.1-1.fc44`)

- **No WebRTC.** Upstream disabled it for 2.54 while replacing the GStreamer
  backend with libwebrtc ("WebRTC support ... is disabled in 2.54",
  https://webkitgtk.org/2026/09/16/webkitgtk-2.54-highlights.html). Checked
  locally 2026-10-08: no `webrtcbin` string in `libwebkitgtk-6.0.so.4.19.4`,
  no PeerConnection feature in `Settings.get_all_features()`.
  `enable-webrtc` is a no-op; `libnice-gstreamer1` is not needed.
- `UserContentManager::script-message-received` does not say which view
  sent a message, so each view gets its own content manager (cheap; the
  user scripts are shared objects). A page that only checks
  `typeof RTCPeerConnection` or `'RTCPeerConnection' in window` sees no
  change from the WebRTC detector, which only listens for `error` and
  `unhandledrejection` events naming WebRTC classes; an isolated-world
  listener receives both for main-world failures (checked 2026-10-08 with
  local test pages). Cross-origin scripts without CORS report only
  "Script error.", which the detector cannot match.
- getUserMedia microphone capture works without WebRTC and goes straight to
  PipeWire/PulseAudio; cameras go through the xdg-desktop-portal Camera
  interface. Internal feature `GetUserMediaRequiresFocus` is on, so capture
  needs a focused window (WebKit 2.54 `GStreamerCaptureDeviceManager.cpp`,
  `PipeWireCaptureDeviceManager.cpp`).
- On by default: WebCodecs, MSE, OffscreenCanvas, WebGL 2. Off: WebGPU,
  WebTransport. Local GStreamer has H.264, AAC, VP8/9, AV1 and Opus decoders.
- Defaults (read via PyGObject 2026-10-08): cookie accept policy
  `NoThirdParty`, ITP off, cookies kept in memory unless
  `CookieManager::set_persistent_storage` is called. localStorage, IndexedDB
  and service workers persist automatically under the `NetworkSession` data
  directory.
- Default user agent: `Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15
  (KHTML, like Gecko) Version/60.5 Safari/605.1.15`. The `60.5` is deliberate
  (WebKit `UserAgentGLib.cpp`). GNOME Web sends it unchanged. WebKit's quirk
  table has no entry for muse.ai or facebook.com; it swaps the platform to
  macOS for whatsapp.com only. Do not append application details: Google's
  login breaks on a branded UA, per the same quirk table.
- If `show-notification` is not handled, WebKit posts its own desktop
  notification, and clicking it cannot raise our window
  (`WebKitNotificationProvider.cpp`). We handle it.
- **NVIDIA offload turns muse.ai's avatar solid green.** The avatar is a
  looping, muted 720×720 H.264 MP4 played from a `blob:` URL in a `<video>`.
  Launched with `__NV_PRIME_RENDER_OFFLOAD=1 __GLX_VENDOR_LIBRARY_NAME=nvidia`
  (Zach's kitty keybinding sets both, so `make run` from that terminal
  inherits them), it renders solid green on screen (center pixel 0,75,0),
  with or without `WEBKIT_GST_DMABUF_SINK_DISABLED`. Without them it renders
  correctly. Checked 2026-10-08 with the app's own on-screen capture
  (`win.debug-screenshot`). The app removes the offload variables at startup
  (`src/startup_env.rs`).
- `WebView::get_snapshot` is not a faithful picture of video: it drew the
  avatar black under default settings even when the screen showed it
  correctly. Use `win.debug-screenshot` with the window in front.
- Voice mode and the Secure VM "browser take over" view both work in
  WebKitGTK 2.54 without WebRTC (Zach, 2026-10-08).
- `run-file-chooser` needs no handler; the default opens a native chooser.
- `Download` `destination` is a local path in API 6.0, not a URI.

- A `WebView` in a window that has never been shown still loads and runs
  its page: `calliope --background` reached the Muse title with
  `document.visibilityState` "hidden" (2026-10-08). Hiding a window with
  `set_visible(false)` keeps the page alive the same way, which is what lets
  background mode keep receiving web notifications. `visibilityState` is a
  quick way to tell over `debug-eval` whether a window is on screen.

- A key the page does not handle comes back to GTK: on GTK 4,
  `PageClientImpl::doneWithKeyEvent` re-queues an unhandled keydown with
  `webkitWebViewBasePropagateKeyEvent` (`gdk_display_put_event`), so a
  bubble-phase controller on the window sees it; a key the page handled
  (`preventDefault`) does not come back. Arrow keys are always treated as
  handled (WebKit source at tag `webkitgtk-2.54.1`, read 2026-10-08).

- A window covered by other windows reports `document.visibilityState`
  "hidden", and muse.ai's logged-out page then gives its form no layout
  boxes (`getClientRects()` empty), so a prompt fill must wait until the
  window is on screen (checked 2026-10-08).

- Logged in, muse.ai's home page keeps a looping 720×720 H.264 avatar video
  playing through Media Source Extensions while the window is on screen
  (checked 2026-10-08 with a probe counting playing videos). The web
  process decodes it with `libgstlibav` (software) and loads
  `videoconvertscale`; that video is most of the idle cost of a visible
  window. Covered or hidden, WebKit pauses it and the app idles near 0 %.
- Fedora's `mesa-va-drivers` exposes only JPEG and MPEG-2 decoding on this
  AMD GPU (`gst-inspect-1.0 | grep va:`); H.264, HEVC and VP9 hardware
  decoding needs RPM Fusion's `mesa-va-drivers-freeworld`.
- Benchmarks must run in a compositor of their own: on the desktop, whether
  another window covers Calliope swings idle CPU between 0 % and 90 %.
  `scripts/bench-headless` runs `mutter --headless --virtual-monitor` in a
  private D-Bus session; the page reports itself visible and focused there.

## Graphics on this laptop

- AMD Cezanne iGPU (`/dev/dri/renderD128`, radeonsi) drives the session;
  the NVIDIA RTX 3060 (driver 615.71, `renderD129`) is PRIME offload only
  (`glxinfo -B`, 2026-10-08). WebKit picks the EGL display's device, so it
  renders on AMD unless the app is launched with offload variables.
- WebKit bug 280210 (Wayland "Error 71" crash with NVIDIA explicit sync)
  is open and lists driver 615.71.09; it only applies when offloaded to
  NVIDIA. Ladder, cheapest first: `GSK_RENDERER=ngl`,
  `__NV_DISABLE_EXPLICIT_SYNC=1`, `WEBKIT_DISABLE_DMABUF_RENDERER=1`,
  `WEBKIT_SKIA_ENABLE_CPU_RENDERING=1`, `WEBKIT_DISABLE_COMPOSITING_MODE=1`,
  `GDK_BACKEND=x11`.

## GNOME 50 (Shell 50.5, Mutter, xdg-desktop-portal-gnome 50.0)

- GNOME Shell reads search providers only from system data dirs:
  `collectFromDatadirs('search-providers', false)` in `remoteSearch.js`
  (extracted from `/usr/lib64/gnome-shell/libshell-18.so`, 2026-10-08). The
  `.ini` goes in `/usr/local/share/gnome-shell/search-providers/` with sudo.
  The `.desktop` and D-Bus `.service` it points to may live in `~/.local/share`.
- A GNOME custom keyboard shortcut launches its command with an activation
  token that has no surface, so Mutter refuses focus and shows "Muse is
  ready" instead (gsd-media-keys `launch_app`, Mutter
  `meta-wayland-activation.c`). The GlobalShortcuts portal sends a usable
  `activation_token` in its `Activated` signal options; pass it to
  `gtk::Window::set_startup_id` before `present()`.
- GlobalShortcuts portal binding (xdg-desktop-portal 1.22.1, -gnome 50.0,
  GNOME Settings 50.0; sources read 2026-10-08): `BindShortcuts` goes to
  GNOME Settings' `GlobalShortcutsProvider`, which shows its dialog only when
  the app asks for a shortcut id it has not stored
  (`cc_global_shortcut_dialog_present` returns at once when
  `has_new_shortcuts` is false). Stored bindings live in the
  `org.gnome.settings-daemon.global-shortcuts` schema: `applications` lists
  app ids, each with a relocatable `shortcuts` key. A session can bind only
  once (`shortcuts_session->bound`). The portal reports `GlobalShortcuts`
  version 1, so `ConfigureShortcuts` (v2) is unavailable; users change a
  binding in Settings → Keyboard → View and Customize Shortcuts.
  `preferred_trigger` uses the XDG form (`CTRL+ALT+m`). Host apps identify
  themselves with `org.freedesktop.host.portal.Registry.Register` before any
  other portal call; the Claude desktop app is bound this way here.
- `Registry.Register` fails with "App info not found" unless a desktop file
  for the app id is installed (checked 2026-10-08), so global shortcuts work
  only for the installed `io.github.cszach.Calliope`, never for a `.Devel`
  instance. Register and `CreateSession` succeed for the installed id.
- The portal's `Activated` signal carries an `activation_token` when Mutter
  provides one (`globalshortcuts.c`, 50.0).
- GNOME has no tray. The AppIndicator extension (`appindicatorsupport@rgcjonas.gmail.com`,
  v66 here, checked 2026-10-09) hosts `org.kde.StatusNotifierWatcher` and shows each
  registered StatusNotifierItem in the top bar. It hides items whose `Status` is
  `Passive`, shows `AttentionIconName` for `NeedsAttention`, and calls
  `ProvideXdgActivationToken(token)` before `Activate` and before a menu click
  (`appIndicator.js`, `dbusMenu.js`). The `ksni` crate (0.3.6) does not implement that
  method. When the extension starts (login, re-enable) it also scans the bus
  (`tools/busAnalyzer.js`, 2 s later) and files any object at `/StatusNotifierItem`
  under the connection's first well-known name; `indicatorId()` keys items as that
  name, or `<unique>@<path>` for one registered by unique name. An app registering
  with its unique name after the scan therefore gets a second icon (seen on Zach's
  login, 2026-10-09); registering with the app id gives the same key either way.
  An icon the extension cannot load appears as three dots
  (`image-loading-symbolic`). GNOME Shell looks icons up through `icon-theme.cache`, so an
  icon copied into `~/.local/share/icons` without `gtk4-update-icon-cache` stays invisible
  to it.
- There is no app-side "always on top" on GNOME Wayland. The user can use the
  window menu's "Always on Top" or bind `org.gnome.desktop.wm.keybindings
  toggle-above`.
- Zach's custom keybinding slot `custom0` is taken (Ctrl+Alt+A runs kitty).
- A notification's header icon is the app's desktop-file icon drawn with
  `-st-icon-style: symbolic` and desaturated (`.message-source-icon` in the
  Shell theme, `messageList.js`), so Shell picks `<app-id>-symbolic` when
  one exists and otherwise a grey version of the full-colour icon. The
  notification's own `gicon` shows in full colour in the body (checked
  2026-10-08).
- GNOME Shell 50 calls a provider's `ActivateResult(id, terms, timestamp)`
  with no activation token (`remoteSearch.js`, extracted from
  `libshell-18.so`), so presenting an existing window from it may only
  show "Muse is ready". A D-Bus-started `--gapplication-service` instance
  exits as soon as it is idle unless it sets an inactivity timeout; Muse
  uses 30 s and holds the app for each call, which restarts the timer
  (checked 2026-10-08).
- `GApplication` turns a bare command-line argument such as `muse.ai` into
  `file://$PWD/muse.ai` before `open` sees it (checked 2026-10-08), so the
  app filters `open` to http and https links.
- The session bus here is dbus-broker. A newly installed
  `~/.local/share/dbus-1/services/*.service` is picked up after
  `org.freedesktop.DBus.ReloadConfig`, which `make install` calls.

## Flatpak (checked 2026-10-09)

- `org.gnome.Platform//51` (Flathub build of 2026-10-04) has GTK 4.24,
  libadwaita, WebKitGTK 2.54.1 (`libwebkitgtk-6.0.so.4.19.4`, the same release
  as Fedora 44's, so still no WebRTC), and GStreamer's libav, VA,
  PipeWire and PulseAudio plugins. Its `GST_PLUGIN_SYSTEM_PATH` includes the
  `org.freedesktop.Platform.codecs-extra` extension. It is built on
  freedesktop 26.08, so the Rust SDK extension is `rust-stable//26.08`.
- Uninstalling `org.freedesktop.Sdk//26.08` also removes
  `org.freedesktop.Platform.codecs-extra//26.08-extra`, a related ref that
  installed apps still use; reinstall it afterwards.
- The `org.flatpak.Builder` app runs the build on the host through the
  Flatpak session helper, so its state and build directories must be at the
  same path inside its sandbox and outside. Its `/tmp` and `/var/tmp` are
  private, so only paths under `$HOME` work. CI uses
  `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51` (built on the
  `flatpak-builder-lint` image, so the linter is on `PATH`), run
  `--privileged`.
- A sandboxed app cannot read GNOME's `org.gnome.settings-daemon.global-shortcuts`
  schema (the runtime does not ship it and GSettings uses the keyfile
  backend), so Calliope remembers a successful bind in its config. Only host
  apps call `Registry.Register`; Flatpak identifies sandboxed ones.
- The Background portal (`RequestBackground`, with `autostart` and
  `commandline`) asks the user once, then writes the autostart entry on the
  host itself. A request with `autostart: false` removes it.
- Flatpak exports `share/gnome-shell/search-providers/*.ini` and
  `share/dbus-1/services/*.service` from the app into its exports directory,
  which is on the session's `XDG_DATA_DIRS`. So the search provider needs no
  sudo, and the service's `Exec` is rewritten to `flatpak run`.
- If every Flatpak launch fails with `bwrap: Can't find source path
  /run/user/1000/doc/by-app/<app id>`, the document portal is running but
  its FUSE mount at `/run/user/1000/doc` is gone; `systemctl --user restart
  xdg-document-portal` mounts it again. Seen on Zach's machine 2026-10-09,
  the morning /home had filled up; GNOME Software's Open did nothing until
  the restart.
- `flatpak run --env=XDG_CONFIG_HOME=…` (and the other XDG variables) does not
  take effect: Flatpak sets them to `~/.var/app/<id>/…` itself. A test run of
  an installed app id therefore uses the real profile in `~/.var/app`, even under
  another `--gapplication-app-id` (checked 2026-10-09, by mistake, on Zach's
  profile).
- xdg-dbus-proxy (`flatpak-proxy.c`, main, read 2026-10-09) treats an outgoing
  message with no destination as talking to the bus, which every app may do, so
  a sandboxed app's broadcast signals (such as StatusNotifierItem's `NewStatus`)
  get out. Incoming method calls are not filtered, and replies are allowed once
  per outstanding call, so the top bar extension can call into the app with only
  `--talk-name=org.kde.StatusNotifierWatcher`. Checked live: the sandboxed build
  registers and its properties read from outside.
- Flatpak forces `DefaultDisabled=true` into every search provider `.ini` it exports
  (`flatpak-dir.c`, `g_key_file_set_boolean (keyfile, "Shell Search Provider",
  "DefaultDisabled", TRUE)`, main, read 2026-10-09). GNOME Shell then skips the provider,
  without logging, until its desktop id is in `org.gnome.desktop.search-providers enabled`,
  which the Search panel in Settings sets (`remoteSearch.js`, `loadRemoteSearchProviders`).
  So a Flatpak app's overview search is off until the user switches it on there. Seen on
  Zach's machine with Calliope, Authenticator, Eyedropper and Icon Library (2026-10-09).
- Looking Glass (`lookingGlass.js`, GNOME Shell 50) splits the input on every `;` and
  prefixes the last piece with `return`, so any semicolon inside a block yields `undefined`.
  It predeclares `GLib`, `Gio`, `Shell`, `St`, `Main` and allows `await import(...)`.
- GitHub Pages for this repository (enabled 2026-10-09 with the Actions
  source) is served at `https://zachnguyen.com/calliope/`: the account's
  user site has a custom domain, and `cszach.github.io/calliope/` answers
  301 to it. The repository URLs use the canonical address.

## Shipping: Flathub, trademarks, the official apps (researched 2026-10-08)

- Flathub (docs.flathub.org/docs/for-app-authors/requirements): web
  wrappers are refused unless they add "significant polish, functionality,
  or meaningful desktop integration"; there is no rule that the site owner
  must submit. App names and icons "must not violate any trademarks", with
  the example that a WhatsApp client cannot have "WhatsApp" in its name.
  Flathub's generative-AI policy requires disclosing AI-generated code,
  forbids AI-assisted manifests and AI-driven submission pull requests, and
  lets reviewers reject on the extent of generated code. So Calliope ships
  from a self-hosted Flatpak repository (Zach's choice, 2026-10-08).
- Meta's trademark page (meta.com/brand/resources/meta/our-trademarks/)
  bars Meta marks "as or as part of any" trademark, name, username or
  domain, and anything confusingly similar; WhatsApp's adds phonetic
  takeoffs. Meta filed "META MUSE" (serial 50039578, 2026-08-08). Hence the
  name Calliope, chosen by Zach for recalling the Muses by meaning, not
  sound. muse.ai/terms (2026-09-08) forbid reverse engineering, distilling,
  and hiding the automated nature of automated actions; nothing names
  third-party clients. Calliope injects only the scripts in `data/js/` and
  acts in the page only when the user asks.
- GNOME HIG (developer.gnome.org/hig): names under 15 characters, no
  trademarks of others, every app needs a symbolic icon, 1024×600 must
  work and phone-friendly apps should reach 360×294. GNOME Circle is closed
  to submissions (2026-05-29) and rejects AI-generated work.
- The official Muse Mac app (direct download from ai.meta.com/muse/download,
  2026-09-17) adds local computer use (Files, Mail, Messages, Calendar,
  screen recording, accessibility control) and task-done notifications.
  No global shortcut or menu-bar icon is documented. Local computer use is
  out of reach for a website host; M0 targets website parity plus
  Calliope's own desktop features (Zach, 2026-10-08).
- Dark mode reaches pages without extra code: libadwaita 1.9 sets
  `gtk-interface-color-scheme`, which WebKitGTK reads for
  `prefers-color-scheme` and updates live; checked with a probe for forced
  light, forced dark and the system default. The GNOME accent colour does
  not reach page CSS (`AccentColor` stayed WebKit's blue).
