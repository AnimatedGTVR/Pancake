#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

ISO_OUT="${ISO_OUT:-iso/out/debian}"
DEBIAN_PROFILE="distro/debian-live"
DEBIAN_WORK="/tmp/pancake-debian-live"
PANCAKE_BIN="target/release/pancake"

echo "==> Installing build deps..."
apt-get update -qq
apt-get install -y --no-install-recommends \
    ca-certificates curl live-build \
    build-essential pkg-config \
    libwayland-dev libinput-dev libdrm-dev libgbm-dev \
    libegl-dev libgles-dev libseat-dev libxkbcommon-dev \
    libudev-dev libpixman-1-dev libsystemd-dev

# ── Binary: always compile inside the container ───────────────────────────────
# Reusing a host binary risks GLIBC version mismatches (e.g. host Arch/glibc 2.43
# vs Debian Trixie glibc 2.40). Build here to guarantee compat.
echo "==> Compiling Pancake inside Debian container (glibc compatibility)..."
if [ ! -f "$HOME/.cargo/bin/cargo" ]; then
    curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain stable
fi
export PATH="$HOME/.cargo/bin:$PATH"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$(nproc)}"
cargo build --release -j "$CARGO_BUILD_JOBS"

# ── Stage ─────────────────────────────────────────────────────────────────────
rm -rf "$DEBIAN_WORK"
mkdir -p "$DEBIAN_WORK" "$ISO_OUT"
cp -a "$DEBIAN_PROFILE/." "$DEBIAN_WORK/"
install -Dm755 "$PANCAKE_BIN" \
    "$DEBIAN_WORK/config/includes.chroot/usr/local/bin/pancake"

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
