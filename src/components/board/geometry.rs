//! Board geometry in art pixels: where edges are, and which cell or edge a point is on.

use common::{Board, Orientation, Pos};

use crate::components::sprites::{CELL, Span};

pub(super) fn span_of(board: &Board, edge: usize) -> Span {
    let (a, b) = board.endpoints(edge);
    Span {
        horizontal: board.edges[edge].orientation == Orientation::Horizontal,
        a: (i32::from(a.x), i32::from(a.y)),
        b: (i32::from(b.x), i32::from(b.y)),
    }
}

/// Midpoint of an edge in art pixels.
pub(super) fn midpoint(board: &Board, edge: usize) -> (f64, f64) {
    let (a, b) = board.endpoints(edge);
    let c = f64::from(CELL);
    (
        f64::midpoint(f64::from(a.x), f64::from(b.x)) * c + c / 2.0,
        f64::midpoint(f64::from(a.y), f64::from(b.y)) * c + c / 2.0,
    )
}

/// For every cell (row major) the edges passing over it.
pub(super) fn cell_edges(board: &Board) -> Vec<Vec<usize>> {
    let w = board.puzzle.width as usize;
    let mut cells = vec![Vec::new(); w * board.puzzle.height as usize];
    for e in 0..board.edges.len() {
        for p in board.cells(e) {
            cells[p.y as usize * w + p.x as usize].push(e);
        }
    }
    cells
}

/// The edge whose gap contains the point, preferring the line closest to it.
pub(super) fn edge_at(board: &Board, cells: &[Vec<usize>], px: f64, py: f64) -> Option<usize> {
    let pos = cell_of(board, px, py)?;
    if board.puzzle.island_at(pos).is_some() {
        return None;
    }
    let candidates = cells.get(pos.y as usize * board.puzzle.width as usize + pos.x as usize)?;
    let c = f64::from(CELL);
    candidates.iter().copied().min_by(|&e, &f| {
        let dist = |edge: usize| match board.edges[edge].orientation {
            Orientation::Horizontal => (py - (f64::from(pos.y) * c + c / 2.0)).abs(),
            Orientation::Vertical => (px - (f64::from(pos.x) * c + c / 2.0)).abs(),
        };
        dist(e).total_cmp(&dist(f))
    })
}

pub(super) fn cell_of(board: &Board, px: f64, py: f64) -> Option<Pos> {
    if px < 0.0 || py < 0.0 {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "flooring a non-negative pixel offset to a cell index, `as` saturates and the \
                  result is bounds-checked against the board size below"
    )]
    let (x, y) = ((px / f64::from(CELL)) as u32, (py / f64::from(CELL)) as u32);
    let (x, y) = (u8::try_from(x).ok()?, u8::try_from(y).ok()?);
    (x < board.puzzle.width && y < board.puzzle.height).then(|| Pos::new(x, y))
}

/// Rounds a pixel position or scroll offset to whole pixels.
#[expect(
    clippy::cast_possible_truncation,
    reason = "pixel values on screen are far inside the i32 range, and `as` saturates anyway"
)]
pub(super) fn whole_px(v: f64) -> i32 {
    v.round() as i32
}

/// The edge leaving `island` in the direction of the drag vector, if any.
pub(super) fn edge_in_direction(board: &Board, island: usize, dx: f64, dy: f64) -> Option<usize> {
    let horizontal = dx.abs() >= dy.abs();
    let forward = if horizontal { dx > 0.0 } else { dy > 0.0 };
    board.incident[island].iter().copied().find(|&e| {
        let edge = board.edges[e];
        let is_horizontal = edge.orientation == Orientation::Horizontal;
        // `a` is left of / above `b`, so going forward from `a` follows the edge
        is_horizontal == horizontal && (edge.a == island) == forward
    })
}
