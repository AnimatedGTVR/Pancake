using Pancake.Shell;

namespace Pancake.Render;

// Full port of src/render/borders.rs -- window focus-ring borders,
// Hyprland-style. Unlike aero.rs/gpu.rs, this file was never actually
// GPU-side logic: it just computes four colored rectangles per window
// from Space geometry. It was blocked earlier this session purely on
// PancakeSpace not existing yet -- now that it does, this is a complete,
// direct port, not a partial slice.
public readonly record struct BorderElement(Rectangle Rect, float R, float G, float B, float A);

public static class Borders
{
    // Border thickness in logical pixels.
    private const int BorderPx = 3;

    // Active border: warm amber/syrup -- distinctive against the cool blue glass.
    private static readonly (float R, float G, float B, float A) ActiveColor = (0.96f, 0.67f, 0.20f, 0.95f);

    // Inactive border: barely-visible cool slate.
    private static readonly (float R, float G, float B, float A) InactiveColor = (0.22f, 0.26f, 0.40f, 0.50f);

    /// Emit border render elements for all windows in the space.
    /// Elements are Physical-space rectangles (scale applied). Draw these
    /// before window content so borders appear underneath.
    public static List<BorderElement> CollectBorders(PancakeSpace space, PancakeWindow? focused, double outputScale)
    {
        var output = new List<BorderElement>();
        var bp = (int)Math.Round(BorderPx * outputScale);

        foreach (var window in space.Elements())
        {
            var geo = space.ElementGeometry(window);
            if (geo is not { } g) continue;

            var color = focused is not null && focused.Equals(window) ? ActiveColor : InactiveColor;

            var px = (int)(g.Loc.X * outputScale);
            var py = (int)(g.Loc.Y * outputScale);
            var pw = (int)(g.Size.W * outputScale);
            var ph = (int)(g.Size.H * outputScale);

            // Four strips: top, bottom, left, right.
            (Point Loc, Size Size)[] strips =
            {
                (new Point(px - bp, py - bp), new Size(pw + bp * 2, bp)),          // top
                (new Point(px - bp, py + ph), new Size(pw + bp * 2, bp)),          // bottom
                (new Point(px - bp, py), new Size(bp, ph)),                        // left
                (new Point(px + pw, py), new Size(bp, ph)),                        // right
            };

            foreach (var (loc, size) in strips)
            {
                output.Add(new BorderElement(new Rectangle(loc, size), color.R, color.G, color.B, color.A));
            }
        }

        return output;
    }
}
