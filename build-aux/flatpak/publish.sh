#!/bin/sh
# Turns the repository flatpak-builder exported into the published site:
#   OUT/repo/               the signed OSTree repository, with static deltas
#   OUT/calliope.flatpakref one-click install from GNOME Software
#   OUT/calliope.flatpakrepo  the repository alone, for `flatpak remote-add`
#   OUT/index.html          a page linking both
#
# Usage: publish.sh BUILD_REPO OUT
# The signing key must already be in GPG's keyring (GNUPGHOME may point
# elsewhere); its fingerprint is read from there. SITE overrides the address
# the site will be served from.
set -eu

APP_ID=io.github.cszach.Calliope
BRANCH=stable
SITE=${SITE:-https://cszach.github.io/calliope}
RUNTIME_REPO=https://dl.flathub.org/repo/flathub.flatpakrepo
HERE=$(dirname "$0")

src=$1
out=$2
key=$(gpg --list-secret-keys --with-colons | awk -F: '/^fpr/ { print $10; exit }')
[ -n "$key" ] || { echo "no secret key in the keyring" >&2; exit 1; }
# The committed public key is what users can check the site against.
# CHECK_KEY=0 skips this, for a trial run with a throwaway key.
if [ "${CHECK_KEY:-1}" = 1 ] && [ "$(gpg --show-keys --with-colons "$HERE/repo-key.asc" | awk -F: '/^fpr/ { print $10; exit }')" != "$key" ]; then
	echo "the signing key does not match build-aux/flatpak/repo-key.asc" >&2
	exit 1
fi
gpg_key=$(gpg --export "$key" | base64 -w0)

mkdir -p "$out"
ostree init --mode=archive-z2 --repo="$out/repo"
flatpak build-commit-from --src-repo="$src" --gpg-sign="$key" \
	"$out/repo" "app/$APP_ID/x86_64/$BRANCH"
flatpak build-update-repo --generate-static-deltas --prune \
	--title=Calliope --default-branch="$BRANCH" --gpg-sign="$key" "$out/repo"

cat > "$out/calliope.flatpakref" <<EOF
[Flatpak Ref]
Name=$APP_ID
Branch=$BRANCH
Title=Calliope
Url=$SITE/repo/
SuggestRemoteName=calliope
Homepage=https://github.com/cszach/calliope
RuntimeRepo=$RUNTIME_REPO
IsRuntime=false
GPGKey=$gpg_key
EOF

cat > "$out/calliope.flatpakrepo" <<EOF
[Flatpak Repo]
Title=Calliope
Url=$SITE/repo/
Homepage=https://github.com/cszach/calliope
Comment=Calliope, an unofficial desktop client for Meta's Muse
DefaultBranch=$BRANCH
GPGKey=$gpg_key
EOF

cp "$HERE/index.html" "$out/index.html"
# GitHub Pages would otherwise run Jekyll over the repository.
touch "$out/.nojekyll"
