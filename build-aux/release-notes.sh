#!/bin/sh
# Prints, as Markdown, the notes of the metainfo <release> for VERSION (by
# default the current one). Fails when the metainfo has no such release.
set -eu
here=$(dirname "$0")
version=${1:-$("$here/version.sh")}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
appstreamcli metainfo-to-news --format=markdown \
	"$here/../data/io.github.cszach.Calliope.metainfo.xml" "$tmp/news.md" >/dev/null
# Keep the body of that version's section; GitHub needs a blank line
# between a paragraph and the list after it.
awk -v want="Version $version" '
	/^Version / { on = ($0 == want); next }
	!on || /^-+$/ || /^Released: / { next }
	!found && $0 == "" { next }
	/^ \* / && prev != "" && prev !~ /^ \* / { print "" }
	{ print; prev = $0; found = 1 }
	END { exit !found }
' "$tmp/news.md"
