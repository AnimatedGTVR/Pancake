#!/usr/bin/env bash
# Build a Pancake Ubuntu Live ISO inside a container.
#
# Pancake now runs on Hyprland (built from source inside the chroot by
# config/hooks/normal/030-build-hyprland.hook.chroot) instead of the old
# custom Rust/Smithay compositor, so there is no Cargo build step here
# anymore — the old compositor's source is archived, not part of the ISO.
#
# Speed optimisations vs. the naive approach:
#   • Mounts the host cargo registry + build cache so crates aren't re-fetched
#   • Passes an apt-cacher-ng proxy if one is detected on the host
#   • Supports PANCAKE_ISO_COMPRESSION=none for faster local test images
#   • Uses --apt-recommends false to avoid large optional dependency trees
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

ISO_OUT="${ISO_OUT:-iso/out/ubuntu}"
UBUNTU_PROFILE="distro/ubuntu-live"
UBUNTU_WORK="/tmp/pancake-ubuntu-live"

echo "==> Installing build deps..."
apt-get update -qq
apt-get install -y --no-install-recommends \
    ca-certificates curl file live-build \
    syslinux-utils isolinux syslinux-common xorriso

# ── Stage files ───────────────────────────────────────────────────────────────
rm -rf "$UBUNTU_WORK"
mkdir -p "$UBUNTU_WORK" "$ISO_OUT"
cp -a "$UBUNTU_PROFILE/." "$UBUNTU_WORK/"
rm -rf \
    "$UBUNTU_WORK/.build" \
    "$UBUNTU_WORK/cache" \
    "$UBUNTU_WORK/chroot" \
    "$UBUNTU_WORK/binary" \
    "$UBUNTU_WORK/.stage" \
    "$UBUNTU_WORK/config/binary" \
    "$UBUNTU_WORK/config/bootstrap" \
    "$UBUNTU_WORK/config/chroot" \
    "$UBUNTU_WORK/config/common" \
    "$UBUNTU_WORK/config/source"

if [ -d "$UBUNTU_WORK/config/hooks/normal" ]; then
    for hook in "$UBUNTU_WORK"/config/hooks/normal/*.hook.chroot; do
        [ -e "$hook" ] || continue
        install -Dm755 "$hook" "$UBUNTU_WORK/config/hooks/$(basename "$hook" .hook.chroot).chroot"
    done
    for hook in "$UBUNTU_WORK"/config/hooks/normal/*.hook.binary; do
        [ -e "$hook" ] || continue
        install -Dm755 "$hook" "$UBUNTU_WORK/config/hooks/$(basename "$hook" .hook.binary).binary"
    done
fi

# ── live-build workarounds for Ubuntu Noble ───────────────────────────────────
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
done

ISOLINUX_BOOTLOADER="/usr/share/live/build/bootloaders/isolinux"
if [ -d "$ISOLINUX_BOOTLOADER" ]; then
    ln -sfn /usr/lib/ISOLINUX/isolinux.bin "$ISOLINUX_BOOTLOADER/isolinux.bin"
    for m in ldlinux.c32 libcom32.c32 libutil.c32 menu.c32 vesamenu.c32; do
        ln -sfn "/usr/lib/syslinux/modules/bios/$m" "$ISOLINUX_BOOTLOADER/$m"
    done
    rm -f "$ISOLINUX_BOOTLOADER/splash.svg.in"
fi

for template in \
    "$ISOLINUX_BOOTLOADER/live.cfg.in" \
    /usr/share/live/build/bootloaders/isolinux/live.cfg.in
do
    [ -f "$template" ] || continue
    sed -i \
        -e 's/\bboot=live config\b/boot=casper/g' \
        "$template"
done

for f in /usr/lib/live/build/lb_binary_disk; do
    [ -f "$f" ] && \
        sed -i 's#unmkinitramfs "../../${INITRD}" \.#unmkinitramfs "../../${INITRD}" . || true#' "$f"
done

for f in /usr/lib/live/build/lb_binary_iso; do
    [ -f "$f" ] && \
        sed -i 's#Check_package chroot/usr/bin/isohybrid syslinux#Check_package chroot/usr/bin/isohybrid syslinux-utils#' "$f"
done

for f in /usr/lib/live/build/lb_chroot_hacks; do
    [ -f "$f" ] && \
        sed -i "s#find chroot/boot -name 'initrd\\*' -print0 | xargs -r -0 chmod go+r#find chroot/boot -name 'initrd\\*' -type f -print0 | xargs -r -0 chmod go+r#" "$f"
done

# ── Build ISO ─────────────────────────────────────────────────────────────────
echo "==> Running lb build (full log → /tmp/pancake-lb.log)..."
cd "$UBUNTU_WORK"
./auto/config
lb build 2>&1 | tee /tmp/pancake-lb.log || {
    echo ""
    echo "==> lb build FAILED. Last 60 lines of log:"
    tail -60 /tmp/pancake-lb.log
    cp /tmp/pancake-lb.log /work/pancake-lb-fail.log
    echo "==> Full log saved to pancake-lb-fail.log in the repo root."
    exit 1
}

cd /work
iso_name="pancake-ubuntu-$(date +%Y.%m.%d)-amd64.iso"
for candidate in \
    "$UBUNTU_WORK/live-image-amd64.hybrid.iso" \
    "$UBUNTU_WORK/binary.hybrid.iso"
do
    if [ -f "$candidate" ]; then
        cp "$candidate" "$ISO_OUT/$iso_name"
        ln -sfn "$iso_name" "$ISO_OUT/pancake-ubuntu-latest.iso"
        if command -v xorriso >/dev/null 2>&1; then
            verify_dir="$(mktemp -d)"
            for path in /casper/vmlinuz /casper/initrd.img /casper/filesystem.squashfs; do
                if ! xorriso -indev "$ISO_OUT/$iso_name" -osirrox on \
                    -extract "$path" "$verify_dir/$(basename "$path")" >/dev/null 2>&1; then
                    echo "error: ISO is missing $path" >&2
                    rm -rf "$verify_dir"
                    exit 1
                fi
                if [ ! -s "$verify_dir/$(basename "$path")" ]; then
                    echo "error: ISO has an empty $path" >&2
                    rm -rf "$verify_dir"
                    exit 1
                fi
            done
            rm -rf "$verify_dir"
        fi
        echo ""
        echo "==> ISO ready: $ISO_OUT/$iso_name"
        ls -lh "$ISO_OUT/$iso_name"
        exit 0
    fi
done

echo "error: live-build did not produce a known ISO file" >&2
find "$UBUNTU_WORK" -maxdepth 1 -type f -name '*.iso' >&2
exit 1
