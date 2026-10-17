#!/bin/sh
# Installs LinuxLink from this folder.
#
#   ./install.sh                         for your user (~/.local), no sudo needed
#   sudo PREFIX=/usr/local ./install.sh  for everyone on this computer
#
# A per-user install updates itself from inside the app.
set -eu

PREFIX="${PREFIX:-$HOME/.local}"
HERE="$(dirname "$(readlink -f "$0")")"
ID=io.github.Okaza03.LinuxLink

install -Dm755 "$HERE/linuxlink" "$PREFIX/bin/linuxlink"
install -Dm644 "$HERE/share/icons/hicolor/scalable/apps/$ID.svg" "$PREFIX/share/icons/hicolor/scalable/apps/$ID.svg"
for size in 128 256; do
  install -Dm644 "$HERE/share/icons/hicolor/${size}x${size}/apps/$ID.png" "$PREFIX/share/icons/hicolor/${size}x${size}/apps/$ID.png"
done
install -Dm644 "$HERE/share/metainfo/$ID.metainfo.xml" "$PREFIX/share/metainfo/$ID.metainfo.xml"
mkdir -p "$PREFIX/share/applications"
# Point the menu entry at the installed binary, in case it isn't on PATH.
sed "s|^Exec=linuxlink|Exec=$PREFIX/bin/linuxlink|" "$HERE/share/applications/$ID.desktop" \
  > "$PREFIX/share/applications/$ID.desktop"

update-desktop-database "$PREFIX/share/applications" >/dev/null 2>&1 || true
gtk-update-icon-cache -q "$PREFIX/share/icons/hicolor" >/dev/null 2>&1 || true

echo "LinuxLink is installed in $PREFIX. Start it from your app menu or run: linuxlink"
