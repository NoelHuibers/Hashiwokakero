//! A human-style solver used to rate the difficulty of generated puzzles.
//!
//! It keeps a range `lo..=hi` of possible bridge counts for every edge and narrows the ranges
//! with three levels of techniques, always preferring the simplest one:
//!
//! 1. **Basic** (what beginners do): island capacities (an island needing `r` bridges whose
//!    other neighbours can take at most `m` needs at least `r - m` on the remaining edge, and
//!    can take at most `r - (bridges already certain elsewhere)`), no crossing a certain
//!    bridge, and the isolation rules (two 1s, or two 2s with a double bridge, would form a
//!    closed group).
//! 2. **Connectivity**: a group of connected islands that is not the whole puzzle must not
//!    become closed (a bridge that would saturate the group is impossible), and a group with
//!    a single possible exit has to use it.
//! 3. **Trial**: assume a value for one edge, propagate with levels 1 and 2 and drop the value
//!    if it leads to a contradiction (one step of lookahead).
//!
//! All techniques are sound, so a puzzle solved completely by this solver has a unique solution.
//!
//! Both functions also report their work (see [`Analysis::work`]): the generator's budget counts
//! it instead of the time, so its puzzles don't depend on the speed of the machine.

use std::cell::Cell;

use common::Board;

/// How far the deduction solver gets and what it needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analysis {
    /// Edges (possible bridges) of the puzzle.
    pub edges: usize,
    /// Fraction of the edges decided by basic propagation alone, from the empty board.
    pub basic_fraction: f64,
    /// Fraction of the edges decided at the end.
    pub decided_fraction: f64,
    /// Rounds of basic propagation (over all islands) until the first fixpoint.
    pub basic_rounds: u32,
    /// How often the solver had to fall back to connectivity reasoning.
    pub connectivity_steps: u32,
    /// How often the solver had to fall back to trial and error.
    pub trial_steps: u32,
    /// Completely solved with the techniques above.
    pub solved: bool,
    /// Edges the solver couldn't decide.
    pub undecided: Vec<usize>,
    /// Work spent, about proportional to the time: one unit per island or edge looked at.
    pub work: u64,
}

impl Analysis {
    /// A rough difficulty score, 0 for puzzles solved by basic propagation only.
    #[expect(
        clippy::cast_precision_loss,
        reason = "edge counts of a puzzle are far below 2^52"
    )]
    pub fn score(&self) -> f64 {
        let undecided_basic = 1.0 - self.basic_fraction;
        let trials = f64::from(self.trial_steps) / self.edges.max(1) as f64;
        let unsolved = if self.solved {
            0.0
        } else {
            1.0 - self.decided_fraction
        };
        100.0 * undecided_basic
            + 2.0 * f64::from(self.connectivity_steps.min(20))
            + 300.0 * trials
            + 150.0 * unsolved
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    lo: Vec<u8>,
    hi: Vec<u8>,
    /// Islands whose edges changed since basic propagation last looked at them.
    dirty: Vec<usize>,
}

impl State {
    fn decided(&self) -> usize {
        self.lo.iter().zip(&self.hi).filter(|(l, h)| l == h).count()
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "edge counts of a puzzle are far below 2^52"
    )]
    fn decided_fraction(&self) -> f64 {
        if self.lo.is_empty() {
            1.0
        } else {
            self.decided() as f64 / self.lo.len() as f64
        }
    }
}

struct Contradiction;

struct Solver<'a> {
    board: &'a Board,
    required: Vec<u8>,
    /// See [`Analysis::work`].
    work: Cell<u64>,
}

/// Analyzes a puzzle. `max_trial_steps` bounds the work spent on trial and error.
pub fn analyze(board: &Board, max_trial_steps: u32) -> Analysis {
    let solver = Solver::new(board);
    let edges = board.edges.len();
    let mut analysis = Analysis {
        edges,
        ..Default::default()
    };
    let mut state = solver.initial();
    if let Ok(rounds) = solver.basic(&mut state) {
        analysis.basic_rounds = rounds;
    } else {
        analysis.undecided = (0..edges).collect();
        analysis.work = solver.work.get();
        return analysis;
    }
    analysis.basic_fraction = state.decided_fraction();

    let result = (|| -> Result<(), Contradiction> {
        loop {
            if state.decided() == edges {
                analysis.solved = solver.connected(&state);
                return Ok(());
            }
            if solver.connectivity(&mut state)? {
                analysis.connectivity_steps += 1;
                solver.basic(&mut state)?;
                continue;
            }
            if analysis.trial_steps < max_trial_steps && solver.trial(&mut state)? {
                analysis.trial_steps += 1;
                solver.propagate(&mut state)?;
                continue;
            }
            return Ok(());
        }
    })();
    if result.is_err() {
        analysis.solved = false;
    }
    analysis.decided_fraction = state.decided_fraction();
    analysis.undecided = (0..edges).filter(|&e| state.lo[e] < state.hi[e]).collect();
    analysis.work = solver.work.get();
    analysis
}

