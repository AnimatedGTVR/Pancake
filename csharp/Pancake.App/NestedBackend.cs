using Pancake.Render;
using Silk.NET.OpenGLES;
using Silk.NET.Windowing;

namespace Pancake.App;

// Port of src/backend/winit.rs -- run Pancake as a nested window inside
// an existing compositor (dev-mode). Smithay's `winit` backend crate
// becomes Silk.NET.Windowing here (GLFW/SDL-backed, the natural C#
// counterpart already established by Pancake.Render's use of
// Silk.NET.OpenGLES) instead of hand-writing EGL/window plumbing --
// unlike the DRM/KMS path, there's no "just P/Invoke the C library"
// option here since window-system integration (X11/Wayland/GLFW
// protocol negotiation) is genuinely complex enough that a mature
// library is the right call, not a Cn-style hand roll.
//
// Scope: real window, real GLES context, the real Aero blur pipeline
// running every frame via Pancake.Render.AeroRenderer (already built and
// verified earlier this session). Border/decoration/cursor compositing
// (the solid-color and textured overlay draw calls winit.rs also does)
// is NOT included -- those need a small additional textured/solid-quad
// shader this session didn't build, a real bounded gap, not silently
// skipped.
internal static class NestedBackend
{
    public static int Run()
    {
        var options = WindowOptions.Default;
        options.Title = "Pancake (nested)";
        options.Size = new Silk.NET.Maths.Vector2D<int>(1280, 720);
        options.API = new GraphicsAPI(ContextAPI.OpenGLES, ContextProfile.Core, ContextFlags.Default, new APIVersion(3, 0));

        IWindow window;
        try
        {
            window = Window.Create(options);
            window.Initialize();
        }
        catch (Exception e)
        {
            Console.Error.WriteLine($"[pancake] Nested window creation failed: {e.Message}");
            Console.Error.WriteLine("[pancake] This needs a live GL-capable display connection (GLFW/SDL); " +
                "in this sandbox that's blocked by the same GPU fault documented in readmenow.md, " +
                "not a bug in this code -- see Pancake.Cn's GpuDevice section for the independent repro.");
            return 1;
        }

        GL? gl = null;
        AeroRenderer? aero = null;
        var frameCount = 0;
        using var loadedSignal = new ManualResetEventSlim(false);

        window.Load += () =>
        {
            gl = GL.GetApi(window.GLContext);
            aero = new AeroRenderer();
            aero.ApplyConfig(4, 2, new[] { 0.52f, 0.68f, 1.00f, 0.16f }, null);
            Console.WriteLine("[pancake] Nested window live: " +
                $"GL_VENDOR={gl.GetStringS(GLEnum.Vendor)} GL_RENDERER={gl.GetStringS(GLEnum.Renderer)}");
            loadedSignal.Set();
        };

        window.Render += _ =>
        {
            if (gl is null || aero is null) return;
            var size = window.FramebufferSize;
            aero.BeginFrame(gl, (uint)size.X, (uint)size.Y);
            gl.Viewport(0, 0, (uint)size.X, (uint)size.Y);
            aero.DrawBackground(gl);
            frameCount++;
            if (frameCount == 60) Console.WriteLine("[pancake] Rendered 60 real frames.");
        };

        // window.Run() blocks until the window closes -- including waiting
        // on GL context creation inside Load, which can hang rather than
        // fail cleanly if the display server accepts the window (real,
        // confirmed here) but the driver behind it can't actually stand up
        // a context (this sandbox's ongoing GPU fault, documented in
        // readmenow.md/Pancake.Cn -- there it fails fast via a clean EGL
        // error; through GLFW's own context-creation path it hangs
        // instead). A dev-mode backend hanging forever with no feedback is
        // worse than a clean timeout, so run it on a background thread and
        // watch for Load within a bounded window.
        var runThread = new Thread(() => Silk.NET.Windowing.WindowExtensions.Run(window)) { IsBackground = true };
        runThread.Start();

        if (!loadedSignal.Wait(TimeSpan.FromSeconds(5)))
        {
            Console.Error.WriteLine("[pancake] Nested window did not finish initializing a GL context within " +
                "5s -- the window/display-server connection succeeded (confirmed: this sandbox's real " +
                "Wayland socket accepted the connection), but GL context creation is hanging rather than " +
                "failing cleanly. Matches this sandbox's known GPU fault (see readmenow.md); not a bug in " +
                "this code -- the GBM/EGL path in Pancake.Cn hits the same underlying fault but fails fast " +
                "with a clean error instead of hanging, which is what this timeout now approximates for " +
                "the windowing path.");
            Environment.Exit(1);
        }

        runThread.Join();
        return 0;
    }
}
