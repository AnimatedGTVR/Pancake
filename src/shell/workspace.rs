/// Virtual workspace manager — up to 9 numbered workspaces (Super+1..9).
///
/// Each workspace independently supports BSP tiling or free-floating layout.
/// Toggle tiling with Super+Space; new windows split the focused tile.
use smithay::{
    desktop::{Space, Window},
    utils::{Logical, Point, Rectangle, Size},
};
use tracing::info;

use crate::shell::layout::{self, SplitDir, TileTree, DECO_H, MIN_TILE};

pub const NUM_WORKSPACES: usize = 9;

// ── Per-workspace state ───────────────────────────────────────────────────────

struct WsState {
    windows:     Vec<(Window, Point<i32, Logical>)>,
    tree:        TileTree,
    tiling:      bool,
    next_split:  SplitDir,
}

impl Default for WsState {
    fn default() -> Self {
        Self {
            windows:    Vec::new(),
            tree:       TileTree::Empty,
            tiling:     false,
            next_split: SplitDir::H,
        }
    }
}

// ── Workspace manager ─────────────────────────────────────────────────────────

pub struct WorkspaceManager {
    states: [WsState; NUM_WORKSPACES],
    pub active: usize,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self {
            states: std::array::from_fn(|_| WsState::default()),
            active: 0,
        }
    }
}

impl WorkspaceManager {
    pub fn new() -> Self { Self::default() }

    // ── Window lifecycle ──────────────────────────────────────────────────────

    /// Register a newly mapped window in the active workspace.
    /// Pass the currently focused window so the BSP tree splits it.
    pub fn add_window(&mut self, window: Window, pos: Point<i32, Logical>, focused: Option<&Window>) {
        let ws = &mut self.states[self.active];
        ws.windows.push((window.clone(), pos));
        if ws.tiling {
            let dir = ws.next_split;
            ws.tree.insert(window, focused, dir, 0);
        }
    }

    /// Remove a window from whichever workspace owns it.
    /// Returns the workspace index if found.
    pub fn remove_window(&mut self, window: &Window) -> Option<usize> {
        for (i, ws) in self.states.iter_mut().enumerate() {
            if let Some(idx) = ws.windows.iter().position(|(w, _)| w == window) {
                ws.windows.remove(idx);
                ws.tree.remove(window);
                return Some(i);
            }
        }
        None
    }

    /// Update the stored position for a window (after an interactive move).
    pub fn update_position(&mut self, window: &Window, pos: Point<i32, Logical>) {
        for ws in &mut self.states {
            for (w, p) in ws.windows.iter_mut() {
                if w == window { *p = pos; return; }
            }
        }
    }

    // ── Workspace switching ───────────────────────────────────────────────────

    pub fn switch_to(&mut self, space: &mut Space<Window>, new_idx: usize) -> bool {
        if new_idx >= NUM_WORKSPACES || new_idx == self.active {
            return false;
        }

        let cur = self.active;
        for (win, pos) in &mut self.states[cur].windows {
            if let Some(geo) = space.element_geometry(win) { *pos = geo.loc; }
        }

        let to_unmap: Vec<Window> = self.states[cur].windows.iter().map(|(w, _)| w.clone()).collect();
        for w in to_unmap { space.unmap_elem(&w); }

        self.active = new_idx;
        let to_map: Vec<(Window, Point<i32, Logical>)> = self.states[new_idx].windows.clone();
        for (win, pos) in to_map { space.map_element(win, pos, false); }

        info!("Switched to workspace {}", new_idx + 1);
        true
    }

    // ── Window-to-workspace movement ─────────────────────────────────────────

