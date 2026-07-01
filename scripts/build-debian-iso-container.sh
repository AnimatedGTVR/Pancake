#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

ISO_OUT="${ISO_OUT:-iso/out/debian}"
DEBIAN_PROFILE="distro/debian-live"
DEBIAN_WORK="/tmp/pancake-debian-live"

echo "==> Installing Debian build dependencies inside the container..."
apt-get update
apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    live-build \
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

echo "==> Building Pancake inside Debian so the binary matches the Debian base..."
cargo build --release

echo "==> Preparing Debian Live profile..."
rm -rf "$DEBIAN_WORK"
mkdir -p "$DEBIAN_WORK" "$ISO_OUT"
cp -a "$DEBIAN_PROFILE/." "$DEBIAN_WORK/"

echo "==> Staging Pancake binary into Debian Live profile..."
install -Dm755 target/release/pancake \
    "$DEBIAN_WORK/config/includes.chroot/usr/local/bin/pancake"

echo "==> Building Debian Live ISO..."
cd "$DEBIAN_WORK"
lb build

cd /work
iso_name="pancake-debian-$(date +%Y.%m.%d)-amd64.iso"
cp "$DEBIAN_WORK/live-image-amd64.hybrid.iso" "$ISO_OUT/$iso_name"
ln -sfn "$iso_name" "$ISO_OUT/pancake-debian-latest.iso"

echo ""
echo "==> ISO ready:"
ls -lh "$ISO_OUT/$iso_name"
ls -lh "$ISO_OUT/pancake-debian-latest.iso"
