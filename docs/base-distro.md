# Pancake OS Base Distro

Pancake is its own desktop environment and should not be branded or structured as Abora.

The default live ISO base is Debian Live. The legacy Arch Linux archiso profile is still present under `iso/profile` as a fallback while the Debian image is being brought up.

## Default ISO

Build the Debian Live ISO:

```sh
sudo make iso
```

On an Arch/pacman host, install a container runtime first:

```sh
sudo pacman -S docker
sudo systemctl enable --now docker
```

The build runs Debian's `live-build` inside a Debian container. Do not install `live-build` on the Arch host.

The container installs a current Rust toolchain with `rustup`, then builds Pancake inside Debian before staging it into the live root at `/usr/local/bin/pancake`. This avoids both Rust version drift from Debian's packaged compiler and glibc/library mismatches from copying an Arch-built binary into a Debian image.

The Debian ISO is written to:

```sh
iso/out/debian/pancake-debian-latest.iso
```

Boot this file. Older files named like `iso/out/pancake-YYYY.MM.DD-x86_64.iso` are legacy Arch ISOs.

## Legacy Arch ISO

The old Arch-based image can still be built explicitly:

```sh
sudo make iso-arch
```

Keep this only as a temporary comparison target while the Debian base is tested.

Legacy Arch ISO output goes under:

```sh
iso/out/arch/
```

## Live Session

The Debian image boots to a root shell on TTY1.

Use:

```sh
start-pancake
```

to launch Pancake without an automatic terminal client.

Use:

```sh
start-pancake-terminal
```

to launch Pancake and request a startup `foot` terminal.

Logs are written to `/root/pancake.log`.
