using NWayland;
using NWayland.Protocols.Wayland;

namespace Pancake.Wayland;

// wl_data_device_manager wire objects (clipboard + drag-and-drop).
// Scoped deliberately narrow: state.rs's own DataDeviceHandler/
// SelectionHandler/ClientDndGrabHandler/ServerDndGrabHandler impls are
// literally empty method bodies -- every real mechanic (fd-forwarding
// clipboard data between clients, drag grab overlays, cross-client
// selection broadcast) lives inside Smithay's own DataDeviceState,
// which is library-internal code, not something in Pancake's own
// source to port. So this is the same shape as Seat.cs: real object
// lifecycle (the global, create_data_source, get_data_device), not a
// reimplementation of Smithay's internal clipboard-proxying machinery
// -- that would be building new infrastructure state.rs never needed
// to write itself, not porting Pancake.
internal sealed class DataDeviceManagerListener : WlDataDeviceManager.ServerListener
{
    protected override void CreateDataSource(WlDataDeviceManager.Server resource, NewId<WlDataSource.Server, WlDataSource.ServerListener> id) =>
        id.GetAndConsume(new DataSourceListener());

    protected override void GetDataDevice(WlDataDeviceManager.Server resource, NewId<WlDataDevice.Server, WlDataDevice.ServerListener> id, WlSeat.Server? seat) =>
        id.GetAndConsume(new DataDeviceListener());

    protected override void Release(WlDataDeviceManager.Server resource) => resource.Dispose();
}

internal sealed class DataDeviceListener : WlDataDevice.ServerListener
{
    protected override void StartDrag(WlDataDevice.Server resource, WlDataSource.Server? source, WlSurface.Server? origin, WlSurface.Server? icon, uint serial) { }
    protected override void SetSelection(WlDataDevice.Server resource, WlDataSource.Server? source, uint serial) { }
    protected override void Release(WlDataDevice.Server resource) => resource.Dispose();
}

internal sealed class DataSourceListener : WlDataSource.ServerListener
{
    protected override void Offer(WlDataSource.Server resource, string mimeType) { }
    protected override void Destroy(WlDataSource.Server resource) => resource.Dispose();
    protected override void SetActions(WlDataSource.Server resource, WlDataDeviceManager.DndActionEnum dndActions) { }
}

internal sealed class DataOfferListener : WlDataOffer.ServerListener
{
    protected override void Accept(WlDataOffer.Server resource, uint serial, string? mimeType) { }
    protected override void Receive(WlDataOffer.Server resource, string mimeType, WaylandFd fd) { }
    protected override void Destroy(WlDataOffer.Server resource) => resource.Dispose();
    protected override void Finish(WlDataOffer.Server resource) { }
    protected override void SetActions(WlDataOffer.Server resource, WlDataDeviceManager.DndActionEnum dndActions, WlDataDeviceManager.DndActionEnum preferredAction) { }
}
