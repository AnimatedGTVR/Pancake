using System.Net.Sockets;
using System.Text;

namespace Pancake.Syrup.Smoke;

// A minimal, hand-rolled Wayland wire-protocol client -- deliberately not
// using any Wayland client library (not even NWayland's own client half),
// so that a passing test here proves Pancake.Wayland's server round-trips
// genuine wire bytes, not just "another NWayland-based program talked to
// itself." Implements just enough of the wire format (message framing,
// string/new_id/uint arguments, wl_registry.bind's special dynamic new_id
// encoding) to do a real get_registry + sync + bind + create_surface
// exchange.
internal sealed class RawWaylandClient : IDisposable
{
    private readonly Socket _socket;
    private readonly List<(uint Name, string Interface, uint Version)> _globals = new();
    private bool _syncDone;
    private bool _sawError;
    private string _errorMessage = "";

    private RawWaylandClient(Socket socket) => _socket = socket;

    public static RawWaylandClient Connect(string socketPath)
    {
        var socket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
        socket.Connect(new UnixDomainSocketEndPoint(socketPath));
        return new RawWaylandClient(socket);
    }

    public IReadOnlyList<(uint Name, string Interface, uint Version)> Globals => _globals;
    public bool SawError => _sawError;
    public string ErrorMessage => _errorMessage;

    // wl_display(1).get_registry(new_id=2), wl_display(1).sync(new_id=3),
    // then read events until wl_callback(3).done fires -- by Wayland's
    // request-ordering guarantee, every wl_registry.global for objects
    // that existed before sync was sent arrives before that done event.
    public void FetchRegistryAndSync()
    {
        SendMessage(1, 1, WriteUint(2));       // wl_display.get_registry -> registry id 2
        SendMessage(1, 0, WriteUint(3));       // wl_display.sync -> callback id 3

        while (!_syncDone)
        {
            var (objectId, opcode, args) = ReadMessage();
            HandleEvent(objectId, opcode, args);
        }
    }

    public void BindAndCreateSurface(uint compositorGlobalName, uint compositorObjectId, uint surfaceObjectId)
    {
        // wl_registry(2).bind(name, interface, version, id) -- bind's
        // `id` argument is a *dynamic* new_id: unlike a normal new_id
        // (just a uint), the wire protocol encodes it as
        // (string interface, uint version, uint new_object_id) because
        // the target interface isn't fixed by the protocol XML.
        var args = new List<byte>();
        args.AddRange(WriteUint(compositorGlobalName));
        args.AddRange(WriteString("wl_compositor"));
        args.AddRange(WriteUint(6));
        args.AddRange(WriteUint(compositorObjectId));
        SendMessage(2, 0, args.ToArray());

        // wl_compositor(compositorObjectId).create_surface(new_id) --
        // fixed-interface new_id, so just a plain uint on the wire.
        SendMessage(compositorObjectId, 0, WriteUint(surfaceObjectId));
    }

    // Generic wl_registry.bind for any global -- same dynamic new_id
    // encoding as BindAndCreateSurface, just parameterized.
    public void Bind(uint globalName, string interfaceName, uint version, uint newObjectId)
    {
        var args = new List<byte>();
        args.AddRange(WriteUint(globalName));
        args.AddRange(WriteString(interfaceName));
        args.AddRange(WriteUint(version));
        args.AddRange(WriteUint(newObjectId));
        SendMessage(2, 0, args.ToArray());
    }

    // xdg_wm_base(wmBaseId).get_xdg_surface(new_id, surface) -- opcode 2.
    // Wire arg order is (new_id, surface object), matching the protocol
    // XML's request argument order.
    public void GetXdgSurface(uint wmBaseId, uint newXdgSurfaceId, uint surfaceObjectId)
    {
        var args = new List<byte>();
        args.AddRange(WriteUint(newXdgSurfaceId));
        args.AddRange(WriteUint(surfaceObjectId));
        SendMessage(wmBaseId, 2, args.ToArray());
    }

