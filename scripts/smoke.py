#!/usr/bin/env python3
"""Starts Calliope in clean, headless desktops and checks that it works.

Everything runs as a new user would meet it: a fresh home directory, its own
runtime directory and D-Bus session, no Calliope config, and (with --flatpak)
a Flatpak installation holding only Calliope. Each case starts a display
server, launches Calliope with --debug, waits for the start page to load,
asks the page for its device pixel ratio, takes a screenshot of the whole
screen, and quits.

Cases are DISPLAY@SCALE[+safe]: DISPLAY is "wayland" (a headless GNOME Shell
whose virtual monitor is set to SCALE through gdctl) or "x11" (Xvfb, with
GDK_SCALE=SCALE; GTK scales X11 by whole numbers only); "+safe" adds
--safe-graphics. Wayland needs gnome-shell and a system bus (GNOME Shell
will not start without one), X11 needs Xvfb and ImageMagick.

Usage: scripts/smoke.py [--binary PATH | --flatpak BUNDLE]
                        [--cases wayland@1,wayland@1.25,...] [--out DIR]

Prints a Markdown table and writes the screenshots and logs to DIR (default
target/smoke). Exits 1 when a case fails.
"""

import argparse
import json
import math
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

APP_ID = "io.github.cszach.Calliope"
OBJECT = "/io/github/cszach/Calliope"
REPO = Path(__file__).resolve().parent.parent
DEFAULT_CASES = "wayland@1,wayland@1.25,wayland@2,x11@1,x11@2,wayland@1+safe"
# Big enough that the default 1100x800 window fits at scale 2.
MONITOR = (2560, 1600)
WAYLAND_DISPLAY = "smoke-wayland"
X11_DISPLAY = ":77"
LOADED = re.compile(r"INFO\s+calliope::tab\] Finished (https://\S+)")
EVAL = re.compile(r"debug-eval: (\{.*\})")
TROUBLE = re.compile(r"panicked|web process (crashed|terminated)|CRITICAL", re.I)
PROBE = (
    "JSON.stringify({dpr: devicePixelRatio, host: location.host, "
    "title: document.title, width: innerWidth, "
    'webgl2: !!document.createElement("canvas").getContext("webgl2")})'
)


def isolated_env(root):
    """The environment of a user who has never run anything."""
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("XDG_", "WAYLAND_", "DBUS_", "GDK_", "GSK_"))
           and k not in ("DISPLAY", "HOME")}
    env["HOME"] = str(root / "home")
    # Short: Wayland socket paths must fit in 108 bytes.
    env["XDG_RUNTIME_DIR"] = tempfile.mkdtemp(prefix="calliope-smoke-")
    return env


def call(*args, check=False):
    return subprocess.run(["gdbus", "call", "--session", *args],
                          capture_output=True, text=True, check=check)


def wait_for(predicate, timeout, step=0.2):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = predicate()
        if value:
            return value
        time.sleep(step)
    return None


def start_wayland(log):
    proc = subprocess.Popen(
        ["gnome-shell", "--headless", "--wayland", "--no-x11",
         "--virtual-monitor", f"{MONITOR[0]}x{MONITOR[1]}",
         f"--wayland-display={WAYLAND_DISPLAY}"],
        stdout=log, stderr=subprocess.STDOUT)
    socket = Path(os.environ["XDG_RUNTIME_DIR"]) / WAYLAND_DISPLAY
    if not wait_for(socket.exists, 20) or not wait_for(
            lambda: call("--dest", "org.gnome.Mutter.DisplayConfig",
                         "--object-path", "/org/gnome/Mutter/DisplayConfig",
                         "--method", "org.gnome.Mutter.DisplayConfig.GetCurrentState"
                         ).returncode == 0, 20):
        raise RuntimeError("gnome-shell did not start")
    return proc


