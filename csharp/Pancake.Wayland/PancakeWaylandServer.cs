using System.Net.Sockets;
using NWayland;
using NWayland.Protocols.Wayland;
using NWayland.Server;
using Pancake.Shell;

namespace Pancake.Wayland;

// Real bootstrap of an NWayland-based Wayland server: listening socket,
// client accept, wl_compositor global advertisement, and wl_surface
// creation. Scope matches src/handlers/compositor.rs's actual job
// (surface/region lifecycle) plus the real frame loop (see RepaintLoop
// below) that ties Pancake.Cn's GPU bring-up and Pancake.Render's
// AeroRenderer into actual per-frame work -- the piece Pancake.App's
// udev backend explicitly flagged as missing. Real DRM scanout/pageflip
// (turning a rendered frame into actual on-screen pixels) still isn't
// built -- only enumeration exists in Pancake.Cn.DrmResources -- so this
// proves the loop drives real GL rendering and real client-visible
// frame-callback delivery, not that pixels reach a real display.
public sealed class PancakeWaylandServer : IAsyncDisposable
{
    private readonly WaylandServer _server;
    private readonly string _socketPath;
    private Socket? _listenSocket;
    private Thread? _runThread;
    private Thread? _acceptThread;
    private Thread? _repaintThread;
    private volatile bool _disposed;
    private uint _serial;

    private Pancake.Cn.GpuDevice? _gpu;
    private Silk.NET.OpenGLES.GL? _gl;
    private Pancake.Render.AeroRenderer? _aero;
    private readonly List<WlCallback.Server> _pendingFrameCallbacks = new();
    public int FramesRendered { get; private set; }
    public bool GpuAvailable => _gpu is not null;

    public string SocketPath => _socketPath;
    public int SurfacesCreated => _surfacesCreated;
    private int _surfacesCreated;

    // The real app-logic half of xdg_shell.rs's new_toplevel/
    // toplevel_destroyed, now that PancakeSpace/PancakeWindow/
    // WorkspaceManager exist to support it -- was the gap this whole
    // Space/Window design pass was meant to close.
    public PancakeSpace Space { get; } = new();
    public WorkspaceManager Workspaces { get; } = new();
    private PancakeWindow? _focusedWindow;
    private readonly Dictionary<NWayland.Protocols.XdgShell.XdgToplevel.Server, PancakeWindow> _toplevelWindows = new();

    public PancakeWaylandServer(string socketPath)
    {
        _server = new WaylandServer();
        _socketPath = socketPath;
    }

    public void Start(string? renderNodePath = "/dev/dri/renderD128")
    {
        if (File.Exists(_socketPath)) File.Delete(_socketPath);

        _listenSocket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
        _listenSocket.Bind(new UnixDomainSocketEndPoint(_socketPath));
        _listenSocket.Listen(4);

        if (renderNodePath is not null && File.Exists(renderNodePath))
        {
            try
            {
                _gpu = Pancake.Cn.GpuDevice.Open(renderNodePath);
                _gl = Silk.NET.OpenGLES.GL.GetApi(_gpu.GetProcAddress);
                _aero = new Pancake.Render.AeroRenderer();
                _aero.ApplyConfig(4, 2, new[] { 0.52f, 0.68f, 1.00f, 0.16f }, null);
            }
            catch (Exception e)
            {
                // Same known sandbox GPU fault documented throughout
                // readmenow.md/Pancake.Cn -- fails fast and cleanly here,
                // same as GpuDevice.Open's own direct callers. The
                // repaint loop still runs without GPU rendering (frame
                // callbacks alone), matching real compositor behavior
                // when a client legitimately has no output attached yet.
                Console.Error.WriteLine($"[PancakeWaylandServer] GPU unavailable, running without rendering: {e.Message}");
                _gpu = null;
                _gl = null;
                _aero = null;
            }
        }

        _runThread = new Thread(RunLoop) { Name = "PancakeWaylandRun", IsBackground = true };
        _runThread.Start();

        _acceptThread = new Thread(AcceptLoop) { Name = "PancakeWaylandAccept", IsBackground = true };
        _acceptThread.Start();

        _repaintThread = new Thread(RepaintLoop) { Name = "PancakeWaylandRepaint", IsBackground = true };
        _repaintThread.Start();
    }

    private sealed record NewClientFd(int Fd);
    private sealed record RepaintTick;

    // Matches udev.rs's "20 Hz repaint safety-net" timer -- real frame
    // pacing tied to actual VBlank needs the DRM pageflip/atomic-commit
    // machinery this session didn't build, so a fixed-interval timer is
    // the honest stand-in, same simplification philosophy as the rest of
    // this project's "real for the common case, documented and bounded"
    // approach.
    private void RepaintLoop()
    {
        while (!_disposed)
        {
            Thread.Sleep(50);
            try { _server.Post(new RepaintTick()); }
            catch (ObjectDisposedException) { break; }
        }
    }

