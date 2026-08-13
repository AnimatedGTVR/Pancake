using NWayland;
using NWayland.Protocols.XdgDecorationUnstableV1;

namespace Pancake.Wayland;

// Full port of src/handlers/xdg_decoration.rs. Unlike compositor.rs/
// xdg_shell.rs, this handler has no dependency on Space/Window at all --
// it's pure protocol-level mode negotiation (tell clients whether to draw
// their own title bar or let the compositor draw one), so this is a
// complete, not partial, port.
internal sealed class ZxdgDecorationManagerListener : ZxdgDecorationManagerV1.ServerListener
{
    protected override void Destroy(ZxdgDecorationManagerV1.Server resource) => resource.Dispose();

    protected override void GetToplevelDecoration(ZxdgDecorationManagerV1.Server resource,
        NewId<ZxdgToplevelDecorationV1.Server, ZxdgToplevelDecorationV1.ServerListener> id,
        NWayland.Protocols.XdgShell.XdgToplevel.Server? toplevel)
    {
        var decoration = id.GetAndConsume(new ZxdgToplevelDecorationListener());
        // Port of new_decoration: Pancake doesn't draw server-side title
        // bars yet, so always start every toplevel as client-side-decorated.
        decoration.Configure(ZxdgToplevelDecorationV1.ModeEnum.ClientSide);
    }
}

internal sealed class ZxdgToplevelDecorationListener : ZxdgToplevelDecorationV1.ServerListener
{
    protected override void Destroy(ZxdgToplevelDecorationV1.Server resource) => resource.Dispose();

    // Port of request_mode: ServerSide requests are downgraded to
    // ClientSide (same "doesn't draw SSD yet" reason as above); any other
    // requested mode is granted as-is.
    protected override void SetMode(ZxdgToplevelDecorationV1.Server resource, ZxdgToplevelDecorationV1.ModeEnum mode)
    {
        var granted = mode == ZxdgToplevelDecorationV1.ModeEnum.ServerSide
            ? ZxdgToplevelDecorationV1.ModeEnum.ClientSide
            : mode;
        resource.Configure(granted);
    }

    // Port of unset_mode: falls back to the compositor's default choice,
    // same as a freshly created decoration object.
    protected override void UnsetMode(ZxdgToplevelDecorationV1.Server resource) =>
        resource.Configure(ZxdgToplevelDecorationV1.ModeEnum.ClientSide);
}
