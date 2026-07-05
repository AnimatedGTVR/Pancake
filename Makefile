PREFIX   ?= /usr/local
BINDIR   := $(PREFIX)/bin
CARGO    := cargo
DESTDIR  ?=

# Build profile: release by default, override with PROFILE=dev
PROFILE  ?= release
ifeq ($(PROFILE),release)
  CARGO_FLAGS := --release
  TARGET_DIR  := target/release
else
  CARGO_FLAGS :=
  TARGET_DIR  := target/debug
endif

# Parallel jobs: default to nproc, but cap at 4 on weak machines.
# Override: JOBS=2 make build
NPROC := $(shell nproc 2>/dev/null || echo 4)
JOBS  ?= $(NPROC)

ISO_OUT      := iso/out
UBUNTU_ISO_OUT := $(ISO_OUT)/ubuntu
DEBIAN_ISO_OUT := $(ISO_OUT)/debian
ARCH_ISO_OUT   := $(ISO_OUT)/arch
UBUNTU_PROFILE := distro/ubuntu-live
UBUNTU_WORK    := /tmp/pancake-ubuntu-live
DEBIAN_PROFILE := distro/debian-live
DEBIAN_WORK    := /tmp/pancake-debian-live
ISO_PROFILE    := iso/profile

CONTAINER_RUNTIME ?= $(shell command -v podman 2>/dev/null || command -v docker 2>/dev/null)

# Cargo cache directories — mount from host so the container doesn't re-download
CARGO_REGISTRY  := $(HOME)/.cargo/registry
CARGO_GIT       := $(HOME)/.cargo/git
CARGO_TARGET    := $(CURDIR)/target

