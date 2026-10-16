#!/usr/bin/env bash
set -euo pipefail

# Keep the geometric cadence mark in sync with the interface accent.
for icon in src-tauri/icons/32x32.png src-tauri/icons/128x128.png src-tauri/icons/128x128@2x.png; do
  temporary="${icon%.png}.rgba.png"
  convert "$icon" -alpha on -fill '#E10600' -opaque '#F4483E' PNG32:"$temporary"
  mv "$temporary" "$icon"
done
