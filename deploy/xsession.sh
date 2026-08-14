#!/bin/sh
set -eu

if command -v xset >/dev/null 2>&1; then
    xset s off
    xset s noblank
    xset -dpms
fi

RESOLUTION="${ART_DISPLAY_RESOLUTION:-1920x1080}"
MODULES="${ART_DISPLAY_MODULES:-/opt/art-display/modules}"

exec /usr/local/bin/art-display \
    --kiosk \
    --modules-path "$MODULES" \
    --resolution "$RESOLUTION"
