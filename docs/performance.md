# Performance

How much Calliope costs to run, what was tried to make it cheaper, and what each
attempt changed. Numbers come from `scripts/bench.py`; rerun it after any change
that could move them.

## Method

- **Machine:** Zach's laptop: AMD Cezanne iGPU (session), Fedora 44, GNOME 50 on
  Wayland, WebKitGTK 2.54.1, GTK 4.22, libadwaita 1.9.
- **Build:** release (`cargo build --release`), run under the app id
  `io.github.cszach.Calliope.Bench` with throwaway XDG directories.
- **Profile:** a copy of a logged-in profile, so muse.ai loads the real chat app
  rather than the login page. It was deleted after the runs.
- **Scenarios:** `idle` (one window, one tab), `tabs3` (three tabs of muse.ai),
  `background` (`--background`: the page loads in a hidden window).
- **Measures:** PSS of the whole process tree after 60 s of settling (the
  logged-in app does a second burst of work around 35 s), CPU over the next 30
  s, and the time from launch to the first finished load of muse.ai. Three runs
  per scenario; tables show the median and the range.
- **Noise:** the range across runs of the same build is the noise floor. A
  change counts only if it moves the median by more than that range.

The logged-in page dominates: one web process holds about two thirds of the
total. While the window is on screen, muse.ai's avatar video keeps about 40 %
of a core busy; covered, minimised or hidden, Calliope idles near 0 %.

## Where the cost is (baseline, logged in, window on screen)

| Process | PSS | CPU at idle |
|---|---|---|
| muse.ai page (one web process) | ~420–470 MiB | ~45 % |
| Calliope UI (GTK, compositing) | ~95–155 MiB | ~10–14 % |
| second web process | ~64 MiB | 0 % |
| network process | ~35–53 MiB | 0 % |
| sandbox helpers (bwrap, xdg-dbus-proxy) | ~2 MiB | 0 % |

The idle CPU is muse.ai's avatar: a looping 720×720 H.264 video on the home
page, decoded in software because Fedora's Mesa has no H.264 hardware
decoding (docs/notes.md). It stops whenever the window is covered,
minimised or hidden; the official app shows the same video, so Calliope
leaves it alone. With RPM Fusion's `mesa-va-drivers-freeworld` the video
can decode on the GPU instead.

## Experiments

Run with `scripts/bench-headless`, three runs each; medians with ranges.

| Change | Idle PSS MiB | Idle CPU % | Kept? |
|---|---|---|---|
| baseline | 827 (808–883) | 38 (38–39) | |
| `WEBKIT_GST_DMABUF_SINK_DISABLED=1` | 829 (816–835) | 45 (39–45) | no: no gain, more CPU |
| `WEBKIT_GST_DISABLE_GL_SINK=1` | 829 (827–837) | 39 (38–40) | no: no change |
| `GSK_RENDERER=ngl` | 873 (868–880) | 40 (37–42) | no: 46 MiB more |
| cache model `DocumentViewer` | 820 (817–821) | 38 (38–39) | no: within noise |
| hardware acceleration `Never` | 742 (700–819) | 59 (59–90) | no: far more CPU |
| memory pressure limit 300 MB | 730 (723–734) | 42 (40–42) | |
| memory pressure limit 400 MB | 720 (677–722) | 43 (42–46) | |
| memory pressure limit 500 MB | 738 (661–744) | 42 (41–44) | **yes** |
| limit 300 MB + `DocumentViewer` | 725 (721–736) | 41 (40–41) | no: same as 300 alone |
| 500 MB + `ProcessSwapOnCrossSiteNavigation` off | 747 (742–757) | 38 (38–39) | no: within noise |

The second web process (about 64 MiB) appears only on muse.ai, not on a
plain page or `about:blank`, and muse.ai has no service worker and only
same-site iframes. WebKit logs that it turns on process prewarming after a
cross-site process swap, which muse.ai's login redirects through
facebook.com cause. Turning the `ProcessSwapOnCrossSiteNavigation` feature
off did not remove it (the swap is decided at the process-pool level), and
no public API controls prewarming, so it stays.

A memory pressure limit is the one change that moves memory beyond noise:
WebKit's web processes release caches and collect garbage before memory
gets tight. 300, 400 and 500 MB are equal within noise, so Calliope uses
500, the least eager. Build options were not benchmarked: the binary is
4.5 MB of a 95–155 MiB UI process, below the run-to-run noise.

## Result

| Scenario | Baseline PSS | Final PSS | Final CPU % |
|---|---|---|---|
| idle | 827 (808–883) | **733** (685–748) | 38 (36–50) |
| tabs3 | 1197 (883–1198) | **1054** (1037–1074) | 47 (47–48) |
| background (hidden window) | 338 (225–348) | 355 (354–359) | 0.1 |

Stopping rule: the second round (300, 400, 500 MB, and the combination with
the cache model) differed by less than the run-to-run range, and every
other lever made things worse or did nothing, so tuning stops here. The
next real gain would come from the page itself or from hardware video
decoding, neither of which Calliope controls.

Startup to the first finished load of muse.ai is about 1.5–2.5 s and
network-bound; occasional runs take up to 9 s.

## Rerun

```sh
cargo build --release
make bench PROFILE=/path/to/profile   # XDG roots: data/, config/, cache/
```
