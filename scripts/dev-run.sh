#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/host"

exec cargo run -- \
  --resolution "${ART_DISPLAY_RESOLUTION:-1280x720}" \
  "$@"
