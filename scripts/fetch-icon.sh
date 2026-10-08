#!/bin/sh
# Downloads Muse's official app icon, listed in https://muse.ai/manifest.json,
# into data/icons/official/ for personal use. Meta's marks need permission to
# redistribute, so the directory is git-ignored; `make install` uses these
# PNGs in place of the repo's own icon when they are present.
set -eu

dest=data/icons/official
base=https://muse.ai/images/pwa_icons/any
mkdir -p "$dest"
for size in "$@"; do
	tmp="$dest/$size.png.part"
	curl --fail --silent --show-error --location --retry 2 \
		--output "$tmp" "$base/$size.png?v=3"
	if [ "$(file --brief --mime-type "$tmp")" != image/png ]; then
		rm -f "$tmp"
		echo "fetch-icon: $base/$size.png is not a PNG; has the manifest changed?" >&2
		exit 1
	fi
	mv "$tmp" "$dest/$size.png"
done
echo "Official icon saved in $dest. Run 'make install' to use it."
