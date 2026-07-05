/// Virtual workspace manager — up to 9 numbered workspaces (Super+1..9).
///
/// Windows are stored per-workspace. Only the active workspace's windows are
/// present in the `Space`; switching swaps them in/out.
use smithay::{
    desktop::{Space, Window},
    utils::{Logical, Point},
};
use tracing::info;

pub const NUM_WORKSPACES: usize = 9;

#[derive(Default)]
pub struct WorkspaceManager {
    /// Per-workspace window list: (window handle, last known logical position).
    workspaces: [Vec<(Window, Point<i32, Logical>)>; NUM_WORKSPACES],
    pub active: usize,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Window lifecycle ──────────────────────────────────────────────────────

    /// Register a newly mapped window in the active workspace.
    pub fn add_window(&mut self, window: Window, pos: Point<i32, Logical>) {
        self.workspaces[self.active].push((window, pos));
    }

    /// Remove a window from whichever workspace owns it.
    /// Returns the workspace index if found.
    pub fn remove_window(&mut self, window: &Window) -> Option<usize> {
        for (i, ws) in self.workspaces.iter_mut().enumerate() {
            if let Some(idx) = ws.iter().position(|(w, _)| w == window) {
                ws.remove(idx);
                return Some(i);
            }
        }
        None
    }

    /// Update the stored position for a window (after an interactive move).
    pub fn update_position(&mut self, window: &Window, pos: Point<i32, Logical>) {
        for ws in &mut self.workspaces {
            for (w, p) in ws.iter_mut() {
                if w == window {
                    *p = pos;
                    return;
                }
            }
        }
    }

    // ── Workspace switching ───────────────────────────────────────────────────

    /// Switch to workspace `new_idx`.  Unmaps the current workspace's windows
    /// from `space` and maps the target's windows.  Returns `true` if the
    /// workspace actually changed.
    pub fn switch_to(
        &mut self,
        space: &mut Space<Window>,
        new_idx: usize,
    ) -> bool {
        if new_idx >= NUM_WORKSPACES || new_idx == self.active {
            return false;
        }

        // Snapshot current positions before unmapping
        let cur = self.active;
        for (win, pos) in &mut self.workspaces[cur] {
            if let Some(geo) = space.element_geometry(win) {
                *pos = geo.loc;
            }
        }

        // Unmap current workspace windows
        let to_unmap: Vec<Window> = self.workspaces[cur]
            .iter()
            .map(|(w, _)| w.clone())
            .collect();
        for w in to_unmap {
            space.unmap_elem(&w);
        }

        // Map target workspace windows
        self.active = new_idx;
        let to_map: Vec<(Window, Point<i32, Logical>)> =
            self.workspaces[new_idx].clone();
        for (win, pos) in to_map {
            space.map_element(win, pos, false);
        }

        info!("Switched to workspace {}", new_idx + 1);
        true
    }

    // ── Window-to-workspace movement ─────────────────────────────────────────

    /// Move `window` from the active workspace to `target_idx`.
    /// The window is unmapped from the space (since the target workspace is not
    /// active) but kept in the target workspace's list for when you switch there.
    pub fn move_window_to(
        &mut self,
        space: &mut Space<Window>,
        window: &Window,
        target_idx: usize,
    ) {
        if target_idx >= NUM_WORKSPACES || target_idx == self.active {
            return;
        }

        let cur = self.active;
        let idx = match self.workspaces[cur].iter().position(|(w, _)| w == window) {
            Some(i) => i,
            None => return,
        };

        let (win, mut pos) = self.workspaces[cur].remove(idx);
        // Snapshot latest position
        if let Some(geo) = space.element_geometry(&win) {
            pos = geo.loc;
        }
        space.unmap_elem(&win);
        self.workspaces[target_idx].push((win, pos));
        info!(
            "Moved window to workspace {}",
            target_idx + 1
        );
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    pub fn active_windows(&self) -> &[(Window, Point<i32, Logical>)] {
        &self.workspaces[self.active]
    }

    #[allow(dead_code)]
    pub fn window_workspace(&self, window: &Window) -> Option<usize> {
        for (i, ws) in self.workspaces.iter().enumerate() {
            if ws.iter().any(|(w, _)| w == window) {
                return Some(i);
            }
        }
        None
    }
}
