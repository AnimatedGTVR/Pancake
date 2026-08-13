namespace Pancake.Shell;

public enum SplitDir
{
    H,
    V,
}

public static class SplitDirExtensions
{
    public static SplitDir Toggle(this SplitDir dir) => dir == SplitDir.H ? SplitDir.V : SplitDir.H;
}

/// Direction for focus/swap navigation.
public enum NavDir
{
    Left,
    Right,
    Up,
    Down,
}

// Port of src/shell/layout.rs's TileTree. Generic over the window handle
// type (TWindow) instead of hardcoding Smithay's Window, since the
// window-identity type on the C# side isn't decided yet (depends on the
// still-unported src/handlers/ + NWayland integration) -- the tree's own
// logic (splitting, neighbor search, ratio adjustment) doesn't care what
// a window handle actually is, only that it can be compared for equality,
// same as Rust's `Window: PartialEq` bound implied by every `==` in the
// original file.
public abstract class TileTree<TWindow> where TWindow : IEquatable<TWindow>
{
    public static readonly TileTree<TWindow> Empty = new EmptyNode();

    public bool IsEmpty => this is EmptyNode;

    public abstract bool Contains(TWindow win);

    /// Insert `newWin` by splitting the leaf holding `focused`.
    /// If focused is null or not found, splits the rightmost leaf.
    public abstract TileTree<TWindow> Insert(TWindow newWin, TWindow? focused, SplitDir nextDir);

    /// Remove a window. Collapses empty branches automatically.
    public abstract (TileTree<TWindow> Tree, bool Found) Remove(TWindow win);

    /// Walk leaves in order, collecting (Window, tileRect).
    public abstract void CollectRects(Rectangle area, List<(TWindow Window, Rectangle Rect)> outList);

    public List<(TWindow Window, Rectangle Rect)> CollectRects(Rectangle area)
    {
        var result = new List<(TWindow, Rectangle)>();
        CollectRects(area, result);
        return result;
    }

    /// Find the tile-tree neighbor of `focused` in direction `dir`.
    public TWindow? FindNeighbor(TWindow focused, NavDir dir, Rectangle area)
    {
        var rects = CollectRects(area);
        var myIndex = rects.FindIndex(e => e.Window.Equals(focused));
        if (myIndex < 0) return default;
        var myR = rects[myIndex].Rect;

        var candidates = rects.Where(e => !e.Window.Equals(focused)).ToList();

        IEnumerable<(TWindow Window, Rectangle Rect)> filtered = dir switch
        {
            NavDir.Left => candidates.Where(e => e.Rect.Loc.X + e.Rect.Size.W <= myR.Loc.X + 2),
            NavDir.Right => candidates.Where(e => e.Rect.Loc.X >= myR.Loc.X + myR.Size.W - 2),
            NavDir.Up => candidates.Where(e => e.Rect.Loc.Y + e.Rect.Size.H <= myR.Loc.Y + 2),
            NavDir.Down => candidates.Where(e => e.Rect.Loc.Y >= myR.Loc.Y + myR.Size.H - 2),
            _ => Enumerable.Empty<(TWindow, Rectangle)>(),
        };

        Func<(TWindow Window, Rectangle Rect), int> key = dir switch
        {
            NavDir.Left => e => myR.Loc.X - (e.Rect.Loc.X + e.Rect.Size.W),
            NavDir.Right => e => e.Rect.Loc.X - (myR.Loc.X + myR.Size.W),
            NavDir.Up => e => myR.Loc.Y - (e.Rect.Loc.Y + e.Rect.Size.H),
            NavDir.Down => e => e.Rect.Loc.Y - (myR.Loc.Y + myR.Size.H),
            _ => _ => 0,
        };

        var ordered = filtered.OrderBy(key).ToList();
        return ordered.Count > 0 ? ordered[0].Window : default;
    }

    /// Swap two windows in the tree.
    public abstract void Swap(TWindow a, TWindow b);

    /// Adjust the ratio of the split directly containing `win`.
    public abstract void AdjustRatio(TWindow win, float delta);

    private sealed class EmptyNode : TileTree<TWindow>
    {
        public override bool Contains(TWindow win) => false;

        public override TileTree<TWindow> Insert(TWindow newWin, TWindow? focused, SplitDir nextDir) =>
            new LeafNode(newWin);

