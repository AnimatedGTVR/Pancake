using System.Runtime.InteropServices;

namespace Pancake.Cn;

// P/Invoke bindings against libdrm.so's mode-setting helpers
// (xf86drmMode.h). This is the connector/CRTC/encoder enumeration slice
// of src/backend/gpu.rs's GpuData::init -- also plain shared-library
// P/Invoke, same as Gbm.cs/Egl.cs, confirming Cn doesn't need to be a
// separate language for this layer either. Struct layouts below mirror
// xf86drmMode.h's field order/types exactly so Sequential layout
// reproduces the same C alignment/padding on x86_64 Linux.
[StructLayout(LayoutKind.Sequential)]
internal struct DrmModeRes
{
    public int count_fbs;
    public nint fbs;
    public int count_crtcs;
    public nint crtcs;
    public int count_connectors;
    public nint connectors;
    public int count_encoders;
    public nint encoders;
    public uint min_width, max_width;
    public uint min_height, max_height;
}

[StructLayout(LayoutKind.Sequential)]
internal struct DrmModeConnector
{
    public uint connector_id;
    public uint encoder_id;
    public uint connector_type;
    public uint connector_type_id;
    public int connection; // drmModeConnection enum: 1=connected 2=disconnected 3=unknown
    public uint mmWidth, mmHeight;
    public int subpixel;
    public int count_modes;
    public nint modes;
    public int count_props;
    public nint props;
    public nint prop_values;
    public int count_encoders;
    public nint encoders;
}

internal static partial class Drm
{
    private const string Lib = "libdrm.so.2";

    internal const int DRM_MODE_CONNECTED = 1;

    [LibraryImport(Lib)]
    internal static partial nint drmModeGetResources(int fd);

    [LibraryImport(Lib)]
    internal static partial void drmModeFreeResources(nint ptr);

    [LibraryImport(Lib)]
    internal static partial nint drmModeGetConnector(int fd, uint connectorId);

    [LibraryImport(Lib)]
    internal static partial void drmModeFreeConnector(nint ptr);
}
