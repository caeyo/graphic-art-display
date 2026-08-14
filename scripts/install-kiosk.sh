#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="${PREFIX:-/usr/local}"
OPT_ROOT="${OPT_ROOT:-/opt/art-display}"
ART_USER="${ART_USER:-artdisplay}"
SKIP_BUILD=0
SKIP_ENABLE=0
RESOLUTION="${ART_DISPLAY_RESOLUTION:-1920x1080}"

usage() {
    cat <<EOF
Usage: sudo $0 [options]

Install art-display as a systemd kiosk service (X11 + xinit on tty1).

Options:
  --skip-build       Do not run cargo build --release
  --skip-enable      Install files but do not enable/start the service
  --resolution WxH   Default render resolution (default: ${RESOLUTION})
  --prefix PATH      Binary/script prefix (default: ${PREFIX})
  --opt PATH         Modules install root (default: ${OPT_ROOT})
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-build) SKIP_BUILD=1 ;;
        --skip-enable) SKIP_ENABLE=1 ;;
        --resolution) RESOLUTION="$2"; shift ;;
        --prefix) PREFIX="$2"; shift ;;
        --opt) OPT_ROOT="$2"; shift ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
    shift
done

if [[ "${EUID}" -ne 0 ]]; then
    echo "Run as root (sudo)." >&2
    exit 1
fi

if command -v apt-get >/dev/null 2>&1; then
    echo "Installing system packages..."
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y \
        xorg \
        xinit \
        x11-xserver-utils \
        libgl1 \
        libegl1 \
        libx11-6 \
        libxrandr2 \
        libxi6 \
        libxcursor1
fi

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    echo "Building release binary..."
    cargo build --release --manifest-path "$ROOT/host/Cargo.toml"
fi

BINARY="$ROOT/host/target/release/art-display"
if [[ ! -x "$BINARY" ]]; then
    echo "Missing release binary: $BINARY" >&2
    echo "Run without --skip-build or place a binary at that path." >&2
    exit 1
fi

if ! id "$ART_USER" >/dev/null 2>&1; then
    echo "Creating user $ART_USER..."
    useradd --create-home --shell /bin/bash "$ART_USER"
fi

usermod -aG video,render,input,tty "$ART_USER" 2>/dev/null || usermod -aG video,input,tty "$ART_USER"

install -d "$PREFIX/bin"
install -d "$PREFIX/lib/art-display"
install -d "$OPT_ROOT/modules"
install -d /etc/art-display

install -m 755 "$BINARY" "$PREFIX/bin/art-display"
install -m 755 "$ROOT/deploy/xsession.sh" "$PREFIX/lib/art-display/xsession.sh"
install -m 755 "$ROOT/deploy/console-blanking-off.sh" "$PREFIX/lib/art-display/console-blanking-off.sh"
cp -a "$ROOT/modules/." "$OPT_ROOT/modules/"
install -m 644 "$ROOT/docs/KIOSK.md" "$OPT_ROOT/README.md"

if [[ ! -f /etc/art-display/env ]]; then
    install -m 644 "$ROOT/deploy/env.example" /etc/art-display/env
fi

if grep -q '^ART_DISPLAY_RESOLUTION=' /etc/art-display/env; then
    sed -i "s|^ART_DISPLAY_RESOLUTION=.*|ART_DISPLAY_RESOLUTION=${RESOLUTION}|" /etc/art-display/env
else
    echo "ART_DISPLAY_RESOLUTION=${RESOLUTION}" >> /etc/art-display/env
fi

chown -R "$ART_USER:$ART_USER" "$OPT_ROOT"

install -m 644 "$ROOT/deploy/art-display.service" /etc/systemd/system/art-display.service
systemctl daemon-reload

if [[ "$SKIP_ENABLE" -eq 0 ]]; then
    systemctl disable getty@tty1.service 2>/dev/null || true
    systemctl enable art-display.service
    systemctl restart art-display.service
    echo "art-display.service enabled and started."
    systemctl --no-pager --full status art-display.service || true
else
    echo "Skipped enable/start (--skip-enable)."
fi

cat <<EOF

Installed:
  Binary:   ${PREFIX}/bin/art-display
  Modules:  ${OPT_ROOT}/modules
  Service:  /etc/systemd/system/art-display.service
  Config:   /etc/art-display/env

Logs: journalctl -u art-display -f

Install the NVIDIA proprietary driver separately on the target hardware.
EOF
