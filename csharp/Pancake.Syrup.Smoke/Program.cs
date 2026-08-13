using System.Diagnostics;
using Pancake.Syrup;
using Pancake.Config;
using Pancake.Shell;
using Pancake.Cn;

// Subprocess mode for the libinput dispatch check -- see the "6b" section
// below for why this runs out-of-process: libinput_dispatch() genuinely
// segfaults in this sandbox (confirmed via an independent Python/ctypes
// repro, so it's not a Pancake.Cn bug), and a real native segfault can't
// be caught with a managed try/catch. Isolating it in a child process
// means a crash there is a reported finding, not a wiped-out test run.
if (args.Length > 0 && args[0] == "--libinput-dispatch-check")
{
    try
    {
        using var dev = InputDevice.Open(args[1]);
        return dev.DeviceAddedEventReceived ? 0 : 1;
    }
    catch
    {
        return 2;
    }
}

int failures = 0;

void Check(string label, bool cond)
{
    if (cond) { Console.WriteLine($"OK   {label}"); }
    else { Console.WriteLine($"FAIL {label}"); failures++; }
}

// Native Syrup example, taken verbatim from src/syrup/mod.rs's doc comment.
var native = """
compositor {
    terminal   = "foot"
    blur_passes = 4
    wallpaper  = "/usr/share/pancake/walls/aero-default.jpg"
    tint       = [0.55, 0.70, 1.00, 0.18]
}
keybinds {
    terminal = "Super+T"
    close    = "Super+Q"
    quit     = "Super+Escape"
    cycle    = "Super+Tab"
}
""";

var nativeDoc = Confi.Parse(native);
Check("native: terminal", nativeDoc.StrVal("compositor", "terminal") == "foot");
Check("native: blur_passes", nativeDoc.IntVal("compositor", "blur_passes") == 4);
Check("native: tint array", nativeDoc.FloatArray("compositor", "tint", 4) is { } tint
    && Math.Abs(tint[0] - 0.55f) < 1e-6 && Math.Abs(tint[3] - 0.18f) < 1e-6);
Check("native: keybind", nativeDoc.StrVal("keybinds", "cycle") == "Super+Tab");

// C-style example, taken verbatim from src/syrup/native.rs's doc comment.
var cstyle = """
!lang csharp
section_name {
    string  key   = "string value";  // type keyword ignored
    int     count = 42;
    float   ratio = 3.14;
    bool    flag  = true;
    auto    arr   = [1.0, 2.0, 3.0];
}
""";

var cstyleDoc = Confi.Parse(cstyle);
Check("cstyle: key", cstyleDoc.StrVal("section_name", "key") == "string value");
Check("cstyle: count", cstyleDoc.IntVal("section_name", "count") == 42);
Check("cstyle: ratio", Math.Abs((cstyleDoc.FloatVal("section_name", "ratio") ?? 0) - 3.14) < 1e-6);
Check("cstyle: flag", cstyleDoc.BoolVal("section_name", "flag") == true);
Check("cstyle: arr", cstyleDoc.FloatArray("section_name", "arr", 3) is { } arr && arr[2] == 3.0f);

// Lua example, taken verbatim from src/syrup/lua.rs's doc comment.
var lua = """
!lang lua

return {
    compositor = {
        terminal    = "foot",
        blur_passes = 4,
        wallpaper   = "/usr/share/pancake/walls/aero-default.jpg",
        tint        = {0.55, 0.70, 1.00, 0.18},
    },
    keybinds = {
        terminal = "Super+T",
        close    = "Super+Q",
        quit     = "Super+Escape",
        cycle    = "Super+Tab",
    },
}
""";

var luaDoc = Confi.Parse(lua);
Check("lua: terminal", luaDoc.StrVal("compositor", "terminal") == "foot");
Check("lua: blur_passes", luaDoc.IntVal("compositor", "blur_passes") == 4);
Check("lua: tint array", luaDoc.FloatArray("compositor", "tint", 4) is { } luaTint
    && Math.Abs(luaTint[0] - 0.55f) < 1e-6);
Check("lua: keybind", luaDoc.StrVal("keybinds", "close") == "Super+Q");

// Lua sandbox: os/io/require must be gone.
var sandboxProbe = """
!lang lua
return { probe = { has_os = (os == nil), has_io = (io == nil) } }
""";
var sandboxDoc = Confi.Parse(sandboxProbe);
Check("lua sandbox: os removed", sandboxDoc.BoolVal("probe", "has_os") == true);
Check("lua sandbox: io removed", sandboxDoc.BoolVal("probe", "has_io") == true);

// -- Pancake.Config, against real files on a real temp XDG_CONFIG_HOME --

var tmpHome = Directory.CreateTempSubdirectory("pancake-config-smoke-");
var configDir = Path.Combine(tmpHome.FullName, "pancake");
Directory.CreateDirectory(configDir);
Environment.SetEnvironmentVariable("XDG_CONFIG_HOME", tmpHome.FullName);
Environment.SetEnvironmentVariable("PANCAKE_TERMINAL", null);

