using System.Runtime.InteropServices;

namespace Pancake.Cn;

// P/Invoke bindings against libinput.so -- the real hardware-input half
// of what input.rs would eventually run on top of (see Pancake.Wayland's
// Seat.cs for the wl_seat/wl_pointer/wl_keyboard wire-protocol half).
// Uses the "path" backend (libinput_path_create_context +
// libinput_path_add_device) rather than the udev backend real Pancake
// uses (libinput_udev_create_context) -- the udev backend needs seat
// assignment via libseat/logind to enumerate devices automatically,
// which is session/privilege territory beyond a single evdev node;
// the path backend talks to the exact same libinput.so and processes
// real evdev devices, it's just handed explicit device paths instead of
// discovering them. Same "real slice of the real library" approach as
// Gbm.cs/Egl.cs/Drm.cs.
internal static unsafe partial class Libinput
{
    private const string Lib = "libinput.so.10";

    // libinput_event_type (relevant subset; full enum has ~30 members).
    internal const int LIBINPUT_EVENT_DEVICE_ADDED = 1;
    internal const int LIBINPUT_EVENT_DEVICE_REMOVED = 2;

    [StructLayout(LayoutKind.Sequential)]
    internal struct LibinputInterface
    {
        public nint OpenRestricted;
        public nint CloseRestricted;
    }

    [UnmanagedCallersOnly(CallConvs = new[] { typeof(System.Runtime.CompilerServices.CallConvCdecl) })]
    internal static int OpenRestricted(byte* path, int flags, void* userData)
    {
        var pathStr = Marshal.PtrToStringUTF8((nint)path) ?? "";
        return Libc.open(pathStr, flags);
    }

    [UnmanagedCallersOnly(CallConvs = new[] { typeof(System.Runtime.CompilerServices.CallConvCdecl) })]
    internal static void CloseRestricted(int fd, void* userData) => Libc.close(fd);

    [LibraryImport(Lib)]
    internal static partial nint libinput_path_create_context(in LibinputInterface iface, nint userData);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial nint libinput_path_add_device(nint libinput, string path);

    [LibraryImport(Lib)]
    internal static partial int libinput_get_fd(nint libinput);

    [LibraryImport(Lib)]
    internal static partial int libinput_dispatch(nint libinput);

    [LibraryImport(Lib)]
    internal static partial nint libinput_get_event(nint libinput);

    [LibraryImport(Lib)]
    internal static partial int libinput_event_get_type(nint evt);

    [LibraryImport(Lib)]
    internal static partial void libinput_event_destroy(nint evt);

    [LibraryImport(Lib)]
    internal static partial nint libinput_unref(nint libinput);
}
