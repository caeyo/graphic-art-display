#!/usr/bin/env bash
set -euo pipefail

if ! systemctl is-enabled art-display.service >/dev/null 2>&1; then
    echo "art-display.service is not enabled." >&2
    exit 1
fi

if ! systemctl is-active art-display.service >/dev/null 2>&1; then
    echo "art-display.service is not active." >&2
    systemctl --no-pager --full status art-display.service >&2 || true
    exit 1
fi

echo "art-display.service is active."
journalctl -u art-display -n 20 --no-pager
