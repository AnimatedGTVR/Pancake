namespace Pancake.Shell;

// Full port of src/shell/workspace.rs, now that PancakeWindow and
// PancakeSpace exist to unblock it (it was the reason this file couldn't
// be ported earlier this session). Uses TileTree<PancakeWindow> --
// exactly the generic instantiation TileTree.cs was built for.
public sealed class WorkspaceManager
{
    public const int NumWorkspaces = 9;

    private sealed class WsState
    {
        public readonly List<(PancakeWindow Window, Point Pos)> Windows = new();
        public TileTree<PancakeWindow> Tree = TileTree<PancakeWindow>.Empty;
        public bool Tiling;
        public SplitDir NextSplit = SplitDir.H;
    }

    private readonly WsState[] _states = Enumerable.Range(0, NumWorkspaces).Select(_ => new WsState()).ToArray();

    public int Active { get; private set; }

    // ── Window lifecycle ────────────────────────────────────────────

    /// Register a newly mapped window in the active workspace. Pass the
    /// currently focused window so the BSP tree splits it.
    public void AddWindow(PancakeWindow window, Point pos, PancakeWindow? focused)
    {
        var ws = _states[Active];
        ws.Windows.Add((window, pos));
        if (ws.Tiling)
        {
            ws.Tree = ws.Tree.Insert(window, focused, ws.NextSplit);
        }
    }

    /// Remove a window from whichever workspace owns it. Returns the
    /// workspace index if found.
    public int? RemoveWindow(PancakeWindow window)
    {
        for (var i = 0; i < _states.Length; i++)
        {
            var ws = _states[i];
            var idx = ws.Windows.FindIndex(w => w.Window.Equals(window));
            if (idx >= 0)
            {
                ws.Windows.RemoveAt(idx);
                var (newTree, _) = ws.Tree.Remove(window);
                ws.Tree = newTree;
                return i;
            }
        }
        return null;
    }

    /// Update the stored position for a window (after an interactive move).
    public void UpdatePosition(PancakeWindow window, Point pos)
    {
        foreach (var ws in _states)
        {
            var idx = ws.Windows.FindIndex(w => w.Window.Equals(window));
            if (idx >= 0)
            {
                ws.Windows[idx] = (window, pos);
                return;
            }
        }
    }

    // ── Workspace switching ─────────────────────────────────────────

    public bool SwitchTo(PancakeSpace space, int newIdx)
    {
        if (newIdx < 0 || newIdx >= NumWorkspaces || newIdx == Active) return false;

        var cur = _states[Active];
        for (var i = 0; i < cur.Windows.Count; i++)
        {
            var (win, pos) = cur.Windows[i];
            if (space.ElementGeometry(win) is { } geo) cur.Windows[i] = (win, geo.Loc);
        }

        foreach (var (win, _) in cur.Windows.ToList()) space.UnmapElement(win);

        Active = newIdx;
        foreach (var (win, pos) in _states[newIdx].Windows.ToList()) space.MapElement(win, pos, false);

        return true;
    }

    // ── Window-to-workspace movement ────────────────────────────────

    public void MoveWindowTo(PancakeSpace space, PancakeWindow window, int targetIdx)
    {
        if (targetIdx < 0 || targetIdx >= NumWorkspaces || targetIdx == Active) return;

        var cur = _states[Active];
        var idx = cur.Windows.FindIndex(w => w.Window.Equals(window));
        if (idx < 0) return;

        var (win, pos) = cur.Windows[idx];
        cur.Windows.RemoveAt(idx);
        var (newTree, _) = cur.Tree.Remove(win);
        cur.Tree = newTree;

        if (space.ElementGeometry(win) is { } geo) pos = geo.Loc;
        space.UnmapElement(win);
        _states[targetIdx].Windows.Add((win, pos));
    }

    // ── Tiling ───────────────────────────────────────────────────────

    public bool IsTiling => _states[Active].Tiling;

    /// Toggle tiling mode on the active workspace.
    public void ToggleTiling(PancakeSpace space, Rectangle outputGeo)
    {
        var ws = _states[Active];
        ws.Tiling = !ws.Tiling;

        if (ws.Tiling)
        {
            ws.Tree = TileTree<PancakeWindow>.Empty;
            foreach (var (win, _) in ws.Windows.ToList())
                ws.Tree = ws.Tree.Insert(win, null, ws.NextSplit);
        }
        else
        {
            ws.Tree = TileTree<PancakeWindow>.Empty;
        }

        DoApplyTiles(ws, space, outputGeo);
    }

    /// Re-apply the tiling layout to the active workspace.
    public void ApplyTiles(PancakeSpace space, Rectangle outputGeo) =>
        DoApplyTiles(_states[Active], space, outputGeo);

    private static void DoApplyTiles(WsState ws, PancakeSpace space, Rectangle outputGeo)
    {
        if (!ws.Tiling) return;
        var area = Layout.TileArea(outputGeo);
        foreach (var (win, tileRect) in ws.Tree.CollectRects(area))
        {
            var contentH = Math.Max(Layout.MinTile, tileRect.Size.H - Layout.DecoH);
            var contentLoc = new Point(tileRect.Loc.X, tileRect.Loc.Y + Layout.DecoH);
            space.SetElementSize(win, new Size(tileRect.Size.W, contentH));
            space.MapElement(win, contentLoc, false);
        }
    }

    /// Adjust the ratio of the split containing the focused window.
    public void AdjustRatio(PancakeWindow focused, float delta) =>
        _states[Active].Tree.AdjustRatio(focused, delta);

    /// Swap the focused window with its neighbor in the given direction.
    public bool SwapNeighbor(PancakeWindow focused, NavDir dir, Rectangle outputGeo)
    {
        var area = Layout.TileArea(outputGeo);
        var ws = _states[Active];
        var neighbor = ws.Tree.FindNeighbor(focused, dir, area);
        if (neighbor is null) return false;
        ws.Tree.Swap(focused, neighbor);
        return true;
    }

    /// Find the tiling neighbor of `focused` in `dir`.
    public PancakeWindow? TileNeighbor(PancakeWindow focused, NavDir dir, Rectangle outputGeo)
    {
        var area = Layout.TileArea(outputGeo);
        return _states[Active].Tree.FindNeighbor(focused, dir, area);
    }

    // ── Queries ──────────────────────────────────────────────────────

    public IReadOnlyList<(PancakeWindow Window, Point Pos)> ActiveWindows() => _states[Active].Windows;

    public int? WindowWorkspace(PancakeWindow window)
    {
        for (var i = 0; i < _states.Length; i++)
        {
            if (_states[i].Windows.Any(w => w.Window.Equals(window))) return i;
        }
        return null;
    }
}
