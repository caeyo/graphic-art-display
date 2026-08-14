#!/bin/sh
set -eu

if [ -w /dev/tty1 ] && command -v setterm >/dev/null 2>&1; then
    setterm -blank 0 -powerdown 0 -powersave off </dev/tty1 >/dev/tty1 2>&1 || true
fi
