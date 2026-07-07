/// Window border decorations — Hyprland-style focus rings.
///
/// For each visible window we render four thin `SolidColorRenderElement` strips
/// just outside the window geometry.  The focused window gets a brighter accent
/// border; all others get a dimmer inactive colour.
use smithay::{
    backend::renderer::{
        element::{solid::SolidColorRenderElement, Id, Kind},
        Color32F,
    },
    desktop::{Space, Window},
    utils::{Physical, Point, Rectangle, Size},
};

// Border thickness in logical pixels
const BORDER_PX: i32 = 3;

// Active border: warm amber/syrup — distinctive against the cool blue glass
const ACTIVE_COLOR: Color32F = Color32F::new(0.96, 0.67, 0.20, 0.95);

// Inactive border: barely-visible cool slate
const INACTIVE_COLOR: Color32F = Color32F::new(0.22, 0.26, 0.40, 0.50);

/// Emit border render elements for all windows in the space.
///
/// Elements are Physical-space rectangles (scale=1). Push them *before* the
/// window elements so borders appear underneath window content.
pub fn collect_borders(
    space: &Space<Window>,
    focused: Option<&Window>,
    output_scale: f64,
) -> Vec<SolidColorRenderElement> {
    let mut out = Vec::new();
    let bp = (BORDER_PX as f64 * output_scale).round() as i32;

    for window in space.elements() {
        let geo = match space.element_geometry(window) {
            Some(g) => g,
            None => continue,
        };

        let color = if focused.map(|f| f == window).unwrap_or(false) {
            ACTIVE_COLOR
        } else {
            INACTIVE_COLOR
        };

        // Physical geometry
        let px = (geo.loc.x as f64 * output_scale) as i32;
        let py = (geo.loc.y as f64 * output_scale) as i32;
        let pw = (geo.size.w as f64 * output_scale) as i32;
        let ph = (geo.size.h as f64 * output_scale) as i32;

        // Four strips: top, bottom, left, right
        let strips: [(Point<i32, Physical>, Size<i32, Physical>); 4] = [
            // top
            ((px - bp, py - bp).into(),          (pw + bp * 2, bp).into()),
            // bottom
            ((px - bp, py + ph).into(),           (pw + bp * 2, bp).into()),
            // left
            ((px - bp, py).into(),                (bp, ph).into()),
            // right
            ((px + pw, py).into(),                (bp, ph).into()),
        ];

        for (loc, size) in strips {
            out.push(SolidColorRenderElement::new(
                Id::new(),
                Rectangle::new(loc, size),
                0usize,
                color,
                Kind::Unspecified,
            ));
        }
    }

    out
}
