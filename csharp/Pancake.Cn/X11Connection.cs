namespace Pancake.Cn;

// Real xcb connection bring-up -- connect, read the real handshake
// reply (protocol version, resource id base, the first screen's root
// window + dimensions), disconnect. This is the load-bearing assumption
// XWayland window-manager support would rest on: does a real xcb
// connection from C# actually work.
public sealed class X11Connection : IDisposable
{
    private nint _conn;

    public int ScreenNumber { get; private set; }
    public uint ProtocolMajorVersion { get; private set; }
    public uint ProtocolMinorVersion { get; private set; }
    public uint ResourceIdBase { get; private set; }
    public uint RootWindow { get; private set; }
    public uint RootWidth { get; private set; }
    public uint RootHeight { get; private set; }

    public static unsafe X11Connection Connect(string? displayName = null)
    {
        var conn = Xcb.xcb_connect(displayName, out var screen);
        var result = new X11Connection { _conn = conn, ScreenNumber = screen };

        var err = Xcb.xcb_connection_has_error(conn);
        if (err != 0)
        {
            result.Dispose();
            throw new InvalidOperationException($"xcb_connect failed (error code {err}) -- is DISPLAY set to a real X server?");
        }

        var setupPtr = Xcb.xcb_get_setup(conn);
        if (setupPtr == 0)
        {
            result.Dispose();
            throw new InvalidOperationException("xcb_get_setup returned null");
        }

        var setup = *(XcbSetup*)setupPtr;
        result.ProtocolMajorVersion = setup.ProtocolMajorVersion;
        result.ProtocolMinorVersion = setup.ProtocolMinorVersion;
        result.ResourceIdBase = setup.ResourceIdBase;

        var iter = Xcb.xcb_setup_roots_iterator(setupPtr);
        // Walk to the screen xcb_connect() selected (screen >= 0 means
        // "use this index"; a negative screen number back from
        // xcb_connect would mean the display string had no explicit
        // screen suffix, which libxcb treats as 0).
        var targetIndex = Math.Max(0, screen);
        for (var i = 0; i < targetIndex && iter.Rem > 0; i++)
        {
            iter.Data += sizeof(XcbScreen);
            iter.Rem--;
        }

        if (iter.Rem > 0 && iter.Data != 0)
        {
            var scr = *(XcbScreen*)iter.Data;
            result.RootWindow = scr.Root;
            result.RootWidth = scr.WidthInPixels;
            result.RootHeight = scr.HeightInPixels;
        }

        return result;
    }

    public void Dispose()
    {
        if (_conn != 0)
        {
            Xcb.xcb_disconnect(_conn);
            _conn = 0;
        }
    }
}
