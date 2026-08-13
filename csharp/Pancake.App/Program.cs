// Port of src/main.rs -- the actual CLI entry point tying config,
// logging, SIGHUP handling, and backend selection together. Being
// deliberately honest here rather than papering over gaps: this wires up
// everything that's real from this session (PancakeWaylandServer, the
// Space/WorkspaceManager/CompositorState it now feeds, Pancake.Cn's
// GBM/EGL/DRM/libinput bring-up, and now the real frame loop tying a
// live GPU context to AeroRenderer + wl_callback delivery) but does NOT
// claim a fully working compositor -- real DRM atomic modeset/pageflip
// (turning a rendered frame into actual on-screen pixels) still isn't
// built, only enumeration exists in Pancake.Cn.DrmResources. That's
// real remaining work, tracked honestly below, not hidden behind a
// working `--help` output.

using Pancake.Config;

var winit = args.Contains("--winit");
byte? tty = null;
var ttyIdx = Array.IndexOf(args, "--tty");
if (ttyIdx >= 0 && ttyIdx + 1 < args.Length && byte.TryParse(args[ttyIdx + 1], out var t))
    tty = t;

Log("Pancake starting");

ReloadSignal.Install();

if (winit)
{
    Log("Starting Pancake with nested-window backend (development mode)");
    Log("NOTE: real GLES rendering (the Aero blur pipeline) runs each frame; " +
        "border/decoration/cursor compositing is not included yet (needs a small " +
        "solid-quad shader this session didn't build) -- see readmenow.md.");
    return Pancake.App.NestedBackend.Run();
}

Log($"Starting Pancake with udev/DRM backend{(tty is { } t2 ? $" (tty {t2})" : "")}");
return await RunUdevBackend();

static void Log(string message) => Console.WriteLine($"[pancake] {message}");

static async Task<int> RunUdevBackend()
{
    var config = PancakeConfig.Load();
    Log($"Config loaded: terminal={config.Terminal}, blur_passes={config.BlurPasses}");

    var runtimeDir = Environment.GetEnvironmentVariable("XDG_RUNTIME_DIR") ?? "/tmp";
    var socketPath = Path.Combine(runtimeDir, "pancake-wayland-0");

    await using var server = new Pancake.Wayland.PancakeWaylandServer(socketPath);
    server.Start();
    Log($"Wayland socket: {socketPath}");
    Log($"GPU device: {(server.GpuAvailable ? "live" : "unavailable (see readmenow.md -- this sandbox's known GPU fault, caught cleanly)")}");

    Log("NOTE: the frame loop is real (GpuDevice + AeroRenderer + real " +
        "wl_callback delivery, see readmenow.md), but real DRM atomic modeset/" +
        "pageflip isn't built -- only enumeration exists in " +
        "Pancake.Cn.DrmResources -- so even with a live GPU, rendered frames " +
        "don't reach an actual screen yet. A real client can connect, create " +
        "windows, and get real frame callbacks (see the wayland: checks in " +
        "Pancake.Syrup.Smoke).");

    Log("Press Ctrl+C to exit.");
    var exitSignal = new ManualResetEventSlim(false);
    Console.CancelKeyPress += (_, e) =>
    {
        e.Cancel = true;
        exitSignal.Set();
    };
    exitSignal.Wait();

    return 0;
}
