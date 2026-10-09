#!/usr/bin/env python3
"""Measures Calliope's memory, CPU and startup time.

Each scenario starts a fresh release build under its own app id
(io.github.cszach.Calliope.Bench) with its own XDG directories, copied from
--profile when given (use a copy of a logged-in profile for realistic
numbers), waits for the page to settle, then records:

  pss_mib   proportional set size of the whole process tree (the UI process,
            WebKit's web and network processes and their sandbox helpers)
  cpu_pct   CPU time of the tree over the idle window, as % of one core
  start_s   (startup only) seconds from launch to the first finished load of
            the start page, read from the app's millisecond log timestamps

Scenarios:
  idle        one window, one tab
  tabs3       one window, three tabs of the start page
  background  started with --background: the page loads in a hidden window

Usage: scripts/bench.py [--profile DIR] [--binary PATH] [--runs N]
                        [--settle S] [--window S]
                        [--scenarios idle,tabs3,background] [--label TEXT]

DIR holds XDG roots: DIR/data, DIR/config, DIR/cache. Results print as a
Markdown table; --json adds the raw runs.
"""

import argparse
import json
import os
import re
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time
from datetime import datetime
from pathlib import Path

APP_ID = "io.github.cszach.Calliope.Bench"
OBJECT = "/io/github/cszach/Calliope/Bench"
REPO = Path(__file__).resolve().parent.parent
BINARY = REPO / "target/release/calliope"
TICKS = os.sysconf("SC_CLK_TCK")
FINISHED = re.compile(r"^\[(\S+) INFO\s+calliope::tab\] Finished https://muse\.ai/")


def bus_pid():
    out = subprocess.run(
        ["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus",
         "--object-path", "/org/freedesktop/DBus",
         "--method", "org.freedesktop.DBus.GetConnectionUnixProcessID", APP_ID],
        capture_output=True, text=True)
    numbers = re.findall(r"\d+", out.stdout)
    return int(numbers[-1]) if out.returncode == 0 and numbers else None


def tree(root):
    """`root` and all its descendants. WebKit spawns helpers from worker
    threads, so this scans every process's parent rather than reading the
    main thread's children."""
    parent = {}
    for entry in Path("/proc").iterdir():
        if entry.name.isdigit():
            try:
                fields = (entry / "stat").read_text().rsplit(")", 1)[1].split()
                parent[int(entry.name)] = int(fields[1])
            except (OSError, IndexError):
                pass
    found, frontier = [root], [root]
    while frontier:
        frontier = [p for p, pp in parent.items() if pp in frontier]
        found += frontier
    return found


def pss_kib(pid):
    try:
        for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
            if line.startswith("Pss:"):
                return int(line.split()[1])
    except OSError:
        pass
    return 0


def cpu_ticks(pid):
    try:
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return int(fields[11]) + int(fields[12])
    except (OSError, IndexError):
        return 0


def launch(env, args, log):
    return subprocess.Popen(
        [str(BINARY), f"--gapplication-app-id={APP_ID}", *args],
        env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)


def quit_app(proc):
    subprocess.run(
        ["gdbus", "call", "--session", "--dest", APP_ID, "--object-path", OBJECT,
         "--method", "org.freedesktop.Application.ActivateAction", "quit", "[]", "{}"],
        capture_output=True)
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        proc.wait()


def first_finished(log_path, started):
    for line in Path(log_path).read_text(errors="replace").splitlines():
        m = FINISHED.match(line)
        if m:
            stamp = datetime.fromisoformat(m.group(1).replace("Z", "+00:00"))
            return round(stamp.timestamp() - started, 2)
    return None