/// Result of [`other_solution`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Uniqueness {
    /// No solution other than the known one exists.
    Unique,
    /// Another solution, bridge counts per edge.
    Other(Vec<u8>),
    /// The search gave up (node limit).
    Unknown,
}

/// Searches a solution different from `known` with backtracking on top of the deduction rules.
///
/// Much faster than the SAT solver on the small, almost unique puzzles of the generator, but
/// gives up after `node_limit` search nodes.
pub fn other_solution(board: &Board, known: &[u8], node_limit: u32) -> Uniqueness {
    other_solution_with_work(board, known, node_limit).0
}

/// [`other_solution`] and the work it took, see [`Analysis::work`].
pub fn other_solution_with_work(board: &Board, known: &[u8], node_limit: u32) -> (Uniqueness, u64) {
    let solver = Solver::new(board);
    let mut nodes = 0;
    let uniqueness = match solver.search(solver.initial(), known, &mut nodes, node_limit) {
        Some(Ok(other)) => Uniqueness::Other(other),
        Some(Err(())) => Uniqueness::Unknown,
        None => Uniqueness::Unique,
    };
    (uniqueness, solver.work.get())
}

impl<'a> Solver<'a> {
    fn new(board: &'a Board) -> Self {
        Self {
            board,
            required: board.puzzle.islands.iter().map(|i| i.required).collect(),
            work: Cell::new(0),
        }
    }

    fn charge(&self, units: usize) {
        self.work
            .set(self.work.get() + u64::try_from(units).unwrap_or(u64::MAX));
    }

    /// Depth first search for a solution other than `known`: `None` if there is none,
    /// `Some(Err)` if the node limit was hit.
    fn search(
        &self,
        mut s: State,
        known: &[u8],
        nodes: &mut u32,
        limit: u32,
    ) -> Option<Result<Vec<u8>, ()>> {
        *nodes += 1;
        if *nodes > limit {
            return Some(Err(()));
        }
        self.propagate(&mut s).ok()?;
        // Branch on the undecided edge with the smallest range
        let Some(e) = (0..s.lo.len())
            .filter(|&e| s.lo[e] < s.hi[e])
            .min_by_key(|&e| (s.hi[e] - s.lo[e], e))
        else {
            // Everything decided and propagate checked the numbers, crossings and connectivity
            return (s.lo != known && self.board.check(&s.lo).solved).then_some(Ok(s.lo));
        };
        // Values that differ from the known solution first, they lead to other solutions
        let mut values = (s.lo[e]..=s.hi[e]).collect::<Vec<_>>();
        values.sort_by_key(|&v| v == known[e]);
        for v in values {
            self.charge(s.lo.len());
            let mut t = s.clone();
            if self.set_lo(&mut t, e, v).is_err() || self.set_hi(&mut t, e, v).is_err() {
                continue;
            }
            if let Some(result) = self.search(t, known, nodes, limit) {
                return Some(result);
            }
        }
        None
    }

    fn initial(&self) -> State {
        let board = self.board;
        let mut hi = board
            .edges
            .iter()
            .map(|e| 2.min(self.required[e.a]).min(self.required[e.b]))
            .collect::<Vec<_>>();
        // Isolation: two 1s, or two 2s with a double bridge, would form a closed group
        if self.required.len() > 2 {
            for (e, edge) in board.edges.iter().enumerate() {
                match (self.required[edge.a], self.required[edge.b]) {
                    (1, 1) => hi[e] = 0,
                    (2, 2) => hi[e] = hi[e].min(1),
                    _ => {}
                }
            }
        }
        State {
            lo: vec![0; board.edges.len()],
            hi,
            dirty: (0..self.required.len()).collect(),
        }
    }

