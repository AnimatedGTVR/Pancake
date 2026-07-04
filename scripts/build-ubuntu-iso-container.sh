#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

ISO_OUT="${ISO_OUT:-iso/out/ubuntu}"
UBUNTU_PROFILE="distro/ubuntu-live"
UBUNTU_WORK="/tmp/pancake-ubuntu-live"

echo "==> Installing Ubuntu build dependencies inside the container..."
apt-get update
apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    live-build \
    syslinux-utils \
    build-essential \
    pkg-config \
    libwayland-dev \
    libinput-dev \
    libdrm-dev \
    libgbm-dev \
    libegl-dev \
    libgles-dev \
    libseat-dev \
    libxkbcommon-dev \
    libudev-dev \
    libpixman-1-dev \
    libsystemd-dev

echo "==> Installing current Rust toolchain inside the container..."
curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable
export PATH="/root/.cargo/bin:${PATH}"
rustc --version
cargo --version

echo "==> Building Pancake inside Ubuntu so the binary matches the Ubuntu base..."
cargo build --release

echo "==> Preparing Ubuntu Live profile..."
rm -rf "$UBUNTU_WORK"
mkdir -p "$UBUNTU_WORK" "$ISO_OUT"
cp -a "$UBUNTU_PROFILE/." "$UBUNTU_WORK/"

echo "==> Staging Pancake binary into Ubuntu Live profile..."
install -Dm755 target/release/pancake \
    "$UBUNTU_WORK/config/includes.chroot/usr/local/bin/pancake"

echo "==> Working around Ubuntu Noble live-build syslinux theme bug..."
# lb_binary_syslinux tries to use Ubuntu's old gfxboot overlay, but
# gfxboot-theme-ubuntu is not available in Noble's repositories.
for SYSLINUX_SCRIPT in \
    /usr/lib/live/build/lb_binary_syslinux \
    /usr/lib/live/build/binary_syslinux
do
    [ -f "$SYSLINUX_SCRIPT" ] || continue

    sed -i \
        -e '/Check_package chroot\/usr\/bin\/syslinux syslinux/a\
			Check_package chroot/usr/lib/ISOLINUX/isolinux.bin isolinux' \
        -e '/Check_package .*gfxboot-theme-ubuntu/d' \
        -e '/tar xfz .*gfxboot-theme-ubuntu\/bootlogo\.tar\.gz/d' \
        -e '/# Hack around the removal of support in gfxboot/,/rm -rf "$tmpdir"/d' \
        -e 's|mv binary/live/vmlinuz-\* binary/live/vmlinuz|if ls binary/casper/vmlinuz-* >/dev/null 2>\&1; then mv binary/casper/vmlinuz-* binary/casper/vmlinuz; elif ls binary/live/vmlinuz-* >/dev/null 2>\&1; then mv binary/live/vmlinuz-* binary/live/vmlinuz; fi|' \
        -e 's|mv binary/live/initrd.img-\* binary/live/initrd.img|if ls binary/casper/initrd.img-* >/dev/null 2>\&1; then mv binary/casper/initrd.img-* binary/casper/initrd.img; elif ls binary/live/initrd.img-* >/dev/null 2>\&1; then mv binary/live/initrd.img-* binary/live/initrd.img; fi|' \
        -e 's#@KERNEL@|/live/vmlinuz#@KERNEL@|/casper/vmlinuz#' \
        -e 's#@INITRD@|/live/initrd.img#@INITRD@|/casper/initrd.img#' \
        "$SYSLINUX_SCRIPT"
    echo "==> Patched $SYSLINUX_SCRIPT (removed missing Ubuntu gfxboot theme)"
done

ISOLINUX_BOOTLOADER="/usr/share/live/build/bootloaders/isolinux"
if [ -d "$ISOLINUX_BOOTLOADER" ]; then
    ln -sfn /usr/lib/ISOLINUX/isolinux.bin "$ISOLINUX_BOOTLOADER/isolinux.bin"
    for module in \
        ldlinux.c32 \
        libcom32.c32 \
        libutil.c32 \
        menu.c32 \
        vesamenu.c32
    do
        ln -sfn "/usr/lib/syslinux/modules/bios/$module" "$ISOLINUX_BOOTLOADER/$module"
    done
    rm -f "$ISOLINUX_BOOTLOADER/splash.svg.in"
    echo "==> Patched $ISOLINUX_BOOTLOADER symlinks for Noble's syslinux layout"
fi

for template in \
    "$ISOLINUX_BOOTLOADER/live.cfg.in" \
    /usr/share/live/build/bootloaders/isolinux/live.cfg.in
do
    [ -f "$template" ] || continue

    sed -i \
        -e 's/\bboot=live config\b/boot=casper/g' \
        -e '/label .*failsafe/,/^$/s/[[:space:]]append[[:space:]]/ append pancake.debug_shell=1 /' \
        "$template"
    echo "==> Patched $template to use Ubuntu casper boot args"
done

DISK_SCRIPT="/usr/lib/live/build/lb_binary_disk"
if [ -f "$DISK_SCRIPT" ]; then
    sed -i \
        -e 's#^\(\s*\)unmkinitramfs "../../${INITRD}" \.$#\1unmkinitramfs "../../${INITRD}" . || true#' \
        "$DISK_SCRIPT"
    echo "==> Patched $DISK_SCRIPT to tolerate optional casper UUID extraction warnings"
fi

ISO_SCRIPT="/usr/lib/live/build/lb_binary_iso"
if [ -f "$ISO_SCRIPT" ]; then
    sed -i \
        -e 's#Check_package chroot/usr/bin/isohybrid syslinux#Check_package chroot/usr/bin/isohybrid syslinux-utils#' \
        "$ISO_SCRIPT"
    echo "==> Patched $ISO_SCRIPT to install Noble's isohybrid package"
fi

echo "==> Building Ubuntu Live ISO..."
cd "$UBUNTU_WORK"
lb build

cd /work
iso_name="pancake-ubuntu-$(date +%Y.%m.%d)-amd64.iso"
if [ -f "$UBUNTU_WORK/live-image-amd64.hybrid.iso" ]; then
    cp "$UBUNTU_WORK/live-image-amd64.hybrid.iso" "$ISO_OUT/$iso_name"
elif [ -f "$UBUNTU_WORK/binary.hybrid.iso" ]; then
    cp "$UBUNTU_WORK/binary.hybrid.iso" "$ISO_OUT/$iso_name"
else
    echo "error: live-build did not produce a known ISO output file" >&2
    find "$UBUNTU_WORK" -maxdepth 1 -type f -name '*.iso' -print >&2
    exit 1
fi
ln -sfn "$iso_name" "$ISO_OUT/pancake-ubuntu-latest.iso"

echo ""
echo "==> ISO ready:"
ls -lh "$ISO_OUT/$iso_name"
ls -lh "$ISO_OUT/pancake-ubuntu-latest.iso"
