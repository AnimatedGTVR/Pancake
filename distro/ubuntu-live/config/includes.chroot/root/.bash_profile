export RUST_LOG="${RUST_LOG:-pancake=warn,smithay=warn}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/0}"
export LIBSEAT_BACKEND="${LIBSEAT_BACKEND:-seatd}"

printf '\n'
printf '  =================================\n'
printf '  |   Pancake Desktop Environment  |\n'
printf '  =================================\n'

if grep -qw 'pancake.debug_shell=1' /proc/cmdline 2>/dev/null; then
    printf '   Failsafe shell requested.\n'
    printf '   Run start-pancake to launch the compositor.\n\n'
    return 0 2>/dev/null || exit 0
fi

printf '   Starting compositor...\n\n'

# Auto-launch when root logs into TTY1 and no session is already running.
if [ "$(tty 2>/dev/null)" = "/dev/tty1" ] \
    && [ -z "${WAYLAND_DISPLAY:-}" ] \
    && [ -z "${DISPLAY:-}" ]; then
    /usr/local/bin/start-pancake
    printf '\nPancake exited. You are back at the live shell.\n'
fi
