namespace Pancake.Render;

// Full port of src/render/cursor.rs. RGBA pixels + dimensions + hotspot
// for the pointer cursor, tries the real system xcursor theme first,
// falls back to a built-in 16x16 white arrow.
public sealed class CursorImage
{
    public required byte[] Pixels { get; init; } // RGBA8
    public required uint Width { get; init; }
    public required uint Height { get; init; }
    public required uint HotX { get; init; }
    public required uint HotY { get; init; }
}

public static class Cursor
{
    public const uint DefaultCursorSize = 24;

    /// Load the system cursor, falling back to the built-in arrow.
    public static CursorImage LoadDefault() => TryXcursor() ?? BuiltinArrow();

    // -- xcursor attempt --

    private static CursorImage? TryXcursor()
    {
        var theme = Environment.GetEnvironmentVariable("XCURSOR_THEME");
        var theme_ = string.IsNullOrEmpty(theme) ? "default" : theme;
        var wantSize = uint.TryParse(Environment.GetEnvironmentVariable("XCURSOR_SIZE"), out var s)
            ? s
            : DefaultCursorSize;

        var path = FindCursorFile(theme_, "default");
        if (path is null) return null;

        byte[] data;
        try { data = File.ReadAllBytes(path); }
        catch { return null; }

        var images = XcursorFile.Parse(data);
        if (images is null || images.Count == 0) return null;

        var img = images
            .OrderBy(i => Math.Abs((int)i.Size - (int)wantSize))
            .First();

        var pixels = new byte[img.PixelsArgb.Length * 4];
        for (var i = 0; i < img.PixelsArgb.Length; i++)
        {
            var argb = img.PixelsArgb[i];
            var a = (byte)((argb >> 24) & 0xFF);
            var r = (byte)((argb >> 16) & 0xFF);
            var g = (byte)((argb >> 8) & 0xFF);
            var b = (byte)(argb & 0xFF);
            pixels[i * 4 + 0] = r;
            pixels[i * 4 + 1] = g;
            pixels[i * 4 + 2] = b;
            pixels[i * 4 + 3] = a;
        }

        return new CursorImage { Pixels = pixels, Width = img.Width, Height = img.Height, HotX = img.XHot, HotY = img.YHot };
    }

    // Simplified theme resolution: checks the conventional cursor-theme
    // directories directly, without walking a theme's `index.theme`
    // Inherits= chain the way real freedesktop icon-theme resolution
    // does (real Rust `xcursor::CursorTheme::load` does that full walk).
    // Bounded, documented simplification -- same "real for the common
    // case, not a partial imitation of the full spec" approach as other
    // simplifications this session (place_meeting's AABB collision, the
    // arrays-always-copy semantics, etc). Falls back to the "default"
    // theme name, which on most real systems is itself a symlink to
    // whatever the active theme actually is.
    private static string? FindCursorFile(string theme, string cursorName)
    {
        var home = Environment.GetEnvironmentVariable("HOME") ?? "";
        string[] roots =
        {
            Path.Combine(home, ".icons"),
            Path.Combine(home, ".local", "share", "icons"),
            "/usr/share/icons",
            "/usr/local/share/icons",
        };

        foreach (var themeName in new[] { theme, "default" })
        {
            foreach (var root in roots)
            {
                var candidate = Path.Combine(root, themeName, "cursors", cursorName);
                if (File.Exists(candidate)) return candidate;
            }
        }

        return null;
    }

    // -- Built-in 16x16 arrow (fallback) --

    private static CursorImage BuiltinArrow()
    {
        const int s = 16;
        string[] map =
        {
            "B...............",
            "BB..............",
            "BWB.............",
            "BWWB............",
            "BWWWB...........",
            "BWWWWB..........",
            "BWWWWWB.........",
            "BWWWWWWB........",
            "BWWWWWWWB.......",
            "BWWWWWB.........",
            "BWWBWWB.........",
            "BWB.BWWB........",
            "BB...BWWB.......",
            "B.....BWB.......",
            "......BB........",
            "................",
        };

        var pixels = new byte[s * s * 4];
        for (var y = 0; y < map.Length; y++)
        {
            var row = map[y];
            for (var x = 0; x < row.Length; x++)
            {
                var i = (y * s + x) * 4;
                switch (row[x])
                {
                    case 'B':
                        pixels[i] = 0; pixels[i + 1] = 0; pixels[i + 2] = 0; pixels[i + 3] = 255;
                        break;
                    case 'W':
                        pixels[i] = 255; pixels[i + 1] = 255; pixels[i + 2] = 255; pixels[i + 3] = 255;
                        break;
                }
            }
        }

        return new CursorImage { Pixels = pixels, Width = s, Height = s, HotX = 0, HotY = 0 };
    }
}