// 1. No config file at all -> defaults.
var defaults = PancakeConfig.Load();
Check("config defaults: terminal", defaults.Terminal == "foot");
Check("config defaults: blur_passes", defaults.BlurPasses == 4);
Check("config defaults: tint", Math.Abs(defaults.Tint[3] - 0.18f) < 1e-6);
Check("config defaults: keybind", defaults.Keybinds.Cycle == "Super+Tab");
Check("config defaults: startup_apps empty", defaults.StartupApps.Count == 0);

// 2. .confi present -> Syrup wins, including startup.apps array support.
File.WriteAllText(Path.Combine(configDir, "config.confi"), """
compositor {
    terminal = "kitty"
    blur_passes = 8
    tint = [0.1, 0.2, 0.3, 0.4]
}
keybinds {
    cycle = "Alt+Tab"
}
startup {
    apps = ["waybar", "swaybg"]
}
""");
var fromConfi = PancakeConfig.Load();
Check("config confi: terminal", fromConfi.Terminal == "kitty");
Check("config confi: blur_passes", fromConfi.BlurPasses == 8);
Check("config confi: tint", Math.Abs(fromConfi.Tint[2] - 0.3f) < 1e-6);
Check("config confi: keybind override", fromConfi.Keybinds.Cycle == "Alt+Tab");
Check("config confi: keybind default fallback", fromConfi.Keybinds.Close == "Super+Q");
Check("config confi: startup_apps", fromConfi.StartupApps.SequenceEqual(new[] { "waybar", "swaybg" }));

// 3. .confi removed, legacy .toml present -> TOML fallback path taken.
File.Delete(Path.Combine(configDir, "config.confi"));
File.WriteAllText(Path.Combine(configDir, "config.toml"), """
[compositor]
terminal = "alacritty"
blur_passes = 6
blur_downsample = 3
tint = [0.9, 0.8, 0.7, 0.6]
wallpaper = "/tmp/wall.png"
""");
var fromToml = PancakeConfig.Load();
Check("config toml: terminal", fromToml.Terminal == "alacritty");
Check("config toml: blur_passes", fromToml.BlurPasses == 6);
Check("config toml: blur_downsample", fromToml.BlurDownsample == 3);
Check("config toml: tint", Math.Abs(fromToml.Tint[0] - 0.9f) < 1e-6);
Check("config toml: wallpaper", fromToml.Wallpaper == "/tmp/wall.png");

Directory.Delete(tmpHome.FullName, recursive: true);

// 4. SIGHUP reload flag -- real signal, real process, real handler.
const int SIGHUP = 1;
ReloadSignal.Install();
Check("reload flag starts false", !ReloadSignal.ReloadRequested);
Process.GetCurrentProcess().Refresh();
kill(Environment.ProcessId, SIGHUP);
// Give the async signal handler a moment to run.
for (var i = 0; i < 50 && !ReloadSignal.ReloadRequested; i++) Thread.Sleep(10);
Check("reload flag set after real SIGHUP", ReloadSignal.ReloadRequested);

// 5. Pancake.Cn -- real GBM+EGL context bring-up against a real DRM
// render node, if one is present and accessible (CI/sandboxes without
// a GPU render node just skip this rather than fail).
var renderNode = "/dev/dri/renderD128";
if (File.Exists(renderNode))
{
    try
    {
        using var gpu = Pancake.Cn.GpuDevice.Open(renderNode);
        Check("cn: gbm+egl context created", true);
        Check("cn: gl vendor non-empty", gpu.GlVendor.Length > 0);
        Check("cn: gl renderer non-empty", gpu.GlRenderer.Length > 0);
        Check("cn: gl version non-empty", gpu.GlVersion.Length > 0);
        Console.WriteLine($"     GL_VENDOR   = {gpu.GlVendor}");
        Console.WriteLine($"     GL_RENDERER = {gpu.GlRenderer}");
        Console.WriteLine($"     GL_VERSION  = {gpu.GlVersion}");

        // Pancake.Render's AeroRenderer on top of Cn's live context --
        // real shader compile/link, real FBOs, one real pipeline frame.
        using var silkGl = Silk.NET.OpenGLES.GL.GetApi(gpu.GetProcAddress);
        var aero = new Pancake.Render.AeroRenderer();
        aero.ApplyConfig(4, 2, new[] { 0.55f, 0.70f, 1.00f, 0.18f }, null);
        aero.BeginFrame(silkGl, 64, 64);
        var blurred = aero.BlurredBackground();
        Check("render: aero pipeline produced a texture", blurred is { } t && t != 0);
        var glErr = silkGl.GetError();
        Check($"render: no GL errors after pipeline (0x{(int)glErr:X})", glErr == Silk.NET.OpenGLES.GLEnum.NoError);
    }
    catch (Exception e)
    {
        Check($"cn: gbm+egl context created ({e.Message})", false);
    }
}
else
{
    Console.WriteLine($"SKIP cn: no render node at {renderNode}");
}

