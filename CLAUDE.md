# Muse

An unofficial GNOME desktop client for Meta's Muse AI agent (https://muse.ai).
It hosts the real web app in the system WebKitGTK engine (no Chromium), keeps
the login across restarts, and adds what a browser tab can't: a quick-ask
window on a global shortcut, background notifications, a GNOME Shell search
provider, and tabs and windows that share one session.

Rust + GTK 4 + libadwaita + WebKitGTK 6.0 (`webkit6` crate), Fedora 44, GNOME
50, Wayland. App id `io.github.cszach.Muse`, binary `muse`.

## Start here, every session

The issue list is the status. There is no "where we left off" note.

```sh
gh issue list                     # everything open
gh issue list --label needs-zach  # waiting on Zach; do not start these
```

Pick the lowest-numbered open issue whose `Depends on:` issues are closed and
that is not labelled `needs-zach`. The issue is the spec. Read `docs/notes.md`
before touching WebKit, D-Bus, GNOME Shell or the Muse site: it holds the facts
that cost time to find.

## Commands

```sh
make check   # fmt --check, clippy -D warnings, tests: the gate
make run     # cargo run -- --debug
make build   # release build
make install # into ~/.local (binary, desktop file, D-Bus service, icons)
```

`make check` must pass before work is reported as done; a Stop hook enforces
it (`.claude/hooks/check-on-stop.sh`). CI runs it on every PR. Never weaken a
test or silence a lint to get green; fix the cause.

## How to work here

`docs/how-we-work.md` has the reasoning. In short:

- **Claude decides.** Plan in the issue, build, review, merge. Mechanism and UI
  are Claude's calls: follow the GNOME HIG and stock libadwaita patterns. Zach
  vetoes after the fact by opening an issue.
- **Ask Zach only for what only Zach can do**: log in with his one-time code,
  run `sudo`, bind a shortcut in GNOME Settings, try a Muse feature with his
  account, or a product question with no sensible default. Batch these into
  one `needs-zach` issue rather than stopping work.
- **Verify by running, not by reading.** Pure logic gets a unit test written
  first. GTK and WebKit glue gets launched and exercised.
- **One issue, one branch, one PR.** Branch `N-short-name` from `main`, commit,
  `make check`, push, open the PR, run `/code-review` on anything non-trivial,
  fix findings, then `gh pr merge N --squash --delete-branch` once CI is green.

## Records

**If it is not in a committed file or an issue, it does not exist.** Write a
finding down when it is established, not at the end of the session:

- facts about WebKitGTK, GNOME, portals or muse.ai go in `docs/notes.md`, each
  with its source and the date it was checked;
- why a change was made, and the alternative rejected, go in the commit body
  or PR description;
- state the current fact, not its history ("we do B", never "we did A; now B").

Commits reference the issue and end with `Closes #N` (or `Refs #N` when the
issue stays open). No `Co-Authored-By` trailer. Commits are GPG-signed; never
bypass signing. Never write a closing keyword next to an issue you are not
closing, even negated.

## Style

`cargo fmt` defaults, clippy clean. Modules are small and named for what they
own (see `src/`). Prefer `glib::clone!` with weak refs over strong captures in
signal handlers, so windows and views can be freed. No `unwrap()` on anything
the network, the filesystem or the user controls; log and degrade instead.

## Gotchas

- **WebKitGTK 2.54 has no WebRTC.** `RTCPeerConnection` does not exist until
  2.56. Microphone capture still works. See `docs/notes.md`.
- **Create WebKit objects after GTK init** (in `startup`), the
  `NetworkSession` before any `WebView`. A popup view uses `related_view`
  only; setting `network_session` too is a construct-time critical.
- **Cookies persist only after `set_persistent_storage`**, called before the
  first load.
- **GNOME Shell reads search providers from system data dirs only**, so the
  provider `.ini` is the one file that needs `sudo`.
- **Markdown files are re-wrapped by a global formatter hook** when written
  with Edit or Write. Check `git diff --stat` after touching one.
- `.claude/` is committed on purpose. Machine-local overrides go in
  `.claude/settings.local.json`, which is ignored.
