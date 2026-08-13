// Port of src/main.rs -- the actual CLI entry point tying config,
// logging, SIGHUP handling, and backend selection together. Being
// deliberately honest here rather than papering over gaps: this wires up
// everything that's real from this session (PancakeWaylandServer, the
// Space/WorkspaceManager/CompositorState it now feeds, Pancake.Cn's
// GBM/EGL/DRM/libinput bring-up) but does NOT claim a fully working
// compositor -- the actual frame loop connecting a live GPU context to
// Wayland surface damage to on-screen pixels was never built this
// session (that's real remaining work, tracked honestly below, not
// hidden behind a working `--help` output).

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

    Log("NOTE: GPU device bring-up (Pancake.Cn.GpuDevice) and the Wayland server " +
        "(Pancake.Wayland.PancakeWaylandServer) both exist and are verified " +
        "independently (see readmenow.md), but nothing in this session wired them " +
        "into a single frame loop -- there is no live rendering yet. A real client " +
        "can connect and create windows (see the wayland: checks in " +
        "Pancake.Syrup.Smoke), but nothing draws them to a screen.");

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