    pub fn move_window_to(&mut self, space: &mut Space<Window>, window: &Window, target_idx: usize) {
        if target_idx >= NUM_WORKSPACES || target_idx == self.active { return; }

        let cur = self.active;
        let idx = match self.states[cur].windows.iter().position(|(w, _)| w == window) {
            Some(i) => i,
            None => return,
        };
        let (win, mut pos) = self.states[cur].windows.remove(idx);
        self.states[cur].tree.remove(&win);
        if let Some(geo) = space.element_geometry(&win) { pos = geo.loc; }
        space.unmap_elem(&win);
        self.states[target_idx].windows.push((win, pos));
        info!("Moved window to workspace {}", target_idx + 1);
    }

    // ── Tiling ───────────────────────────────────────────────────────────────

    pub fn is_tiling(&self) -> bool { self.states[self.active].tiling }

    /// Toggle tiling mode on the active workspace.
    pub fn toggle_tiling(&mut self, space: &mut Space<Window>, output_geo: Rectangle<i32, Logical>) {
        let ws = &mut self.states[self.active];
        ws.tiling = !ws.tiling;
        info!("Workspace {} tiling: {}", self.active + 1, ws.tiling);

        if ws.tiling {
            // Build a fresh tree from the current window list
            ws.tree = TileTree::Empty;
            let wins: Vec<Window> = ws.windows.iter().map(|(w, _)| w.clone()).collect();
            for win in wins {
                let dir = ws.next_split;
                ws.tree.insert(win, None, dir, 0);
            }
        } else {
            ws.tree = TileTree::Empty;
        }

        // Apply (or release) the tiling layout
        Self::do_apply_tiles(&self.states[self.active], space, output_geo);
    }

    /// Re-apply the tiling layout to the active workspace.
    pub fn apply_tiles(&mut self, space: &mut Space<Window>, output_geo: Rectangle<i32, Logical>) {
        Self::do_apply_tiles(&self.states[self.active], space, output_geo);
    }

    fn do_apply_tiles(ws: &WsState, space: &mut Space<Window>, output_geo: Rectangle<i32, Logical>) {
        if !ws.tiling { return; }
        let area = layout::tile_area(output_geo);
        for (win, tile_rect) in ws.tree.collect_rects(area) {
            let content_h = (tile_rect.size.h - DECO_H).max(MIN_TILE);
            let content_loc = Point::from((tile_rect.loc.x, tile_rect.loc.y + DECO_H));
            if let Some(tl) = win.toplevel() {
                tl.with_pending_state(|s| {
                    s.size = Some(Size::from((tile_rect.size.w, content_h)));
                });
                tl.send_pending_configure();
            }
            space.map_element(win, content_loc, false);
        }
    }

    /// Adjust the ratio of the split containing the focused window.
    pub fn adjust_ratio(&mut self, focused: &Window, delta: f32) {
        self.states[self.active].tree.adjust_ratio(focused, delta);
    }

    /// Swap the focused window with its neighbor in the given direction.
    pub fn swap_neighbor(
        &mut self,
        focused: &Window,
        dir: layout::NavDir,
        output_geo: Rectangle<i32, Logical>,
    ) -> bool {
        let area = layout::tile_area(output_geo);
        let ws = &mut self.states[self.active];
        if let Some(neighbor) = ws.tree.find_neighbor(focused, dir, area) {
            ws.tree.swap(focused, &neighbor);
            true
        } else {
            false
        }
    }

    /// Find the tiling neighbor of `focused` in `dir`.
    pub fn tile_neighbor(
        &self,
        focused: &Window,
        dir: layout::NavDir,
        output_geo: Rectangle<i32, Logical>,
    ) -> Option<Window> {
        let area = layout::tile_area(output_geo);
        self.states[self.active].tree.find_neighbor(focused, dir, area)
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    pub fn active_windows(&self) -> &[(Window, Point<i32, Logical>)] {
        &self.states[self.active].windows
    }

    #[allow(dead_code)]
    pub fn window_workspace(&self, window: &Window) -> Option<usize> {
        for (i, ws) in self.states.iter().enumerate() {
            if ws.windows.iter().any(|(w, _)| w == window) { return Some(i); }
        }
        None
    }
}
