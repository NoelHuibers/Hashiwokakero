//! Board geometry in art pixels: where edges are, and which cell or edge a point is on.

use common::{Board, Orientation, Pos};

use crate::components::sprites::{CELL, Span};

/// Where the bridges of an edge go, also for the board of the preview images (`server::og`).
pub(crate) fn span_of(board: &Board, edge: usize) -> Span {
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

#[cfg(test)]
mod tests {
    use common::Puzzle;

    use super::*;

    /// A vertical bridge from the top crossing a horizontal one in the middle row, and two
    /// shorter ones in the bottom right corner:
    ///
    /// ```text
    /// . A . . .
    /// . . . . .
    /// B . . . C
    /// . . . . .
    /// . D . . E
    /// ```
    fn board() -> Board {
        Board::new(Puzzle::from_grid(&[
            vec![0, 1, 0, 0, 0],
            vec![0, 0, 0, 0, 0],
            vec![2, 0, 0, 0, 3],
            vec![0, 0, 0, 0, 0],
            vec![0, 2, 0, 0, 1],
        ]))
    }

    /// The islands in reading order.
    const A: usize = 0;
    const B: usize = 1;
    const C: usize = 2;
    const D: usize = 3;
    const E: usize = 4;

    fn edge(board: &Board, a: usize, b: usize) -> usize {
        board.edge_between(a, b).unwrap()
    }

    /// The centre of a cell in art pixels.
    fn centre(x: u8, y: u8) -> (f64, f64) {
        let c = f64::from(CELL);
        (f64::from(x) * c + c / 2.0, f64::from(y) * c + c / 2.0)
    }

    #[test]
    fn points_fall_into_the_cell_under_them() {
        let b = board();
        assert_eq!(cell_of(&b, 0.0, 0.0), Some(Pos::new(0, 0)));
        // The last pixel of a cell and the first of the next
        assert_eq!(cell_of(&b, 15.9, 16.0), Some(Pos::new(0, 1)));
        assert_eq!(cell_of(&b, 79.9, 79.9), Some(Pos::new(4, 4)));
    }

    #[test]
    fn points_off_the_board_are_in_no_cell() {
        let b = board();
        for (px, py) in [
            (-0.1, 5.0),
            (5.0, -0.1),
            (80.0, 5.0),
            (5.0, 80.0),
            // Beyond what a cell index can hold
            (1e9, 5.0),
            (5.0, f64::INFINITY),
        ] {
            assert_eq!(cell_of(&b, px, py), None, "({px}, {py})");
        }
    }

    #[test]
    fn every_water_cell_knows_the_edges_over_it() {
        let b = board();
        let cells = cell_edges(&b);
        let at = |x: usize, y: usize| cells[y * 5 + x].clone();
        assert_eq!(cells.len(), 25);
        // Where the two long bridges cross
        let mut crossing = at(1, 2);
        crossing.sort_unstable();
        let mut both = vec![edge(&b, A, D), edge(&b, B, C)];
        both.sort_unstable();
        assert_eq!(crossing, both);
        assert_eq!(at(2, 2), [edge(&b, B, C)]);
        assert_eq!(at(1, 1), [edge(&b, A, D)]);
        assert_eq!(at(4, 3), [edge(&b, C, E)]);
        assert_eq!(at(3, 4), [edge(&b, D, E)]);
        // Islands and open water have none
        assert!(at(1, 0).is_empty() && at(3, 1).is_empty());
        assert_eq!(cells.iter().map(Vec::len).sum::<usize>(), 3 + 3 + 1 + 2);
    }

    #[test]
    fn a_point_on_water_picks_the_edge_passing_over_it() {
        let b = board();
        let cells = cell_edges(&b);
        let (x, y) = centre(2, 2);
        assert_eq!(edge_at(&b, &cells, x, y), Some(edge(&b, B, C)));
        let (x, y) = centre(4, 3);
        assert_eq!(edge_at(&b, &cells, x + 7.0, y - 7.0), Some(edge(&b, C, E)));
    }

    /// Where two edges cross, the one whose line is closer to the point.
    #[test]
    fn at_a_crossing_the_closer_line_wins() {
        let b = board();
        let cells = cell_edges(&b);
        let (x, y) = centre(1, 2);
        assert_eq!(edge_at(&b, &cells, x + 6.0, y + 1.0), Some(edge(&b, B, C)));
        assert_eq!(edge_at(&b, &cells, x - 1.0, y - 6.0), Some(edge(&b, A, D)));
    }

    #[test]
    fn islands_open_water_and_the_outside_have_no_edge() {
        let b = board();
        let cells = cell_edges(&b);
        let (x, y) = centre(1, 0);
        assert_eq!(edge_at(&b, &cells, x, y), None);
        let (x, y) = centre(3, 1);
        assert_eq!(edge_at(&b, &cells, x, y), None);
        assert_eq!(edge_at(&b, &cells, -3.0, 40.0), None);
        assert_eq!(edge_at(&b, &cells, 40.0, 200.0), None);
    }

    #[test]
    fn midpoints_lie_halfway_between_the_island_centres() {
        let b = board();
        // A (1, 0) to D (1, 4): the centre of cell (1, 2)
        assert_eq!(midpoint(&b, edge(&b, A, D)), centre(1, 2));
        // D (1, 4) to E (4, 4): on the line between cells (2, 4) and (3, 4)
        let (x, y) = midpoint(&b, edge(&b, D, E));
        assert!((x - 48.0).abs() < 1e-9 && (y - centre(0, 4).1).abs() < 1e-9);
    }

    #[test]
    fn a_drag_follows_the_edge_in_its_main_direction() {
        let b = board();
        let (bc, ce) = (edge(&b, B, C), edge(&b, C, E));
        assert_eq!(edge_in_direction(&b, B, 10.0, 3.0), Some(bc));
        assert_eq!(edge_in_direction(&b, C, -10.0, 3.0), Some(bc));
        assert_eq!(edge_in_direction(&b, C, 2.0, 10.0), Some(ce));
        assert_eq!(edge_in_direction(&b, E, -2.0, -10.0), Some(ce));
        assert_eq!(edge_in_direction(&b, A, 0.0, 1.0), Some(edge(&b, A, D)));
        // As far sideways as down counts as sideways
        assert_eq!(edge_in_direction(&b, D, 5.0, 5.0), Some(edge(&b, D, E)));
    }

    #[test]
    fn a_drag_towards_no_neighbour_finds_no_edge() {
        let b = board();
        // B has a neighbour to the right only, C none to the right or above
        assert_eq!(edge_in_direction(&b, B, -10.0, 0.0), None);
        assert_eq!(edge_in_direction(&b, B, 0.0, 10.0), None);
        assert_eq!(edge_in_direction(&b, C, 10.0, 0.0), None);
        assert_eq!(edge_in_direction(&b, C, 0.0, -10.0), None);
        assert_eq!(edge_in_direction(&b, A, 0.0, -10.0), None);
    }

    #[test]
    fn pixels_round_to_the_nearest_whole_one() {
        assert_eq!(whole_px(1.49), 1);
        assert_eq!(whole_px(2.5), 3);
        assert_eq!(whole_px(-2.5), -3);
        assert_eq!(whole_px(-0.4), 0);
    }
}