    // xdg_surface(xdgSurfaceId).get_toplevel(new_id) -- opcode 1.
    public void GetToplevel(uint xdgSurfaceId, uint newToplevelId) =>
        SendMessage(xdgSurfaceId, 1, WriteUint(newToplevelId));

    // xdg_surface(xdgSurfaceId).ack_configure(serial) -- opcode 4.
    public void AckConfigure(uint xdgSurfaceId, uint serial) =>
        SendMessage(xdgSurfaceId, 4, WriteUint(serial));

    // xdg_toplevel(toplevelId).destroy() -- opcode 0.
    public void DestroyToplevel(uint toplevelId) =>
        SendMessage(toplevelId, 0, Array.Empty<byte>());

    // wl_seat(seatId).get_pointer/get_keyboard/get_touch(new_id) -- opcodes 0,1,2.
    public void GetPointer(uint seatId, uint newId) => SendMessage(seatId, 0, WriteUint(newId));
    public void GetKeyboard(uint seatId, uint newId) => SendMessage(seatId, 1, WriteUint(newId));
    public void GetTouch(uint seatId, uint newId) => SendMessage(seatId, 2, WriteUint(newId));

    // wl_data_device_manager(managerId).create_data_source(new_id) -- opcode 0.
    public void CreateDataSource(uint managerId, uint newSourceId) =>
        SendMessage(managerId, 0, WriteUint(newSourceId));

    // wl_data_device_manager(managerId).get_data_device(new_id, seat) -- opcode 1.
    public void GetDataDevice(uint managerId, uint newDeviceId, uint seatId)
    {
        var args = new List<byte>();
        args.AddRange(WriteUint(newDeviceId));
        args.AddRange(WriteUint(seatId));
        SendMessage(managerId, 1, args.ToArray());
    }

    public uint? SeatCapabilities;
    public string? SeatName;

    public void WaitForSeatInfo(uint seatId, int timeoutMs)
    {
        _socket.ReceiveTimeout = timeoutMs;
        try
        {
            while (SeatCapabilities is null || SeatName is null)
            {
                var (objectId, opcode, args) = ReadMessage();
                HandleEvent(objectId, opcode, args);
                if (objectId == seatId && opcode == 0) // capabilities(uint)
                {
                    var offset = 0;
                    SeatCapabilities = ReadUint(args, ref offset);
                }
                else if (objectId == seatId && opcode == 1) // name(string)
                {
                    var offset = 0;
                    SeatName = ReadString(args, ref offset);
                }
            }
        }
        catch (SocketException)
        {
        }
    }

    // wl_surface(surfaceId).commit() -- opcode 6 in the wl_surface
    // request order (destroy isn't a wl_surface request; the real
    // opcode list is attach=1,damage=2,frame=3,set_opaque_region=4,
    // set_input_region=5,commit=6,...).
    public void CommitSurface(uint surfaceId) =>
        SendMessage(surfaceId, 6, Array.Empty<byte>());

    public uint? XdgSurfaceConfigureSerial;
    public (int Width, int Height)? ToplevelConfigureSize;

    private void HandleXdgEvent(uint objectId, ushort opcode, byte[] args, uint xdgSurfaceId, uint toplevelId)
    {
        if (objectId == xdgSurfaceId && opcode == 0) // xdg_surface.configure(serial)
        {
            var offset = 0;
            XdgSurfaceConfigureSerial = ReadUint(args, ref offset);
        }
        else if (objectId == toplevelId && opcode == 0) // xdg_toplevel.configure(w, h, states)
        {
            var offset = 0;
            var w = (int)ReadUint(args, ref offset);
            var h = (int)ReadUint(args, ref offset);
            ToplevelConfigureSize = (w, h);
        }
    }