# Detect a local apt-cacher-ng instance (localhost:3142)
APT_PROXY_RUNNING := $(shell curl -sf --max-time 1 http://localhost:3142/acng-report.html >/dev/null 2>&1 && echo yes || echo no)
ifeq ($(APT_PROXY_RUNNING),yes)
  APT_PROXY_ENV := -e APT_HTTP_PROXY=http://172.17.0.1:3142
else
  APT_PROXY_ENV :=
endif

.PHONY: all build check clean install uninstall run run-winit fmt lint deps \
        iso iso-ubuntu iso-ubuntu-fast iso-debian iso-arch iso-info iso-clean \
        qemu-ubuntu qemu-debian cache-proxy help

## Default target
all: build

## Compile (release)
build:
	$(CARGO) build $(CARGO_FLAGS) -j $(JOBS)

## Type-check without producing a binary (fast)
check:
	$(CARGO) check

## Run tests
test:
	$(CARGO) test $(CARGO_FLAGS)

## Format source
fmt:
	$(CARGO) fmt

## Lint with clippy
lint:
	$(CARGO) clippy -- -D warnings

## Verify system dependencies are present
deps:
	@echo "Checking system dependencies..."
	@pkg-config --exists wayland-server  && echo "  wayland-server  OK" || (echo "  wayland-server  MISSING"; exit 1)
	@pkg-config --exists libinput        && echo "  libinput        OK" || (echo "  libinput        MISSING"; exit 1)
	@pkg-config --exists libdrm          && echo "  libdrm          OK" || (echo "  libdrm          MISSING"; exit 1)
	@pkg-config --exists egl             && echo "  egl             OK" || (echo "  egl             MISSING"; exit 1)
	@pkg-config --exists libseat         && echo "  libseat         OK" || (echo "  libseat         MISSING"; exit 1)
	@echo "All dependencies satisfied."

## Launch inside an existing compositor (dev mode)
run-winit: build
	WAYLAND_DEBUG=0 ./$(TARGET_DIR)/pancake --winit

## Launch on bare hardware (needs a TTY + seatd/logind)
run: build
	./$(TARGET_DIR)/pancake

## Install binary to PREFIX (default /usr/local)
install: build
	install -Dm755 $(TARGET_DIR)/pancake $(DESTDIR)$(BINDIR)/pancake
	@echo "Installed to $(DESTDIR)$(BINDIR)/pancake"

## Remove installed binary
uninstall:
	rm -f $(DESTDIR)$(BINDIR)/pancake

## Remove build artifacts
clean:
	$(CARGO) clean

## ── ISO targets ───────────────────────────────────────────────────────────────
##
## Speed tips:
##   1. Run `make cache-proxy` once to start apt-cacher-ng — packages are cached
##      locally so each rebuild only downloads changed/new packages.
##   2. The build script reuses your host-compiled binary automatically — no
##      Rust recompile inside the container on subsequent builds.
##   3. `make iso-ubuntu-fast` skips squashfs compression for faster local tests.
##   4. `--apt-recommends false` skips optional dep bloat.

## Primary: Ubuntu 24.04 LTS Live ISO [fastest, recommended]
iso: iso-ubuntu

PANCAKE_ISO_COMPRESSION ?= gzip

iso-ubuntu:
	@test -n "$(CONTAINER_RUNTIME)" || \
	  { echo "ERROR: no container runtime. Install docker or podman."; exit 1; }
	mkdir -p $(UBUNTU_ISO_OUT)
	"$(CONTAINER_RUNTIME)" run --rm --privileged \
	  -v "$(CURDIR)":/work \
	  -v "$(CARGO_REGISTRY)":/root/.cargo/registry \
	  -v "$(CARGO_GIT)":/root/.cargo/git \
	  -w /work \
	  -e ISO_OUT="$(UBUNTU_ISO_OUT)" \
	  -e CARGO_BUILD_JOBS="$(JOBS)" \
	  -e PANCAKE_ISO_COMPRESSION="$(PANCAKE_ISO_COMPRESSION)" \
	  $(APT_PROXY_ENV) \
	  ubuntu:noble \
	  ./scripts/build-ubuntu-iso-container.sh

## Faster local Ubuntu ISO: larger output, skips squashfs compression
iso-ubuntu-fast:
	$(MAKE) iso-ubuntu PANCAKE_ISO_COMPRESSION=none

## Debian Live ISO [legacy]
iso-debian:
	@test -n "$(CONTAINER_RUNTIME)" || \
	  { echo "ERROR: no container runtime. Install docker or podman."; exit 1; }
	mkdir -p $(DEBIAN_ISO_OUT)
	"$(CONTAINER_RUNTIME)" run --rm --privileged \
	  -v "$(CURDIR)":/work \
	  -v "$(CARGO_REGISTRY)":/root/.cargo/registry \
	  -v "$(CARGO_GIT)":/root/.cargo/git \
	  -w /work \
	  -e ISO_OUT="$(DEBIAN_ISO_OUT)" \
	  -e CARGO_BUILD_JOBS="$(JOBS)" \
	  $(APT_PROXY_ENV) \
	  debian:trixie \
	  ./scripts/build-debian-iso-container.sh

## Arch Linux live ISO [legacy, requires archiso on host]
iso-arch: build
	@command -v mkarchiso >/dev/null 2>&1 || \
	  { echo "ERROR: archiso not installed. Run: sudo pacman -S archiso"; exit 1; }
	install -Dm755 target/release/pancake $(ISO_PROFILE)/airootfs/usr/local/bin/pancake
	rm -rf /tmp/pancake-iso-work
	mkdir -p "$(ARCH_ISO_OUT)"
	mkarchiso -v -w /tmp/pancake-iso-work -o "$(ARCH_ISO_OUT)" "$(ISO_PROFILE)"
	@ls -lh $(ARCH_ISO_OUT)/pancake-*.iso 2>/dev/null || ls -lh $(ARCH_ISO_OUT)/*.iso

## ── QEMU boot targets ─────────────────────────────────────────────────────────

qemu-ubuntu:
	$(eval ISO := $(shell ls -t $(UBUNTU_ISO_OUT)/pancake-ubuntu-*.iso 2>/dev/null | head -1))
	@test -n "$(ISO)" || \
	  { echo "ERROR: no Ubuntu ISO found. Run: sudo make iso"; exit 1; }
	@echo "==> Booting $(ISO)"
	qemu-system-x86_64 \
	  $(shell command -v kvm >/dev/null 2>&1 && echo "-enable-kvm" || true) \
	  -m 2048 -smp 2 \
	  -cdrom "$(ISO)" -boot d \
	  -vga virtio \
	  -display gtk,show-cursor=on,gl=on \
	  -device virtio-tablet \
	  -no-reboot

qemu-debian:
	$(eval ISO := $(shell ls -t $(DEBIAN_ISO_OUT)/pancake-debian-*.iso 2>/dev/null | head -1))
	@test -n "$(ISO)" || \
	  { echo "ERROR: no Debian ISO found. Run: sudo make iso-debian"; exit 1; }
	@echo "==> Booting $(ISO)"
	qemu-system-x86_64 \
	  $(shell command -v kvm >/dev/null 2>&1 && echo "-enable-kvm" || true) \
	  -m 2048 -smp 2 \
	  -cdrom "$(ISO)" -boot d \
	  -vga std \
	  -display gtk,show-cursor=on \
	  -no-reboot

## ── Utility ───────────────────────────────────────────────────────────────────

## Start apt-cacher-ng (caches apt packages — makes repeated ISO builds MUCH faster)
cache-proxy:
	@command -v apt-cacher-ng >/dev/null 2>&1 || \
	  { echo "Installing apt-cacher-ng..."; sudo apt install -y apt-cacher-ng; }
	sudo systemctl enable --now apt-cacher-ng
	@echo "apt-cacher-ng running on port 3142."
	@echo "Subsequent ISO builds will use cached packages — no re-downloads."

iso-info:
	@./scripts/iso-info.sh

## Remove ISO work directories and outputs
iso-clean:
	rm -rf "$(UBUNTU_WORK)" "$(DEBIAN_WORK)" /tmp/pancake-iso-work "$(ISO_OUT)"
	@echo "ISO artifacts removed."

help:
	@echo "Pancake compositor — build targets"
	@echo ""
	@echo "  make [all]         Build release binary (default)"
	@echo "  make check         Type-check only — fast, no binary produced"
	@echo "  make run-winit     Run nested inside an existing compositor"
	@echo "  make run           Run on bare hardware (TTY)"
	@echo "  make install       Install to PREFIX (default /usr/local)"
	@echo "  make clean         Remove build artifacts"
	@echo ""
	@echo "  make iso           Build Ubuntu 24.04 Live ISO  [primary]"
	@echo "  make iso-debian    Build Debian Live ISO        [legacy]"
	@echo "  make iso-arch      Build Arch Linux Live ISO    [legacy]"
	@echo "  make qemu-ubuntu   Boot the Ubuntu ISO in QEMU (KVM + virtio-vga)"
	@echo "  make qemu-debian   Boot the Debian ISO in QEMU"
	@echo "  make iso-clean     Remove ISO work dirs and output"
	@echo ""
	@echo "  make cache-proxy   Install + start apt-cacher-ng (faster repeated builds)"
	@echo ""
	@echo "  PROFILE=dev make build    Debug binary"
	@echo "  JOBS=2 make iso           Limit CPU usage during Rust compile"
	@echo "  PREFIX=/usr make install  Install to /usr/bin"
