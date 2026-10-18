//! Where the pointers are: a drag from an island, and the fingers on a touch screen.

#[derive(Clone, Copy, Debug)]
pub(super) struct Drag {
    pub(super) island: usize,
    pub(super) start: (f64, f64),
}

/// Fingers on the board (phones): one finger taps, drags from an island or pans a zoomed board,
/// two fingers pinch to zoom.
#[derive(Default)]
pub(super) struct Touches {
    /// Pointer id and its last position on screen.
    pub(super) active: Vec<(i32, f64, f64)>,
    /// Where the first finger went down.
    pub(super) start: (f64, f64),
    /// Moved too far (or a second finger came) to count as a tap.
    pub(super) moved: bool,
    /// A tap on the water builds when the finger lifts, so panning doesn't build bridges.
    pub(super) tap: Option<usize>,
    /// Distance between the two fingers and the zoom when the pinch started.
    pub(super) pinch: Option<(f64, f64)>,
}

impl Touches {
    pub(super) fn midpoint(&self) -> Option<(f64, f64)> {
        match self.active.as_slice() {
            [(_, ax, ay), (_, bx, by), ..] => Some(((ax + bx) / 2.0, (ay + by) / 2.0)),
            _ => None,
        }
    }

    pub(super) fn spread(&self) -> Option<f64> {
        match self.active.as_slice() {
            [(_, ax, ay), (_, bx, by), ..] => Some((ax - bx).hypot(ay - by)),
            _ => None,
        }
    }
}
