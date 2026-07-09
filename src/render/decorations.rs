/// Server-side window decorations — title bars with macOS-style controls.
///
/// Each window gets a 30 px title bar rendered above its mapped position.
/// Control dots (left side, macOS-style): close (red), minimize (amber), maximize (green).
use smithay::{
    backend::renderer::{
        element::{solid::SolidColorRenderElement, Id, Kind},
        Color32F,
    },
    desktop::{Space, Window},
    utils::{Physical, Point, Rectangle},
};

pub use crate::shell::layout::DECO_H;

// Title bar background
const BAR_ACTIVE:   Color32F = Color32F::new(0.10, 0.16, 0.38, 0.93);
const BAR_INACTIVE: Color32F = Color32F::new(0.07, 0.10, 0.22, 0.80);

// Control dot colours (close / minimize / maximize)
const BTN_CLOSE: Color32F = Color32F::new(0.878, 0.361, 0.361, 1.0);  // #e05c5c
const BTN_MIN:   Color32F = Color32F::new(0.941, 0.659, 0.188, 1.0);  // #f0a830
const BTN_MAX:   Color32F = Color32F::new(0.361, 0.761, 0.361, 1.0);  // #5cc25c

const BTN_SZ: i32 = 12;   // dot width / height in logical px
const BTN_Y_OFF: i32 = 9; // vertical offset inside 30 px bar: (30 - 12) / 2
const BTN_X0: i32 = 10;   // left margin for close dot
const BTN_GAP: i32 = 6;   // gap between dots

/// Which part of a window decoration was clicked.
#[derive(Clone)]
pub enum DecoHit {
    TitleBar(Window),
    Close(Window),
    Minimize(Window),
    Maximize(Window),
}

/// Return the three button rects (close, min, max) in logical space for a bar at `bar`.
fn btn_rects(bar: Rectangle<i32, smithay::utils::Logical>)
    -> [Rectangle<i32, smithay::utils::Logical>; 3]
{
    let y = bar.loc.y + BTN_Y_OFF;
    [
        Rectangle::new((bar.loc.x + BTN_X0,                       y).into(), (BTN_SZ, BTN_SZ).into()),
        Rectangle::new((bar.loc.x + BTN_X0 + BTN_SZ + BTN_GAP,   y).into(), (BTN_SZ, BTN_SZ).into()),
        Rectangle::new((bar.loc.x + BTN_X0 + (BTN_SZ + BTN_GAP) * 2, y).into(), (BTN_SZ, BTN_SZ).into()),
    ]
}

/// Emit decoration render elements for every window in the space.
///
/// Returns physical-space rectangles (same convention as borders.rs).
/// Push these *before* border and window elements in the render list.
pub fn collect_decorations(
    space: &Space<Window>,
    focused: Option<&Window>,
    output_scale: f64,
) -> Vec<SolidColorRenderElement> {
    let mut out = Vec::new();

    for window in space.elements() {
        let geo = match space.element_geometry(window) { Some(g) => g, None => continue };

        // Bar sits ABOVE the window content
        let bar_log = Rectangle::new(
            (geo.loc.x, geo.loc.y - DECO_H).into(),
            (geo.size.w, DECO_H).into(),
        );

        let is_focused = focused.map(|f| f == window).unwrap_or(false);
        let bar_color  = if is_focused { BAR_ACTIVE } else { BAR_INACTIVE };

        // Helper to convert logical → physical rect
        let to_phys = |r: Rectangle<i32, smithay::utils::Logical>| -> Rectangle<i32, Physical> {
            Rectangle::new(
                ((r.loc.x as f64 * output_scale) as i32, (r.loc.y as f64 * output_scale) as i32).into(),
                ((r.size.w as f64 * output_scale) as i32, (r.size.h as f64 * output_scale) as i32).into(),
            )
        };

        // Title bar background
        out.push(SolidColorRenderElement::new(
            Id::new(), to_phys(bar_log), 0usize, bar_color, Kind::Unspecified,
        ));

        // Control dots
        let [close_r, min_r, max_r] = btn_rects(bar_log);
        let dot_colors = [BTN_CLOSE, BTN_MIN, BTN_MAX];
        for (r, color) in [close_r, min_r, max_r].iter().zip(dot_colors.iter()) {
            out.push(SolidColorRenderElement::new(
                Id::new(), to_phys(*r), 0usize, *color, Kind::Unspecified,
            ));
        }
    }

    out
}

/// Test whether a logical-space pointer position hits a decoration zone.
/// Returns the hit type and the window it belongs to.
pub fn hit_test(space: &Space<Window>, pos: Point<f64, smithay::utils::Logical>) -> Option<DecoHit> {
    let pos_i = Point::<i32, smithay::utils::Logical>::from((pos.x as i32, pos.y as i32));

    for window in space.elements().rev() {
        let geo = match space.element_geometry(window) { Some(g) => g, None => continue };

        let bar = Rectangle::new(
            (geo.loc.x, geo.loc.y - DECO_H).into(),
            (geo.size.w, DECO_H).into(),
        );
        if !bar.contains(pos_i) { continue; }

        let [close_r, min_r, max_r] = btn_rects(bar);
        if close_r.contains(pos_i) { return Some(DecoHit::Close(window.clone())); }
        if min_r.contains(pos_i)   { return Some(DecoHit::Minimize(window.clone())); }
        if max_r.contains(pos_i)   { return Some(DecoHit::Maximize(window.clone())); }

        return Some(DecoHit::TitleBar(window.clone()));
    }
    None
}
