using System.Runtime.InteropServices;

namespace Pancake.Cn;

// P/Invoke bindings against libEGL.so -- context creation bound to a GBM
// device, no window system involved. This is the piece that stays "Cn"
// even after collapsing Cn to plain C#: it's real unsafe/P-Invoke work,
// just expressed in ordinary C# instead of a made-up DSL.
internal static unsafe partial class Egl
{
    private const string Lib = "libEGL.so.1";

    internal const int EGL_NONE = 0x3038;
    internal const int EGL_RENDERABLE_TYPE = 0x3040;
    internal const int EGL_OPENGL_ES2_BIT = 0x0004;
    internal const int EGL_CONTEXT_CLIENT_VERSION = 0x3098;
    internal const int EGL_PLATFORM_GBM_KHR = 0x31D7;

    [LibraryImport(Lib)]
    internal static partial nint eglGetPlatformDisplay(int platform, nint nativeDisplay, nint attribList);

    [LibraryImport(Lib)]
    internal static partial int eglInitialize(nint dpy, out int major, out int minor);

    [LibraryImport(Lib)]
    internal static partial int eglBindAPI(uint api);

    [LibraryImport(Lib)]
    internal static partial int eglChooseConfig(nint dpy, int[] attribList, out nint config, int configSize, out int numConfig);

    [LibraryImport(Lib)]
    internal static partial nint eglCreateContext(nint dpy, nint config, nint shareContext, int[] attribList);

    [LibraryImport(Lib)]
    internal static partial int eglMakeCurrent(nint dpy, nint draw, nint read, nint ctx);

    [LibraryImport(Lib)]
    internal static partial int eglDestroyContext(nint dpy, nint ctx);

    [LibraryImport(Lib)]
    internal static partial int eglTerminate(nint dpy);

    [LibraryImport(Lib)]
    internal static partial int eglGetError();

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial nint eglGetProcAddress(string procname);

    internal const uint EGL_OPENGL_ES_API = 0x30A0;
}
