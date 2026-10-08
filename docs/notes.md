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
- Unknown until tried with an account: whether voice mode and the Secure VM
  live view use WebRTC (which this WebKit lacks) or WebSockets.

## WebKitGTK 2.54.1 (Fedora 44 package `webkitgtk6.0-2.54.1-1.fc44`)

- **No WebRTC.** Upstream disabled it for 2.54 while replacing the GStreamer
  backend with libwebrtc ("WebRTC support ... is disabled in 2.54",
  https://webkitgtk.org/2026/09/16/webkitgtk-2.54-highlights.html). Checked
  locally 2026-10-08: no `webrtcbin` string in `libwebkitgtk-6.0.so.4.19.4`,
  no PeerConnection feature in `Settings.get_all_features()`.
  `enable-webrtc` is a no-op; `libnice-gstreamer1` is not needed.
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
- `run-file-chooser` needs no handler; the default opens a native chooser.
- `Download` `destination` is a local path in API 6.0, not a URI.

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
- There is no app-side "always on top" on GNOME Wayland. The user can use the
  window menu's "Always on Top" or bind `org.gnome.desktop.wm.keybindings
  toggle-above`.
- Zach's custom keybinding slot `custom0` is taken (Ctrl+Alt+A runs kitty).
