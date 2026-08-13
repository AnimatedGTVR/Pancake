namespace Pancake.Shell;

// Plain logical-space integer geometry. Smithay's Point/Size/Rectangle<i32,
// Logical> carry a phantom coordinate-space type parameter Rust uses to
// stop you mixing Logical/Physical/Buffer coordinates at compile time --
// C# doesn't need the phantom-type trick to get the same safety here since
// Pancake.Shell only ever deals in Logical space, so this is a plain
// (x, y, w, h) struct, not a generic.
public readonly record struct Point(int X, int Y);

public readonly record struct Size(int W, int H);

public readonly record struct Rectangle(Point Loc, Size Size)
{
    public Rectangle(int x, int y, int w, int h) : this(new Point(x, y), new Size(w, h)) { }

    public bool Contains(Point p) =>
        p.X >= Loc.X && p.X < Loc.X + Size.W && p.Y >= Loc.Y && p.Y < Loc.Y + Size.H;
}