    // Reads events until both xdg_surface.configure and
    // xdg_toplevel.configure have been observed, or the timeout elapses.
    public void WaitForXdgConfigure(uint xdgSurfaceId, uint toplevelId, int timeoutMs)
    {
        _socket.ReceiveTimeout = timeoutMs;
        try
        {
            while (XdgSurfaceConfigureSerial is null || ToplevelConfigureSize is null)
            {
                var (objectId, opcode, args) = ReadMessage();
                HandleEvent(objectId, opcode, args);
                HandleXdgEvent(objectId, opcode, args, xdgSurfaceId, toplevelId);
            }
        }
        catch (SocketException)
        {
            // Leaves the *Configure fields null/incomplete -- caller checks.
        }
    }

    // zxdg_decoration_manager_v1(managerId).get_toplevel_decoration(new_id, toplevel) -- opcode 1.
    public void GetToplevelDecoration(uint managerId, uint newDecorationId, uint toplevelId)
    {
        var args = new List<byte>();
        args.AddRange(WriteUint(newDecorationId));
        args.AddRange(WriteUint(toplevelId));
        SendMessage(managerId, 1, args.ToArray());
    }

    // zxdg_toplevel_decoration_v1(decorationId).set_mode(mode) -- opcode 1.
    public void SetDecorationMode(uint decorationId, uint mode) =>
        SendMessage(decorationId, 1, WriteUint(mode));

    // zxdg_toplevel_decoration_v1(decorationId).unset_mode() -- opcode 2.
    public void UnsetDecorationMode(uint decorationId) =>
        SendMessage(decorationId, 2, Array.Empty<byte>());

    public uint? LastDecorationMode;

    // Reads events until a zxdg_toplevel_decoration_v1.configure(mode)
    // event arrives for the given decoration object, or times out.
    public void WaitForDecorationConfigure(uint decorationId, int timeoutMs)
    {
        _socket.ReceiveTimeout = timeoutMs;
        LastDecorationMode = null;
        try
        {
            while (LastDecorationMode is null)
            {
                var (objectId, opcode, args) = ReadMessage();
                HandleEvent(objectId, opcode, args);
                if (objectId == decorationId && opcode == 0)
                {
                    var offset = 0;
                    LastDecorationMode = ReadUint(args, ref offset);
                }
            }
        }
        catch (SocketException)
        {
        }
    }

    // wl_compositor(compositorId).create_surface(new_id) -- reusable for
    // any additional surface beyond the one BindAndCreateSurface makes.
    public void CreateSurface(uint compositorId, uint newSurfaceId) =>
        SendMessage(compositorId, 0, WriteUint(newSurfaceId));

    // zwlr_layer_shell_v1(shellId).get_layer_surface(new_id, surface, output, layer, namespace) -- opcode 0.
    // `output` is a nullable object id: 0 means null (let the compositor pick).
    public void GetLayerSurface(uint shellId, uint newLayerSurfaceId, uint surfaceId, uint outputId, uint layer, string @namespace)
    {
        var args = new List<byte>();
        args.AddRange(WriteUint(newLayerSurfaceId));
        args.AddRange(WriteUint(surfaceId));
        args.AddRange(WriteUint(outputId));
        args.AddRange(WriteUint(layer));
        args.AddRange(WriteString(@namespace));
        SendMessage(shellId, 0, args.ToArray());
    }

    // zwlr_layer_surface_v1(layerSurfaceId).ack_configure(serial) -- opcode 6
    // (destroy=0? no -- real request order: set_size=0, set_anchor=1,
    // set_exclusive_zone=2, set_margin=3, set_keyboard_interactivity=4,
    // get_popup=5, ack_configure=6, destroy=7, set_layer=8, set_exclusive_edge=9,
    // matching the ServerListener declaration order reflected earlier).
    public void AckLayerConfigure(uint layerSurfaceId, uint serial) =>
        SendMessage(layerSurfaceId, 6, WriteUint(serial));

    public (uint Width, uint Height, uint Serial)? LayerConfigure;

