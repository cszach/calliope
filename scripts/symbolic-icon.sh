#!/bin/sh
# Prints a symbolic icon (SVG) in the shape of the square PNG given: its
# opaque parts in one colour, which GNOME Shell replaces with the top bar's
# text colour. With --dot it adds a red dot in the top right corner, for
# while a notification is waiting. Made at install time because the PNG is
# Meta's artwork, which is never committed.
#
# GNOME Shell fills every rect, path, ellipse, circle and polygon of a
# symbolic icon with the text colour, in masks too, and gives class "error"
# its red. So the shape is a rect, and the mask is built from what Shell
# leaves alone: the image, turned white by a filter so that only its
# transparency counts, and a stroked line (a round dot of its own) for the
# gap around the red dot.
set -eu
png=$1
if [ "${2:-}" = --dot ]; then
	gap='<line x1="104" y1="24" x2="104" y2="24.01" stroke="#000" stroke-width="64" stroke-linecap="round"/>'
	dot='<circle class="error" fill="#e01b24" cx="104" cy="24" r="22"/>'
else
	gap=
	dot=
fi
cat <<SVG
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="128" height="128" viewBox="0 0 128 128">
  <defs>
    <filter id="white" color-interpolation-filters="sRGB">
      <feColorMatrix type="matrix" values="0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0 0 0 1 0"/>
    </filter>
    <mask id="shape" maskUnits="userSpaceOnUse" x="0" y="0" width="128" height="128">
      <image width="128" height="128" filter="url(#white)" xlink:href="data:image/png;base64,$(base64 -w0 "$png")"/>
      $gap
    </mask>
  </defs>
  <rect width="128" height="128" fill="#2e3436" mask="url(#shape)"/>
  $dot
</svg>
SVG
