export RUST_LOG=pancake=debug,smithay=warn
export RUST_BACKTRACE=1
export XDG_RUNTIME_DIR=/run/user/0
export LIBSEAT_BACKEND="${LIBSEAT_BACKEND:-seatd}"

echo ""
echo "  ____                          _"
echo " |  _ \\ __ _ _ __   ___ __ _| | _____"
echo " | |_) / _\` | '_ \\ / __/ _\` | |/ / _ \\"
echo " |  __/ (_| | | | | (_| (_| |   <  __/"
echo " |_|   \\__,_|_| |_|\\___\\__,_|_|\\_\\___|"
echo ""
echo " Pancake Desktop Environment — Live Session"
echo " Ready on TTY1."
echo " Run 'start-pancake' to launch the compositor."
echo " Run 'start-pancake-terminal' to also launch foot."
echo " Logs will be written to /root/pancake.log"
echo ""
