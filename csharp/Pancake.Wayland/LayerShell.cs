using NWayland;
using NWayland.Protocols.Wlr.WlrLayerShellUnstableV1;

namespace Pancake.Wayland;

// Wire-protocol layer of src/handlers/layer_shell.rs (wlr-layer-shell --
// the protocol waybar/dunst/wofi etc. use for panels, notifications, and
// launchers). Same split as xdg_shell.rs: the zwlr_layer_shell_v1 ->
// zwlr_layer_surface_v1 object lifecycle and the configure/ack_configure
// handshake are real here; what new_layer_surface/layer_destroyed do with
// the result (compute real geometry from output size + anchor/margin/
// exclusive-zone, register in a layer_map, reserve space for other
// windows) needs the still-unported Space/Output-equivalent, so it isn't
// here yet. The Configure sent below is a placeholder (0,0 = "size not
// yet determined by real layout"), same honesty as xdg_toplevel's
// initial Configure(0, 0, ...) before real geometry exists.
internal sealed class ZwlrLayerShellListener : ZwlrLayerShellV1.ServerListener
{
    protected override void Destroy(ZwlrLayerShellV1.Server resource) => resource.Dispose();

    protected override void GetLayerSurface(ZwlrLayerShellV1.Server resource,
        NewId<ZwlrLayerSurfaceV1.Server, ZwlrLayerSurfaceV1.ServerListener> id,
        NWayland.Protocols.Wayland.WlSurface.Server? surface,
        NWayland.Protocols.Wayland.WlOutput.Server? output,
        ZwlrLayerShellV1.LayerEnum layer,
        string @namespace)
    {
        var listener = new ZwlrLayerSurfaceListener(@namespace, layer);
        var layerSurface = id.GetAndConsume(listener);
        listener.Bind(layerSurface);
        layerSurface.Configure(0, 0, 0);
    }
}

internal sealed class ZwlrLayerSurfaceListener(string @namespace, ZwlrLayerShellV1.LayerEnum layer) : ZwlrLayerSurfaceV1.ServerListener
{
    public string Namespace { get; } = @namespace;
    public ZwlrLayerShellV1.LayerEnum Layer { get; private set; } = layer;
    private ZwlrLayerSurfaceV1.Server? _resource;

    internal void Bind(ZwlrLayerSurfaceV1.Server resource) => _resource = resource;

    protected override void SetSize(ZwlrLayerSurfaceV1.Server resource, uint width, uint height) { }
    protected override void SetAnchor(ZwlrLayerSurfaceV1.Server resource, ZwlrLayerSurfaceV1.AnchorEnum anchor) { }
    protected override void SetExclusiveZone(ZwlrLayerSurfaceV1.Server resource, int zone) { }
    protected override void SetMargin(ZwlrLayerSurfaceV1.Server resource, int top, int right, int bottom, int left) { }
    protected override void SetKeyboardInteractivity(ZwlrLayerSurfaceV1.Server resource, ZwlrLayerSurfaceV1.KeyboardInteractivityEnum keyboardInteractivity) { }
    protected override void GetPopup(ZwlrLayerSurfaceV1.Server resource, NWayland.Protocols.XdgShell.XdgPopup.Server? popup) { }
    protected override void AckConfigure(ZwlrLayerSurfaceV1.Server resource, uint serial) { }
    protected override void SetLayer(ZwlrLayerSurfaceV1.Server resource, ZwlrLayerShellV1.LayerEnum layer) => Layer = layer;
    protected override void SetExclusiveEdge(ZwlrLayerSurfaceV1.Server resource, ZwlrLayerSurfaceV1.AnchorEnum edge) { }
    protected override void Destroy(ZwlrLayerSurfaceV1.Server resource) => resource.Dispose();
}