def shell_screenshot(path):
    """Saves the screen through GNOME Shell, which serves only a few bus
    names; this private session's bus has no other claimant to them."""
    import gi
    gi.require_version("Gio", "2.0")
    from gi.repository import Gio, GLib
    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                  "RequestName", GLib.Variant("(su)", ("org.gnome.SettingsDaemon.MediaKeys", 0)),
                  None, Gio.DBusCallFlags.NONE, -1)
    reply = bus.call_sync("org.gnome.Shell.Screenshot", "/org/gnome/Shell/Screenshot",
                          "org.gnome.Shell.Screenshot", "Screenshot",
                          GLib.Variant("(bbs)", (False, False, str(path))),
                          None, Gio.DBusCallFlags.NONE, 10000)
    return reply.unpack()[0]


def x11_screenshot(path):
    return subprocess.run(["import", "-display", X11_DISPLAY, "-window", "root", str(path)],
                          capture_output=True).returncode == 0


def set_scale(scale):
    out = subprocess.run(["gdctl", "show"], capture_output=True, text=True, check=True)
    monitor = re.search(r"Monitor (\S+)", out.stdout).group(1)
    subprocess.run(["gdctl", "set", "--logical-monitor", "--primary",
                    "--monitor", monitor, "--scale", scale],
                   capture_output=True, text=True, check=True)


def start_x11(log):
    proc = subprocess.Popen(
        ["Xvfb", X11_DISPLAY, "-screen", "0", f"{MONITOR[0]}x{MONITOR[1]}x24", "-nolisten", "tcp"],
        stdout=log, stderr=subprocess.STDOUT)
    socket = Path("/tmp/.X11-unix") / f"X{X11_DISPLAY[1:]}"
    if not wait_for(socket.exists, 20):
        raise RuntimeError("Xvfb did not start")
    return proc


def forget_calliope(home):
    """No config, login or cache from an earlier case."""
    for path in (".config/calliope", ".local/share/calliope", ".cache/calliope",
                 f".var/app/{APP_ID}"):
        shutil.rmtree(home / path, ignore_errors=True)


def run_case(case, launcher, out, flatpak):
    display, _, rest = case.partition("@")
    scale, _, extra = rest.partition("+")
    home = Path(os.environ["HOME"])
    forget_calliope(home)
    result = {"case": case, "ok": False, "page": "", "dpr": "", "webgl2": "", "note": ""}
    env = dict(os.environ)
    args = ["--debug"] + (["--safe-graphics"] if extra == "safe" else [])
    server_log = open(out / f"{case}.server.log", "w")
    if display == "wayland":
        server = start_wayland(server_log)
        set_scale(scale)
        env["WAYLAND_DISPLAY"] = WAYLAND_DISPLAY
    else:
        server = start_x11(server_log)
        env["DISPLAY"] = X11_DISPLAY
        env["GDK_SCALE"] = scale
    if flatpak:
        command = ["flatpak", "run", "--user"]
        if display == "x11":
            command.append(f"--env=GDK_SCALE={scale}")
        command += [APP_ID] + args
    else:
        command = [str(launcher)] + args
    log_path = out / f"{case}.log"
    log = open(log_path, "w")
    proc = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT,
                            start_new_session=True)
    read = lambda: log_path.read_text(errors="replace")
    window = f"{OBJECT}/window/1"

    def action(name, *params):
        return call("--dest", APP_ID, "--object-path", window,
                    "--method", "org.gtk.Actions.Activate", name,
                    "[" + ", ".join(params) + "]", "{}")

    try:
        loaded = wait_for(lambda: LOADED.search(read()), 90)
        if not loaded:
            result["note"] = "start page did not finish loading in 90 s"
            return result
        time.sleep(5)  # the login redirects settle, and the page draws
        sent = action("debug-eval", f"<'{PROBE}'>")
        if sent.returncode != 0:
            result["note"] = sent.stderr.strip()
        probe = wait_for(lambda: EVAL.search(read()), 10)
        if probe:
            values = json.loads(probe.group(1))
            result["page"] = values["host"]
            result["dpr"] = values["dpr"]
            result["webgl2"] = "yes" if values["webgl2"] else "no"
        shot = (shell_screenshot if display == "wayland" else x11_screenshot)(
            out / f"{case}.png")
        alive = proc.poll() is None
        # WebKitGTK draws a fractional scale at the next whole one, which
        # the compositor scales down (docs/notes.md).
        expected = math.ceil(float(scale))
        trouble = TROUBLE.search(read())
        result["ok"] = (alive and probe is not None and shot
                        and float(result["dpr"]) == expected and not trouble)
        if not result["ok"]:
            result["note"] = (trouble.group(0) if trouble else
                              "exited early" if not alive else
                              f"expected dpr {expected}" if probe else
                              result["note"] or "no probe answer")
        return result
    finally:
        call("--dest", APP_ID, "--object-path", OBJECT,
             "--method", "org.freedesktop.Application.ActivateAction", "quit", "[]", "{}")
        try:
            proc.wait(15)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            result["note"] = (result["note"] + "; did not quit").lstrip("; ")
            result["ok"] = False
        server.terminate()
        server.wait(10)


