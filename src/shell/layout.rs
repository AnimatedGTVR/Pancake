/// BSP tiling layout engine.
///
/// The layout tree is a binary tree where:
///  - Leaves hold windows
///  - Internal nodes are splits (H = left|right, V = top|bottom)
///
/// New windows always split the focused leaf at a 50/50 ratio.
/// Splits alternate H/V by tree depth unless overridden by the user.
use smithay::{
    desktop::{Space, Window},
    output::Output,
    utils::{Logical, Rectangle},
};

/// Pixels reserved at the bottom for the panel.
pub const PANEL_H: i32 = 54;
/// Server-side decoration height (title bar above each window).
pub const DECO_H: i32 = 30;
/// Gap between tiles (pixels).
pub const TILE_GAP: i32 = 6;
/// Outer gap around the tile area.
pub const OUTER_GAP: i32 = 10;
/// Minimum tile dimension after deducting decoration.
pub const MIN_TILE: i32 = 120;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SplitDir { H, V }

impl SplitDir {
    pub fn toggle(self) -> Self { if self == SplitDir::H { SplitDir::V } else { SplitDir::H } }
}

/// Direction for focus/swap navigation.
#[derive(Clone, Copy, Debug)]
pub enum NavDir { Left, Right, Up, Down }

#[derive(Clone)]
pub enum TileTree {
    Empty,
    Leaf(Window),
    Split {
        dir:   SplitDir,
        ratio: f32,
        a:     Box<TileTree>,
        b:     Box<TileTree>,
    },
}

impl Default for TileTree {
    fn default() -> Self { TileTree::Empty }
}

impl TileTree {
    pub fn is_empty(&self) -> bool { matches!(self, TileTree::Empty) }

    pub fn contains(&self, win: &Window) -> bool {
        match self {
            TileTree::Empty => false,
            TileTree::Leaf(w) => w == win,
            TileTree::Split { a, b, .. } => a.contains(win) || b.contains(win),
        }
    }

    /// Insert `new_win` by splitting the leaf holding `focused`.
    /// If focused is None or not found, splits the rightmost leaf.
    /// `next_dir` is used for the new split; future splits inside `a` alternate.
    pub fn insert(&mut self, new_win: Window, focused: Option<&Window>, next_dir: SplitDir, depth: usize) {
        match self {
            TileTree::Empty => {
                *self = TileTree::Leaf(new_win);
            }
            TileTree::Leaf(existing) => {
                let old = existing.clone();
                *self = TileTree::Split {
                    dir:   next_dir,
                    ratio: 0.5,
                    a:     Box::new(TileTree::Leaf(old)),
                    b:     Box::new(TileTree::Leaf(new_win)),
                };
            }
            TileTree::Split { a, b, dir, .. } => {
                // Prefer the side containing focused; fall back to b
                let alt_dir = dir.toggle();
                if focused.map(|f| a.contains(f)).unwrap_or(false) {
                    a.insert(new_win, focused, alt_dir, depth + 1);
                } else if focused.map(|f| b.contains(f)).unwrap_or(false) {
                    b.insert(new_win, focused, alt_dir, depth + 1);
                } else {
                    b.insert(new_win, None, alt_dir, depth + 1);
                }
            }
        }
    }

