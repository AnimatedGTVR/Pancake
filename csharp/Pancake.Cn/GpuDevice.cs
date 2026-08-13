namespace Pancake.Cn;

// Port of the real-hardware slice of src/backend/gpu.rs's GpuData::init:
// open a DRM device node, wrap it in a GBM device, and stand up an EGL
// context bound to it with no window system involved. This is the part
// that was flagged as the actual Cn/unsafe surface -- ordinary C#
// unsafe + P/Invoke against libgbm/libEGL, no separate compiler needed.
//
// What this does NOT cover (still to port, needs a real DRM master / a
// real display to test against, unlike this file's GBM+EGL bring-up
// which was verified against a real render node): connector/CRTC/encoder
// enumeration and atomic modeset/page-flip, which live behind libdrm.so
// and require DRM_MASTER on a "card" node, not a render node.
public sealed class GpuDevice : IDisposable
{
    private int _fd = -1;
    private nint _gbm;
    private nint _eglDisplay;
    private nint _eglContext;

    public string GlVendor { get; private set; } = "";
    public string GlRenderer { get; private set; } = "";
    public string GlVersion { get; private set; } = "";

    public static GpuDevice Open(string path)
    {
        var dev = new GpuDevice();
        try
        {
            dev._fd = Libc.open(path, Libc.O_RDWR | Libc.O_CLOEXEC);
            if (dev._fd < 0)
                throw new InvalidOperationException($"open({path}) failed (errno via Marshal.GetLastPInvokeError={System.Runtime.InteropServices.Marshal.GetLastPInvokeError()})");

            dev._gbm = Gbm.gbm_create_device(dev._fd);
            if (dev._gbm == 0)
                throw new InvalidOperationException("gbm_create_device failed");

            dev._eglDisplay = Egl.eglGetPlatformDisplay(Egl.EGL_PLATFORM_GBM_KHR, dev._gbm, 0);
            if (dev._eglDisplay == 0)
                throw new InvalidOperationException("eglGetPlatformDisplay failed");

            if (Egl.eglInitialize(dev._eglDisplay, out var major, out var minor) == 0)
                throw new InvalidOperationException($"eglInitialize failed (eglGetError=0x{Egl.eglGetError():X})");

            if (Egl.eglBindAPI(Egl.EGL_OPENGL_ES_API) == 0)
                throw new InvalidOperationException("eglBindAPI failed");

            int[] configAttribs =
            {
                Egl.EGL_RENDERABLE_TYPE, Egl.EGL_OPENGL_ES2_BIT,
                Egl.EGL_NONE,
            };
            if (Egl.eglChooseConfig(dev._eglDisplay, configAttribs, out var config, 1, out var numConfig) == 0 || numConfig == 0)
                throw new InvalidOperationException("eglChooseConfig found no matching config");

            int[] contextAttribs = { Egl.EGL_CONTEXT_CLIENT_VERSION, 2, Egl.EGL_NONE };
            dev._eglContext = Egl.eglCreateContext(dev._eglDisplay, config, 0, contextAttribs);
            if (dev._eglContext == 0)
                throw new InvalidOperationException("eglCreateContext failed");

            if (Egl.eglMakeCurrent(dev._eglDisplay, 0, 0, dev._eglContext) == 0)
                throw new InvalidOperationException("eglMakeCurrent (surfaceless) failed");

            dev.GlVendor = ReadGlString(GlesQuery.GL_VENDOR);
            dev.GlRenderer = ReadGlString(GlesQuery.GL_RENDERER);
            dev.GlVersion = ReadGlString(GlesQuery.GL_VERSION);

            return dev;
        }
        catch
        {
            dev.Dispose();
            throw;
        }
    }

    // Hands a raw GL function-pointer loader to whoever wants to build a
    // typed GL wrapper (e.g. Silk.NET.OpenGLES.GL.GetApi(GetProcAddress))
    // on top of this context, without Pancake.Cn itself depending on a GL
    // binding library -- Cn's job ends at "here is a live context."
    public nint GetProcAddress(string name) => Egl.eglGetProcAddress(name);

    private static string ReadGlString(uint name)
    {
        var ptr = GlesQuery.glGetString(name);
        return ptr == 0 ? "" : System.Runtime.InteropServices.Marshal.PtrToStringUTF8(ptr) ?? "";
    }

    public void Dispose()
    {
        if (_eglContext != 0 && _eglDisplay != 0)
        {
            Egl.eglMakeCurrent(_eglDisplay, 0, 0, 0);
            Egl.eglDestroyContext(_eglDisplay, _eglContext);
            _eglContext = 0;
        }
        if (_eglDisplay != 0)
        {
            Egl.eglTerminate(_eglDisplay);
            _eglDisplay = 0;
        }
        if (_gbm != 0)
        {
            Gbm.gbm_device_destroy(_gbm);
            _gbm = 0;
        }
        if (_fd >= 0)
        {
            Libc.close(_fd);
            _fd = -1;
        }
    }
}
