namespace Pancake.Shell;

// Port of Smithay's `desktop::Space<Window>` -- the container every
// handler file (compositor.rs, xdg_shell.rs, layer_shell.rs, input.rs,
// xwayland.rs) and workspace.rs/layout.rs reads and mutates. This is the
// piece whose absence was the actual blocker on every "app logic" half
// of the handlers ported so far (only the wire-protocol halves were
// portable without it). Covers exactly the operations those files
// actually call: map/unmap/raise a window, read its geometry, enumerate
// mapped windows and outputs. Damage tracking and render-element
// collection (Smithay's `Space::render_elements_for_output`) stay out --
// those are Pancake.Render's job once a live GL context exists, not this
// container's.
public sealed class PancakeSpace
{
    // Stacking order: index 0 is the bottom of the stack, last is the
    // topmost -- matches Smithay's Space::elements() iteration order,
    // which render code walks bottom-to-top so later elements draw over
    // earlier ones.
    private readonly List<(PancakeWindow Window, Rectangle Geometry)> _elements = new();
    private readonly List<(PancakeOutput Output, Point Loc)> _outputs = new();

    // ── Windows ──────────────────────────────────────────────────────

    /// Map (or reposition, if already mapped) a window at `loc`, using
    /// its last known size (zero if never set). `activate` is accepted
    /// for signature parity with Smithay's map_element (which uses it to
    /// decide initial focus at a higher layer) -- Space itself doesn't
    /// act on it, same as the real Space::map_element.
    public void MapElement(PancakeWindow window, Point loc, bool activate = false)
    {
        var idx = _elements.FindIndex(e => e.Window.Equals(window));
        if (idx >= 0)
        {
            var size = _elements[idx].Geometry.Size;
            _elements[idx] = (window, new Rectangle(loc, size));
        }
        else
        {
            _elements.Add((window, new Rectangle(loc, new Size(0, 0))));
        }
    }

    public void UnmapElement(PancakeWindow window) =>
        _elements.RemoveAll(e => e.Window.Equals(window));

    /// Move `window` to the top of the stacking order (drawn last, on
    /// top, and implicitly "focused" in z-order terms). No-op if not
    /// mapped, same as Smithay's raise_element on an unmapped window.
    public void RaiseElement(PancakeWindow window, bool activate = true)
    {
        var idx = _elements.FindIndex(e => e.Window.Equals(window));
        if (idx < 0) return;
        var entry = _elements[idx];
        _elements.RemoveAt(idx);
        _elements.Add(entry);
    }

    public Rectangle? ElementGeometry(PancakeWindow window)
    {
        var idx = _elements.FindIndex(e => e.Window.Equals(window));
        return idx >= 0 ? _elements[idx].Geometry : null;
    }

    /// Update a mapped window's size (the size half of its geometry) --
    /// covers what a real wl_surface.commit with a new buffer size would
    /// eventually drive once the render/damage pipeline exists.
    public void SetElementSize(PancakeWindow window, Size size)
    {
        var idx = _elements.FindIndex(e => e.Window.Equals(window));
        if (idx < 0) return;
        _elements[idx] = (window, new Rectangle(_elements[idx].Geometry.Loc, size));
    }

    /// Bottom-to-top stacking order, matching Space::elements().
    public IReadOnlyList<PancakeWindow> Elements() => _elements.Select(e => e.Window).ToList();

    public int ElementCount => _elements.Count;

    public bool Contains(PancakeWindow window) => _elements.Any(e => e.Window.Equals(window));

    // ── Outputs ──────────────────────────────────────────────────────

    public void MapOutput(PancakeOutput output, Point loc)
    {
        var idx = _outputs.FindIndex(o => o.Output == output);
        if (idx >= 0) _outputs[idx] = (output, loc);
        else _outputs.Add((output, loc));
    }

    public void UnmapOutput(PancakeOutput output) =>
        _outputs.RemoveAll(o => o.Output == output);

    public IReadOnlyList<PancakeOutput> Outputs() => _outputs.Select(o => o.Output).ToList();

    public Rectangle? OutputGeometry(PancakeOutput output)
    {
        // Output geometry needs a size, which (like window size) comes
        // from the real DRM mode once Pancake.Cn's output enumeration is
        // wired in here -- callers that need a concrete size should pass
        // it in via MapOutputWithSize until then.
        var idx = _outputs.FindIndex(o => o.Output == output);
        if (idx < 0) return null;
        return _outputSizes.TryGetValue(output, out var size)
            ? new Rectangle(_outputs[idx].Loc, size)
            : null;
    }

    private readonly Dictionary<PancakeOutput, Size> _outputSizes = new();

    public void MapOutputWithSize(PancakeOutput output, Point loc, Size size)
    {
        MapOutput(output, loc);
        _outputSizes[output] = size;
    }

    /// Buffer-release/cleanup bookkeeping in real Smithay; a documented
    /// no-op here until the render/damage pipeline that would drive it
    /// exists (same "deferred, not forgotten" pattern as elsewhere).
    public void Refresh() { }
}