    internal void RegisterFrameCallback(WlCallback.Server callback) => _pendingFrameCallbacks.Add(callback);

    // Runs on the dispatch thread (via the RepaintTick custom event in
    // RunLoop) -- real GL rendering when a GPU context exists, and real
    // wl_callback.done delivery either way, since frame-callback delivery
    // is a protocol-level promise to the client independent of whether
    // this process can actually reach a display.
    private void DoRepaint()
    {
        if (_gl is not null && _aero is not null)
        {
            _aero.BeginFrame(_gl, 1920, 1080);
            _aero.DrawBackground(_gl);
            FramesRendered++;
        }

        if (_pendingFrameCallbacks.Count == 0) return;
        var timeMs = (uint)Environment.TickCount;
        foreach (var cb in _pendingFrameCallbacks)
        {
            try { cb.Done(timeMs); }
            catch (ObjectDisposedException) { /* client disconnected between commit and this tick */ }
        }
        _pendingFrameCallbacks.Clear();
    }

    private void AcceptLoop()
    {
        while (!_disposed)
        {
            Socket clientSocket;
            try
            {
                clientSocket = _listenSocket!.Accept();
            }
            catch (Exception)
            {
                break;
            }

            var fd = (int)clientSocket.SafeHandle.DangerousGetHandle();
            clientSocket.SafeHandle.SetHandleAsInvalid();
            _server.Post(new NewClientFd(fd));
        }
    }

    private void RunLoop()
    {
        while (!_disposed)
        {
            NWayland.Server.WaylandServerEvent evt;
            try
            {
                evt = _server.NextEvent();
            }
            catch (ObjectDisposedException)
            {
                break;
            }

            switch (evt)
            {
                case WaylandCustomEvent custom when custom.State is NewClientFd nf:
                    var client = _server.AddClient(nf.Fd);
                    client.AddGlobal("wl_compositor", 6);
                    client.AddGlobal("xdg_wm_base", 6);
                    client.AddGlobal("zxdg_decoration_manager_v1", 1);
                    client.AddGlobal("zwlr_layer_shell_v1", 4);
                    client.AddGlobal("wl_seat", 8);
                    client.AddGlobal("wl_data_device_manager", 3);
                    break;

                case WaylandCustomEvent custom when custom.State is RepaintTick:
                    DoRepaint();
                    break;

                case WaylandServerSyncEvent sync:
                    sync.Complete(++_serial);
                    break;

                case WaylandServerRegistryBindEvent bind:
                    switch (bind.Global.Interface)
                    {
                        case "wl_compositor":
                            bind.Accept<WlCompositor.Server>(new CompositorListener(this));
                            break;
                        case "xdg_wm_base":
                            var wmBaseListener = new XdgWmBaseListener();
                            wmBaseListener.ToplevelCreated += OnToplevelCreated;
                            wmBaseListener.ToplevelDestroyed += OnToplevelDestroyed;
                            bind.Accept<NWayland.Protocols.XdgShell.XdgWmBase.Server>(wmBaseListener);
                            break;
                        case "zxdg_decoration_manager_v1":
                            bind.Accept<NWayland.Protocols.XdgDecorationUnstableV1.ZxdgDecorationManagerV1.Server>(new ZxdgDecorationManagerListener());
                            break;
                        case "zwlr_layer_shell_v1":
                            bind.Accept<NWayland.Protocols.Wlr.WlrLayerShellUnstableV1.ZwlrLayerShellV1.Server>(new ZwlrLayerShellListener());
                            break;
                        case "wl_seat":
                            var seat = bind.Accept<WlSeat.Server>(new SeatListener());
                            seat.Capabilities(WlSeat.CapabilityEnum.Pointer | WlSeat.CapabilityEnum.Keyboard);
                            seat.Name("seat0");
                            break;
                        case "wl_data_device_manager":
                            bind.Accept<NWayland.Protocols.Wayland.WlDataDeviceManager.Server>(new DataDeviceManagerListener());
                            break;
                    }
                    break;

                case WaylandServerRequestEvent request:
                    try { request.Dispatch(); }
                    finally { request.Dispose(); }
                    break;
            }
        }
    }

    internal void NoteSurfaceCreated() => Interlocked.Increment(ref _surfacesCreated);
    public int ToplevelsCreated => _toplevelsCreated;
    private int _toplevelsCreated;

