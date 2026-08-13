using NWayland;
using NWayland.Protocols.XdgShell;

namespace Pancake.Wayland;

// Wire-protocol layer of src/handlers/xdg_shell.rs -- the xdg_wm_base ->
// xdg_surface -> xdg_toplevel object lifecycle and the configure/
// ack_configure handshake that makes a wl_surface become an actual
// window. This is the single most important exchange in the whole
// compositor: it's how every real application window gets created.
//
// PancakeWaylandServer now hooks ToplevelCreated/ToplevelDestroyed to do
// the real app-logic half (space mapping, workspace registration, focus)
// via PancakeSpace/WorkspaceManager -- this file stays just the wire
// protocol + event plumbing, same separation as before. Retiling and
// move/resize grabs still aren't wired (no wl_seat/wl_pointer input
// events exist yet -- that's input.rs territory). xdg_popup/
// xdg_positioner are stubbed to satisfy the interface (protocol-required,
// since a client is entitled to call CreatePositioner even if this
// compositor doesn't use popups yet) but do not implement real popup
// placement -- same "real slice now, rest deferred and documented"
// pattern as the wl_compositor port.
internal sealed class XdgWmBaseListener : XdgWmBase.ServerListener
{
    public event Action<XdgSurface.Server>? SurfaceCreated;
    public event Action<XdgToplevel.Server, XdgSurface.Server>? ToplevelCreated;
    public event Action<XdgToplevel.Server>? ToplevelDestroyed;

    protected override void Destroy(XdgWmBase.Server resource) => resource.Dispose();

    protected override void CreatePositioner(XdgWmBase.Server resource, NewId<XdgPositioner.Server, XdgPositioner.ServerListener> id) =>
        id.GetAndConsume(new XdgPositionerListener());

    protected override void GetXdgSurface(XdgWmBase.Server resource, NewId<XdgSurface.Server, XdgSurface.ServerListener> id, NWayland.Protocols.Wayland.WlSurface.Server? surface)
    {
        var listener = new XdgSurfaceListener(this);
        var xdgSurface = id.GetAndConsume(listener);
        SurfaceCreated?.Invoke(xdgSurface);
    }

    protected override void Pong(XdgWmBase.Server resource, uint serial) { }

    internal void NoteToplevelCreated(XdgToplevel.Server toplevel, XdgSurface.Server surface) =>
        ToplevelCreated?.Invoke(toplevel, surface);

    internal void NoteToplevelDestroyed(XdgToplevel.Server toplevel) =>
        ToplevelDestroyed?.Invoke(toplevel);
}

internal sealed class XdgSurfaceListener(XdgWmBaseListener wmBase) : XdgSurface.ServerListener
{
    protected override void Destroy(XdgSurface.Server resource) => resource.Dispose();

    protected override void GetToplevel(XdgSurface.Server resource, NewId<XdgToplevel.Server, XdgToplevel.ServerListener> id)
    {
        var toplevelListener = new XdgToplevelListener(wmBase);
        var toplevel = id.GetAndConsume(toplevelListener);
        wmBase.NoteToplevelCreated(toplevel, resource);

        // Real xdg-shell handshake: server sends xdg_surface.configure
        // after xdg_toplevel.configure, client must ack_configure before
        // the surface's next commit takes effect. Matches xdg_shell.rs's
        // new_toplevel: pending state -> send_configure.
        toplevel.Configure(0, 0, ReadOnlySpan<byte>.Empty);
        resource.Configure(1);
    }

    protected override void GetPopup(XdgSurface.Server resource, NewId<XdgPopup.Server, XdgPopup.ServerListener> id, XdgSurface.Server? parent, XdgPositioner.Server? positioner) =>
        id.GetAndConsume(new XdgPopupListener());

    protected override void SetWindowGeometry(XdgSurface.Server resource, int x, int y, int width, int height) { }

    protected override void AckConfigure(XdgSurface.Server resource, uint serial) { }
}

internal sealed class XdgToplevelListener(XdgWmBaseListener wmBase) : XdgToplevel.ServerListener
{
    public string? Title { get; private set; }
    public string? AppId { get; private set; }

    protected override void Destroy(XdgToplevel.Server resource)
    {
        wmBase.NoteToplevelDestroyed(resource);
        resource.Dispose();
    }
    protected override void SetParent(XdgToplevel.Server resource, XdgToplevel.Server? parent) { }
    protected override void SetTitle(XdgToplevel.Server resource, string title) => Title = title;
    protected override void SetAppId(XdgToplevel.Server resource, string appId) => AppId = appId;
    protected override void ShowWindowMenu(XdgToplevel.Server resource, NWayland.Protocols.Wayland.WlSeat.Server? seat, uint serial, int x, int y) { }
    protected override void Move(XdgToplevel.Server resource, NWayland.Protocols.Wayland.WlSeat.Server? seat, uint serial) { }
    protected override void Resize(XdgToplevel.Server resource, NWayland.Protocols.Wayland.WlSeat.Server? seat, uint serial, XdgToplevel.ResizeEdgeEnum edges) { }
    protected override void SetMaxSize(XdgToplevel.Server resource, int width, int height) { }
    protected override void SetMinSize(XdgToplevel.Server resource, int width, int height) { }
    protected override void SetMaximized(XdgToplevel.Server resource) { }
    protected override void UnsetMaximized(XdgToplevel.Server resource) { }
    protected override void SetFullscreen(XdgToplevel.Server resource, NWayland.Protocols.Wayland.WlOutput.Server? output) { }
    protected override void UnsetFullscreen(XdgToplevel.Server resource) { }
    protected override void SetMinimized(XdgToplevel.Server resource) { }
}

internal sealed class XdgPositionerListener : XdgPositioner.ServerListener
{
    protected override void Destroy(XdgPositioner.Server resource) => resource.Dispose();
    protected override void SetSize(XdgPositioner.Server resource, int width, int height) { }
    protected override void SetAnchorRect(XdgPositioner.Server resource, int x, int y, int width, int height) { }
    protected override void SetAnchor(XdgPositioner.Server resource, XdgPositioner.AnchorEnum anchor) { }
    protected override void SetGravity(XdgPositioner.Server resource, XdgPositioner.GravityEnum gravity) { }
    protected override void SetConstraintAdjustment(XdgPositioner.Server resource, XdgPositioner.ConstraintAdjustmentEnum constraintAdjustment) { }
    protected override void SetOffset(XdgPositioner.Server resource, int x, int y) { }
    protected override void SetReactive(XdgPositioner.Server resource) { }
    protected override void SetParentSize(XdgPositioner.Server resource, int width, int height) { }
    protected override void SetParentConfigure(XdgPositioner.Server resource, uint serial) { }
}

internal sealed class XdgPopupListener : XdgPopup.ServerListener
{
    protected override void Destroy(XdgPopup.Server resource) => resource.Dispose();
    protected override void Grab(XdgPopup.Server resource, NWayland.Protocols.Wayland.WlSeat.Server? seat, uint serial) { }
    protected override void Reposition(XdgPopup.Server resource, XdgPositioner.Server? positioner, uint token) { }
}
