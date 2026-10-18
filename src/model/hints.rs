//! Hints and checks: which bridge to fix next, which bridges are wrong.

use common::Board;

/// The bridge a hint fixes: wrong bridges first, then a missing bridge at the most constrained
/// island. Returns the edge and its value in the solution.
pub fn pick_hint(board: &Board, current: &[u8], solution: &[u8]) -> Option<(usize, u8)> {
    let n = board.edges.len();
    if current.len() != n || solution.len() != n {
        return None;
    }
    let wrong = (0..n).find(|&e| current[e] > solution[e]);
    let missing_at = |island: usize| {
        board.incident[island]
            .iter()
            .filter(|&&e| current[e] < solution[e])
            .count()
    };
    let edge = wrong.or_else(|| {
        (0..n)
            .filter(|&e| current[e] < solution[e])
            .min_by_key(|&e| missing_at(board.edges[e].a).min(missing_at(board.edges[e].b)))
    })?;
    Some((edge, solution[edge]))
}

/// Edges with more bridges than in the solution.
pub fn wrong_edges(current: &[u8], solution: &[u8]) -> Vec<usize> {
    current
        .iter()
        .zip(solution)
        .enumerate()
        .filter(|(_, (c, s))| c > s)
        .map(|(e, _)| e)
        .collect()
}
