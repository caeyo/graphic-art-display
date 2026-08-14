# Kiosk deployment

Phase 3 adds systemd auto-start, crash restart, journal logging, and screen blanking
disabled for a minimal Debian X11 kiosk (no desktop environment).

## Requirements

- Debian 12 (or similar) minimal install
- Proprietary **NVIDIA driver** on the display unit (install separately)
- No display manager or desktop environment required

## Quick install

From the repo root on the target machine:

```bash
sudo ./scripts/install-kiosk.sh --resolution 1920x1080
```

This will:

1. Install Xorg, xinit, and runtime libraries (`apt`)
2. Build `art-display` in release mode
3. Create the `artdisplay` system user
4. Install the binary to `/usr/local/bin/art-display`
5. Copy modules to `/opt/art-display/modules`
6. Install and enable `art-display.service` on **tty1**

## Configuration

Edit `/etc/art-display/env`:

```bash
ART_DISPLAY_RESOLUTION=1920x1080
ART_DISPLAY_MODULES=/opt/art-display/modules
```

Restart after changes:

```bash
sudo systemctl restart art-display
```

## Service behaviour

- Starts X on `:0` via `xinit` on `/dev/tty1`
- Runs `art-display --kiosk` fullscreen (Esc and window close are ignored)
- Disables DPMS and screensaver (`xset`) plus console blanking (`setterm`)
- Restarts automatically on crash (`Restart=always`, 3 s backoff)
- Logs to journald: `journalctl -u art-display -f`

## Controls (production)

| Key | Action |
|-----|--------|
| `→` / `n` | Next module |
| `←` / `p` | Previous module |
| `r` | Reload current module |

## Smoke test

After install or reboot:

```bash
./scripts/kiosk-smoke-test.sh
```

Or manually:

```bash
systemctl status art-display
journalctl -u art-display -n 50
```

## Updating modules

Copy new module folders to `/opt/art-display/modules`, then press `r` on the
display or restart the service:

```bash
sudo rsync -a modules/ /opt/art-display/modules/
sudo systemctl restart art-display
```

## Development vs production

| | Development | Production kiosk |
|--|-------------|------------------|
| Launch | `./scripts/dev-run.sh` | systemd `art-display.service` |
| Window | Resizable | Fullscreen |
| Quit | `Esc` | Disabled (`--kiosk`) |
| Logs | Terminal | `journalctl -u art-display` |

## Files

```
deploy/
  art-display.service      systemd unit
  xsession.sh              xinit client (blanking + launch)
  console-blanking-off.sh  disable tty blanking before X starts
  env.example              default environment variables
scripts/
  install-kiosk.sh         installer
  kiosk-smoke-test.sh      post-install health check
```
