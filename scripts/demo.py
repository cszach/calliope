#!/usr/bin/env python3
"""Records Calliope's demo video and screenshots, by script.

The tour runs in a headless GNOME Shell of its own, so it records the same
desktop every time and never touches yours: a fresh home holding only a copy
of the demo profile, its own D-Bus session, runtime directory and PipeWire.
Mutter's ScreenCast API records the screen and its RemoteDesktop API moves
the pointer and types; the demo-extension next to this script hides the
sharing indicator that input session would put in the top bar, and tells the
script where windows, labels and notification banners are. Calliope is the
installed Flatpak, as people get it.

Commands:
  login        Opens Calliope from the demo profile on your desktop, to log
               in to the demo account once. Quit it with Ctrl+Q when done.
  record       Runs the tour, saves the raw recording, the timeline and
               window screenshots to OUT, then edits the video (see edit).
  edit         Cuts OUT/demo.mp4 from the last recording: speeds up the
               waits, zooms in on each action, and adds captions.

Usage: scripts/demo.py login [--profile DIR]
       scripts/demo.py record [--profile DIR] [--out DIR]
       scripts/demo.py edit [--out DIR]

The profile defaults to ~/.local/share/calliope-demo and OUT to target/demo.
Needs gnome-shell, pipewire, wireplumber, GStreamer's pipewiresrc and x264enc,
ffmpeg, and Python's GObject bindings and Pillow.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

APP_ID = "io.github.cszach.Calliope"
REPO = Path(__file__).resolve().parent.parent
EXTENSION = REPO / "scripts/demo-extension"
EXTENSION_UUID = json.loads((EXTENSION / "metadata.json").read_text())["uuid"]
DEFAULT_PROFILE = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share")) / "calliope-demo"
DEFAULT_OUT = REPO / "target/demo"
# Logical size 1440x900 at scale 2: a common laptop, with sharp text.
MONITOR = (2880, 1800)
SCALE = 2
WAYLAND_DISPLAY = "demo-wayland"
# The size Calliope's window opens at, which the screenshots also use.
WINDOW = (1000, 700)
# The app grid shows these apps, as a stock GNOME would, and Calliope.
GRID_APPS = {
    "org.gnome.Calculator", "org.gnome.Calendar", "org.gnome.Characters",
    "org.gnome.clocks", "org.gnome.Contacts", "org.gnome.Loupe", "org.gnome.Maps",
    "org.gnome.Nautilus", "org.gnome.Papers", "org.gnome.Settings", "org.gnome.Showtime",
    "org.gnome.Snapshot", "org.gnome.Software", "org.gnome.SystemMonitor",
    "org.gnome.TextEditor", "org.gnome.Weather", "org.gnome.Yelp", "org.gnome.Decibels",
}
QUICK_ASK_KEYS = "Ctrl+Alt+M"
# The editor's camera takes this long to reach a shot's focus; clicks wait
# for it so they happen on screen.
SETTLE = 0.9

# The settings a first-time user would have once set up, for the tour.
DEMO_CONFIG = f"""\
[window]
width = {WINDOW[0]}
height = {WINDOW[1]}

[quick_ask]
shortcut_bound = true