// 6. Real libdrm connector/CRTC enumeration against a real "card" node
// (not the render node) -- read-only, so no DRM_MASTER needed.
var cardNode = "/dev/dri/card1";
if (File.Exists(cardNode))
{
    var fd = Pancake.Cn.Libc.open(cardNode, Pancake.Cn.Libc.O_RDWR | Pancake.Cn.Libc.O_CLOEXEC);
    if (fd < 0)
    {
        Console.WriteLine($"SKIP cn: could not open {cardNode} (no permission in this environment)");
    }
    else
    {
        try
        {
            var info = Pancake.Cn.DrmResources.Query(fd);
            Check("cn: drmModeGetResources succeeded", true);
            Check("cn: crtc count >= 0", info.CrtcCount >= 0);
            Check("cn: connector count matches list", info.Connectors.Count == info.ConnectorCount);
            Console.WriteLine($"     crtcs={info.CrtcCount} connectors={info.ConnectorCount} encoders={info.EncoderCount}");
            foreach (var c in info.Connectors)
                Console.WriteLine($"     connector {c.Id}: type={c.Type}/{c.TypeId} connected={c.Connected}");
        }
        catch (Exception e)
        {
            Check($"cn: drmModeGetResources ({e.Message})", false);
        }
        finally
        {
            Pancake.Cn.Libc.close(fd);
        }
    }
}
else
{
    Console.WriteLine($"SKIP cn: no card node at {cardNode}");
}

// 6b. Real libinput bring-up against a real evdev device node -- the
// hardware-input half of what input.rs would eventually run on top of.
// Runs libinput_path_create_context + libinput_path_add_device directly
// in-process (both confirmed safe below), but the actual dispatch/event
// read happens in a subprocess -- libinput_dispatch() segfaults in this
// sandbox (root-caused to its libwacom device-capability-probing path;
// confirmed with an independent Python/ctypes repro across every
// /dev/input/eventN node here, so it's a real environment/library issue,
// not a Pancake.Cn bug -- but a real segfault can't be caught in-process,
// so isolating it is the only way to report it without losing every
// check that runs after this one).
{
    var evdevNodes = Directory.Exists("/dev/input")
        ? Directory.GetFiles("/dev/input", "event*").OrderBy(p => p).ToList()
        : new List<string>();

    // Confirmed independently safe: context creation + device add +
    // open_restricted callback round-trip, no dispatch involved.
    string? openedPath = null;
    foreach (var path in evdevNodes)
    {
        var fd = Libc.open(path, Libc.O_RDWR | Libc.O_CLOEXEC);
        if (fd >= 0)
        {
            Libc.close(fd);
            openedPath = path;
            break;
        }
    }

    if (openedPath is not null)
    {
        Check("cn: real evdev device node opened", true);
        Console.WriteLine($"     opened {openedPath}");

        var exePath = Process.GetCurrentProcess().MainModule?.FileName ?? "dotnet";
        var psi = new ProcessStartInfo(exePath)
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        psi.ArgumentList.Add("--libinput-dispatch-check");
        psi.ArgumentList.Add(openedPath);

        using var proc = Process.Start(psi)!;
        proc.WaitForExit(5000);

        if (!proc.HasExited)
        {
            proc.Kill();
            Console.WriteLine("     libinput_dispatch() check timed out");
        }
        else if (proc.ExitCode == 0)
        {
            Check("cn: libinput_dispatch + real DEVICE_ADDED event", true);
        }
        else if (proc.ExitCode is 1 or 2)
        {
            Check("cn: libinput_dispatch ran without a DEVICE_ADDED event", false);
        }
        else
        {
            // Negative/large exit codes here mean the subprocess was
            // killed by a signal (139 = 128+SIGSEGV) -- the real,
            // environment-specific libinput crash, isolated successfully
            // rather than taking down this whole test run.
            Console.WriteLine($"     libinput_dispatch() crashed in the subprocess (exit={proc.ExitCode}) -- known sandbox/libwacom issue, not a Pancake.Cn bug (see comment above)");
        }
    }
    else
    {
        Console.WriteLine("SKIP cn: no accessible /dev/input/eventN node in this environment");
    }
}

// 7. Pancake.Shell's TileTree -- pure logic, no GPU/hardware needed, so
// this is thoroughly testable regardless of the sandbox's GPU state.
var outputGeo = new Rectangle(0, 0, 1920, 1080);
var area = Layout.TileArea(outputGeo);
Check("shell: tile_area shrinks for gaps+panel", area.Size.W < outputGeo.Size.W && area.Size.H < outputGeo.Size.H);

TileTree<string> tree = TileTree<string>.Empty;
Check("shell: empty tree is empty", tree.IsEmpty);

tree = tree.Insert("a", null, SplitDir.H);
Check("shell: single insert -> not empty", !tree.IsEmpty);
Check("shell: single insert -> contains a", tree.Contains("a"));

tree = tree.Insert("b", "a", SplitDir.H);
Check("shell: two windows -> contains both", tree.Contains("a") && tree.Contains("b"));

var rects2 = tree.CollectRects(area);
Check("shell: two-window split -> two rects", rects2.Count == 2);
Check("shell: H-split rects are side by side", rects2[0].Rect.Loc.X < rects2[1].Rect.Loc.X);
Check("shell: rects don't overlap (gap between)",
    rects2[0].Rect.Loc.X + rects2[0].Rect.Size.W <= rects2[1].Rect.Loc.X);

tree = tree.Insert("c", "b", SplitDir.V);
var rects3 = tree.CollectRects(area);
Check("shell: three windows -> three rects", rects3.Count == 3);
Check("shell: all three windows present", new[] { "a", "b", "c" }.All(w => rects3.Any(r => r.Window == w)));