    // Port of xdg_shell.rs's new_toplevel: compute cascaded initial
    // geometry, map + raise the window, register it in the active
    // workspace (splitting the previously-focused window if tiling).
    // Keyboard-focus wiring (Smithay's `keyboard.set_focus`) is out of
    // scope here -- that's input.rs's job, still blocked on real
    // wl_seat/wl_keyboard wire objects this session didn't build.
    private void OnToplevelCreated(NWayland.Protocols.XdgShell.XdgToplevel.Server toplevel, NWayland.Protocols.XdgShell.XdgSurface.Server surface)
    {
        Interlocked.Increment(ref _toplevelsCreated);

        var firstOutput = Space.Outputs().Count > 0 ? Space.Outputs()[0] : null;
        var outputGeo = firstOutput is not null ? Space.OutputGeometry(firstOutput) : null;
        var geometry = Layout.InitialGeometry(outputGeo, Space.ElementCount);

        var window = new PancakeWindow { Backend = toplevel };
        Space.MapElement(window, geometry.Loc, true);
        Space.SetElementSize(window, geometry.Size);
        Space.RaiseElement(window, true);
        Workspaces.AddWindow(window, geometry.Loc, _focusedWindow);
        _focusedWindow = window;
        _toplevelWindows[toplevel] = window;
    }

    // Port of xdg_shell.rs's toplevel_destroyed: drop focus if this was
    // the focused window, remove from the workspace, unmap from the
    // space. Move/resize-grab cancellation isn't ported (no grab state
    // exists yet -- that's input.rs territory too).
    private void OnToplevelDestroyed(NWayland.Protocols.XdgShell.XdgToplevel.Server toplevel)
    {
        if (!_toplevelWindows.Remove(toplevel, out var window)) return;
        if (ReferenceEquals(_focusedWindow, window)) _focusedWindow = null;
        Workspaces.RemoveWindow(window);
        Space.UnmapElement(window);
    }

    public async ValueTask DisposeAsync()
    {
        _disposed = true;
        _listenSocket?.Close();
        await _server.DisposeAsync();
        if (File.Exists(_socketPath)) File.Delete(_socketPath);
    }
}

internal sealed class CompositorListener(PancakeWaylandServer server) : WlCompositor.ServerListener
{
    protected override void CreateSurface(WlCompositor.Server resource, NewId<WlSurface.Server, WlSurface.ServerListener> surface)
    {
        surface.GetAndConsume(new SurfaceListener(server));
        server.NoteSurfaceCreated();
    }

    protected override void CreateRegion(WlCompositor.Server resource, NewId<WlRegion.Server, WlRegion.ServerListener> region)
    {
        region.GetAndConsume(new RegionListener());
    }

    protected override void Release(WlCompositor.Server resource) => resource.Dispose();
}

// Buffer attach/damage tracking still depends on the Space-side surface
// geometry this session didn't wire that deep, but Frame is real now --
// wl_surface.frame is the actual client-visible contract the repaint
// loop above fulfils (see PancakeWaylandServer.DoRepaint).
internal sealed class SurfaceListener(PancakeWaylandServer server) : WlSurface.ServerListener
{
    protected override void Destroy(WlSurface.Server resource) => resource.Dispose();
    protected override void Attach(WlSurface.Server resource, WlBuffer.Server? buffer, int x, int y) { }
    protected override void Damage(WlSurface.Server resource, int x, int y, int width, int height) { }
    protected override void Frame(WlSurface.Server resource, NewId<WlCallback.Server, WlCallback.ServerListener> callback) =>
        server.RegisterFrameCallback(callback.GetAndConsume());
    protected override void SetOpaqueRegion(WlSurface.Server resource, WlRegion.Server? region) { }
    protected override void SetInputRegion(WlSurface.Server resource, WlRegion.Server? region) { }
    protected override void Commit(WlSurface.Server resource) { }
    protected override void SetBufferTransform(WlSurface.Server resource, WlOutput.TransformEnum transform) { }
    protected override void SetBufferScale(WlSurface.Server resource, int scale) { }
    protected override void DamageBuffer(WlSurface.Server resource, int x, int y, int width, int height) { }
    protected override void Offset(WlSurface.Server resource, int x, int y) { }
    protected override void GetRelease(WlSurface.Server resource, NewId<WlCallback.Server, WlCallback.ServerListener> callback) =>
        callback.GetAndConsume();
}

internal sealed class RegionListener : WlRegion.ServerListener
{
    protected override void Destroy(WlRegion.Server resource) => resource.Dispose();
    protected override void Add(WlRegion.Server resource, int x, int y, int width, int height) { }
    protected override void Subtract(WlRegion.Server resource, int x, int y, int width, int height) { }
}
