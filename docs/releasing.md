# Releasing

The version lives in `Cargo.toml`. A test (`tests/version.rs`) fails unless
the newest `<release>` in `data/io.github.cszach.Calliope.metainfo.xml` has
the same version, and that entry's description becomes the release notes.

1. On a branch for the release issue, set `version` in `Cargo.toml`, run
   `cargo check` so `Cargo.lock` follows, and add a `<release version="X.Y.Z"
   date="YYYY-MM-DD">` at the top of `<releases>` in the metainfo, with a
   short description of what changed. `build-aux/release-notes.sh` shows the
   notes as they will appear.
2. `make check`, then open the PR and merge it as usual.
3. Tag the merge commit on `main` and push the tag:

   ```sh
   git tag -s vX.Y.Z -m "Calliope X.Y.Z"
   git push origin vX.Y.Z
   ```

The Release workflow then checks that the tag matches `Cargo.toml`, publishes
the signed Flatpak repository to GitHub Pages, and creates the GitHub release
with `calliope-X.Y.Z-x86_64.flatpak`, `calliope-X.Y.Z.tar.xz` (the source with
vendored crates, from `make dist`) and `SHA256SUMS`.

To publish the Flatpak repository without a release, for a fix that should
reach installs before the next version, run the workflow by hand:
`gh workflow run Release --ref main`.
