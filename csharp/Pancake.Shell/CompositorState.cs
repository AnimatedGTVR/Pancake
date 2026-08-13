namespace Pancake.Shell;

public enum SnapDirection
{
    Left,
    Right,
    Up,
    Down,
}

// Port of the Space/WorkspaceManager-orchestration half of state.rs's
// PancakeState -- retile/toggle_tiling/focus_tile/swap_tile/resize_tile/
// cycle_focus/snap_focused. These methods turned out to be almost
// entirely PancakeSpace + WorkspaceManager mutation once both existed,
// so this is a real, nearly-complete port of that slice, not another
// partial one.
//
// What's deliberately NOT here: actually sending a wl_keyboard.enter/
// leave focus-change event to a client. Every method below updates
// FocusedWindow (the state xdg_shell.rs's new_toplevel/toplevel_destroyed
// already read/write in Pancake.Wayland), but the real
// `keyboard.set_focus(...)` wire call needs a live wl_keyboard resource
// tracked per-client, which Pancake.Wayland's Seat.cs creates but doesn't
// yet wire up to actual focus-change events (no input events exist to
// trigger it -- Pancake.Cn's libinput layer only proves device
// enumeration works in this sandbox, see readmenow.md). That's the next
// concrete gap, not a design decision to revisit.
public sealed class PancakeCompositorState
{
    public PancakeSpace Space { get; } = new();
    public WorkspaceManager Workspaces { get; } = new();
    public PancakeWindow? FocusedWindow { get; set; }
    public (double X, double Y) CursorPos { get; set; }
    public (PancakeWindow Window, (double X, double Y) Offset)? MoveGrab { get; set; }

    /// Get the primary output geometry.
    public Rectangle? OutputGeo()
    {
        var outputs = Space.Outputs();
        return outputs.Count > 0 ? Space.OutputGeometry(outputs[0]) : null;
    }

    /// Re-apply the BSP tiling layout for the active workspace. No-op
    /// when the workspace is in floating mode.
    public void Retile()
    {
        if (OutputGeo() is { } geo) Workspaces.ApplyTiles(Space, geo);
    }

    /// Toggle tiling on the active workspace and re-layout.
    public void ToggleTiling()
    {
        if (OutputGeo() is { } geo) Workspaces.ToggleTiling(Space, geo);
    }

    /// Move keyboard focus to the neighboring tile in `dir` (tiling mode only).
    public void FocusTile(NavDir dir)
    {
        if (FocusedWindow is not { } focused) return;
        if (OutputGeo() is not { } geo) return;
        var neighbor = Workspaces.TileNeighbor(focused, dir, geo);
        if (neighbor is null) return;

        Space.RaiseElement(neighbor, true);
        FocusedWindow = neighbor;
    }

    /// Swap focused tile with its neighbor in `dir` and re-layout.
    public void SwapTile(NavDir dir)
    {
        if (FocusedWindow is not { } focused) return;
        if (OutputGeo() is not { } geo) return;
        if (Workspaces.SwapNeighbor(focused, dir, geo))
            Workspaces.ApplyTiles(Space, geo);
    }

    /// Resize focused tile by moving the split ratio.
    public void ResizeTile(NavDir dir)
    {
        const float step = 0.05f;
        if (FocusedWindow is not { } focused) return;
        if (OutputGeo() is not { } geo) return;
        var delta = dir is NavDir.Right or NavDir.Down ? step : -step;
        Workspaces.AdjustRatio(focused, delta);
        Workspaces.ApplyTiles(Space, geo);
    }

    /// Cycle keyboard focus to the next window in the space.
    public void CycleFocus()
    {
        var windows = Space.Elements();
        if (windows.Count == 0) return;

        PancakeWindow next;
        if (FocusedWindow is { } cur)
        {
            var pos = windows.ToList().FindIndex(w => w.Equals(cur));
            next = pos >= 0 ? windows[(pos + 1) % windows.Count] : windows[0];
        }
        else
        {
            next = windows[0];
        }

        Space.RaiseElement(next, true);
        FocusedWindow = next;
    }

    /// Snap the focused window to a half of the output (left/right) or
    /// maximize/restore (up/down). Inspired by Hyprland's Super+arrow layout.
    public void SnapFocused(SnapDirection direction)
    {
        if (OutputGeo() is not { } outputGeo) return;
        if (FocusedWindow is not { } win) return;

        Point loc;
        Size size;
        switch (direction)
        {
            case SnapDirection.Left:
                loc = outputGeo.Loc;
                size = new Size(outputGeo.Size.W / 2, outputGeo.Size.H);
                break;
            case SnapDirection.Right:
                loc = new Point(outputGeo.Loc.X + outputGeo.Size.W / 2, outputGeo.Loc.Y);
                size = new Size(outputGeo.Size.W / 2, outputGeo.Size.H);
                break;
            case SnapDirection.Up:
                loc = outputGeo.Loc;
                size = outputGeo.Size;
                break;
            case SnapDirection.Down:
            default:
                var count = Space.ElementCount;
                var x = 64 + count * 32;
                var y = 64 + count * 32;
                loc = new Point(x, y);
                size = new Size(outputGeo.Size.W * 2 / 3, outputGeo.Size.H * 2 / 3);
                break;
        }

        Space.MapElement(win, loc, true);
        Space.SetElementSize(win, size);
        Workspaces.UpdatePosition(win, loc);
    }
}