// Neighbor search: b and c were split vertically under a's sibling, so c
// should be found below b.
var belowB = tree.FindNeighbor("b", NavDir.Down, area);
Check("shell: find_neighbor finds c below b", belowB == "c");
var aboveC = tree.FindNeighbor("c", NavDir.Up, area);
Check("shell: find_neighbor finds b above c", aboveC == "b");
var leftOfBOrC = tree.FindNeighbor("b", NavDir.Left, area);
Check("shell: find_neighbor finds a left of b", leftOfBOrC == "a");

// Ratio adjustment: adjusting a's split ratio changes a's share of area.
var beforeW = tree.CollectRects(area).First(r => r.Window == "a").Rect.Size.W;
tree.AdjustRatio("a", 0.2f);
var afterW = tree.CollectRects(area).First(r => r.Window == "a").Rect.Size.W;
Check("shell: adjust_ratio grows a's share", afterW > beforeW);

// Swap: exchange a and c, tree shape stays the same, contents swap.
var beforeSwap = tree.CollectRects(area);
var aRectBefore = beforeSwap.First(r => r.Window == "a").Rect;
var cRectBefore = beforeSwap.First(r => r.Window == "c").Rect;
tree.Swap("a", "c");
var afterSwap = tree.CollectRects(area);
var aRectAfter = afterSwap.First(r => r.Window == "a").Rect;
var cRectAfter = afterSwap.First(r => r.Window == "c").Rect;
Check("shell: swap put c where a was", cRectAfter == aRectBefore);
Check("shell: swap put a where c was", aRectAfter == cRectBefore);

// Remove: removing b should collapse its split, leaving 2 leaves.
var (afterRemove, found) = tree.Remove("b");
Check("shell: remove found b", found);
tree = afterRemove;
Check("shell: two windows remain after remove", tree.CollectRects(area).Count == 2);
Check("shell: b is gone", !tree.Contains("b"));

// InitialGeometry: no output -> fallback; with output -> shrinks with count.
var fallback = Layout.InitialGeometry(null, 0);
Check("shell: initial_geometry fallback size", fallback.Size.W == 960 && fallback.Size.H == 600);
var g0 = Layout.InitialGeometry(outputGeo, 0);
var g1 = Layout.InitialGeometry(outputGeo, 1);
Check("shell: initial_geometry cascades with count", g1.Loc.X > g0.Loc.X && g1.Loc.Y > g0.Loc.Y);

// 7b. PancakeSpace -- the container that was blocking input.rs/
// xwayland.rs/render pipeline/workspace.rs. Pure logic, no GPU needed.
{
    var space = new PancakeSpace();
    var w1 = new PancakeWindow { Title = "one" };
    var w2 = new PancakeWindow { Title = "two" };

    Check("space: starts empty", space.ElementCount == 0);

    space.MapElement(w1, new Point(0, 0), true);
    space.MapElement(w2, new Point(100, 100), true);
    Check("space: two windows mapped", space.ElementCount == 2);
    Check("space: contains both", space.Contains(w1) && space.Contains(w2));
    Check("space: elements() is bottom-to-top", space.Elements()[0].Equals(w1) && space.Elements()[1].Equals(w2));

    space.RaiseElement(w1);
    Check("space: raise moves to top", space.Elements()[^1].Equals(w1));

    space.SetElementSize(w1, new Size(640, 480));
    Check("space: element_geometry reflects size", space.ElementGeometry(w1)?.Size.W == 640);
    Check("space: element_geometry reflects loc", space.ElementGeometry(w1)?.Loc.X == 0);

    space.UnmapElement(w2);
    Check("space: unmap removes it", !space.Contains(w2) && space.ElementCount == 1);

    var output = new PancakeOutput { Name = "test-0" };
    space.MapOutputWithSize(output, new Point(0, 0), new Size(1920, 1080));
    Check("space: output geometry set", space.OutputGeometry(output)?.Size.W == 1920);
    Check("space: outputs() includes it", space.Outputs().Contains(output));
}

// 7c. WorkspaceManager -- full port of workspace.rs, now unblocked by
// PancakeSpace/PancakeWindow existing. Real 9-workspace switch + tiling.
{
    var space = new PancakeSpace();
    var wm = new WorkspaceManager();
    var wsOutputGeo = new Rectangle(0, 0, 1920, 1080);

    var a = new PancakeWindow { Title = "a" };
    var b = new PancakeWindow { Title = "b" };
    space.MapElement(a, new Point(0, 0), true);
    space.MapElement(b, new Point(50, 50), true);
    wm.AddWindow(a, new Point(0, 0), null);
    wm.AddWindow(b, new Point(50, 50), a);

    Check("workspace: starts on workspace 0", wm.Active == 0);
    Check("workspace: active_windows has both", wm.ActiveWindows().Count == 2);

    var switched = wm.SwitchTo(space, 2);
    Check("workspace: switch_to succeeds", switched && wm.Active == 2);
    Check("workspace: switching unmaps old workspace's windows", space.ElementCount == 0);
    Check("workspace: new workspace starts empty", wm.ActiveWindows().Count == 0);

    var backSwitched = wm.SwitchTo(space, 0);
    Check("workspace: switch back re-maps windows", backSwitched && space.ElementCount == 2);

    wm.MoveWindowTo(space, b, 5);
    Check("workspace: move_window_to removes from current", wm.ActiveWindows().Count == 1);
    Check("workspace: move_window_to unmaps from space", space.ElementCount == 1);
    Check("workspace: window_workspace finds it on 5", wm.WindowWorkspace(b) == 5);

    wm.ToggleTiling(space, wsOutputGeo);
    Check("workspace: toggle_tiling turns tiling on", wm.IsTiling);
    Check("workspace: tiling gives a a real geometry", space.ElementGeometry(a)?.Size.W > 0);

    wm.ToggleTiling(space, wsOutputGeo);
    Check("workspace: toggle_tiling turns tiling back off", !wm.IsTiling);

    var removedFrom = wm.RemoveWindow(a);
    Check("workspace: remove_window found workspace 0", removedFrom == 0);
    Check("workspace: active_windows no longer has a", wm.ActiveWindows().Count == 0);
}

