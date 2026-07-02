PREFIX   ?= /usr/local
BINDIR   := $(PREFIX)/bin
CARGO    := cargo
DESTDIR  ?=

# Build profile: use release by default, override with PROFILE=dev
PROFILE  ?= release
ifeq ($(PROFILE),release)
  CARGO_FLAGS := --release
  TARGET_DIR  := target/release
else
  CARGO_FLAGS :=
  TARGET_DIR  := target/debug
endif

ISO_PROFILE  := iso/profile
ISO_WORK     := /tmp/pancake-iso-work
ISO_OUT      := iso/out
DEBIAN_ISO_OUT := $(ISO_OUT)/debian
ARCH_ISO_OUT   := $(ISO_OUT)/arch
DEBIAN_PROFILE := distro/debian-live
DEBIAN_WORK    := /tmp/pancake-debian-live
CONTAINER_RUNTIME ?= $(shell command -v podman 2>/dev/null || command -v docker 2>/dev/null)

.PHONY: all build check clean install uninstall run run-winit fmt lint deps iso iso-debian iso-arch iso-info iso-clean qemu-debian

## Default target
all: build

## Compile (release)
build:
	$(CARGO) build $(CARGO_FLAGS)

## Type-check without producing a binary
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
	@echo "Removed $(DESTDIR)$(BINDIR)/pancake"

## Remove build artifacts
clean:
	$(CARGO) clean

## Build a Debian Live ISO containing Pancake
##
## Requires on pacman hosts: sudo pacman -S docker
## Then enable Docker:       sudo systemctl enable --now docker
## Run:                      sudo make iso
##
## Output: iso/out/pancake-debian-YYYY.MM.DD-amd64.iso
iso: iso-debian

iso-debian:
	@test -n "$(CONTAINER_RUNTIME)" || \
	  { echo "ERROR: no container runtime found."; \
	    echo "On pacman hosts run: sudo pacman -S docker"; \
	    echo "Then run: sudo systemctl enable --now docker"; \
	    exit 1; }
	"$(CONTAINER_RUNTIME)" run --rm --privileged \
	  -v "$(CURDIR)":/work \
	  -w /work \
	  -e ISO_OUT="$(DEBIAN_ISO_OUT)" \
	  debian:trixie \
	  ./scripts/build-debian-iso-container.sh

## Boot the latest Debian ISO in QEMU with correct display settings.
##
## Uses -vga std (VESA-compatible, works without guest drivers) so the
## framebuffer console shows text.  KVM is used when available.
##
## Usage: make qemu-debian
qemu-debian:
	$(eval ISO := $(shell ls -t $(DEBIAN_ISO_OUT)/pancake-debian-*.iso 2>/dev/null | head -1))
	@test -n "$(ISO)" || \
	  { echo "ERROR: no Debian ISO found in $(DEBIAN_ISO_OUT). Run: sudo make iso"; exit 1; }
	@echo "==> Booting $(ISO)"
	qemu-system-x86_64 \
	  $(shell command -v kvm >/dev/null 2>&1 && echo "-enable-kvm" || true) \
	  -m 2048 \
	  -smp 2 \
	  -cdrom "$(ISO)" \
	  -boot d \
	  -vga std \
	  -display gtk,show-cursor=on \
	  -no-reboot

## Build the legacy Arch Linux live ISO containing Pancake
##
## Requires archiso:  sudo pacman -S archiso
## Run as root:       sudo make iso-arch
##
## Output: iso/out/pancake-YYYY.MM.DD-x86_64.iso
iso-arch: build
	@command -v mkarchiso >/dev/null 2>&1 || \
	  { echo "ERROR: archiso not installed.  Run: sudo pacman -S archiso"; exit 1; }
	@echo "==> Staging Pancake binary into ISO profile..."
	install -Dm755 target/release/pancake $(ISO_PROFILE)/airootfs/usr/local/bin/pancake
	@echo "==> Cleaning previous work directory..."
	rm -rf "$(ISO_WORK)"
	mkdir -p "$(ARCH_ISO_OUT)"
	@echo "==> Building ISO (this takes a few minutes)..."
	mkarchiso -v -w "$(ISO_WORK)" -o "$(ARCH_ISO_OUT)" "$(ISO_PROFILE)"
	@echo ""
	@echo "==> ISO ready:"
	@ls -lh $(ARCH_ISO_OUT)/pancake-*.iso 2>/dev/null || ls -lh $(ARCH_ISO_OUT)/*.iso

iso-info:
	@./scripts/iso-info.sh

## Remove ISO work directories and output
iso-clean:
	rm -rf "$(ISO_WORK)" "$(DEBIAN_WORK)" "$(ISO_OUT)"
	@echo "ISO artifacts removed."

help:
	@echo "Pancake compositor — build targets:"
	@echo ""
	@echo "  make [all]       Build release binary (default)"
	@echo "  make build       Same as above"
	@echo "  make check       Type-check only (fast, no binary)"
	@echo "  make test        Run test suite"
	@echo "  make fmt         Format source with rustfmt"
	@echo "  make lint        Clippy with -D warnings"
	@echo "  make deps        Verify system libraries are present"
	@echo "  make run-winit   Run nested inside an existing compositor"
	@echo "  make run         Run on bare hardware (TTY)"
	@echo "  make install     Install to PREFIX (default /usr/local)"
	@echo "  make uninstall   Remove installed binary"
	@echo "  make clean       Remove build artifacts"
	@echo "  make iso         Build Debian Live ISO through Docker/Podman"
	@echo "  make iso-arch    Build legacy Arch Linux live ISO"
	@echo "  make qemu-debian Boot the Debian ISO in QEMU (-vga std)"
	@echo "  make iso-info    Show ISO labels so Arch/Debian artifacts are obvious"
	@echo "  make iso-clean   Remove ISO work/output directories"
	@echo ""
	@echo "  PROFILE=dev make build   Build debug binary instead"
	@echo "  PREFIX=/usr make install  Install to /usr/bin"
