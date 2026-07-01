/// Window placement logic.
///
/// For the initial scaffold this is a simple cascade — each new window opens
/// 32px down and to the right of the previous one so windows don't stack.
/// A full tiling / floating layout engine will live here later.
use smithay::{
    desktop::{Space, Window},
    utils::{Logical, Point},
};

const CASCADE_STEP: i32 = 32;

/// Return a sensible initial position for a new window in `space`.
pub fn initial_position(space: &Space<Window>) -> Point<i32, Logical> {
    let count = space.elements().count() as i32;
    let x = 64 + count * CASCADE_STEP;
    let y = 64 + count * CASCADE_STEP;
    (x, y).into()
}