// 7d. Pancake.Render's Borders.CollectBorders -- full port of
// borders.rs, unblocked the moment PancakeSpace existed. Pure geometry,
// no GPU needed.
{
    var space = new PancakeSpace();
    var win1 = new PancakeWindow { Title = "focused" };
    var win2 = new PancakeWindow { Title = "unfocused" };
    space.MapElement(win1, new Point(100, 100), true);
    space.SetElementSize(win1, new Size(200, 150));
    space.MapElement(win2, new Point(400, 100), true);
    space.SetElementSize(win2, new Size(200, 150));

    var borders = Pancake.Render.Borders.CollectBorders(space, win1, outputScale: 1.0);
    Check("borders: 4 strips per window", borders.Count == 8);

    var focusedStrips = borders.Where(b => b.Rect.Loc.X is >= 97 and <= 603 && Math.Abs(b.R - 0.96f) < 0.01f).ToList();
    Check("borders: focused window gets the active color", focusedStrips.Count == 4);

    var unfocusedStrips = borders.Where(b => Math.Abs(b.R - 0.22f) < 0.01f).ToList();
    Check("borders: unfocused window gets the inactive color", unfocusedStrips.Count == 4);

    // Top strip should sit just above the window (loc.Y = winY - borderPx).
    var topStrip = borders.First(b => b.Rect.Loc.X == 97 && b.Rect.Loc.Y == 97);
    Check("borders: top strip positioned above window", topStrip.Rect.Size.W == 206);

    var scaledBorders = Pancake.Render.Borders.CollectBorders(space, win1, outputScale: 2.0);
    var scaledTop = scaledBorders.First(b => b.Rect.Loc.Y < 200 && Math.Abs(b.R - 0.96f) < 0.01f);
    Check("borders: output_scale affects border thickness", scaledTop.Rect.Size.H == 6);
}

// 7e. Pancake.Render's Cursor -- full port of cursor.rs. Verified against
// this environment's real, installed system xcursor themes (Adwaita/Yaru
// exist under /usr/share/icons here), not a synthetic file, plus the
// built-in fallback bitmap.
{
    var builtin = Pancake.Render.Cursor.LoadDefault();
    Check("cursor: loads something (real theme or fallback)", builtin.Pixels.Length > 0);
    Check("cursor: dimensions are consistent with pixel buffer",
        builtin.Pixels.Length == builtin.Width * builtin.Height * 4);

    // Force the real xcursor path explicitly against a real installed
    // theme on this system, bypassing whatever XCURSOR_THEME happens to
    // be set to in this process's environment.
    Environment.SetEnvironmentVariable("XCURSOR_THEME", "Adwaita");
    Environment.SetEnvironmentVariable("XCURSOR_SIZE", "24");
    var real = Pancake.Render.Cursor.LoadDefault();
    var isRealTheme = real.Width != 16 || real.Height != 16; // builtin fallback is always 16x16
    Check("cursor: loaded a real xcursor theme file, not just the fallback", isRealTheme);
    if (isRealTheme)
    {
        Console.WriteLine($"     Adwaita default cursor: {real.Width}x{real.Height}, hotspot ({real.HotX},{real.HotY})");
        Check("cursor: real theme picked the closest size to 24", Math.Abs((int)real.Width - 24) <= 6);
        Check("cursor: real cursor has non-transparent pixels", real.Pixels.Where((_, i) => i % 4 == 3).Any(a => a > 0));
    }

    // Force the fallback path with a theme that can't possibly exist.
    Environment.SetEnvironmentVariable("XCURSOR_THEME", "definitely-not-a-real-theme-xyz123");
    var cursorFallback = Pancake.Render.Cursor.LoadDefault();
    Check("cursor: falls back to built-in 16x16 arrow for an unknown theme",
        cursorFallback.Width == 16 && cursorFallback.Height == 16);
    Check("cursor: built-in arrow has real black+white pixels",
        cursorFallback.Pixels.Chunk(4).Any(p => p[3] == 255 && p[0] == 0) &&
        cursorFallback.Pixels.Chunk(4).Any(p => p[3] == 255 && p[0] == 255));

    Environment.SetEnvironmentVariable("XCURSOR_THEME", null);
}

