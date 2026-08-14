using System.Runtime.InteropServices;

namespace Pancake.Cn;

// P/Invoke bindings against libxcb.so -- the X11 client protocol layer
// XWayland's window-manager half (src/handlers/xwayland.rs) would run
// on top of. Confirmed via readmenow.md's research: no viable C# X11
// library exists, but libxcb.so is an ordinary shared library with a
// stable C API, so this is the same "real slice of a real library"
// approach as Gbm.cs/Egl.cs/Drm.cs/Libinput.cs -- not a new pattern.
//
// This is deliberately just connection setup (connect, read the
// handshake reply, disconnect) -- proving the load-bearing assumption
// (a real xcb_connect against a real X server works from C#) before
// building out the actual window-manager request/event surface
// (CreateWindow, ConfigureWindow, property get/set, ICCCM/EWMH) that
// real XWayland integration would need.
[StructLayout(LayoutKind.Sequential)]
internal struct XcbSetup
{
    public byte Status;
    public byte Pad0;
    public ushort ProtocolMajorVersion;
    public ushort ProtocolMinorVersion;
    public ushort Length;
    public uint ReleaseNumber;
    public uint ResourceIdBase;
    public uint ResourceIdMask;
    public uint MotionBufferSize;
    public ushort VendorLen;
    public ushort MaximumRequestLength;
    public byte RootsLen;
    public byte PixmapFormatsLen;
    public byte ImageByteOrder;
    public byte BitmapFormatBitOrder;
    public byte BitmapFormatScanlineUnit;
    public byte BitmapFormatScanlinePad;
    public byte MinKeycode;
    public byte MaxKeycode;
    // 4 bytes padding follow; not needed for the fields read here.
}

[StructLayout(LayoutKind.Sequential)]
internal struct XcbScreen
{
    public uint Root;
    public uint DefaultColormap;
    public uint WhitePixel;
    public uint BlackPixel;
    public uint CurrentInputMasks;
    public ushort WidthInPixels;
    public ushort HeightInPixels;
    public ushort WidthInMillimeters;
    public ushort HeightInMillimeters;
    public ushort MinInstalledMaps;
    public ushort MaxInstalledMaps;
    public uint RootVisual;
    public byte BackingStores;
    public byte SaveUnders;
    public byte RootDepth;
    public byte AllowedDepthsLen;
}

[StructLayout(LayoutKind.Sequential)]
internal struct XcbScreenIterator
{
    public nint Data; // XcbScreen*
    public int Rem;
    public int Index;
}

internal static partial class Xcb
{
    private const string Lib = "libxcb.so.1";

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial nint xcb_connect(string? displayname, out int screenp);

    [LibraryImport(Lib)]
    internal static partial int xcb_connection_has_error(nint c);

    [LibraryImport(Lib)]
    internal static partial nint xcb_get_setup(nint c);

    [LibraryImport(Lib)]
    internal static partial XcbScreenIterator xcb_setup_roots_iterator(nint setup);

    [LibraryImport(Lib)]
    internal static partial void xcb_disconnect(nint c);

    [LibraryImport(Lib)]
    internal static partial int xcb_flush(nint c);
}
