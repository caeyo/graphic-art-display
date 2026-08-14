---
name: GPU Art Display Stack
overview: "Snapshot of the GPU art kiosk plan as of 2026-07-22: Phases 1–3 are implemented in-repo (Rust OpenGL host, three shader modules, systemd kiosk). Remaining work is VM/install validation, authoring docs, and optional native plugins."
todos:
  - id: scaffold-host
    content: Scaffold Rust OpenGL host app with fullscreen window and render loop
    status: pending
  - id: shader-module-loader
    content: Implement module.toml + GLSL compute/fragment loading with standard uniforms
    status: pending
  - id: example-modules
    content: Add Mandelbrot, Lorenz attractor, and Gray-Scott shader-pack examples
    status: pending
  - id: input-switching
    content: Wire keyboard next/prev/reload; LIRC still optional/not done
    status: pending
  - id: systemd-kiosk
    content: Add art-display.service, blanking scripts, install-kiosk.sh, and --kiosk host flag
    status: pending
  - id: module-authoring-docs
    content: Document module.toml schema and shader authoring guide
    status: pending
  - id: dev-native-mode
    content: Windowed-by-default plus --fullscreen/--kiosk/--modules-path/--resolution
    status: pending
  - id: vm-test-env
    content: Add QEMU/KVM + Vagrant (or virt-install) config to provision Debian kiosk VM and run install script
    status: pending
  - id: dev-docs
    content: Document native vs VM testing workflows (KIOSK.md exists; DEVELOPMENT.md does not)
    status: pending
  - id: phase4-validate-install
    content: Test install-kiosk.sh on clean VM then physical unit; document NVIDIA driver install
    status: pending
isProject: false
---

# GPU Art Display Stack — work snapshot (2026-07-22)

This is an **updated snapshot** of the original stack plan after Phases 1–3 were implemented in [graphic-art-display](/home/carter/Code/graphic-art-display). Architecture and key decisions are unchanged; phases below record what shipped versus what is still open.

**Hardware target:** full PC, GTX 1660 Super. **Stack:** Debian 12 minimal + proprietary NVIDIA + X11 + OpenGL 4.6 + Rust host + GLSL compute shader packs. No launcher UI.

---

## Current repo layout (actual)

```
graphic-art-display/
  host/                         # Rust OpenGL 4.6 runtime (glutin/winit/glow)
    src/main.rs args.rs module.rs renderer.rs
    shaders/display.vert display.frag
  modules/
    lorenz-attractor/           # state = trail
    mandelbrot/                 # no persistent state
    reaction-diffusion/         # state = ping-pong (Gray-Scott)
  deploy/
    art-display.service
    xsession.sh                 # xset blanking off + art-display --kiosk
    console-blanking-off.sh     # setterm on tty1
    env.example
  scripts/
    dev-run.sh
    install-kiosk.sh            # landed in Phase 3 (originally Phase 4)
    kiosk-smoke-test.sh
  docs/KIOSK.md
```

Not present yet: `dev/Vagrantfile`, `vm-up.sh`, `MODULE_AUTHORING.md`, `DEVELOPMENT.md`, LIRC example, native plugins.

---

## Phase status

### Phase 0 — Dev environment — **partial (native done; VM skipped)**

Shipped:
- Windowed by default (not `--windowed`; original flag inverted)
- [`scripts/dev-run.sh`](scripts/dev-run.sh): `cargo run -- --modules-path ../modules --resolution 1280x720`
- CLI: `--modules-path`, `--resolution`, `--fullscreen`, `--kiosk`

Not shipped (optional unless you want off-hardware kiosk testing):
- Vagrant/libvirt VM, `vm-up.sh`, `vm-smoke-test.sh`
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md)

CLI deviation from original plan: `--windowed` / `--no-watchdog` were **not** used. Production uses `--kiosk` (fullscreen + ignore Esc/close).

### Phase 1 — Prove the GPU loop — **done**