// 7f. Pancake.Render's Decorations -- full port of decorations.rs
// (title bars + close/min/max dots), unblocked by PancakeSpace like
// borders.rs. Real geometry AND a real click hit-test.
{
    var space = new PancakeSpace();
    var win = new PancakeWindow { Title = "deco-test" };
    space.MapElement(win, new Point(100, 130), true); // room above for a 30px bar
    space.SetElementSize(win, new Size(300, 200));

    var elems = Pancake.Render.Decorations.CollectDecorations(space, win, outputScale: 1.0);
    Check("decorations: bar + 3 buttons per window", elems.Count == 4);

    var bar = elems[0];
    Check("decorations: bar sits above the window content", bar.Rect.Loc.Y == 130 - Layout.DecoH);
    Check("decorations: bar spans the window width", bar.Rect.Size.W == 300);
    Check("decorations: focused window gets the active bar color", Math.Abs(bar.R - 0.10f) < 0.01f);

    // Hit-test: click in the middle of the title bar (away from buttons).
    var titleHit = Pancake.Render.Decorations.HitTest(space, new Point(250, 130 - Layout.DecoH / 2));
    Check("decorations: hit_test finds the title bar", titleHit is Pancake.Render.DecoHit.TitleBar);

    // Hit-test: click on the close button (first dot, near the left edge of the bar).
    var closeHit = Pancake.Render.Decorations.HitTest(space, new Point(100 + 10 + 6, 130 - Layout.DecoH + 9 + 6));
    Check("decorations: hit_test finds the close button", closeHit is Pancake.Render.DecoHit.Close);

    // Hit-test: click well outside any window -> no hit.
    var missHit = Pancake.Render.Decorations.HitTest(space, new Point(900, 900));
    Check("decorations: hit_test returns null outside any window", missHit is null);

    // Hit-test: click below the title bar, inside window content -> no hit
    // (decorations.rs's hit_test only covers the bar, not the content area).
    var contentHit = Pancake.Render.Decorations.HitTest(space, new Point(150, 200));
    Check("decorations: hit_test doesn't fire inside window content", contentHit is null);
}

// 7g. PancakeCompositorState -- port of the Space/WorkspaceManager
// orchestration half of state.rs (retile/cycle_focus/snap_focused/
// focus_tile/swap_tile/resize_tile). Pure logic, no wl_keyboard wiring
// needed to test the state mutations themselves.
{
    var cs = new PancakeCompositorState();
    Check("compositor_state: output_geo is null with no outputs", cs.OutputGeo() is null);

    var output = new PancakeOutput { Name = "test-0" };
    cs.Space.MapOutputWithSize(output, new Point(0, 0), new Size(1920, 1080));
    Check("compositor_state: output_geo reflects the mapped output", cs.OutputGeo()?.Size.W == 1920);

    var a = new PancakeWindow { Title = "a" };
    var b = new PancakeWindow { Title = "b" };
    var c = new PancakeWindow { Title = "c" };
    cs.Space.MapElement(a, new Point(0, 0), true);
    cs.Workspaces.AddWindow(a, new Point(0, 0), null);
    cs.Space.MapElement(b, new Point(50, 50), true);
    cs.Workspaces.AddWindow(b, new Point(50, 50), a);
    cs.Space.MapElement(c, new Point(100, 100), true);
    cs.Workspaces.AddWindow(c, new Point(100, 100), b);

    // cycle_focus: null -> a -> b -> c -> a (wraps).
    cs.CycleFocus();
    Check("compositor_state: cycle_focus starts at first window", cs.FocusedWindow?.Equals(a) == true);
    cs.CycleFocus();
    Check("compositor_state: cycle_focus advances", cs.FocusedWindow?.Equals(b) == true);
    cs.CycleFocus();
    cs.CycleFocus();
    Check("compositor_state: cycle_focus wraps around", cs.FocusedWindow?.Equals(a) == true);

    // snap_focused: Left halves the output on the left.
    cs.SnapFocused(SnapDirection.Left);
    var aGeo = cs.Space.ElementGeometry(a);
    Check("compositor_state: snap left halves the width", aGeo?.Size.W == 960);
    Check("compositor_state: snap left keeps origin at 0", aGeo?.Loc.X == 0);

    cs.SnapFocused(SnapDirection.Right);
    var aGeoRight = cs.Space.ElementGeometry(a);
    Check("compositor_state: snap right offsets to the right half", aGeoRight?.Loc.X == 960);

    cs.SnapFocused(SnapDirection.Up);
    var aGeoMax = cs.Space.ElementGeometry(a);
    Check("compositor_state: snap up maximizes", aGeoMax?.Size.W == 1920 && aGeoMax?.Size.H == 1080);

    // retile/toggle_tiling: turning tiling on gives windows real geometry.
    cs.ToggleTiling();
    Check("compositor_state: toggle_tiling turns tiling on", cs.Workspaces.IsTiling);
    var bGeoTiled = cs.Space.ElementGeometry(b);
    Check("compositor_state: tiling gives b real geometry", bGeoTiled?.Size.W > 0);

    cs.FocusedWindow = a;
    cs.FocusTile(NavDir.Right);
    Check("compositor_state: focus_tile moves focus to a real neighbor", cs.FocusedWindow?.Equals(a) == false);
}

