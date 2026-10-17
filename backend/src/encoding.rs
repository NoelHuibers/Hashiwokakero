//! CNF encoding of a puzzle, based on the geometry computed by [`common::Board`].
//!
//! Every possible bridge (`Board::edges[e]`) gets two variables:
//! - `x_e1 = 2e + 1`: at least one bridge is built on `e`,
//! - `x_e2 = 2e + 2`: a second bridge is built on `e`.
//!
//! See `Clauses.md` for the rules. Connectivity is not part of the static encoding, it is
//! enforced lazily by [`cut_clause`] (see [`crate::api`]).

use std::fmt::Write;

use common::{Board, Puzzle};
use itertools::Itertools;

/// The static CNF of a puzzle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoding {
    pub num_vars: usize,
    pub clauses: Vec<Vec<i32>>,
}

/// Variable of the `k`-th bridge (1 or 2) on edge `edge`.
pub fn var(edge: usize, k: u8) -> i32 {
    debug_assert!(k == 1 || k == 2);
    (2 * edge + k as usize) as i32
}

/// The edge and bridge number (1 or 2) of a variable.
pub fn edge_of(var: i32) -> (usize, u8) {
    let v = var.unsigned_abs() as usize - 1;
    (v / 2, (v % 2) as u8 + 1)
}

pub fn encode(board: &Board) -> Encoding {
    let mut clauses = Vec::new();
    let islands = &board.puzzle.islands;

    // Rule 1: every island has exactly its number of bridges
    for (i, island) in islands.iter().enumerate() {
        let vars = board.incident[i]
            .iter()
            .flat_map(|&e| [var(e, 1), var(e, 2)])
            .collect::<Vec<_>>();
        clauses.extend(exactly_k_of_n(island.required as usize, &vars));
    }

    // Symmetry breaking: the second bridge implies the first one. Every bridge configuration
    // then corresponds to exactly one model, which is essential for counting solutions.
    for e in 0..board.edges.len() {
        clauses.push(vec![-var(e, 2), var(e, 1)]);
    }

    // Rule 2: crossing edges can't both be used (x_e2 -> x_e1 covers the second bridges)
    for (e, crossing) in board.crossings.iter().enumerate() {
        for &f in crossing.iter().filter(|&&f| e < f) {
            clauses.push(vec![-var(e, 1), -var(f, 1)]);
        }
    }

    // Rule 3: necessary conditions for connectivity
    if islands.len() > 1 {
        let (cut_edges, connected) = cut_edges(board);
        if !connected {
            // Some island can't reach the others at all
            clauses.push(vec![]);
        }
        // An edge whose removal disconnects the graph of possible bridges is used by every
        // connected solution
        for e in cut_edges {
            clauses.push(vec![var(e, 1)]);
        }
    }
    if islands.len() > 2 {
        // Two 1s (or two 2s joined by a double bridge) would form a closed group
        for (e, edge) in board.edges.iter().enumerate() {
            match (islands[edge.a].required, islands[edge.b].required) {
                (1, 1) => clauses.push(vec![-var(e, 1)]),
                (2, 2) => clauses.push(vec![-var(e, 2)]),
                _ => {}
            }
        }
    }

    Encoding {
        num_vars: 2 * board.edges.len(),
        clauses,
    }
}

/// Binomial encoding of "exactly `k` of `vars` are true". An empty clause if `k > vars.len()`.
pub fn exactly_k_of_n(k: usize, vars: &[i32]) -> Vec<Vec<i32>> {
    let n = vars.len();
    if k > n {
        return vec![vec![]];
    }
    // At least k: every subset of n - k + 1 variables contains a true one
    let at_least = vars.iter().copied().combinations(n - k + 1);
    // At most k: every subset of k + 1 variables contains a false one
    let at_most = vars
        .iter()
        .map(|v| -v)
        .combinations(k + 1)
        .filter(|_| k < n);
    at_least.filter(|_| k > 0).chain(at_most).collect()
}

