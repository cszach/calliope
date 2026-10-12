#!/bin/sh
# Downloads Muse's official icons from muse.ai into data/icons/official/: the
# app icon at the sizes given (listed in https://muse.ai/manifest.json), and
# the site's favicon, whose largest image is saved as mark.png for the top
# bar. They are Meta's artwork, so they are never committed (the directory is
# git-ignored): `make install` fetches them and uses them in place of the
# repository's own icons.
#
# Does nothing when everything is already there; --refresh downloads again.
set -eu

dest=data/icons/official
site=https://muse.ai
if [ "${1:-}" = --refresh ]; then
	shift
	rm -rf "$dest"
fi

missing=
for name in mark "$@"; do
	[ -s "$dest/$name.png" ] || missing=1
done
[ -n "$missing" ] || exit 0

is_png() {
	[ "$(head -c 4 "$1" | tail -c 3)" = PNG ]
}

fetch() {
	curl --fail --silent --show-error --location --retry 2 --max-time 30 \
		--output "$2" "$site/$1"
}

# Everything or nothing: a partial set would mix old and new icons.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for size in "$@"; do
	fetch "images/pwa_icons/any/$size.png?v=3" "$tmp/$size.png"
	if ! is_png "$tmp/$size.png"; then
		echo "fetch-icon: the $size pixel icon is not a PNG; has the manifest changed?" >&2
		exit 1
	fi
done

# An ICO file is a 6-byte header (the image count is at byte 4), then one
# 16-byte entry per image: its width at byte 0 (0 means 256), its length at
# byte 8 and its offset at byte 12, little-endian.
fetch favicon.ico "$tmp/favicon.ico"
number() {
	od -An -t"$1" -j"$2" -N"$3" "$tmp/favicon.ico" | tr -d ' '
}
count=$(number u2 4 2)
best=0
i=0
while [ "$i" -lt "${count:-0}" ]; do
	entry=$((6 + 16 * i))
	width=$(number u1 "$entry" 1)
	[ "$width" -ne 0 ] || width=256
	if [ "$width" -gt "$best" ]; then
		best=$width
		length=$(number u4 $((entry + 8)) 4)
		offset=$(number u4 $((entry + 12)) 4)
	fi
	i=$((i + 1))
done
if [ "$best" -eq 0 ]; then
	echo "fetch-icon: $site/favicon.ico holds no images" >&2
	exit 1
fi
tail -c +$((offset + 1)) "$tmp/favicon.ico" | head -c "$length" > "$tmp/mark.png"
if ! is_png "$tmp/mark.png"; then
	echo "fetch-icon: the favicon's largest image is not a PNG" >&2
	exit 1
fi
rm "$tmp/favicon.ico"

mkdir -p "$dest"
mv "$tmp"/*.png "$dest"/
echo "Downloaded Muse's icons from muse.ai into $dest."
