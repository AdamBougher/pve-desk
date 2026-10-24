#!/usr/bin/env bash
# Launch the real pve-desk window, screenshot it, and stop it again.
#
#   .claude/skills/run-pve-desk/app-shot.sh [name] [--keep]
#
# This is the only path that produces a picture of the actual UI: the
# WebDriver harness in driver.mjs asserts on the DOM but its window never
# paints on this machine (see SKILL.md, Gotchas).
#
# Run from the repo root.
set -uo pipefail

NAME="${1:-app}"
KEEP=""
[ "${2:-}" = "--keep" ] && KEEP=1
[ "${1:-}" = "--keep" ] && { KEEP=1; NAME="app"; }

SHOTS="$(cd "$(dirname "$0")" && pwd)/shots"
BIN="src-tauri/target/release/pve-desk"
mkdir -p "$SHOTS"

if [ ! -x "$BIN" ]; then
  echo "building (first run takes a few minutes)..."
  ( cd src-tauri && cargo build --release ) || exit 1
fi

# GDK_BACKEND=x11 for two reasons: the window is only findable with xprop and
# capturable with `import` if it is an X11 window, and on a Wayland session
# WebKit's offscreen paths are what break (DECISIONS.md).
GDK_BACKEND=x11 "$BIN" > /tmp/pve-desk-app.log 2>&1 &
APP=$!
# Kill ONLY this pid. `pkill -x pve-desk` matches by process name and will
# also take down an AppImage or a dev build the user already had open —
# which is exactly what happened while this script was being written.
cleanup() { [ -n "$KEEP" ] || kill "$APP" 2>/dev/null; }
trap cleanup EXIT

for _ in $(seq 1 40); do
  xprop -name pve-desk WM_CLASS >/dev/null 2>&1 && break
  sleep 0.5
done
if ! xprop -name pve-desk WM_CLASS >/dev/null 2>&1; then
  echo "no pve-desk X window appeared after 20s; log:"
  tail -20 /tmp/pve-desk-app.log
  exit 1
fi
# The launcher probes each saved connection on open; give it a moment so the
# status column is filled in rather than caught mid-check.
sleep 2

OUT="$SHOTS/$NAME.png"
# `import -window pve-desk` matches on WM_NAME, which is exactly "pve-desk"
# for the app. (It does NOT work for the driver's MiniBrowser, whose title
# carries an "[Automation] " prefix — driver.mjs resolves that one by id.)
import -window pve-desk "$OUT" || exit 1

COLOURS=$(magick "$OUT" -format "%k" info:)
echo "$OUT  ($(identify -format '%wx%h' "$OUT"), $COLOURS colours)"
[ "$COLOURS" -le 2 ] && echo "WARNING: looks blank — the window did not paint."
[ -n "$KEEP" ] && echo "app left running as pid $APP (kill $APP when done)"
exit 0
