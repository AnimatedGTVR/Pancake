using Pancake.Shell;

namespace Pancake.Render;

/// Which part of a window decoration was clicked.
public abstract record DecoHit
{
    public sealed record TitleBar(PancakeWindow Window) : DecoHit;
    public sealed record Close(PancakeWindow Window) : DecoHit;
    public sealed record Minimize(PancakeWindow Window) : DecoHit;
    public sealed record Maximize(PancakeWindow Window) : DecoHit;
}

// Full port of src/render/decorations.rs -- server-side title bars with
// macOS-style close/minimize/maximize dots. Like borders.rs, this was
// never GPU-side logic (just geometry + a click hit-test), so it was
// blocked purely on PancakeSpace/PancakeWindow, and is now a complete
// port.
public static class Decorations
{
    // Title bar background.
    private static readonly (float R, float G, float B, float A) BarActive = (0.10f, 0.16f, 0.38f, 0.93f);
    private static readonly (float R, float G, float B, float A) BarInactive = (0.07f, 0.10f, 0.22f, 0.80f);

    // Control dot colours (close / minimize / maximize).
    private static readonly (float R, float G, float B, float A) BtnClose = (0.878f, 0.361f, 0.361f, 1.0f); // #e05c5c
    private static readonly (float R, float G, float B, float A) BtnMin = (0.941f, 0.659f, 0.188f, 1.0f);   // #f0a830
    private static readonly (float R, float G, float B, float A) BtnMax = (0.361f, 0.761f, 0.361f, 1.0f);   // #5cc25c

    private const int BtnSz = 12;    // dot width/height in logical px
    private const int BtnYOff = 9;   // vertical offset inside 30px bar: (30-12)/2
    private const int BtnX0 = 10;    // left margin for close dot
    private const int BtnGap = 6;    // gap between dots

    /// Return the three button rects (close, min, max) in logical space for a bar at `bar`.
    private static (Rectangle Close, Rectangle Min, Rectangle Max) BtnRects(Rectangle bar)
    {
        var y = bar.Loc.Y + BtnYOff;
        return (
            new Rectangle(new Point(bar.Loc.X + BtnX0, y), new Size(BtnSz, BtnSz)),
            new Rectangle(new Point(bar.Loc.X + BtnX0 + BtnSz + BtnGap, y), new Size(BtnSz, BtnSz)),
            new Rectangle(new Point(bar.Loc.X + BtnX0 + (BtnSz + BtnGap) * 2, y), new Size(BtnSz, BtnSz))
        );
    }

    /// Emit decoration render elements for every window in the space.
    /// Returns physical-space rectangles (same convention as Borders).
    /// Push these before border and window elements in the render list.
    public static List<BorderElement> CollectDecorations(PancakeSpace space, PancakeWindow? focused, double outputScale)
    {
        var output = new List<BorderElement>();

        Rectangle ToPhys(Rectangle r) => new(
            new Point((int)(r.Loc.X * outputScale), (int)(r.Loc.Y * outputScale)),
            new Size((int)(r.Size.W * outputScale), (int)(r.Size.H * outputScale)));

        foreach (var window in space.Elements())
        {
            var geo = space.ElementGeometry(window);
            if (geo is not { } g) continue;

            // Bar sits ABOVE the window content.
            var barLog = new Rectangle(new Point(g.Loc.X, g.Loc.Y - Layout.DecoH), new Size(g.Size.W, Layout.DecoH));

            var isFocused = focused is not null && focused.Equals(window);
            var barColor = isFocused ? BarActive : BarInactive;

            output.Add(new BorderElement(ToPhys(barLog), barColor.R, barColor.G, barColor.B, barColor.A));

            var (closeR, minR, maxR) = BtnRects(barLog);
            output.Add(new BorderElement(ToPhys(closeR), BtnClose.R, BtnClose.G, BtnClose.B, BtnClose.A));
            output.Add(new BorderElement(ToPhys(minR), BtnMin.R, BtnMin.G, BtnMin.B, BtnMin.A));
            output.Add(new BorderElement(ToPhys(maxR), BtnMax.R, BtnMax.G, BtnMax.B, BtnMax.A));
        }

        return output;
    }

    /// Test whether a logical-space pointer position hits a decoration zone.
    /// Returns the hit type and the window it belongs to.
    public static DecoHit? HitTest(PancakeSpace space, Point pos)
    {
        // Iterate top-to-bottom (reverse stacking order), matching
        // Rust's `space.elements().rev()` -- the topmost window's
        // decoration wins if bars overlap.
        var elements = space.Elements();
        for (var i = elements.Count - 1; i >= 0; i--)
        {
            var window = elements[i];
            var geo = space.ElementGeometry(window);
            if (geo is not { } g) continue;

            var bar = new Rectangle(new Point(g.Loc.X, g.Loc.Y - Layout.DecoH), new Size(g.Size.W, Layout.DecoH));
            if (!bar.Contains(pos)) continue;

            var (closeR, minR, maxR) = BtnRects(bar);
            if (closeR.Contains(pos)) return new DecoHit.Close(window);
            if (minR.Contains(pos)) return new DecoHit.Minimize(window);
            if (maxR.Contains(pos)) return new DecoHit.Maximize(window);

            return new DecoHit.TitleBar(window);
        }
        return null;
    }
}
