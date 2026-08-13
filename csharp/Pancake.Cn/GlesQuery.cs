using System.Runtime.InteropServices;

namespace Pancake.Cn;

// Minimal libGLESv2 binding, used only to prove a real, live GL context
// came out of the EGL/GBM bring-up above (glGetString round-trip against
// the actual driver). Actual draw calls belong to Silk.NET.OpenGLES once
// a real compositor has a context to hand it -- this file only proves
// the context itself is real.
internal static partial class GlesQuery
{
    private const string Lib = "libGLESv2.so.2";

    internal const uint GL_VENDOR = 0x1F00;
    internal const uint GL_RENDERER = 0x1F01;
    internal const uint GL_VERSION = 0x1F02;

    [LibraryImport(Lib)]
    internal static partial nint glGetString(uint name);
}