        public override (TileTree<TWindow>, bool) Remove(TWindow win) => (this, false);

        public override void CollectRects(Rectangle area, List<(TWindow, Rectangle)> outList) { }

        public override void Swap(TWindow a, TWindow b) { }

        public override void AdjustRatio(TWindow win, float delta) { }
    }

    private sealed class LeafNode(TWindow window) : TileTree<TWindow>
    {
        public TWindow Window = window;

        public override bool Contains(TWindow win) => Window.Equals(win);

        public override TileTree<TWindow> Insert(TWindow newWin, TWindow? focused, SplitDir nextDir) =>
            new SplitNode(nextDir, 0.5f, new LeafNode(Window), new LeafNode(newWin));

        public override (TileTree<TWindow>, bool) Remove(TWindow win) =>
            Window.Equals(win) ? (Empty, true) : (this, false);

        public override void CollectRects(Rectangle area, List<(TWindow, Rectangle)> outList) =>
            outList.Add((Window, area));

        public override void Swap(TWindow a, TWindow b)
        {
            if (Window.Equals(a)) Window = b;
            else if (Window.Equals(b)) Window = a;
        }

        public override void AdjustRatio(TWindow win, float delta) { }
    }

    private sealed class SplitNode(SplitDir dir, float ratio, TileTree<TWindow> a, TileTree<TWindow> b) : TileTree<TWindow>
    {
        public SplitDir Dir = dir;
        public float Ratio = ratio;
        public TileTree<TWindow> A = a, B = b;

        public override bool Contains(TWindow win) => A.Contains(win) || B.Contains(win);

        public override TileTree<TWindow> Insert(TWindow newWin, TWindow? focused, SplitDir nextDir)
        {
            var altDir = Dir.Toggle();
            if (focused is not null && A.Contains(focused))
                A = A.Insert(newWin, focused, altDir);
            else if (focused is not null && B.Contains(focused))
                B = B.Insert(newWin, focused, altDir);
            else
                B = B.Insert(newWin, default, altDir);
            return this;
        }

        public override (TileTree<TWindow>, bool) Remove(TWindow win)
        {
            var (newA, foundA) = A.Remove(win);
            A = newA;
            if (foundA)
            {
                return A.IsEmpty ? (B, true) : (this, true);
            }

            var (newB, foundB) = B.Remove(win);
            B = newB;
            if (foundB)
            {
                return B.IsEmpty ? (A, true) : (this, true);
            }

            return (this, false);
        }

        public override void CollectRects(Rectangle area, List<(TWindow, Rectangle)> outList)
        {
            const int g = Layout.TileGap;
            Rectangle ra, rb;
            if (Dir == SplitDir.H)
            {
                var wa = Math.Max(1, (int)(area.Size.W * Ratio) - g / 2);
                var wb = Math.Max(1, area.Size.W - wa - g);
                ra = new Rectangle(area.Loc, new Size(wa, area.Size.H));
                rb = new Rectangle(new Point(area.Loc.X + wa + g, area.Loc.Y), new Size(wb, area.Size.H));
            }
            else
            {
                var ha = Math.Max(1, (int)(area.Size.H * Ratio) - g / 2);
                var hb = Math.Max(1, area.Size.H - ha - g);
                ra = new Rectangle(area.Loc, new Size(area.Size.W, ha));
                rb = new Rectangle(new Point(area.Loc.X, area.Loc.Y + ha + g), new Size(area.Size.W, hb));
            }
            A.CollectRects(ra, outList);
            B.CollectRects(rb, outList);
        }

        public override void Swap(TWindow a, TWindow b)
        {
            A.Swap(a, b);
            B.Swap(a, b);
        }

        public override void AdjustRatio(TWindow win, float delta)
        {
            if (A is LeafNode la && la.Window.Equals(win))
            {
                Ratio = Math.Clamp(Ratio + delta, 0.15f, 0.85f);
            }
            else if (B is LeafNode lb && lb.Window.Equals(win))
            {
                Ratio = Math.Clamp(Ratio - delta, 0.15f, 0.85f);
            }
            else if (A.Contains(win))
            {
                A.AdjustRatio(win, delta);
            }
            else if (B.Contains(win))
            {
                B.AdjustRatio(win, delta);
            }
        }
    }
}