def run(scenario, profile, settle, window):
    with tempfile.TemporaryDirectory(prefix="calliope-bench-") as tmp:
        tmp = Path(tmp)
        for root in ("data", "config", "cache"):
            src = profile / root if profile else None
            if src and src.is_dir():
                shutil.copytree(src, tmp / root, symlinks=True)
            else:
                (tmp / root).mkdir()
        env = dict(os.environ,
                   XDG_DATA_HOME=str(tmp / "data"),
                   XDG_CONFIG_HOME=str(tmp / "config"),
                   XDG_CACHE_HOME=str(tmp / "cache"),
                   RUST_LOG="calliope=info")
        log_path = tmp / "log.txt"
        with open(log_path, "w") as log:
            args = ["--background"] if scenario == "background" else []
            started = time.time()
            proc = launch(env, args, log)
            try:
                deadline = time.time() + 30
                pid = None
                while time.time() < deadline and not pid:
                    time.sleep(0.2)
                    pid = bus_pid()
                if not pid:
                    raise RuntimeError("app did not register on the bus")
                if scenario == "tabs3":
                    time.sleep(3)
                    for _ in range(2):
                        launch(env, ["https://muse.ai/"], log).wait(timeout=20)
                time.sleep(settle)
                start_s = first_finished(log_path, started)
                pids = tree(pid)
                before = {p: cpu_ticks(p) for p in pids}
                w0 = time.time()
                time.sleep(window)
                pids = tree(pid)
                after = {p: cpu_ticks(p) for p in pids}
                w1 = time.time()
                t0, t1 = sum(before.values()), sum(after.values())
                pss = sum(pss_kib(p) for p in pids) / 1024
                if os.environ.get("BENCH_VERBOSE"):
                    for p in pids:
                        comm = Path(f"/proc/{p}/comm").read_text().strip()
                        cpu = 100 * (after.get(p, 0) - before.get(p, 0)) / TICKS / (w1 - w0)
                        if comm not in ("bwrap", "xdg-dbus-proxy"):
                            print(f"    {comm:16} {pss_kib(p) / 1024:6.1f} MiB {cpu:5.1f}% CPU",
                                  file=sys.stderr)
                webprocs = sum(
                    1 for p in pids
                    if Path(f"/proc/{p}/comm").exists()
                    and Path(f"/proc/{p}/comm").read_text().strip().startswith("WebKitWebProces"))
                return {
                    "scenario": scenario,
                    "pss_mib": round(pss, 1),
                    "cpu_pct": round(100 * (t1 - t0) / TICKS / (w1 - w0), 1),
                    "start_s": start_s,
                    "web_processes": webprocs,
                }
            finally:
                quit_app(proc)


def main():
    global BINARY
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--profile", type=Path)
    ap.add_argument("--binary", type=Path)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--settle", type=float, default=60)
    ap.add_argument("--window", type=float, default=30)
    ap.add_argument("--scenarios", default="idle,tabs3,background")
    ap.add_argument("--label", default="")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()
    if a.binary:
        BINARY = a.binary.resolve()
    if not BINARY.exists():
        sys.exit(f"{BINARY} missing: run `cargo build --release` first")

    results = []
    for scenario in a.scenarios.split(","):
        for i in range(a.runs):
            r = run(scenario, a.profile, a.settle, a.window)
            results.append(r)
            print(f"  {scenario} run {i + 1}: {r}", file=sys.stderr)

    def spread(values):
        values = [v for v in values if v is not None]
        if not values:
            return "n/a"
        med = statistics.median(values)
        return f"{med:g} ({min(values):g}–{max(values):g})"

    print(f"\n{a.label}\n" if a.label else "")
    print("| scenario | PSS MiB | CPU % | start s | web processes |")
    print("|---|---|---|---|---|")
    for scenario in a.scenarios.split(","):
        rs = [r for r in results if r["scenario"] == scenario]
        print(f"| {scenario} | {spread([r['pss_mib'] for r in rs])} "
              f"| {spread([r['cpu_pct'] for r in rs])} "
              f"| {spread([r['start_s'] for r in rs])} "
              f"| {spread([r['web_processes'] for r in rs])} |")
    if a.json:
        print(json.dumps(results, indent=1))


if __name__ == "__main__":
    main()
