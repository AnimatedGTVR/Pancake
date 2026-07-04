export RUST_LOG="${RUST_LOG:-pancake=debug,smithay=warn}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/0}"
export LIBSEAT_BACKEND="${LIBSEAT_BACKEND:-seatd}"

echo ""
echo "  ____                          _"
echo " |  _ \ __ _ _ __   ___ __ _  | | _____"
echo " | |_) / _\` | '_ \\ / __/ _\` | | |/ / _ \\"
echo " |  __/ (_| | | | | (_| (_| | |   <  __/"
echo " |_|   \\__,_|_| |_|\\___\\__,_| |_|\\_\\___|"
echo ""
echo " Pancake Desktop Environment"
echo " Starting compositor..."
echo ""

# Auto-launch when root logs into TTY1 and no session is already running.
if [ "$(tty 2>/dev/null)" = "/dev/tty1" ] \
    && [ -z "${WAYLAND_DISPLAY:-}" ] \
    && [ -z "${DISPLAY:-}" ]; then
    exec /usr/local/bin/start-pancake
fi
