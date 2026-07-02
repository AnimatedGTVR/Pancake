# Pancake

**The Sweetest, Smoothest Desktop Environment.**

Pancake is a Wayland compositor written in Rust. Its defining visual identity is *Aero-COSMIC* — a modern take on the Windows Vista/7 Aero Glass aesthetic, where panels, window decorations, and surfaces use real-time frosted-glass blur with a soft blue-white tint.

Built on [Smithay](https://github.com/Smithay/smithay). Ships as a standalone Linux distribution on a Debian Live base.

---

## Status

Early development. The compositor handles:

- XDG-shell toplevels (application windows) — map, raise, maximize, fullscreen, close
- Popup tracking (tooltips, menus, dropdowns)
- Keyboard and pointer input with click-to-focus
- Window cycling with Super+Tab
- XWayland for legacy X11 applications
- Cascading window placement
- Aero blur pipeline (GLSL shaders written, GPU FBO integration in progress)

Both a udev/DRM backend (real hardware) and a Winit backend (nested, for development) are implemented.

---

## Architecture

```
src/
├── main.rs              Entry point, CLI args (--winit, --tty)
├── state.rs             PancakeState — all compositor-wide state in one struct
├── backend/
│   ├── winit.rs         Nested backend: run inside an existing compositor
│   └── udev.rs          Native backend: DRM/KMS + libinput + libseat
├── handlers/
│   ├── compositor.rs    wl_compositor — surface commit, buffer management
│   ├── input.rs         Keyboard, pointer, and axis routing; keybindings
│   ├── xdg_shell.rs     XDG-shell — toplevels, popups, maximize, fullscreen
│   └── xwayland.rs      XWayland integration
├── shell/
│   └── layout.rs        Window placement (cascade; tiling engine planned)
└── render/
    └── aero.rs          Aero frosted-glass blur pipeline (dual-kawase GLSL)
```

### Aero blur pipeline

Every translucent surface — panels, window decorations, popups — will show a real-time blurred copy of whatever is behind them. The technique is *dual-kawase*, the same algorithm used by KWin and Hyprland.

```
1. Render scene to OFFSCREEN_FBO (full res)
2. Downsample  → BLUR_FBO_A (½ res)
3. Dual-Kawase H-pass → BLUR_FBO_B
4. Dual-Kawase V-pass → BLUR_FBO_A
5. Upsample + blue-white tint → final output
```

The GLSL shaders are in [`src/render/aero.rs`](src/render/aero.rs). GPU FBO wiring is the next rendering milestone.

---

## Building

### Dependencies

```sh
# Arch Linux
sudo pacman -S rust wayland libinput libdrm libgbm libseat libxkbcommon

# Debian / Ubuntu
sudo apt install build-essential pkg-config rustup \
    libwayland-dev libinput-dev libdrm-dev libgbm-dev \
    libegl-dev libgles-dev libseat-dev libxkbcommon-dev \
    libudev-dev libpixman-1-dev libsystemd-dev
```

Verify all libraries are present:

```sh
make deps
```

### Compile

```sh
make              # release build (default)
make PROFILE=dev  # debug build
make check        # type-check only (fast)
```

---

## Running

### Nested (development)

Run Pancake as a window inside your existing desktop. No GPU access needed.

```sh
make run-winit
# or
WAYLAND_DEBUG=0 ./target/release/pancake --winit
```

Then launch a Wayland application into it:

```sh
WAYLAND_DISPLAY=wayland-pancake foot
```

### Bare hardware (TTY)

Requires a free TTY, seatd or logind.

```sh
# Switch to TTY2, ensure seatd is running, then:
make run
# or
./target/release/pancake
```

Force a specific TTY:

```sh
./target/release/pancake --tty 2
```

Environment variables:

| Variable | Default | Description |
|---|---|---|
| `PANCAKE_TERMINAL` | `foot` | Terminal launched by Super+T |
| `WAYLAND_DEBUG` | (unset) | Set to `1` for verbose Wayland protocol logging |

---

## Keybindings

| Binding | Action |
|---|---|
| **Super+Q** | Close the focused window |
| **Super+Escape** | Quit Pancake |
| **Super+T** | Launch terminal (`$PANCAKE_TERMINAL`, default: `foot`) |
| **Super+Tab** | Cycle window focus |

---

## ISO / Distribution

Pancake ships as its own Linux distribution on a **Debian Live** base. The legacy Arch Linux profile is kept under `iso/profile/` as a fallback.

### Build the Debian ISO

Requires Docker or Podman. The build runs entirely inside a Debian container — no host pollution.

```sh
# One-time: install a container runtime (Arch hosts)
sudo pacman -S docker
sudo systemctl enable --now docker

# Build
sudo make iso

# Output
iso/out/debian/pancake-debian-YYYY.MM.DD-amd64.iso
iso/out/debian/pancake-debian-latest.iso   ← symlink to latest
```

The container installs a current Rust toolchain with `rustup`, builds Pancake inside Debian (so the binary matches the glibc/library versions), then runs `lb build`.

### Boot the live image

```sh
# QEMU/KVM (recommended for testing)
qemu-system-x86_64 -enable-kvm -m 2G -cdrom iso/out/debian/pancake-debian-latest.iso

# Real hardware
sudo dd if=iso/out/debian/pancake-debian-latest.iso of=/dev/sdX bs=4M status=progress oflag=sync
```

**VirtualBox:** Set the graphics controller to **VMSVGA** (not VBoxVGA) and allocate at least 128 MB video memory. If you see only a blank blue screen, wait 5 seconds for the GRUB menu to auto-select, or press Enter to boot immediately. If it stays blue, select **"Pancake Live (safe mode / VirtualBox)"** from the GRUB menu.

Log in as `root` — no password, just press Enter. The MOTD explains the commands.

```sh
start-pancake             # compositor only (blue background, use Super+T for terminal)
start-pancake-terminal    # compositor + launches foot automatically after socket is ready
```

Compositor logs: `/root/pancake.log`

### Legacy Arch ISO

```sh
sudo pacman -S archiso
sudo make iso-arch
# Output: iso/out/arch/pancake-YYYY.MM.DD-x86_64.iso
```

---

## Make targets

```
make [all]          Build release binary (default)
make build          Same as above
make PROFILE=dev    Build debug binary
make check          Type-check only (no binary)
make test           Run test suite
make fmt            Format source with rustfmt
make lint           Clippy with -D warnings
make deps           Verify system libraries are present
make run-winit      Run nested inside an existing compositor
make run            Run on bare hardware
make install        Install to PREFIX (default /usr/local)
make uninstall      Remove installed binary
make clean          Remove build artifacts
make iso            Build Debian Live ISO (Docker/Podman)
make iso-arch       Build legacy Arch Linux live ISO
make iso-info       Show ISO file details
make iso-clean      Remove ISO work and output directories
```

---

## License

GPL-3.0 — see [Cargo.toml](Cargo.toml).
