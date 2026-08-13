namespace Pancake.Shell;

// Port of src/shell/layout.rs's constants + free functions. The BSP tree
// itself is in TileTree.cs.
public static class Layout
{
    /// Pixels reserved at the bottom for the panel.
    public const int PanelH = 54;
    /// Server-side decoration height (title bar above each window).
    public const int DecoH = 30;
    /// Gap between tiles (pixels).
    public const int TileGap = 6;
    /// Outer gap around the tile area.
    public const int OuterGap = 10;
    /// Minimum tile dimension after deducting decoration.
    public const int MinTile = 120;

    /// Compute the usable area for tiling (minus outer gap and panel).
    public static Rectangle TileArea(Rectangle outputGeo)
    {
        var g = OuterGap;
        return new Rectangle(
            new Point(outputGeo.Loc.X + g, outputGeo.Loc.Y + g),
            new Size(
                Math.Max(1, outputGeo.Size.W - g * 2),
                Math.Max(1, outputGeo.Size.H - g * 2 - PanelH)));
    }

    /// Initial geometry for a new floating window. Takes the first output's
    /// geometry and the current mapped-window count directly rather than a
    /// full Space<Window> -- that abstraction doesn't exist on the C# side
    /// yet (it depends on the still-unported src/handlers/ + NWayland
    /// integration), so this takes exactly the two facts the Rust version
    /// actually reads out of Space for this calculation.
    public static Rectangle InitialGeometry(Rectangle? firstOutputGeo, int existingWindowCount)
    {
        if (firstOutputGeo is not { } geo)
            return new Rectangle(80, 60, 960, 600);

        const int step = 32;
        var count = existingWindowCount;
        var w = Math.Max(640, geo.Size.W - 200 - count * step);
        var h = Math.Max(400, geo.Size.H - 180 - count * step - PanelH);
        return new Rectangle(
            geo.Loc.X + 80 + count * step,
            geo.Loc.Y + 60 + count * step,
            w, h);
    }
}