    public void WaitForLayerConfigure(uint layerSurfaceId, int timeoutMs)
    {
        _socket.ReceiveTimeout = timeoutMs;
        LayerConfigure = null;
        try
        {
            while (LayerConfigure is null)
            {
                var (objectId, opcode, args) = ReadMessage();
                HandleEvent(objectId, opcode, args);
                if (objectId == layerSurfaceId && opcode == 0) // configure(serial, width, height)
                {
                    var offset = 0;
                    var serial = ReadUint(args, ref offset);
                    var w = ReadUint(args, ref offset);
                    var h = ReadUint(args, ref offset);
                    LayerConfigure = (w, h, serial);
                }
            }
        }
        catch (SocketException)
        {
        }
    }

    // Drain any events waiting right now (non-blocking-ish: short timeout),
    // used after create_surface to check for a protocol error without
    // blocking forever on a server that correctly sends nothing back.
    public void DrainPendingEvents(int timeoutMs)
    {
        _socket.ReceiveTimeout = timeoutMs;
        try
        {
            while (true)
            {
                var (objectId, opcode, args) = ReadMessage();
                HandleEvent(objectId, opcode, args);
            }
        }
        catch (SocketException)
        {
            // Timed out with nothing more pending -- expected, not an error.
        }
    }

    private void HandleEvent(uint objectId, ushort opcode, byte[] args)
    {
        if (objectId == 2 && opcode == 0) // wl_registry.global(name, interface, version)
        {
            var offset = 0;
            var name = ReadUint(args, ref offset);
            var iface = ReadString(args, ref offset);
            var version = ReadUint(args, ref offset);
            _globals.Add((name, iface, version));
        }
        else if (objectId == 3 && opcode == 0) // wl_callback.done
        {
            _syncDone = true;
        }
        else if (objectId == 1 && opcode == 0) // wl_display.error(object_id, code, message)
        {
            _sawError = true;
            var offset = 4; // skip the erroring object_id
            var code = ReadUint(args, ref offset);
            var message = ReadString(args, ref offset);
            _errorMessage = $"code={code}: {message}";
        }
    }

    private void SendMessage(uint objectId, ushort opcode, byte[] args)
    {
        var size = (ushort)(8 + args.Length);
        var header = new byte[8];
        BitConverter.GetBytes(objectId).CopyTo(header, 0);
        BitConverter.GetBytes((uint)((size << 16) | opcode)).CopyTo(header, 4);
        _socket.Send(header);
        if (args.Length > 0) _socket.Send(args);
    }

    private (uint ObjectId, ushort Opcode, byte[] Args) ReadMessage()
    {
        var header = ReadExact(8);
        var objectId = BitConverter.ToUInt32(header, 0);
        var sizeAndOpcode = BitConverter.ToUInt32(header, 4);
        var size = (int)(sizeAndOpcode >> 16);
        var opcode = (ushort)(sizeAndOpcode & 0xFFFF);
        var args = size > 8 ? ReadExact(size - 8) : Array.Empty<byte>();
        return (objectId, opcode, args);
    }

    private byte[] ReadExact(int count)
    {
        var buf = new byte[count];
        var read = 0;
        while (read < count)
        {
            var n = _socket.Receive(buf, read, count - read, SocketFlags.None);
            if (n == 0) throw new EndOfStreamException("server closed connection");
            read += n;
        }
        return buf;
    }

    private static byte[] WriteUint(uint v) => BitConverter.GetBytes(v);

    private static byte[] WriteString(string s)
    {
        var bytes = Encoding.UTF8.GetBytes(s);
        var len = bytes.Length + 1; // + null terminator
        var padded = (len + 3) / 4 * 4;
        var result = new byte[4 + padded];
        BitConverter.GetBytes((uint)len).CopyTo(result, 0);
        bytes.CopyTo(result, 4);
        return result;
    }

    private static uint ReadUint(byte[] buf, ref int offset)
    {
        var v = BitConverter.ToUInt32(buf, offset);
        offset += 4;
        return v;
    }

    private static string ReadString(byte[] buf, ref int offset)
    {
        var len = (int)ReadUint(buf, ref offset);
        var s = Encoding.UTF8.GetString(buf, offset, Math.Max(0, len - 1));
        var padded = (len + 3) / 4 * 4;
        offset += padded;
        return s;
    }

    public void Dispose() => _socket.Dispose();
}