    /// Remove a window. Collapses empty branches automatically.
    /// Returns true if the window was found.
    pub fn remove(&mut self, win: &Window) -> bool {
        match self {
            TileTree::Empty => false,
            TileTree::Leaf(w) => {
                if w == win { *self = TileTree::Empty; true } else { false }
            }
            TileTree::Split { a, b, .. } => {
                if a.remove(win) {
                    if a.is_empty() { *self = *std::mem::replace(b, Box::new(TileTree::Empty)); }
                    true
                } else if b.remove(win) {
                    if b.is_empty() { *self = *std::mem::replace(a, Box::new(TileTree::Empty)); }
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Walk leaves in order, collecting (Window, tile_rect).
    /// `area` is the full allocated space for this subtree (including DECO_H).
    pub fn collect_rects(&self, area: Rectangle<i32, Logical>) -> Vec<(Window, Rectangle<i32, Logical>)> {
        match self {
            TileTree::Empty => vec![],
            TileTree::Leaf(w) => vec![(w.clone(), area)],
            TileTree::Split { dir, ratio, a, b } => {
                let g = TILE_GAP;
                let (ra, rb) = match dir {
                    SplitDir::H => {
                        let wa = ((area.size.w as f32 * ratio) as i32 - g / 2).max(1);
                        let wb = (area.size.w - wa - g).max(1);
                        (
                            Rectangle::new(area.loc, (wa, area.size.h).into()),
                            Rectangle::new((area.loc.x + wa + g, area.loc.y).into(), (wb, area.size.h).into()),
                        )
                    }
                    SplitDir::V => {
                        let ha = ((area.size.h as f32 * ratio) as i32 - g / 2).max(1);
                        let hb = (area.size.h - ha - g).max(1);
                        (
                            Rectangle::new(area.loc, (area.size.w, ha).into()),
                            Rectangle::new((area.loc.x, area.loc.y + ha + g).into(), (area.size.w, hb).into()),
                        )
                    }
                };
                let mut out = a.collect_rects(ra);
                out.extend(b.collect_rects(rb));
                out
            }
        }
    }

    /// Find the tile-tree neighbor of `focused` in direction `dir`.
    pub fn find_neighbor(&self, focused: &Window, dir: NavDir, area: Rectangle<i32, Logical>) -> Option<Window> {
        let rects = self.collect_rects(area);
        let (_, my_r) = rects.iter().find(|(w, _)| w == focused)?;

        let candidates: Vec<&(Window, Rectangle<i32, Logical>)> = rects.iter()
            .filter(|(w, _)| w != focused)
            .collect();

        match dir {
            NavDir::Left  => candidates.iter()
                .filter(|(_, r)| r.loc.x + r.size.w <= my_r.loc.x + 2)
                .min_by_key(|(_, r)| my_r.loc.x - (r.loc.x + r.size.w))
                .map(|(w, _)| w.clone()),
            NavDir::Right => candidates.iter()
                .filter(|(_, r)| r.loc.x >= my_r.loc.x + my_r.size.w - 2)
                .min_by_key(|(_, r)| r.loc.x - (my_r.loc.x + my_r.size.w))
                .map(|(w, _)| w.clone()),
            NavDir::Up    => candidates.iter()
                .filter(|(_, r)| r.loc.y + r.size.h <= my_r.loc.y + 2)
                .min_by_key(|(_, r)| my_r.loc.y - (r.loc.y + r.size.h))
                .map(|(w, _)| w.clone()),
            NavDir::Down  => candidates.iter()
                .filter(|(_, r)| r.loc.y >= my_r.loc.y + my_r.size.h - 2)
                .min_by_key(|(_, r)| r.loc.y - (my_r.loc.y + my_r.size.h))
                .map(|(w, _)| w.clone()),
        }
    }

    /// Swap two windows in the tree.
    pub fn swap(&mut self, a: &Window, b: &Window) {
        match self {
            TileTree::Empty => {}
            TileTree::Leaf(w) => {
                if w == a { *w = b.clone(); }
                else if w == b { *w = a.clone(); }
            }
            TileTree::Split { a: la, b: lb, .. } => {
                la.swap(a, b);
                lb.swap(a, b);
            }
        }
    }

    /// Adjust the ratio of the split directly containing `win`.
    pub fn adjust_ratio(&mut self, win: &Window, delta: f32) {
        match self {
            TileTree::Split { a, b, ratio, .. } => {
                // Recurse into whichever side holds `win` first, so a window
                // nested several splits deep resizes the boundary actually
                // next to it, not always this (possibly far outer) split.
                if matches!(**a, TileTree::Leaf(ref w) if w == win) {
                    *ratio = (*ratio + delta).clamp(0.15, 0.85);
                } else if matches!(**b, TileTree::Leaf(ref w) if w == win) {
                    *ratio = (*ratio - delta).clamp(0.15, 0.85);
                } else if a.contains(win) {
                    a.adjust_ratio(win, delta);
                } else if b.contains(win) {
                    b.adjust_ratio(win, delta);
                }
            }
            _ => {}
        }
    }
}

/// Compute the usable area for tiling (minus outer gap and panel).
pub fn tile_area(output_geo: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let g = OUTER_GAP;
    Rectangle::new(
        (output_geo.loc.x + g, output_geo.loc.y + g).into(),
        (
            (output_geo.size.w - g * 2).max(1),
            (output_geo.size.h - g * 2 - PANEL_H).max(1),
        ).into(),
    )
}

/// Initial geometry for a new floating window.
pub fn initial_geometry(space: &Space<Window>) -> Rectangle<i32, Logical> {
    let output_geo = space
        .outputs()
        .next()
        .and_then(|o: &Output| space.output_geometry(o));

    if let Some(geo) = output_geo {
        let count = space.elements().count() as i32;
        let step = 32;
        let w = (geo.size.w - 200 - count * step).max(640);
        let h = (geo.size.h - 180 - count * step - PANEL_H).max(400);
        Rectangle::new(
            (geo.loc.x + 80 + count * step, geo.loc.y + 60 + count * step).into(),
            (w, h).into(),
        )
    } else {
        Rectangle::new((80, 60).into(), (960, 600).into())
    }
}
