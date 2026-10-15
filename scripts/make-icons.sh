#!/usr/bin/env bash
# Regenerates the app and tray icons from the guild site's brand files.
# Usage: scripts/make-icons.sh <site repo>/public/brand   (needs ImageMagick)
set -euo pipefail
brand="${1:?path to the site brand folder}"
out="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/icons"

# App icon: the site's tile.
convert "$brand/tile.png" -resize 256x256 "$out/icon.png"
convert "$brand/tile.png" -define icon:auto-resize=256,128,64,48,32,16 "$out/icon.ico"

# Tray icons: the column mark with a status dot at the top right, once per menu-bar
# brightness. The dot's ring is the bar's own colour, as in the approved design (#2).
# "dark" is for dark bars (light mark); "light" is for light bars (black mark).
dot() { case "$1" in ok) echo '#34C759';; warn) echo '#FFB020';; stop) echo '#FF453A';; esac; }
ring() { case "$1" in dark) echo '#1E1E1E';; light) echo '#E9E9EB';; esac; }
for bar in dark light; do
  mark=(-background none "$brand/mark.png" -resize x52)
  [ "$bar" = light ] && mark+=(-fill black -colorize 100)
  for tone in ok warn stop; do
    convert -size 64x64 xc:none \( "${mark[@]}" \) -geometry +4+10 -composite \
      -fill "$(dot $tone)" -stroke "$(ring $bar)" -strokewidth 3 -draw 'circle 52,12 52,22' \
      "$out/tray/$tone-$bar.png"
  done
done
