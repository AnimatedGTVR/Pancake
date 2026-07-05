export RUST_LOG="${RUST_LOG:-pancake=warn,smithay=warn}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/0}"
export LIBSEAT_BACKEND="${LIBSEAT_BACKEND:-seatd}"

printf '\n'
printf '  =================================\n'
printf '  |   Pancake Desktop Environment  |\n'
printf '  =================================\n'
printf '   Starting compositor...\n\n'

# Auto-launch when root logs into TTY1 and no session is already running.
if [ "$(tty 2>/dev/null)" = "/dev/tty1" ] \
    && [ -z "${WAYLAND_DISPLAY:-}" ] \
    && [ -z "${DISPLAY:-}" ]; then
    exec /usr/local/bin/start-pancake
fi