def inner(opts):
    out = Path(opts.out).resolve()
    # The first-login tour would cover the window.
    subprocess.run(["gsettings", "set", "org.gnome.shell",
                    "welcome-dialog-last-shown-version", "'999'"])
    if opts.flatpak:
        subprocess.run(["flatpak", "--user", "remote-add", "--if-not-exists", "flathub",
                        "https://dl.flathub.org/repo/flathub.flatpakrepo"], check=True)
        subprocess.run(["flatpak", "--user", "install", "-y", "--noninteractive",
                        "--bundle", str(Path(opts.flatpak).resolve())],
                       check=True, stdout=subprocess.DEVNULL)
    results = [run_case(case, opts.binary, out, opts.flatpak)
               for case in opts.cases.split(",")]
    what = f"Flatpak {Path(opts.flatpak).name}" if opts.flatpak else f"binary {opts.binary}"
    print(f"Calliope smoke test, {what}, {time.strftime('%Y-%m-%d')}\n")
    print("| Case | Result | Page | devicePixelRatio | WebGL2 | Note |")
    print("|---|---|---|---|---|---|")
    for r in results:
        print(f"| {r['case']} | {'pass' if r['ok'] else 'FAIL'} | {r['page']} "
              f"| {r['dpr']} | {r['webgl2']} | {r['note']} |")
    return 0 if all(r["ok"] for r in results) else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--binary", default=str(REPO / "target/release/calliope"))
    group.add_argument("--flatpak", metavar="BUNDLE")
    parser.add_argument("--cases", default=DEFAULT_CASES)
    parser.add_argument("--out", default=str(REPO / "target/smoke"))
    opts = parser.parse_args()
    if os.environ.get("CALLIOPE_SMOKE_INNER"):
        sys.exit(inner(opts))
    out = Path(opts.out)
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    root = Path(tempfile.mkdtemp(prefix="calliope-smoke-home-"))
    env = isolated_env(root)
    env["CALLIOPE_SMOKE_INNER"] = "1"
    try:
        status = subprocess.run(["dbus-run-session", "--", sys.executable, __file__,
                                 *sys.argv[1:]], env=env).returncode
    finally:
        # The private session's document portal may still have its FUSE mount.
        if shutil.which("fusermount3"):
            subprocess.run(["fusermount3", "-u", "-q", f"{env['XDG_RUNTIME_DIR']}/doc"],
                           stderr=subprocess.DEVNULL)
        shutil.rmtree(root, ignore_errors=True)
        shutil.rmtree(env["XDG_RUNTIME_DIR"], ignore_errors=True)
    sys.exit(status)


if __name__ == "__main__":
    main()