/// Bridge counts per edge from a model (`model[v]` is the value of variable `v`).
pub fn decode(board: &Board, model: &[bool]) -> Vec<u8> {
    let value = |v: i32| model.get(v as usize).copied().unwrap_or(false);
    (0..board.edges.len())
        .map(|e| value(var(e, 1)) as u8 + (value(var(e, 1)) && value(var(e, 2))) as u8)
        .collect()
}

/// The clause that excludes exactly the given bridge configuration.
pub fn blocking_clause(bridges: &[u8]) -> Vec<i32> {
    bridges
        .iter()
        .enumerate()
        .flat_map(|(e, &count)| {
            [
                if count >= 1 { -var(e, 1) } else { var(e, 1) },
                if count >= 2 { -var(e, 2) } else { var(e, 2) },
            ]
        })
        .collect()
}

/// Connected components of the islands using only built bridges, as a component id per island.
pub fn components(board: &Board, bridges: &[u8]) -> (usize, Vec<usize>) {
    let n = board.puzzle.islands.len();
    let mut comp = vec![usize::MAX; n];
    let mut count = 0;
    let mut stack = Vec::new();
    for start in 0..n {
        if comp[start] != usize::MAX {
            continue;
        }
        comp[start] = count;
        stack.push(start);
        while let Some(i) = stack.pop() {
            for &e in &board.incident[i] {
                let j = board.other(e, i);
                if bridges[e] > 0 && comp[j] == usize::MAX {
                    comp[j] = count;
                    stack.push(j);
                }
            }
        }
        count += 1;
    }
    (count, comp)
}

/// "At least one possible bridge leaves the islands of component `c`".
///
/// Every connected solution satisfies this clause as long as the component doesn't contain all
/// islands, so it can be added whenever a model is disconnected (lazy connectivity).
pub fn cut_clause(board: &Board, comp: &[usize], c: usize) -> Vec<i32> {
    board
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| (comp[edge.a] == c) != (comp[edge.b] == c))
        .map(|(e, _)| var(e, 1))
        .collect()
}

/// Cut edges ("graph bridges") of the graph of possible bridges and whether that graph is
/// connected. Iterative version of Tarjan's algorithm.
pub fn cut_edges(board: &Board) -> (Vec<usize>, bool) {
    let n = board.puzzle.islands.len();
    let mut cuts = Vec::new();
    if n == 0 {
        return (cuts, true);
    }
    let mut disc = vec![usize::MAX; n];
    let mut low = vec![0; n];
    let mut time = 0;
    let mut components = 0;
    // (island, edge used to enter it, index of the next incident edge to look at)
    let mut stack: Vec<(usize, usize, usize)> = Vec::new();
    for root in 0..n {
        if disc[root] != usize::MAX {
            continue;
        }
        components += 1;
        disc[root] = time;
        low[root] = time;
        time += 1;
        stack.push((root, usize::MAX, 0));
        while let Some(&mut (i, parent_edge, ref mut next)) = stack.last_mut() {
            if let Some(&e) = board.incident[i].get(*next) {
                *next += 1;
                if e == parent_edge {
                    continue;
                }
                let j = board.other(e, i);
                if disc[j] == usize::MAX {
                    disc[j] = time;
                    low[j] = time;
                    time += 1;
                    stack.push((j, e, 0));
                } else {
                    low[i] = low[i].min(disc[j]);
                }
            } else {
                stack.pop();
                if let Some(&(p, _, _)) = stack.last() {
                    low[p] = low[p].min(low[i]);
                    if low[i] > disc[p] {
                        cuts.push(parent_edge);
                    }
                }
            }
        }
    }
    cuts.sort_unstable();
    (cuts, components == 1)
}

/// DIMACS text of the static encoding, with comments naming the variables.
pub fn dimacs(board: &Board, encoding: &Encoding) -> String {
    let mut out = String::new();
    let p = &board.puzzle;
    let _ = writeln!(
        out,
        "c Hashiwokakero {}x{} (width x height), {} islands, {} possible bridges",
        p.width,
        p.height,
        p.islands.len(),
        board.edges.len()
    );
    let _ = writeln!(
        out,
        "c Coordinates are (x,y) = (column,row), counted from 0"
    );
    for (e, _) in board.edges.iter().enumerate() {
        let (a, b) = board.endpoints(e);
        for k in 1..=2 {
            let _ = writeln!(
                out,
                "c var {} = bridge ({},{})-({},{}) #{}",
                var(e, k),
                a.x,
                a.y,
                b.x,
                b.y,
                k
            );
        }
    }
    let _ = writeln!(
        out,
        "p cnf {} {}",
        encoding.num_vars,
        encoding.clauses.len()
    );
    for clause in &encoding.clauses {
        for lit in clause {
            let _ = write!(out, "{lit} ");
        }
        out.push_str("0\n");
    }
    out
}

