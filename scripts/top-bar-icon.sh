#!/bin/sh
# Prints an SVG that shows the square PNG given, for the top bar; with --dot
# it adds a red dot in the top right corner, for while a notification is
# waiting. Made at install time because the PNG is Meta's artwork, which is
# never committed.
set -eu
png=$1
if [ "${2:-}" = --dot ]; then
	mask=' mask="url(#cut)"'
	dot='<circle fill="#e01b24" cx="104" cy="24" r="22"/>'
else
	mask=
	dot=
fi
cat <<SVG
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="128" height="128" viewBox="0 0 128 128">
  <mask id="cut">
    <rect width="128" height="128" fill="#fff"/>
    <circle cx="104" cy="24" r="32" fill="#000"/>
  </mask>
  <image width="128" height="128"$mask xlink:href="data:image/png;base64,$(base64 -w0 "$png")"/>
  $dot
</svg>
SVG
