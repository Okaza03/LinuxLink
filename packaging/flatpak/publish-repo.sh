#!/bin/sh
# Turn the release's Flatpak repository into LinuxLink's Flatpak remote and
# website (both served by GitHub Pages), and build the release's .flatpak
# bundles from it.
#
#   publish-repo.sh REPO SITE DIST [KEY-ID]
#
# REPO holds the app's commits for every architecture (release.yml imports
# each runner's bundle into it). SITE is the folder to publish: the website
# from packaging/site plus the repository. One bundle per architecture in REPO
# is written to DIST, as LinuxLink-X.Y.Z-<arch>.flatpak (the in-app updater
# relies on that name).
#
# With KEY-ID (a key in the default GnuPG home), the repository's summary is
# signed and the public key goes into the .flatpakrepo and .flatpakref files
# and the bundles, so flatpak can check what it downloads. The commits
# themselves are signed when they are imported.
#
# LINUXLINK_FLATPAK_BASE changes where the site is served from, to test
# locally:
#   LINUXLINK_FLATPAK_BASE=http://127.0.0.1:8765 publish-repo.sh repo site dist
#   python3 -m http.server -d site 8765
set -eu

REPO=$1
SITE=$2
DIST=$3
KEY=${4:-}

APP=io.github.Okaza03.LinuxLink
BRANCH=master # flatpak-builder's default
REMOTE=linuxlink
BASE=${LINUXLINK_FLATPAK_BASE:-https://okaza03.github.io/LinuxLink}
URL=$BASE/repo/
RUNTIME_REPO=https://dl.flathub.org/repo/flathub.flatpakrepo
TITLE="LinuxLink"
COMMENT="Mix and route your audio like Elgato Wave Link, on Linux"
HOMEPAGE=https://github.com/Okaza03/LinuxLink
ICON=https://raw.githubusercontent.com/Okaza03/LinuxLink/main/packaging/linux/io.github.Okaza03.LinuxLink.svg

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
VERSION=$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)
# The website's download links. Without a tag (a dry run), Cargo.toml's version.
TAG=${LINUXLINK_TAG:-v$VERSION}

SIGN=""
GPGKEY_LINE=""
if [ -n "$KEY" ]; then
    SIGN="--gpg-sign=$KEY"
    PUBKEY=$(mktemp)
    trap 'rm -f "$PUBKEY"' EXIT
    gpg --export "$KEY" > "$PUBKEY"
    [ -s "$PUBKEY" ] || { echo "No public key for $KEY in GnuPG"; exit 1; }
    GPGKEY_LINE="GPGKey=$(base64 -w0 < "$PUBKEY")"
fi

# The site holds only this release: a client that has an older one just
# pulls the new commit. Static deltas make that one download.
# shellcheck disable=SC2086 # $SIGN is empty or one word
flatpak build-update-repo $SIGN \
    --title="$TITLE" --comment="$COMMENT" --homepage="$HOMEPAGE" --icon="$ICON" \
    --default-branch="$BRANCH" --generate-static-deltas --prune "$REPO"

# The bundles name the remote, so a copy installed from one also gets
# `flatpak update` (and the key, so those updates are checked).
mkdir -p "$DIST"
BUNDLES=0
for ARCH in x86_64 aarch64; do
    [ -e "$REPO/refs/heads/app/$APP/$ARCH/$BRANCH" ] || continue
    # shellcheck disable=SC2086
    flatpak build-bundle --arch="$ARCH" --repo-url="$URL" --runtime-repo="$RUNTIME_REPO" \
        ${KEY:+--gpg-keys="$PUBKEY"} "$REPO" "$DIST/LinuxLink-${TAG#v}-$ARCH.flatpak" "$APP" "$BRANCH"
    BUNDLES=$((BUNDLES + 1))
done
[ "$BUNDLES" -gt 0 ] || { echo "$REPO has no $APP on branch $BRANCH"; exit 1; }

rm -rf "$SITE"
mkdir -p "$SITE"
cp -a "$REPO" "$SITE/repo"
rm -rf "$SITE/repo/tmp"

# `flatpak remote-add linuxlink $BASE/linuxlink.flatpakrepo`. A .flatpakrepo
# can't name the runtime's repository, so the instructions add Flathub first.
cat > "$SITE/$REMOTE.flatpakrepo" <<EOF
[Flatpak Repo]
Title=$TITLE
Url=$URL
Homepage=$HOMEPAGE
Comment=$COMMENT
Icon=$ICON
$GPGKEY_LINE
EOF

# `flatpak install $BASE/linuxlink.flatpakref`: adds the remote and installs, in one go
cat > "$SITE/$REMOTE.flatpakref" <<EOF
[Flatpak Ref]
Name=$APP
Branch=$BRANCH
Title=$TITLE
Url=$URL
Homepage=$HOMEPAGE
Comment=$COMMENT
Icon=$ICON
RuntimeRepo=$RUNTIME_REPO
SuggestRemoteName=$REMOTE
IsRuntime=false
$GPGKEY_LINE
EOF

# The website: packaging/site with this release's version and links, and the app icon
RELEASES=https://github.com/Okaza03/LinuxLink/releases
cp -a "$ROOT/packaging/site/." "$SITE/"
cp "$ROOT/packaging/linux/$APP.svg" "$SITE/linuxlink.svg"
sed -i \
    -e "s|@BASE@|$BASE|g" \
    -e "s|@DL@|$RELEASES/download/$TAG|g" \
    -e "s|@TAG@|$TAG|g" \
    -e "s|@VERSION@|${TAG#v}|g" \
    -e "s|@VERSION_LINE@|Latest version: ${TAG#v} · <a href=\"$RELEASES/tag/$TAG\">What's new</a>|g" \
    "$SITE/index.html"