    /// Raises the minimum of an edge, blocking the edges it crosses.
    fn set_lo(&self, s: &mut State, e: usize, lo: u8) -> Result<(), Contradiction> {
        if lo <= s.lo[e] {
            return Ok(());
        }
        if lo > s.hi[e] {
            return Err(Contradiction);
        }
        if s.lo[e] == 0 {
            for &f in &self.board.crossings[e] {
                self.set_hi(s, f, 0)?;
            }
        }
        s.lo[e] = lo;
        let edge = self.board.edges[e];
        s.dirty.extend([edge.a, edge.b]);
        Ok(())
    }

    /// Lowers the maximum of an edge.
    fn set_hi(&self, s: &mut State, e: usize, hi: u8) -> Result<(), Contradiction> {
        if hi >= s.hi[e] {
            return Ok(());
        }
        if hi < s.lo[e] {
            return Err(Contradiction);
        }
        s.hi[e] = hi;
        let edge = self.board.edges[e];
        s.dirty.extend([edge.a, edge.b]);
        Ok(())
    }

    /// Level 1 until nothing changes, returns the number of rounds (waves of islands to look
    /// at). Only islands whose edges changed are looked at again.
    fn basic(&self, s: &mut State) -> Result<u32, Contradiction> {
        let board = self.board;
        let mut queued = vec![false; self.required.len()];
        let mut rounds = 0;
        while !s.dirty.is_empty() {
            rounds += 1;
            let mut wave = std::mem::take(&mut s.dirty);
            wave.retain(|&i| !std::mem::replace(&mut queued[i], true));
            for &i in &wave {
                queued[i] = false;
            }
            for i in wave {
                let incident = &board.incident[i];
                self.charge(1 + incident.len());
                let r = i32::from(self.required[i]);
                let sum_lo: i32 = incident.iter().map(|&e| i32::from(s.lo[e])).sum();
                let sum_hi: i32 = incident.iter().map(|&e| i32::from(s.hi[e])).sum();
                if sum_lo > r || sum_hi < r {
                    return Err(Contradiction);
                }
                for &e in incident {
                    // Stale sums after a change only weaken the bounds until the island is
                    // looked at again (it is queued again by the change)
                    let (lo, hi) = (i32::from(s.lo[e]), i32::from(s.hi[e]));
                    // The new minimum is within `lo..=r` (the other edges' sum is never
                    // negative, `r` comes from a u8), the new maximum within `0..=hi`
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "both new bounds are within 0..=255, see above"
                    )]
                    let (new_lo, new_hi) = (
                        lo.max(r - (sum_hi - hi)) as u8,
                        hi.min(r - (sum_lo - lo)).max(0) as u8,
                    );
                    self.set_lo(s, e, new_lo)?;
                    self.set_hi(s, e, new_hi)?;
                }
            }
        }
        Ok(rounds)
    }

    /// Components of the islands joined by certain bridges: (component per island, count).
    fn components(&self, s: &State) -> (Vec<usize>, usize) {
        let board = self.board;
        let n = self.required.len();
        self.charge(n + board.edges.len());
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
                    if s.lo[e] > 0 && comp[j] == usize::MAX {
                        comp[j] = count;
                        stack.push(j);
                    }
                }
            }
            count += 1;
        }
        (comp, count)
    }

    fn connected(&self, s: &State) -> bool {
        self.components(s).1 <= 1
    }

    /// Level 2, returns whether anything changed.
    fn connectivity(&self, s: &mut State) -> Result<bool, Contradiction> {
        let board = self.board;
        let n = self.required.len();
        let (comp, count) = self.components(s);
        if count <= 1 {
            return Ok(false);
        }
        // The loops below look at every island once and every edge twice
        self.charge(n + 2 * board.edges.len());
        // Bridges still missing and size of every component
        let mut need = vec![0i32; count];
        let mut size = vec![0usize; count];
        for (i, &c) in comp.iter().enumerate() {
            let have: i32 = board.incident[i].iter().map(|&e| i32::from(s.lo[e])).sum();
            need[c] += i32::from(self.required[i]) - have;
            size[c] += 1;
        }
        let mut exits = vec![Vec::new(); count];
        for (e, edge) in board.edges.iter().enumerate() {
            let (ca, cb) = (comp[edge.a], comp[edge.b]);
            if ca != cb && s.hi[e] > 0 {
                exits[ca].push(e);
                exits[cb].push(e);
            }
        }
        let mut changed = false;
        for c in 0..count {
            if size[c] == n {
                continue;
            }
            if need[c] == 0 || exits[c].is_empty() {
                return Err(Contradiction);
            }
            if let [e] = exits[c][..]
                && s.lo[e] == 0
            {
                self.set_lo(s, e, 1)?;
                changed = true;
            }
        }
        if changed {
            return Ok(true);
        }
        for (e, edge) in board.edges.iter().enumerate() {
            if s.lo[e] == s.hi[e] {
                continue;
            }
            let (ca, cb) = (comp[edge.a], comp[edge.b]);
            let (need_after, size_after) = if ca == cb {
                (need[ca] - 2, size[ca])
            } else {
                (need[ca] + need[cb] - 2, size[ca] + size[cb])
            };
            if need_after == 0 && size_after < n {
                // One more bridge would close the group
                self.set_hi(s, e, s.lo[e])?;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Levels 1 and 2 until nothing changes.
    fn propagate(&self, s: &mut State) -> Result<(), Contradiction> {
        loop {
            self.basic(s)?;
            if !self.connectivity(s)? {
                if s.decided() == s.lo.len() && !self.connected(s) {
                    return Err(Contradiction);
                }
                return Ok(());
            }
        }
    }

    /// Level 3: finds one edge where a value leads to a contradiction and excludes it.
    fn trial(&self, s: &mut State) -> Result<bool, Contradiction> {
        let mut undecided = (0..s.lo.len())
            .filter(|&e| s.lo[e] < s.hi[e])
            .collect::<Vec<_>>();
        // Small ranges first, they are the easiest to reason about
        undecided.sort_by_key(|&e| (s.hi[e] - s.lo[e], e));
        for e in undecided {
            let (lo, hi) = (s.lo[e], s.hi[e]);
            let possible = (lo..=hi)
                .filter(|&v| {
                    self.charge(s.lo.len());
                    let mut t = s.clone();
                    self.set_lo(&mut t, e, v).is_ok()
                        && self.set_hi(&mut t, e, v).is_ok()
                        && self.propagate(&mut t).is_ok()
                })
                .collect::<Vec<_>>();
            let (Some(&new_lo), Some(&new_hi)) = (possible.first(), possible.last()) else {
                return Err(Contradiction);
            };
            if new_lo != lo || new_hi != hi {
                self.set_lo(s, e, new_lo)?;
                self.set_hi(s, e, new_hi)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::Puzzle;

    fn analyze_txt(txt: &str) -> Analysis {
        analyze(&Board::new(Puzzle::parse_txt(txt).unwrap()), 100)
    }

    #[test]
    fn basic_solves_simple_puzzle() {
        let a = analyze_txt("3 3\n1.2\n...\n..1\n");
        assert!(a.solved);
        assert_eq!(a.basic_fraction, 1.0);
        assert_eq!(a.trial_steps, 0);
        assert_eq!(a.score(), 0.0);
    }

    #[test]
    fn ambiguous_puzzle_is_not_solved() {
        // test1 has several solutions
        let txt = std::fs::read_to_string("input/test1.txt").unwrap();
        assert!(!analyze_txt(&txt).solved);
        // A square of 2s is unique thanks to the isolation rule
        assert!(analyze_txt("3 3\n2.2\n...\n2.2\n").solved);
    }

    #[test]
    fn search_agrees_with_sat() {
        use common::{SolveOutcome, SolverConfig};
        for i in 1..=24 {
            let Ok(txt) = std::fs::read_to_string(format!("input/test{i}.txt")) else {
                continue;
            };
            let Ok(puzzle) = Puzzle::parse_txt(&txt) else {
                continue;
            };
            let report = crate::api::solve_unique(&puzzle, &SolverConfig::splr()).unwrap();
            let board = Board::new(puzzle);
            if let SolveOutcome::Solved { bridges, unique } = report.outcome {
                let result = other_solution(&board, &bridges, 100_000);
                match (unique, &result) {
                    (Some(true), Uniqueness::Unique) => {}
                    (Some(false), Uniqueness::Other(other)) => {
                        assert!(board.check(other).solved);
                        assert_ne!(other, &bridges);
                    }
                    _ => panic!("test{i}: SAT says {unique:?}, search says {result:?}"),
                }
            }
        }
    }

    #[test]
    fn solves_inputs_with_unique_solution() {
        let txt = std::fs::read_to_string("input/test20.txt").unwrap();
        let a = analyze_txt(&txt);
        assert!(a.solved, "{a:?}");
    }
}
