#!/bin/sh
# Removes what install.sh installed. Use the same PREFIX you installed with.
# Your settings in ~/.config/linuxlink are kept.
set -eu

PREFIX="${PREFIX:-$HOME/.local}"
ID=io.github.Okaza03.LinuxLink

if [ -x "$PREFIX/bin/linuxlink" ]; then
  # Remove virtual devices left by a crashed instance.
  "$PREFIX/bin/linuxlink" --cleanup >/dev/null 2>&1 || true
  # Turn back on devices it turned off for the whole computer.
  "$PREFIX/bin/linuxlink" --restore-devices || true
fi
rm -f "$PREFIX/bin/linuxlink" \
  "$PREFIX/share/applications/$ID.desktop" \
  "$PREFIX/share/icons/hicolor/scalable/apps/$ID.svg" \
  "$PREFIX/share/icons/hicolor/128x128/apps/$ID.png" \
  "$PREFIX/share/icons/hicolor/256x256/apps/$ID.png" \
  "$PREFIX/share/metainfo/$ID.metainfo.xml" \
  "$HOME/.config/autostart/$ID.desktop"

echo "LinuxLink was removed from $PREFIX."