- Rust host, OpenGL 4.6 core, compute dispatch → RGBA8 texture → fullscreen blit
- Explicit GL cleanup on exit (avoids segfault)
- Hot-reload of current module shader (`r`)
- Window resize recreates output (and state) textures

### Phase 2 — Module system — **done** (with extras vs original)

Shipped:
- [`host/src/module.rs`](host/src/module.rs): discover folders with `module.toml`, sort by directory name
- Compute (and fragment kind in loader) + workgroup
- **State modes** (not in original `module.toml` sketch): `none` | `ping-pong` | `trail` (RGBA32F; trail stores integrator at texel `(0,0)`)
- Keyboard: `→`/`n` next, `←`/`p` prev, `r` reload (resets animation + seed), `Esc` quit (disabled in `--kiosk`)
- Three modules: Mandelbrot, Lorenz, Gray-Scott

Standard UBO (binding 0), **as implemented**:

```glsl
layout(std140, binding = 0) uniform FrameData {
    float time;
    float deltaTime;
    uint frame;
    uint seed;       // rolled on switch / r / resize reset
    vec2 resolution;
    vec2 _pad1;
} uFrame;
```

**Deviations from original module contract:**
- No `parameters` array / on-screen param keys yet
- No GLSL `#include` / `common.glsl`
- Ping-pong modules run **3 compute dispatches per frame** after frame 0 (speed vs stability)

**Module behaviour (tuned in development):**

| Module | Randomization on seed roll | Notes |
|--------|----------------------------|--------|
| Mandelbrot | Curated zoom centres (7) | Palette/time zoom unchanged |
| Lorenz | `rho` in 26–35; **fixed** start `(0.1,1,1)` | Rainbow hue `time * 0.06`; `stepsPerFrame = 10` in shader; thinner splat |
| Gray-Scott | F/k from 6 morphologies; seed layout | Low-feed morphologies: 1–2 seeds, stronger `v`; coral/stripes/worms: 3–6 seeds. Host ping-pong ×3/frame |

Resize on stateful modules resets animation (`frame=0`) so shaders re-init.

### Phase 3 — Kiosk hardening — **done in-repo; not validated on VM/hardware**

Shipped:
- [`deploy/art-display.service`](deploy/art-display.service): xinit on tty1, `Restart=always`, journald, conflicts with `getty@tty1`
- Blanking: `xset` in xsession, `setterm` pre-start
- [`scripts/install-kiosk.sh`](scripts/install-kiosk.sh): apt Xorg/xinit, cargo release, user `artdisplay`, `/opt/art-display/modules`, `/etc/art-display/env`
- [`docs/KIOSK.md`](docs/KIOSK.md)
- Host `--kiosk`

Not done in this phase (was listed originally): **integration VM validation**. NVIDIA driver install remains a separate hardware step.

### Phase 4 — OS image / install script — **partially pulled into Phase 3**

`install-kiosk.sh` exists but has **not** been run on a clean Debian VM or the physical unit. Still open:
- VM smoke: service after reboot, kill -9 restart, no DE
- Document NVIDIA proprietary install for the display PC
- Optional read-only root / A/B updates

### Phase 5 — Native plugins — **not started** (optional)

---

## Remaining work (next computer)

1. **Phase 4 validation** — run `install-kiosk.sh` on VM and/or physical GTX 1660 Super; confirm boot-to-art and journal logs.
2. **Docs** — `MODULE_AUTHORING.md` (manifest + UBO + `state` modes) and `DEVELOPMENT.md`.
3. **Optional Phase 0 VM** — only if you want kiosk testing without the physical box.
4. **Optional** — LIRC, module `parameters`, GLSL includes, native `.so` ABI.

## How to run (dev)

```bash
./scripts/dev-run.sh
# extra flags: --fullscreen   or   --kiosk
```

## How to install (kiosk)

```bash
sudo ./scripts/install-kiosk.sh --resolution 1920x1080
journalctl -u art-display -f
```
