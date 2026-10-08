# How we work

A light version of the process in `cszach/tendle`. That one is built for a
production platform with several agents and a human approving plans and
designs. This is a single-user desktop app, so the human gates are gone and
Claude carries the decisions. What stays is the part that keeps work honest
across sessions: issues as the status, a gate that runs the checks, and facts
written down when they are found.

## Issues are the unit of work

Every piece of work is an issue, and the open issue list is the only status.
A new issue starts with a `Depends on:` line, then the what and why, then
checkboxes, then an **Exit check:** that a machine or a person can confirm.
Labels: `bug`, `enhancement`, `needs-zach`.

## Claude decides, Zach vetoes

Claude writes a short plan into the issue (a `## Plan` comment) when the work
is not obvious, then builds it without waiting for approval. Technical
mechanism and user interface are both Claude's calls; for UI, follow the GNOME
Human Interface Guidelines and use stock libadwaita widgets before anything
custom, because a client that looks like every other GNOME app is the goal.
Zach reviews whenever he likes, after the fact, and redirects by opening an
issue.

## Only stop for what only Zach can do

Logging in with his one-time code, `sudo`, binding a shortcut in GNOME
Settings, trying a Muse feature with his account. Collect these in one
`needs-zach` issue with exact steps and keep building everything else, so he
gets one ping per batch, not one per issue. Never enter his credentials or
codes.

## Verify by running

Unit-test pure logic first (link policy, config, file naming, search ids) and
see the test fail without the rule it guards. Launch the app to verify GTK and
WebKit behaviour; a diff that compiles proves little about a webview. Say
plainly in the PR what was exercised and what was not.

## The gate

`make check` (format, clippy with warnings as errors, tests). The Stop hook
runs it whenever `src/`, `Cargo.*` or `data/` changed, and CI runs it on every
PR in a Fedora container with the same GTK and WebKit packages.

## Branches, PRs, merges

One issue, one branch (`N-short-name`), one PR into `main`. Commit as often as
useful; push when `make check` passes. Run `/code-review` on any PR that is
more than a small fix, fix what it finds in a follow-up commit, and merge with
`gh pr merge N --squash --delete-branch` once CI is green. Work is serial: the
codebase is small and most issues touch the same files, so parallel worktrees
cost more than they save.

## Find the root cause

Use `systematic-debugging` before fixing any failure. WebKit and GTK
misbehaviour is usually an init-order or ownership problem, not a missing
setting.

## Look things up

WebKitGTK, GNOME Shell, the portals and muse.ai change faster than training
data. Check the installed version's source or docs (the crate sources are in
`~/.cargo/registry/src/`) and record what you found in `docs/notes.md`.
