using NWayland;
using NWayland.Protocols.Wayland;

namespace Pancake.Wayland;

// Real wl_seat/wl_pointer/wl_keyboard/wl_touch wire objects -- the piece
// input.rs itself never actually creates (Smithay's SeatState/Seat::new
// does that generically; input.rs's own SeatHandler impl is two no-op
// callbacks). This is genuinely new wire-protocol work, not a port of
// input.rs's file content -- what input.rs *does* contain (keybinding
// interception, click-to-focus, move/resize grab tracking) all needs
// real hardware input events, which need Pancake.Cn's still-unbuilt
// libinput layer, so that stays out of scope here. This closes the
// concrete gap readmenow.md flagged: object lifecycle is real, driving
// it with actual input events is not.
internal sealed class SeatListener : WlSeat.ServerListener
{
    protected override void GetPointer(WlSeat.Server resource, NewId<WlPointer.Server, WlPointer.ServerListener> id) =>
        id.GetAndConsume(new PointerListener());

    protected override void GetKeyboard(WlSeat.Server resource, NewId<WlKeyboard.Server, WlKeyboard.ServerListener> id) =>
        id.GetAndConsume(new KeyboardListener());

    protected override void GetTouch(WlSeat.Server resource, NewId<WlTouch.Server, WlTouch.ServerListener> id) =>
        id.GetAndConsume(new TouchListener());

    protected override void Release(WlSeat.Server resource) => resource.Dispose();
}

internal sealed class PointerListener : WlPointer.ServerListener
{
    protected override void SetCursor(WlPointer.Server resource, uint serial, WlSurface.Server? surface, int hotspotX, int hotspotY) { }
    protected override void Release(WlPointer.Server resource) => resource.Dispose();
}

internal sealed class KeyboardListener : WlKeyboard.ServerListener
{
    protected override void Release(WlKeyboard.Server resource) => resource.Dispose();
}

internal sealed class TouchListener : WlTouch.ServerListener
{
    protected override void Release(WlTouch.Server resource) => resource.Dispose();
}
