#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/host"

exec cargo run -- \
  --modules-path "${ART_DISPLAY_MODULES:-$ROOT/modules}" \
  --resolution "${ART_DISPLAY_RESOLUTION:-1280x720}" \
  "$@"