[permissions."https://muse.ai"]
notifications = true
"""

GSETTINGS = [
    # The first-login tour would cover the screen.
    ("org.gnome.shell", "welcome-dialog-last-shown-version", "'999'"),
    ("org.gnome.shell", "enabled-extensions", f"['{EXTENSION_UUID}']"),
    ("org.gnome.shell", "favorite-apps",
     "['org.gnome.Nautilus.desktop', 'org.gnome.TextEditor.desktop']"),
    # Flatpak installs search providers switched off; Settings → Search
    # switches Calliope on. The others are off, so "Ask Muse" is the only
    # result and Enter picks it.
    ("org.gnome.desktop.search-providers", "enabled", f"['{APP_ID}.desktop']"),
    ("org.gnome.desktop.search-providers", "disabled", str(sorted(
        line.split("=", 1)[1].strip()
        for ini in Path("/usr/share/gnome-shell/search-providers").glob("*.ini")
        for line in ini.read_text(errors="replace").splitlines()
        if line.startswith("DesktopId=")))),
    # The binding Calliope's "Set Up Keyboard Shortcut…" leaves behind.
    ("org.gnome.settings-daemon.global-shortcuts", "applications", f"['{APP_ID}']"),
    (f"org.gnome.settings-daemon.global-shortcuts.application:"
     f"/org/gnome/settings-daemon/global-shortcuts/{APP_ID}/", "shortcuts",
     "[('quick-ask', {'shortcuts': <['<Control><Alt>m']>, 'description': <'Open Quick Ask'>})]"),
]


def flatpak_location():
    out = subprocess.run(["flatpak", "info", "--show-location", APP_ID],
                         capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit(f"Calliope's Flatpak is not installed: {out.stderr.strip()}")
    return Path(out.stdout.strip())


def app_data(home):
    return home / ".var/app" / APP_ID


def clean_env(home, runtime):
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("XDG_", "WAYLAND_", "DBUS_", "GDK_", "GSK_"))
           and k not in ("DISPLAY", "HOME")}
    env.update(HOME=str(home), XDG_RUNTIME_DIR=str(runtime),
               XDG_CURRENT_DESKTOP="GNOME", XDG_SESSION_TYPE="wayland")
    return env


def short_tempdir(prefix):
    # Wayland socket paths must fit in 108 bytes.
    base = os.environ.get("TMPDIR", "/tmp")
    return Path(tempfile.mkdtemp(prefix=prefix, dir=base if len(base) < 40 else "/tmp"))


def login(opts):
    """Calliope from the demo profile, on this desktop but on a D-Bus session
    of its own, so it runs beside your own Calliope."""
    flatpak_location()
    profile = Path(opts.profile)
    profile.mkdir(parents=True, exist_ok=True)
    runtime = short_tempdir("calliope-demo-")
    env = clean_env(profile, runtime)
    real_runtime = os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
    display = os.environ.get("WAYLAND_DISPLAY", "wayland-0")
    env["WAYLAND_DISPLAY"] = str(Path(real_runtime) / display)
    print(f"Log in to the demo account, then quit Calliope with Ctrl+Q.\nProfile: {profile}")
    try:
        status = subprocess.run(["dbus-run-session", "--", "flatpak", "run", APP_ID],
                                env=env).returncode
    finally:
        shutil.rmtree(runtime, ignore_errors=True)
    return status


# --- The recorded session -------------------------------------------------

def record(opts):
    location = flatpak_location()
    profile = Path(opts.profile)
    if not (app_data(profile) / "data").exists():
        sys.exit(f"No demo profile in {profile}; run `scripts/demo.py login` first.")
    out = Path(opts.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    root = short_tempdir("calliope-demo-")
    home = root / "home"
    # Login and site data, but not the cache or the window size it left.
    for part in ("data", "config"):
        source = app_data(profile) / part
        if source.exists():
            shutil.copytree(source, app_data(home) / part, symlinks=True)
    config = app_data(home) / "config/calliope/config.toml"
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text(DEMO_CONFIG)
    shutil.copytree(EXTENSION, home / ".local/share/gnome-shell/extensions" / EXTENSION_UUID)
    hide_other_apps(home)
    runtime = root / "run"
    runtime.mkdir(mode=0o700)
    env = clean_env(home, runtime)
    # Calliope's own exports only: not the other apps installed here.
    env["XDG_DATA_DIRS"] = f"{location / 'export/share'}:/usr/local/share:/usr/share"
    env["CALLIOPE_DEMO_INNER"] = "1"
    try:
        status = subprocess.run(
            ["dbus-run-session", "--", sys.executable, __file__, "record",
             "--profile", str(profile), "--out", str(out)], env=env).returncode
    finally:
        subprocess.run(["fusermount3", "-u", "-q", str(runtime / "doc")],
                       stderr=subprocess.DEVNULL)
        shutil.rmtree(root, ignore_errors=True)
    if status == 0 and not opts.no_edit:
        status = edit(opts)
    return status


def hide_other_apps(home):
    """Keeps the apps installed on this machine out of the app grid: a user
    entry with NoDisplay hides the system one with the same id."""
    applications = home / ".local/share/applications"
    applications.mkdir(parents=True, exist_ok=True)
    for directory in (Path("/usr/local/share/applications"), Path("/usr/share/applications")):
        for entry in directory.glob("*.desktop"):
            if entry.stem not in GRID_APPS and not (applications / entry.name).exists():
                (applications / entry.name).write_text(
                    "[Desktop Entry]\nType=Application\nName=Hidden\nNoDisplay=true\n")


class Session:
    """The headless desktop: GNOME Shell, PipeWire, and Mutter's screen cast
    and remote desktop sessions over D-Bus."""

    def __init__(self, out):
        import gi
        gi.require_version("Gio", "2.0")
        gi.require_version("Gst", "1.0")
        from gi.repository import Gio, GLib, Gst
        self.Gio, self.GLib, self.Gst = Gio, GLib, Gst
        Gst.init(None)
        self.out = out
        self.log = open(out / "session.log", "w")
        self.procs = []
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION)
        self.pointer = (MONITOR[0] / SCALE / 2, MONITOR[1] / SCALE / 2)
        self.events = []
        self.pipeline = None

    def spawn(self, *command):
        proc = subprocess.Popen(command, stdout=self.log, stderr=subprocess.STDOUT)
        self.procs.append(proc)
        return proc

    def call(self, dest, path, iface, method, args=None, timeout=10000):
        reply = self.bus.call_sync(dest, path, iface, method, args, None,
                                   self.Gio.DBusCallFlags.NONE, timeout, None)
        return reply.unpack() if reply else ()

    def wait(self, predicate, timeout, step=0.2):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            self.GLib.MainContext.default().iteration(False)
            try:
                value = predicate()
            except self.GLib.Error:
                value = None
            if value:
                return value
            time.sleep(step)
        return None

    def start(self):
        for schema, key, value in GSETTINGS:
            subprocess.run(["gsettings", "set", schema, key, value], check=True)
        self.spawn("pipewire")
        time.sleep(0.5)
        self.spawn("wireplumber")
        self.spawn("gnome-shell", "--headless", "--wayland", "--no-x11",
                   "--virtual-monitor", f"{MONITOR[0]}x{MONITOR[1]}",
                   f"--wayland-display={WAYLAND_DISPLAY}")
        if not self.wait(lambda: self.call(
                "org.gnome.Mutter.DisplayConfig", "/org/gnome/Mutter/DisplayConfig",
                "org.gnome.Mutter.DisplayConfig", "GetCurrentState"), 30):
            raise RuntimeError("gnome-shell did not start")
        subprocess.run(["gdctl", "set", "--logical-monitor", "--primary", "--monitor",
                        "Meta-0", "--scale", str(SCALE)], check=True, stdout=self.log)
        if not self.wait(lambda: self.demo("Windows") is not None, 20):
            raise RuntimeError("the demo extension did not load")
        os.environ["WAYLAND_DISPLAY"] = WAYLAND_DISPLAY
        self.connect_input()

    def demo(self, method, *args):
        params = self.GLib.Variant(f"({'s' * len(args)})", args) if args else None
        return json.loads(self.call("io.github.cszach.CalliopeDemo",
                                    "/io/github/cszach/CalliopeDemo",
                                    "io.github.cszach.CalliopeDemo", method, params)[0])

    def connect_input(self):
        V = self.GLib.Variant
        self.rd = self.call("org.gnome.Mutter.RemoteDesktop", "/org/gnome/Mutter/RemoteDesktop",
                            "org.gnome.Mutter.RemoteDesktop", "CreateSession")[0]
        session_id = self.call("org.gnome.Mutter.RemoteDesktop", self.rd,
                               "org.freedesktop.DBus.Properties", "Get",
                               V("(ss)", ("org.gnome.Mutter.RemoteDesktop.Session",
                                          "SessionId")))[0]
        sc = self.call("org.gnome.Mutter.ScreenCast", "/org/gnome/Mutter/ScreenCast",
                       "org.gnome.Mutter.ScreenCast", "CreateSession",
                       V("(a{sv})", ({"remote-desktop-session-id": V("s", session_id)},)))[0]
        # The pointer drawn into the frames.
        self.stream = self.call("org.gnome.Mutter.ScreenCast", sc,
                                "org.gnome.Mutter.ScreenCast.Session", "RecordMonitor",
                                V("(sa{sv})", ("Meta-0", {"cursor-mode": V("u", 1)})))[0]
        nodes = []
        self.bus.signal_subscribe("org.gnome.Mutter.ScreenCast",
                                  "org.gnome.Mutter.ScreenCast.Stream", "PipeWireStreamAdded",
                                  self.stream, None, self.Gio.DBusSignalFlags.NONE,
                                  lambda *a: nodes.append(a[5].unpack()[0]))
        self.input("Start")
        self.node = self.wait(lambda: nodes and nodes[0], 10)
        if not self.node:
            raise RuntimeError("no PipeWire stream from Mutter")

    def input(self, method, signature="()", *args):
        self.call("org.gnome.Mutter.RemoteDesktop", self.rd,
                  "org.gnome.Mutter.RemoteDesktop.Session", method,
                  self.GLib.Variant(signature, args) if args else None)

    # Recording

    def start_recording(self, path):
        """Near-lossless H.264 at the monitor's size; edit() makes the
        small one."""
        Gst = self.Gst
        self.pipeline = Gst.parse_launch(
            f"pipewiresrc path={self.node} do-timestamp=true keepalive-time=100 "
            "! videoconvert ! videorate ! video/x-raw,format=I420,framerate=30/1 "
            "! x264enc speed-preset=superfast pass=quant quantizer=14 key-int-max=30 "
            f"! matroskamux ! filesink location={path}")
        self.first_frame = None
        sink = self.pipeline.get_by_name("filesink0")

        def probe(pad, info):
            if self.first_frame is None:
                self.first_frame = info.get_buffer().pts
            return Gst.PadProbeReturn.OK
        sink.get_static_pad("sink").add_probe(Gst.PadProbeType.BUFFER, probe)
        self.pipeline.set_state(Gst.State.PLAYING)
        if not self.wait(lambda: self.first_frame is not None, 10):
            raise RuntimeError("the recording got no frames")

    def now(self):
        """The time in the recording, in seconds."""
        clock = self.pipeline.get_clock()
        running = clock.get_time() - self.pipeline.get_base_time()
        return (running - self.first_frame) / self.Gst.SECOND

    def stop_recording(self):
        Gst = self.Gst
        self.pipeline.send_event(Gst.Event.new_eos())
        self.pipeline.get_bus().timed_pop_filtered(
            30 * Gst.SECOND, Gst.MessageType.EOS | Gst.MessageType.ERROR)
        self.pipeline.set_state(Gst.State.NULL)

    def shot(self, caption=None, focus=None, speed=1.0, keys=None):
        """Starts a shot of the edited video here: its caption, the area to
        zoom in on (logical pixels; None is the whole screen), how fast to
        play it, and a key combination to show."""
        self.events.append({"t": self.now(), "caption": caption, "focus": focus,
                            "speed": speed, "keys": keys})

    # Input, in logical pixels

    def move(self, x, y, duration=0.6):
        x0, y0 = self.pointer
        steps = max(1, int(duration * 60))
        for i in range(1, steps + 1):
            f = i / steps
            f = f * f * (3 - 2 * f)  # ease in and out
            px, py = x0 + (x - x0) * f, y0 + (y - y0) * f
            self.input("NotifyPointerMotionAbsolute", "(sdd)", self.stream,
                       px * SCALE, py * SCALE)
            time.sleep(duration / steps)
        self.pointer = (x, y)

    def click(self, x, y):
        self.move(x, y)
        time.sleep(0.15)
        for pressed in (True, False):
            self.input("NotifyPointerButton", "(ib)", 0x110, pressed)  # BTN_LEFT
            time.sleep(0.08)

    def key(self, *names):
        syms = [KEYSYMS.get(n, ord(n) if len(n) == 1 else None) for n in names]
        for sym in syms:
            self.input("NotifyKeyboardKeysym", "(ub)", sym, True)
            time.sleep(0.03)
        for sym in reversed(syms):
            self.input("NotifyKeyboardKeysym", "(ub)", sym, False)
            time.sleep(0.03)

    def type(self, text, per_char=0.06):
        for ch in text:
            self.key("Return" if ch == "\n" else ch)
            time.sleep(per_char)

    # Looking at the screen

    def window(self, app=APP_ID, timeout=30):
        """The first mapped window of `app`."""
        return self.wait(lambda: next((w for w in self.demo("Windows")
                                       if w["app"] == app and w["width"]), None), timeout)

    def focused(self):
        return next((w for w in self.demo("Windows") if w["focused"]), None)

    def banner(self, timeout):
        """A notification banner, once it has slid into place."""
        def settled():
            first = self.demo("Banner")
            time.sleep(0.3)
            return first if first and first == self.demo("Banner") else None
        return self.wait(settled, timeout, step=0.3)

    def find(self, text, timeout=10):
        return self.wait(lambda: self.demo("Find", text), timeout)

    def screenshot_window(self, path):
        """The focused window with its shadow, through GNOME Shell; it serves
        only callers that own the media keys name, free on this bus."""
        V = self.GLib.Variant
        self.call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                  "RequestName", V("(su)", ("org.gnome.SettingsDaemon.MediaKeys", 0)))
        return self.call("org.gnome.Shell.Screenshot", "/org/gnome/Shell/Screenshot",
                         "org.gnome.Shell.Screenshot", "ScreenshotWindow",
                         V("(bbbs)", (True, False, False, str(path))))[0]

    def stop(self):
        for proc in reversed(self.procs):
            proc.terminate()
        for proc in self.procs:
            try:
                proc.wait(10)
            except subprocess.TimeoutExpired:
                proc.kill()


KEYSYMS = {
    "Return": 0xff0d, "Escape": 0xff1b, "Tab": 0xff09, "BackSpace": 0xff08,
    "Super": 0xffeb, "Ctrl": 0xffe3, "Alt": 0xffe9, "Shift": 0xffe1,
    " ": 0x20, "comma": 0x2c,
}


def center(r):
    return r["x"] + r["width"] / 2, r["y"] + r["height"] / 2


def tour(s, out):
    """The scripted tour. Each shot() starts a part of the edited video."""
    # Text Editor stands in for "any other app".
    s.spawn("gnome-text-editor", "--new-window")
    editor = s.window("org.gnome.TextEditor")
    if not editor:
        raise RuntimeError("Text Editor did not open")
    # The first pointer event closes the overview GNOME opens at login.
    s.move(*center(editor), duration=0.1)
    time.sleep(2)
    s.start_recording(out / "raw.mkv")
    time.sleep(1)

    # Launch from the app grid.
    s.shot("Calliope brings Muse to the GNOME desktop")
    s.key("Super", "a")
    time.sleep(1.5)  # the grid slides in
    icon = s.find("Calliope")
    if not icon:
        raise RuntimeError("Calliope is not in the app grid")
    s.shot("Calliope brings Muse to the GNOME desktop", focus=icon)
    time.sleep(SETTLE)
    s.click(*center(icon))
    s.shot("Calliope brings Muse to the GNOME desktop")  # the window opens
    window = s.window()
    if not window:
        raise RuntimeError("Calliope's window did not open")
    s.shot("The full muse.ai, in its own window", focus=window, speed=3)
    time.sleep(8)  # the page loads

    # A question in the main window.
    s.shot("The full muse.ai, in its own window", focus=window)
    s.type("Plan a three-day trip to Lisbon in October\n")
    s.shot("The full muse.ai, in its own window", focus=window, speed=4)
    time.sleep(16)  # the answer streams in
    s.screenshot_window(out / "main.png")

    # Tabs.
    s.shot("Tabs and windows share one login", focus=window, keys="Ctrl+T")
    s.key("Ctrl", "t")
    time.sleep(3)

    # Quick Ask from another app.
    s.shot("Quick Ask from any app", keys="Alt+Tab")
    s.key("Alt", "Tab")
    time.sleep(1)
    editor = s.window("org.gnome.TextEditor")
    s.shot("Quick Ask from any app", focus=editor)
    s.type("Dinner: mushroom risotto for four\n")
    time.sleep(0.5)
    s.shot("Quick Ask from any app", focus=editor, keys=QUICK_ASK_KEYS)
    s.key("Ctrl", "Alt", "m")
    quick = s.wait(lambda: next((w for w in s.demo("Windows")
                                 if w["app"] == APP_ID and w["focused"]
                                 and w["width"] < WINDOW[0]), None), 10)
    if not quick:
        raise RuntimeError("Quick Ask did not open")
    time.sleep(1.5)
    s.shot("Quick Ask from any app", focus=quick)
    s.type("How long does risotto rice take to cook?\n")
    s.shot("Quick Ask from any app", focus=quick, speed=4)
    time.sleep(12)
    s.screenshot_window(out / "quick-ask.png")
    s.shot("Quick Ask from any app", focus=quick, keys="Esc")
    s.key("Escape")
    time.sleep(1.5)

    # Ask from the overview.
    s.shot("Ask Muse from the Activities overview")
    s.key("Super")
    time.sleep(1)
    s.type("convert 180 C to Fahrenheit")
    result = s.find("Ask Muse")
    if result:
        s.shot("Ask Muse from the Activities overview", focus=result)
    time.sleep(1.5)
    s.key("Return")
    # GNOME Shell gives search results no activation token, so it may show
    # "Calliope is ready" instead of raising the window (docs/notes.md).
    s.wait(lambda: s.demo("Banner") or (s.focused() or {}).get("app") == APP_ID, 10)
    banner = s.banner(2)
    if banner and (s.focused() or {}).get("app") != APP_ID:
        s.shot("Ask Muse from the Activities overview", focus=banner)
        time.sleep(SETTLE)
        s.click(*center(banner))
    window = s.window()
    s.shot("Ask Muse from the Activities overview", focus=window, speed=4)
    time.sleep(10)

    # A notification brings Calliope back.
    s.key("Alt", "Tab")
    s.shot("Notifications bring you back", speed=4)
    s.type("Oven: 180 C\n")
    banner = s.banner(30)
    if not banner:
        s.events[-1]["skip"] = True  # Muse sent none
    else:
        s.shot("Notifications bring you back", focus=banner)
        time.sleep(SETTLE)
        s.click(*center(banner))
        time.sleep(2)
        s.shot("Notifications bring you back", focus=s.window())
        time.sleep(2)
    s.shot(None)
    time.sleep(0.5)
    s.stop_recording()

    # The Preferences screenshot is not part of the video.
    window = s.window()
    s.click(window["x"] + window["width"] / 2, window["y"] + 12)  # its header bar
    time.sleep(0.5)
    s.key("Ctrl", "comma")
    time.sleep(2)
    s.screenshot_window(out / "preferences.png")


def record_inner(opts):
    out = Path(opts.out)
    s = Session(out)
    try:
        s.start()
        tour(s, out)
    finally:
        if s.events:
            (out / "events.json").write_text(json.dumps(s.events, indent=1))
        s.stop()
    return 0


def edit(opts):
    import demo_edit  # noqa: the editing half, next to this script
    return demo_edit.edit(Path(opts.out))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("login", "record", "edit"):
        command = commands.add_parser(name)
        if name != "edit":
            command.add_argument("--profile", default=str(DEFAULT_PROFILE))
        if name != "login":
            command.add_argument("--out", default=str(DEFAULT_OUT))
        if name == "record":
            command.add_argument("--no-edit", action="store_true",
                                 help="keep the raw recording only")
    opts = parser.parse_args()
    if opts.command == "login":
        sys.exit(login(opts))
    if opts.command == "record":
        sys.exit(record_inner(opts) if os.environ.get("CALLIOPE_DEMO_INNER")
                 else record(opts))
    sys.exit(edit(opts))


if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).parent))
    main()
