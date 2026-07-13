#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

ISO_OUT="${ISO_OUT:-iso/out/debian}"
DEBIAN_PROFILE="distro/debian-live"
DEBIAN_WORK="/tmp/pancake-debian-live"

# Pancake now runs on Hyprland (installed from trixie-backports, see
# config/archives/trixie-backports.list.chroot) instead of the old custom
# Rust/Smithay compositor, so there is no Cargo build step here anymore.
echo "==> Installing build deps..."
apt-get update -qq
apt-get install -y --no-install-recommends \
    ca-certificates curl live-build

# ── Stage ─────────────────────────────────────────────────────────────────────
rm -rf "$DEBIAN_WORK"
mkdir -p "$DEBIAN_WORK" "$ISO_OUT"
cp -a "$DEBIAN_PROFILE/." "$DEBIAN_WORK/"

# ── Build ─────────────────────────────────────────────────────────────────────
echo "==> Running lb build..."
cd "$DEBIAN_WORK"
lb build

cd /work
iso_name="pancake-debian-$(date +%Y.%m.%d)-amd64.iso"
for candidate in \
    "$DEBIAN_WORK/live-image-amd64.hybrid.iso" \
    "$DEBIAN_WORK/binary.hybrid.iso"
do
    if [ -f "$candidate" ]; then
        cp "$candidate" "$ISO_OUT/$iso_name"
        ln -sfn "$iso_name" "$ISO_OUT/pancake-debian-latest.iso"
        echo ""
        echo "==> ISO ready: $ISO_OUT/$iso_name"
        ls -lh "$ISO_OUT/$iso_name"
        exit 0
    fi
done

echo "error: live-build produced no ISO" >&2
find "$DEBIAN_WORK" -maxdepth 1 -name '*.iso' >&2
exit 1
