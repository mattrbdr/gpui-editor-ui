#!/usr/bin/env bash
# Regenerates the README screenshots in docs/screenshots/ on Linux without a display.
#
# Needs: Xvfb, xdotool, ImageMagick (import, convert), a Vulkan driver (Mesa lavapipe works:
# `apt-get install xvfb xdotool imagemagick mesa-vulkan-drivers`) and the Inter font.
# Usage: tools/screenshots.sh [path/to/spike]   (default: builds the example in release)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/docs/screenshots"
spike="${1:-}"
if [[ -z "$spike" ]]; then
  cargo build --manifest-path "$root/Cargo.toml" -p gpui-editor-ui --example spike --release
  spike="$root/target/release/examples/spike"
fi

display=":${SCREENSHOT_DISPLAY:-97}"
Xvfb "$display" -screen 0 2400x1800x24 -nolisten tcp &
xvfb=$!
trap 'kill "$xvfb" 2>/dev/null || true' EXIT
export DISPLAY="$display"
sleep 1
# 2x rendering for sharp images.
export GPUI_X11_SCALE_FACTOR="${SCALE:-2}"

# Starts the example on the demo document and prints its window id once it has painted.
start() {
  SPIKE_FONT="${SPIKE_FONT:-Inter}" "$spike" "$out/demo.md" >/dev/null 2>&1 &
  app=$!
  for _ in $(seq 50); do
    window="$(xdotool search --pid "$app" 2>/dev/null | tail -1 || true)"
    [[ -n "$window" ]] && break
    sleep 0.2
  done
  sleep 2
}

shot() {
  import -window "$window" "$out/$1.png"
  convert "$out/$1.png" -strip "$out/$1.png"
  if command -v optipng >/dev/null; then optipng -quiet -o2 "$out/$1.png"; fi
}

stop() {
  kill "$app" 2>/dev/null || true
  wait "$app" 2>/dev/null || true
}

# 1. The whole document.
start
shot overview
stop

# 2. The `/` menu on a new paragraph under the first one.
start
xdotool key --window "$window" ctrl+Home Down End Return
xdotool type --window "$window" --delay 80 "/"
sleep 1
shot slash-menu
stop

# 3. A selection across several blocks.
start
xdotool key --window "$window" ctrl+Home Down
xdotool key --window "$window" --delay 60 shift+Down shift+Down shift+Down shift+End
sleep 1
shot selection
stop

echo "Screenshots written to $out"
