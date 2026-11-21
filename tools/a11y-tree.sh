#!/usr/bin/env bash
# Shows what a screen reader sees of the editor on Linux, without a display or a desktop session.
#
# Needs: Xvfb, xdotool, dbus (dbus-run-session), at-spi2-core and python3-pyatspi
# (`apt-get install xvfb xdotool dbus at-spi2-core python3-pyatspi`) and a Vulkan driver.
# Usage: tools/a11y-tree.sh path/to/spike [file.md] ["xdotool keys"] [caret offset]
#   e.g. tools/a11y-tree.sh target/release/examples/spike docs/screenshots/demo.md "End shift+Left"
set -euo pipefail

if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" || -z "${A11Y_TREE_INNER:-}" ]]; then
  A11Y_TREE_INNER=1 exec dbus-run-session -- "$0" "$@"
fi

here="$(cd "$(dirname "$0")" && pwd)"
spike="$1"
python="${PYTHON:-python3}"

# Turn accessibility on for this session: applications only expose their tree when it is enabled.
export GSETTINGS_BACKEND=keyfile XDG_CONFIG_HOME="$(mktemp -d)"
gsettings set org.gnome.desktop.interface toolkit-accessibility true
/usr/libexec/at-spi-bus-launcher --launch-immediately >/dev/null 2>&1 &
sleep 1
/usr/libexec/at-spi2-registryd --use-gnome-session=false >/dev/null 2>&1 &

display=":${A11Y_TREE_DISPLAY:-96}"
Xvfb "$display" -screen 0 1280x900x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1
export DISPLAY="$display"

SPIKE_FONT="${SPIKE_FONT:-Inter}" "$spike" ${2:+"$2"} >/dev/null 2>&1 &
app=$!
trap 'kill "$app" 2>/dev/null || true' EXIT
sleep 5

if [[ -n "${3:-}" ]]; then
  # shellcheck disable=SC2086
  xdotool key --window "$(xdotool search --pid "$app" | tail -1)" $3
  sleep 1
fi
"$python" "$here/a11y_tree.py" ${4:-}
