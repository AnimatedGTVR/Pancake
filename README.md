<<p align="center">
  <img src="./pancakes.png" alt="Pancake Logo" width="240">
</p>

<h1 align="center">Pancake</h1>

<p align="center">
  <b>The sweetest, smoothest desktop environment.</b>
</p>

<p align="center">
  <i>A Wayland compositor with Aero-COSMIC glass vibes.</i>
</p>

---

Pancake is a Wayland compositor written in Rust and built on [Smithay](https://github.com/Smithay/smithay).

Its main visual style is **Aero-COSMIC** — a mix of classic Windows Vista/7 Aero Glass and modern Linux desktop design. The goal is a smooth, glassy, lightweight desktop with frosted blur, soft blue-white tinting, and a clean shell experience.

> [!WARNING]
> Pancake is still in early development and does not currently work reliably in my VMs.
> This will be fixed before the first official Pancake release, **v1**.
>
> Pancake is also **not part of the Abora OS ecosystem**. It is its own separate project.

---

## Status

Pancake currently supports:

* XDG-shell application windows
* Window mapping, raising, maximize, fullscreen, and close
* Popup tracking
* Keyboard and pointer input
* Click-to-focus
* `Super+Tab` window cycling
* XWayland support
* Basic cascading window placement
* Winit backend for nested testing
* DRM/KMS backend for real hardware
* Early Aero blur shader work

The compositor is usable for testing, but not ready as a daily desktop.

---

## Building

### Arch Linux

```sh
sudo pacman -S rust wayland libinput libdrm libgbm libseat libxkbcommon
```

### Debian / Ubuntu

```sh
sudo apt install build-essential pkg-config rustup \
    libwayland-dev libinput-dev libdrm-dev libgbm-dev \
    libegl-dev libgles-dev libseat-dev libxkbcommon-dev \
    libudev-dev libpixman-1-dev libsystemd-dev
```

Build Pancake:

```sh
make
```

Debug build:

```sh
make PROFILE=dev
```

Check only:

```sh
make check
```

---

## Running

### Nested

Run Pancake inside your current desktop:

```sh
make run-winit
```

Launch an app inside it:

```sh
WAYLAND_DISPLAY=wayland-pancake foot
```

### Native

Run Pancake from a TTY:

```sh
make run
```

Or:

```sh
./target/release/pancake
```

---

## Keybindings

| Binding        | Action               |
| -------------- | -------------------- |
| `Super+T`      | Open terminal        |
| `Super+Q`      | Close focused window |
| `Super+Tab`    | Cycle windows        |
| `Super+Escape` | Quit Pancake         |

---

## ISO

Pancake ships as its own Debian Live-based Linux distribution.

Build the ISO:

```sh
sudo make iso
```

Output:

```text
iso/out/debian/pancake-debian-latest.iso
```

Run in QEMU:

```sh
qemu-system-x86_64 -enable-kvm -m 2G -cdrom iso/out/debian/pancake-debian-latest.iso
```

Live ISO login:

```text
user: root
password: none
```

Start Pancake:

```sh
start-pancake
```

Or start Pancake with a terminal:

```sh
start-pancake-terminal
```

Logs:

```text
/root/pancake.log
```

---

## Make Targets

```text
make              Build release binary
make check        Type-check only
make test         Run tests
make fmt          Format code
make lint         Run Clippy
make run-winit    Run nested backend
make run          Run native backend
make iso          Build Debian ISO
make clean        Clean build files
```

---

## License

GPL-3.0