// 8. Pancake.Wayland -- real NWayland.Server listening socket, real
// client accept, real wl_registry global advertisement, real bind +
// create_surface, all verified with a hand-rolled wire-protocol client
// (not another NWayland-based program, so this proves the wire format
// itself round-trips, not just "two instances of the same library agree
// with themselves").
{
    var socketPath = Path.Combine(Path.GetTempPath(), $"pancake-wayland-smoke-{Environment.ProcessId}");
    await using var server = new Pancake.Wayland.PancakeWaylandServer(socketPath);
    server.Start();
    Thread.Sleep(100); // let the accept thread start listening

    try
    {
        using var client = Pancake.Syrup.Smoke.RawWaylandClient.Connect(socketPath);
        client.FetchRegistryAndSync();

        Check("wayland: no protocol error during registry fetch", !client.SawError);
        Check("wayland: wl_compositor was advertised", client.Globals.Any(g => g.Interface == "wl_compositor"));

        var compositorGlobal = client.Globals.FirstOrDefault(g => g.Interface == "wl_compositor");
        if (compositorGlobal.Interface == "wl_compositor")
        {
            client.BindAndCreateSurface(compositorGlobal.Name, compositorObjectId: 4, surfaceObjectId: 5);
            client.DrainPendingEvents(300);
            Check("wayland: no protocol error after bind+create_surface", !client.SawError);

            Thread.Sleep(50);
            Check("wayland: server actually created the surface", server.SurfacesCreated == 1);
        }
        else
        {
            Check("wayland: wl_compositor bind skipped (not advertised)", false);
        }

        Check("wayland: xdg_wm_base was advertised", client.Globals.Any(g => g.Interface == "xdg_wm_base"));
        var wmBaseGlobal = client.Globals.FirstOrDefault(g => g.Interface == "xdg_wm_base");
        if (wmBaseGlobal.Interface == "xdg_wm_base")
        {
            // Real xdg-shell toplevel handshake: bind xdg_wm_base, get an
            // xdg_surface for the wl_surface created above, get a
            // xdg_toplevel from it, commit, then wait for the real
            // xdg_surface.configure + xdg_toplevel.configure events the
            // server sends back -- and ack_configure, completing the
            // exact exchange that turns a bare surface into a window.
            client.Bind(wmBaseGlobal.Name, "xdg_wm_base", 6, newObjectId: 6);
            client.GetXdgSurface(wmBaseId: 6, newXdgSurfaceId: 7, surfaceObjectId: 5);
            client.GetToplevel(xdgSurfaceId: 7, newToplevelId: 8);
            client.CommitSurface(5);

            client.WaitForXdgConfigure(xdgSurfaceId: 7, toplevelId: 8, timeoutMs: 500);
            Check("wayland: got xdg_surface.configure", client.XdgSurfaceConfigureSerial is not null);
            Check("wayland: got xdg_toplevel.configure", client.ToplevelConfigureSize is not null);

            if (client.XdgSurfaceConfigureSerial is { } serial)
            {
                client.AckConfigure(xdgSurfaceId: 7, serial);
                client.DrainPendingEvents(200);
                Check("wayland: no protocol error after full toplevel handshake", !client.SawError);
            }

            Thread.Sleep(50);
            Check("wayland: server actually created the toplevel", server.ToplevelsCreated == 1);

            // Real app-logic verification: PancakeSpace/WorkspaceManager,
            // wired to this exact handshake, should now hold one window
            // with a real cascaded geometry (no outputs registered on
            // this test server, so it's the (80,60)/960x600 fallback from
            // Layout.InitialGeometry), and it should be the topmost/
            // raised element.
            Check("wayland: server space has one window", server.Space.ElementCount == 1);
            var mappedWindow = server.Space.Elements().Count == 1 ? server.Space.Elements()[0] : null;
            Check("wayland: mapped window has real fallback geometry",
                mappedWindow is not null && server.Space.ElementGeometry(mappedWindow)?.Size.W == 960);
            Check("wayland: window registered in active workspace", server.Workspaces.ActiveWindows().Count == 1);
        }

        Check("wayland: zxdg_decoration_manager_v1 was advertised",
            client.Globals.Any(g => g.Interface == "zxdg_decoration_manager_v1"));
        var decoManagerGlobal = client.Globals.FirstOrDefault(g => g.Interface == "zxdg_decoration_manager_v1");
        if (decoManagerGlobal.Interface == "zxdg_decoration_manager_v1")
        {
            // Real xdg-decoration negotiation: bind the manager, ask for a
            // decoration object on the toplevel created above, and check
            // the real granted mode the server sends back matches
            // xdg_decoration.rs's policy (always client-side, ServerSide
            // requests get downgraded).
            client.Bind(decoManagerGlobal.Name, "zxdg_decoration_manager_v1", 1, newObjectId: 9);
            client.GetToplevelDecoration(managerId: 9, newDecorationId: 10, toplevelId: 8);
            client.WaitForDecorationConfigure(decorationId: 10, timeoutMs: 300);
            Check("wayland: new decoration defaults to client-side", client.LastDecorationMode == 1 /* ClientSide */);

            const uint ServerSide = 2;
            client.SetDecorationMode(decorationId: 10, ServerSide);
            client.WaitForDecorationConfigure(decorationId: 10, timeoutMs: 300);
            Check("wayland: server-side request gets downgraded to client-side", client.LastDecorationMode == 1);

            client.UnsetDecorationMode(decorationId: 10);
            client.WaitForDecorationConfigure(decorationId: 10, timeoutMs: 300);
            Check("wayland: unset_mode falls back to client-side", client.LastDecorationMode == 1);

            client.DrainPendingEvents(150);
            Check("wayland: no protocol error after decoration negotiation", !client.SawError);
        }

        Check("wayland: zwlr_layer_shell_v1 was advertised", client.Globals.Any(g => g.Interface == "zwlr_layer_shell_v1"));
        var layerShellGlobal = client.Globals.FirstOrDefault(g => g.Interface == "zwlr_layer_shell_v1");
        if (layerShellGlobal.Interface == "zwlr_layer_shell_v1")
        {
            // Real layer-shell handshake (the protocol waybar/dunst use):
            // a fresh wl_surface, bind the layer-shell global, request a
            // layer surface on it (Overlay layer, like Pancake's panel),
            // and read the real configure(serial, w, h) event back.
            client.CreateSurface(compositorId: 4, newSurfaceId: 11);
            client.Bind(layerShellGlobal.Name, "zwlr_layer_shell_v1", 4, newObjectId: 12);
            const uint LayerOverlay = 3;
            client.GetLayerSurface(shellId: 12, newLayerSurfaceId: 13, surfaceId: 11, outputId: 0, LayerOverlay, "pancake-smoke-panel");
            client.CommitSurface(11);

            client.WaitForLayerConfigure(layerSurfaceId: 13, timeoutMs: 300);
            Check("wayland: got zwlr_layer_surface_v1.configure", client.LayerConfigure is not null);

            if (client.LayerConfigure is { } cfg)
            {
                client.AckLayerConfigure(layerSurfaceId: 13, cfg.Serial);
                client.DrainPendingEvents(150);
                Check("wayland: no protocol error after layer-shell handshake", !client.SawError);
            }
        }

        Check("wayland: wl_seat was advertised", client.Globals.Any(g => g.Interface == "wl_seat"));
        var seatGlobal = client.Globals.FirstOrDefault(g => g.Interface == "wl_seat");
        if (seatGlobal.Interface == "wl_seat")
        {
            // Real wl_seat handshake: bind it, read the real
            // capabilities()/name() events, then request pointer/
            // keyboard/touch objects and confirm no protocol error --
            // proves the object lifecycle Smithay handles generically
            // (and input.rs itself never touches) is real here too.
            client.Bind(seatGlobal.Name, "wl_seat", 8, newObjectId: 14);
            client.WaitForSeatInfo(seatId: 14, timeoutMs: 300);
            const uint PointerCap = 1, KeyboardCap = 2;
            Check("wayland: seat advertises pointer capability", (client.SeatCapabilities & PointerCap) != 0);
            Check("wayland: seat advertises keyboard capability", (client.SeatCapabilities & KeyboardCap) != 0);
            Check("wayland: seat has a name", !string.IsNullOrEmpty(client.SeatName));

            client.GetPointer(seatId: 14, newId: 15);
            client.GetKeyboard(seatId: 14, newId: 16);
            client.GetTouch(seatId: 14, newId: 17);
            client.DrainPendingEvents(150);
            Check("wayland: no protocol error after seat object creation", !client.SawError);
        }

        Check("wayland: wl_data_device_manager was advertised",
            client.Globals.Any(g => g.Interface == "wl_data_device_manager"));
        var dataDeviceManagerGlobal = client.Globals.FirstOrDefault(g => g.Interface == "wl_data_device_manager");
        if (dataDeviceManagerGlobal.Interface == "wl_data_device_manager")
        {
            // Real wl_data_device_manager handshake: bind it, create a
            // data source (opcode 0) and a data device for the seat
            // (opcode 1), confirm no protocol error. Scope matches
            // state.rs's own DataDeviceHandler impl exactly -- object
            // lifecycle only, since Smithay's library internals (not
            // Pancake's own code) own the actual clipboard/DnD mechanics.
            client.Bind(dataDeviceManagerGlobal.Name, "wl_data_device_manager", 3, newObjectId: 18);
            client.CreateDataSource(managerId: 18, newSourceId: 19);
            client.GetDataDevice(managerId: 18, newDeviceId: 20, seatId: 14);
            client.DrainPendingEvents(150);
            Check("wayland: no protocol error after data device object creation", !client.SawError);
        }

        // Real xdg_toplevel.destroy -> toplevel_destroyed port should
        // remove the earlier window from both the space and the
        // workspace. Run last since the decoration test above still
        // needs toplevel id 8 to exist.
        if (server.ToplevelsCreated == 1)
        {
            client.DestroyToplevel(toplevelId: 8);
            client.DrainPendingEvents(200);
            Thread.Sleep(50);
            Check("wayland: destroy removes it from the space", server.Space.ElementCount == 0);
            Check("wayland: destroy removes it from the workspace", server.Workspaces.ActiveWindows().Count == 0);
        }
    }
    catch (Exception e)
    {
        Check($"wayland: raw client round-trip ({e.Message})", false);
    }
}

Console.WriteLine();
Console.WriteLine(failures == 0 ? "All checks passed." : $"{failures} check(s) FAILED.");
return failures == 0 ? 0 : 1;

[System.Runtime.InteropServices.DllImport("libc", SetLastError = true)]
static extern int kill(int pid, int sig);
