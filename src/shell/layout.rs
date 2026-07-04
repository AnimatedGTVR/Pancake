/// Window placement logic.
///
/// For the initial scaffold this is a simple cascade — each new window opens
/// 32px down and to the right of the previous one so windows don't stack.
/// A full tiling / floating layout engine will live here later.
use smithay::{
    desktop::{Space, Window},
    output::Output,
    utils::{Logical, Point, Rectangle, Size},
};

const CASCADE_STEP: i32 = 32;
const WINDOW_MARGIN: i32 = 48;
const MIN_WINDOW_W: i32 = 640;
const MIN_WINDOW_H: i32 = 400;

/// Return a sensible initial position for a new window in `space`.
pub fn initial_position(space: &Space<Window>) -> Point<i32, Logical> {
    let count = space.elements().count() as i32;
    let x = 64 + count * CASCADE_STEP;
    let y = 64 + count * CASCADE_STEP;
    (x, y).into()
}

/// Return a visible first-placement rectangle for a new application window.
///
/// Early clients such as terminals behave much better when the compositor
/// gives them a concrete size during the initial configure.
pub fn initial_geometry(space: &Space<Window>) -> Rectangle<i32, Logical> {
    let output_geo = space
        .outputs()
        .next()
        .and_then(|output: &Output| space.output_geometry(output));

    if let Some(geo) = output_geo {
        let count = space.elements().count() as i32;
        let offset = count * CASCADE_STEP;
        let margin = WINDOW_MARGIN + offset;
        let width = (geo.size.w - margin * 2).max(MIN_WINDOW_W);
        let height = (geo.size.h - margin * 2).max(MIN_WINDOW_H);
        let x = geo.loc.x + margin;
        let y = geo.loc.y + margin;
        Rectangle::new((x, y).into(), Size::from((width, height)))
    } else {
        let loc = initial_position(space);
        Rectangle::new(loc, Size::from((MIN_WINDOW_W, MIN_WINDOW_H)))
    }
}