/// Convenience: board and encoding of a puzzle.
pub fn encode_puzzle(puzzle: &Puzzle) -> (Board, Encoding) {
    let board = Board::new(puzzle.clone());
    let encoding = encode(&board);
    (board, encoding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_k_small() {
        assert_eq!(exactly_k_of_n(2, &[1, 2]), vec![vec![1], vec![2]]);
        assert_eq!(exactly_k_of_n(0, &[1, 2]), vec![vec![-1], vec![-2]]);
        assert_eq!(exactly_k_of_n(3, &[1, 2]), vec![Vec::<i32>::new()]);
        assert_eq!(
            exactly_k_of_n(1, &[1, 2, 3]),
            vec![vec![1, 2, 3], vec![-1, -2], vec![-1, -3], vec![-2, -3]]
        );
        assert!(exactly_k_of_n(0, &[]).is_empty());
    }

    #[test]
    fn exactly_k_binomial() {
        let clauses = exactly_k_of_n(3, &[1, 2, 3, 4]);
        assert_eq!(
            clauses,
            [
                // Each var occurs exactly three times => at least three must be true
                vec![1, 2],
                vec![1, 3],
                vec![1, 4],
                vec![2, 3],
                vec![2, 4],
                vec![3, 4],
                // At least one must be false
                vec![-1, -2, -3, -4]
            ]
        );
        assert_eq!(exactly_k_of_n(2, &[1, 2, 3, 4, 5]).len(), 5 + 10);
        let all = (1..=8).collect::<Vec<_>>();
        assert_eq!(
            exactly_k_of_n(8, &all),
            (1..=8).map(|v| vec![v]).collect::<Vec<_>>()
        );
        // More bridges than variables is unsatisfiable (the old encoder got this wrong)
        assert_eq!(exactly_k_of_n(4, &[5, 6]), vec![Vec::<i32>::new()]);
    }

    #[test]
    fn var_round_trip() {
        for e in 0..10 {
            for k in 1..=2 {
                assert_eq!(edge_of(var(e, k)), (e, k));
            }
        }
    }

    #[test]
    fn finds_cut_edges() {
        // A square with a tail: the tail edge is a cut edge, the square isn't
        let puzzle = Puzzle::parse_txt("5 5\n2.2.1\n.....\n2.2..\n.....\n.....\n").unwrap();
        let board = Board::new(puzzle);
        let (cuts, connected) = cut_edges(&board);
        assert!(connected);
        assert_eq!(cuts.len(), 1);
        let (a, b) = board.endpoints(cuts[0]);
        assert_eq!((a.x, a.y, b.x, b.y), (2, 0, 4, 0));

        let puzzle = Puzzle::parse_txt("3 3\n1.1\n...\n..1\n").unwrap();
        let (cuts, connected) = cut_edges(&Board::new(puzzle));
        assert!(connected);
        assert_eq!(cuts.len(), 2);

        let puzzle = Puzzle::parse_txt("3 3\n1.1\n...\n1..\n").unwrap();
        let puzzle = Puzzle::new(puzzle.width, puzzle.height, {
            let mut i = puzzle.islands.clone();
            i.retain(|i| i.pos.y == 0);
            i.push(common::Island {
                pos: common::Pos::new(1, 2),
                required: 1,
            });
            i
        });
        assert!(!cut_edges(&Board::new(puzzle)).1);
    }

    #[test]
    fn blocking_clause_excludes_only_that_solution() {
        assert_eq!(blocking_clause(&[0, 1, 2]), vec![1, 2, -3, 4, -5, -6]);
    }
}
