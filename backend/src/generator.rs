//! Puzzle generator.
//!
//! 1. **Grow** a connected bridge network: repeatedly pick a free cell that sees an existing
//!    island (or the middle of a bridge, which is then split by a new island) over open water,
//!    preferring cells in empty regions, short bridges and no long parallel bridges, and connect
//!    it with a single or double bridge. Now and then close a cycle instead (a bridge between two
//!    islands that see each other). Islands are never placed orthogonally adjacent.
//! 2. The numbers are the degrees of this intended solution.
//! 3. **Uniqueness**: search another solution with the backtracking search of
//!    [`crate::deduce`] (the SAT solver takes over if the search gives up). If there is one,
//!    **repair** the puzzle where the two solutions differ (split a bridge with a new island,
//!    remove a bridge on a cycle, switch a bridge between single and double, or build the other
//!    solution's bridge) and check again.
//! 4. **Difficulty**: rate the unique puzzle with the human-style solver in [`crate::deduce`]
//!    ([`Analysis::score`]) and tune it towards the target range of the difficulty with a small
//!    local search (ease where the solver gets stuck / add bridges).
//! 5. Repeat for several candidates and keep the one closest to the target range. Its
//!    uniqueness is confirmed by the SAT solver (with the lazy connectivity loop).
//!
//! Everything is driven by a seeded RNG and fixed work limits, so the same [`GenerateParams`]
//! give the same puzzle. Wall-clock limits only exist as a safety net.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use common::{
    Board, Difficulty, GenerateParams, GeneratedPuzzle, Island, MAX_SIZE, MIN_SIZE, Pos, Puzzle,
    SolverConfig,
};
use rand::{RngExt, SeedableRng, rngs::StdRng, seq::SliceRandom};

use crate::api::{Search, other_solution};
use crate::deduce::{self, Analysis, Uniqueness};

/// Knobs of the generator for one difficulty.
#[derive(Clone, Copy, Debug)]
struct Profile {
    /// Islands per cell to aim for.
    density: f64,
    /// Probability that a new bridge is double.
    double: f64,
    /// Probability to close a cycle (an extra bridge between two islands that see each
    /// other) instead of adding an island while growing.
    cycles: f64,
    /// Weight of removing a bridge on a cycle when repairing (fewer cycles, easier puzzles).
    remove: f64,
    /// Acceptable range of [`Analysis::score`].
    score: (f64, f64),
    /// Candidates to try at most (each with several repairs).
    candidates: u32,
    /// Safety net in milliseconds, only reached on pathological inputs.
    budget_ms: u64,
}

fn profile(difficulty: Difficulty) -> Profile {
    match difficulty {
        Difficulty::Easy => Profile {
            density: 0.20,
            double: 0.4,
            cycles: 0.03,
            remove: 3.0,
            score: (0.0, 0.0),
            candidates: 30,
            budget_ms: 250,
        },
        Difficulty::Medium => Profile {
            density: 0.22,
            double: 0.35,
            cycles: 0.12,
            remove: 1.5,
            score: (5.0, 40.0),
            candidates: 20,
            budget_ms: 250,
        },
        Difficulty::Hard => Profile {
            density: 0.24,
            double: 0.35,
            cycles: 0.18,
            remove: 1.0,
            score: (40.0, 58.0),
            candidates: 24,
            budget_ms: 1_200,
        },
        Difficulty::Expert => Profile {
            density: 0.255,
            double: 0.31,
            cycles: 0.26,
            remove: 0.6,
            score: (58.0, 72.0),
            candidates: 30,
            budget_ms: 1_200,
        },
        Difficulty::Oni => Profile {
            density: 0.26,
            double: 0.3,
            cycles: 0.18,
            remove: 0.5,
            score: (72.0, f64::INFINITY),
            candidates: 16,
            budget_ms: 3_000,
        },
    }
}

/// Upper bound of the safety time limit for large grids.
const MAX_BUDGET_MS: u64 = 4_000;

/// The range of [`Analysis::score`] the generator aims for.
pub fn score_range(difficulty: Difficulty) -> (f64, f64) {
    profile(difficulty).score
}

/// The SAT solver for uniqueness checks: splr is much faster than our own CDCL on the hard
/// puzzles and deterministic (when the backtracking search gives up while repairing, the other
/// solution found by the SAT solver decides how the puzzle is repaired).
fn sat_solver() -> SolverConfig {
    SolverConfig {
        timeout_ms: 2_000,
        ..SolverConfig::splr()
    }
}

pub fn generate(params: &GenerateParams) -> GeneratedPuzzle {
    let start = Instant::now();
    let params = GenerateParams {
        width: params.width.clamp(MIN_SIZE, MAX_SIZE),
        height: params.height.clamp(MIN_SIZE, MAX_SIZE),
        ..*params
    };
    let mut profile = profile(params.difficulty);
    // Candidates get more expensive on grids larger than the default size of the difficulty:
    // try fewer of them and allow more time (the time limit is only a safety net, hitting it
    // makes the result depend on the speed of the machine)
    let (dw, dh) = params.difficulty.size();
    let scale =
        (f64::from(params.width) * f64::from(params.height)) / (f64::from(dw) * f64::from(dh));
    if scale > 1.0 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a positive count below the profile's value"
        )]
        let candidates = (f64::from(profile.candidates) / scale.sqrt()).ceil() as u32;
        profile.candidates = candidates.max(4);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a few seconds in milliseconds, scaled by at most 30x30 cells"
        )]
        let budget_ms = (profile.budget_ms as f64 * scale.sqrt()) as u64;
        profile.budget_ms = budget_ms.min(MAX_BUDGET_MS);
    }
    let deadline = start + Duration::from_millis(profile.budget_ms);
    // Mix the parameters into the seed, so e.g. sizes don't share the same random stream
    let seed = params.seed
        ^ u64::from(params.width) << 56
        ^ u64::from(params.height) << 48
        ^ (params.difficulty as u64) << 40;
    let mut rng = StdRng::seed_from_u64(seed);

    let mut best: Option<(f64, Candidate)> = None;
    let mut attempts = 0;
    let mut fallback: Option<Candidate> = None;
    for round in 0..profile.candidates * 2 {
        if Instant::now() >= deadline && (best.is_some() || fallback.is_some()) {
            break;
        }
        // Second half, only if nothing unique was found: fewer cycles, which makes unique
        // puzzles more likely
        let mut p = profile;
        if round >= profile.candidates {
            if best.is_some() {
                break;
            }
            p.cycles /= 3.0;
        }
        attempts += 1;
        let Some(candidate) = make_candidate(&params, &p, &mut rng, deadline) else {
            continue;
        };
        attempts += candidate.repairs;
        if !candidate.unique {
            if fallback.is_none() {
                fallback = Some(candidate);
            }
            continue;
        }
        let distance = distance(candidate.analysis.score(), p.score);
        if best.as_ref().is_none_or(|(d, _)| distance < *d) {
            // The search already showed uniqueness, the SAT solver has to confirm it
            match sat_other_solution(
                &candidate.puzzle,
                &candidate.solution,
                &sat_solver(),
                deadline,
            ) {
                Some(Search::Unsat) => best = Some((distance, candidate)),
                Some(Search::Found(_)) => {
                    debug_assert!(false, "search and SAT solver disagree on uniqueness");
                    continue;
                }
                _ => continue,
            }
        }
        if distance == 0.0 {
            break;
        }
    }

    let candidate = match (best, fallback) {
        (Some((_, c)), _) | (None, Some(c)) => c,
        // Not even a non-unique puzzle (tiny grids): fall back to the smallest valid puzzle
        (None, None) => trivial(&params),
    };
    GeneratedPuzzle {
        puzzle: candidate.puzzle,
        solution: candidate.solution,
        params,
        unique: candidate.unique,
        attempts,
        generate_ms: start.elapsed().as_secs_f64() * 1000.0,
    }
}

fn distance(score: f64, (lo, hi): (f64, f64)) -> f64 {
    if score < lo {
        lo - score
    } else if score > hi {
        score - hi
    } else {
        0.0
    }
}

struct Candidate {
    puzzle: Puzzle,
    solution: Vec<u8>,
    unique: bool,
    repairs: u32,
    analysis: Analysis,
}

fn make_candidate(
    params: &GenerateParams,
    p: &Profile,
    rng: &mut StdRng,
    deadline: Instant,
) -> Option<Candidate> {
    let mut grid = Grid::new(params.width, params.height);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive number of islands below the number of cells"
    )]
    let target = (f64::from(params.width) * f64::from(params.height) * p.density)
        .round()
        .max(2.0) as usize;
    grid.grow(rng, target, p.double, p.cycles);
    if grid.islands.len() < 2 {
        return None;
    }
    grid.add_cycles(rng, p.cycles / 2.0, p.double);

    #[expect(clippy::cast_possible_truncation, reason = "at most 30x30 islands")]
    let max_repairs = 8 + grid.islands.len() as u32 / 4;
    let mut repairs = 0;
    loop {
        let (puzzle, board, solution) = grid.puzzle();
        let other = match deduce::other_solution(&board, &solution, SEARCH_NODES) {
            Uniqueness::Unique => None,
            Uniqueness::Other(other) => Some(other),
            // The search gave up, ask the SAT solver
            Uniqueness::Unknown => {
                match sat_other_solution(&puzzle, &solution, &sat_solver(), deadline) {
                    Some(Search::Unsat) => None,
                    Some(Search::Found(other)) => Some(other),
                    _ => return None,
                }
            }
        };
        let Some(other) = other else {
            let analysis = deduce::analyze(&board, MAX_TRIALS);
            let mut candidate = Candidate {
                puzzle,
                solution,
                unique: true,
                repairs,
                analysis,
            };
            tune(&mut grid, &mut candidate, rng, p, deadline);
            return Some(candidate);
        };
        if repairs >= max_repairs
            || Instant::now() >= deadline
            || !grid.repair(rng, &board, &solution, &other, p)
        {
            return Some(Candidate {
                puzzle,
                solution,
                unique: false,
                repairs,
                analysis: Analysis::default(),
            });
        }
        repairs += 1;
    }
}

/// Local search towards the target difficulty: ease the puzzle where the deduction solver gets
/// stuck (too hard) or add bridges between islands that see each other (too easy). A change is
/// kept if the puzzle stays unique and the score gets closer to the target range.
fn tune(
    grid: &mut Grid,
    candidate: &mut Candidate,
    rng: &mut StdRng,
    p: &Profile,
    deadline: Instant,
) {
    let mut harden_steps = 0;
    for _ in 0..TUNE_STEPS {
        let current = distance(candidate.analysis.score(), p.score);
        if current == 0.0 || Instant::now() >= deadline {
            return;
        }
        let too_hard = candidate.analysis.score() > p.score.1;
        if !too_hard {
            // Hardening rarely works (new bridges tend to allow other solutions)
            harden_steps += 1;
            if harden_steps > HARDEN_STEPS {
                return;
            }
        }
        let mut changed = grid.clone();
        let (_, board, solution) = grid.puzzle();
        let moved = if too_hard {
            changed.ease(rng, &board, &solution, &candidate.analysis.undecided, p)
        } else {
            changed.harden(rng, p)
        };
        if !moved {
            return;
        }
        // Easing changes numbers, which may allow other solutions: repair a few times
        let mut unique = None;
        for _ in 0..TUNE_REPAIRS {
            let (puzzle, board, solution) = changed.puzzle();
            match deduce::other_solution(&board, &solution, SEARCH_NODES) {
                Uniqueness::Unique => {
                    unique = Some((puzzle, board, solution));
                    break;
                }
                Uniqueness::Other(other) => {
                    if !changed.repair(rng, &board, &solution, &other, p) {
                        break;
                    }
                }
                Uniqueness::Unknown => break,
            }
        }
        let Some((puzzle, board, solution)) = unique else {
            continue;
        };
        let analysis = deduce::analyze(&board, MAX_TRIALS);
        if distance(analysis.score(), p.score) < current {
            *grid = changed;
            *candidate = Candidate {
                puzzle,
                solution,
                unique: true,
                repairs: candidate.repairs + 1,
                analysis,
            };
        }
    }
}

/// Steps of the local search towards the target difficulty.
const TUNE_STEPS: u32 = 16;
/// Steps of the local search that try to make a puzzle harder.
const HARDEN_STEPS: u32 = 4;
/// Repairs after a step of the local search that made the puzzle ambiguous.
const TUNE_REPAIRS: u32 = 4;

/// Bound for the trial steps of the deduction solver, high enough to never matter in practice
/// (each step decides something) but keeps the rating bounded.
pub const MAX_TRIALS: u32 = 10_000;

/// Node limit of the backtracking search used to check uniqueness while repairing.
const SEARCH_NODES: u32 = 2_000;

/// Another solution according to the SAT solver (with the lazy connectivity loop).
fn sat_other_solution(
    puzzle: &Puzzle,
    solution: &[u8],
    config: &SolverConfig,
    deadline: Instant,
) -> Option<Search> {
    let check_deadline = deadline
        .max(Instant::now() + Duration::from_millis(200))
        .min(Instant::now() + Duration::from_millis(2_000));
    other_solution(puzzle, solution, config, Some(check_deadline)).ok()
}

/// Two islands with a single bridge: always valid and unique.
fn trivial(params: &GenerateParams) -> Candidate {
    let islands = vec![
        Island {
            pos: Pos::new(0, 0),
            required: 1,
        },
        Island {
            pos: Pos::new(2, 0),
            required: 1,
        },
    ];
    let puzzle = Puzzle::new(params.width, params.height, islands);
    Candidate {
        puzzle,
        solution: vec![1],
        unique: true,
        repairs: 0,
        analysis: Analysis::default(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cell {
    Water,
    Island(usize),
    /// A built bridge, `true` if horizontal.
    Bridge(bool),
}

const DIRECTIONS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Short bridges are more common, long ones are allowed now and then.
fn length_weight(len: i32) -> f64 {
    match len {
        2 | 3 => 1.0,
        4 => 0.6,
        5 => 0.25,
        6 => 0.1,
        _ => 0.04,
    }
}

fn direction(a: Pos, b: Pos) -> (i32, i32) {
    (
        (i32::from(b.x) - i32::from(a.x)).signum(),
        (i32::from(b.y) - i32::from(a.y)).signum(),
    )
}

/// The cell at (x, y), which must be on the grid.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "coordinates on the grid are within 0..30"
)]
fn pos(x: i32, y: i32) -> Pos {
    debug_assert!((0..i32::from(MAX_SIZE)).contains(&x) && (0..i32::from(MAX_SIZE)).contains(&y));
    Pos::new(x as u8, y as u8)
}

/// Picks an item with probability proportional to its weight.
fn pick_weighted<T: Copy>(rng: &mut StdRng, items: &[(T, f64)]) -> Option<T> {
    let total: f64 = items.iter().map(|i| i.1).sum();
    if items.is_empty() || total <= 0.0 {
        return None;
    }
    let mut pick = rng.random_range(0.0..total);
    for &(item, weight) in items {
        pick -= weight;
        if pick < 0.0 {
            return Some(item);
        }
    }
    items.last().map(|i| i.0)
}

/// The intended solution while it is being built.
#[derive(Clone)]
struct Grid {
    w: u8,
    h: u8,
    cells: Vec<Cell>,
    islands: Vec<Pos>,
    /// Built bridges between island indices (smaller index first).
    bridges: BTreeMap<(usize, usize), u8>,
}

#[derive(Clone, Copy, Debug)]
enum Source {
    Island(usize),
    /// The middle of a bridge that gets split by a new island.
    Split(Pos),
}

#[derive(Clone, Copy, Debug)]
enum Repair {
    Split(Pos),
    Remove(usize, usize),
    SetCount(usize, usize, u8),
    Add(usize, usize, u8),
}

impl Grid {
    fn new(w: u8, h: u8) -> Self {
        Self {
            w,
            h,
            cells: vec![Cell::Water; usize::from(w) * usize::from(h)],
            islands: Vec::new(),
            bridges: BTreeMap::new(),
        }
    }

    fn index(&self, x: usize, y: usize) -> usize {
        y * usize::from(self.w) + x
    }

    fn cell(&self, x: i32, y: i32) -> Option<Cell> {
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        (x < usize::from(self.w) && y < usize::from(self.h)).then(|| self.cells[self.index(x, y)])
    }

    fn set(&mut self, p: Pos, cell: Cell) {
        let i = self.index(usize::from(p.x), usize::from(p.y));
        self.cells[i] = cell;
    }

    fn island_at(&self, p: Pos) -> Option<usize> {
        match self.cell(i32::from(p.x), i32::from(p.y)) {
            Some(Cell::Island(i)) => Some(i),
            _ => None,
        }
    }

    /// An island may be placed on water that has no island as orthogonal neighbour.
    fn can_place(&self, x: i32, y: i32) -> bool {
        self.cell(x, y) == Some(Cell::Water) && self.no_island_around(x, y)
    }

    fn no_island_around(&self, x: i32, y: i32) -> bool {
        DIRECTIONS
            .iter()
            .all(|(dx, dy)| !matches!(self.cell(x + dx, y + dy), Some(Cell::Island(_))))
    }

    fn add_island(&mut self, p: Pos) -> usize {
        let i = self.islands.len();
        self.islands.push(p);
        self.set(p, Cell::Island(i));
        i
    }

    /// Water cells strictly between two islands in a line.
    fn between(&self, a: usize, b: usize) -> Vec<Pos> {
        let (pa, pb) = (self.islands[a], self.islands[b]);
        if pa.y == pb.y {
            let (x0, x1) = (pa.x.min(pb.x), pa.x.max(pb.x));
            (x0 + 1..x1).map(|x| Pos::new(x, pa.y)).collect()
        } else {
            let (y0, y1) = (pa.y.min(pb.y), pa.y.max(pb.y));
            (y0 + 1..y1).map(|y| Pos::new(pa.x, y)).collect()
        }
    }

    fn build(&mut self, a: usize, b: usize, count: u8) {
        let horizontal = self.islands[a].y == self.islands[b].y;
        for p in self.between(a, b) {
            self.set(p, Cell::Bridge(horizontal));
        }
        self.bridges.insert((a.min(b), a.max(b)), count);
    }

    fn unbuild(&mut self, a: usize, b: usize) {
        for p in self.between(a, b) {
            self.set(p, Cell::Water);
        }
        self.bridges.remove(&(a.min(b), a.max(b)));
    }

    fn degree(&self, i: usize) -> u8 {
        self.bridges
            .iter()
            .filter(|((a, b), _)| *a == i || *b == i)
            .map(|(_, &c)| c)
            .sum()
    }

    /// How occupied the area around (x, y) is: islands (and a bit for bridges) within
    /// Chebyshev distance 2.
    fn crowd(&self, x: i32, y: i32) -> f64 {
        let mut n = 0.0;
        for dy in -2..=2 {
            for dx in -2..=2 {
                n += match self.cell(x + dx, y + dy) {
                    Some(Cell::Island(_)) => 1.0,
                    Some(Cell::Bridge(_)) => 0.2,
                    _ => 0.0,
                };
            }
        }
        n
    }

    /// Weight factor for the bridge cells next to the path from `p` in direction (dx, dy) over
    /// `len` cells that run parallel to it (long parallel bridges side by side look bad and play
    /// badly): 0.4 to the power of their number.
    fn parallel_factor(&self, p: Pos, (dx, dy): (i32, i32), len: i32) -> f64 {
        let horizontal = dy == 0;
        let parallel = (1..len)
            .map(|k| (i32::from(p.x) + dx * k, i32::from(p.y) + dy * k))
            .flat_map(|(x, y)| [(x + dy, y + dx), (x - dy, y - dx)])
            .filter(|&(x, y)| self.cell(x, y) == Some(Cell::Bridge(horizontal)))
            .count();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "at most two cells per cell of a bridge, which is shorter than 30"
        )]
        let parallel = parallel as i32;
        0.4f64.powi(parallel)
    }

    fn grow(&mut self, rng: &mut StdRng, target: usize, p_double: f64, p_cycle: f64) {
        let (width, height) = (i32::from(self.w), i32::from(self.h));
        // Start away from the border on larger grids
        let margin = |n: i32| if n >= 7 { n / 4 } else { 0 };
        let x = rng.random_range(margin(width)..width - margin(width));
        let y = rng.random_range(margin(height)..height - margin(height));
        self.add_island(pos(x, y));

        let mut options = Vec::new();
        while self.islands.len() < target {
            // Close a cycle now and then, before the lines of sight get blocked
            if self.islands.len() > 3 && rng.random_bool(p_cycle) {
                let edges = self
                    .free_edges()
                    .into_iter()
                    .filter(|&(a, b)| self.degree(a) < 8 && self.degree(b) < 8)
                    .map(|(a, b)| {
                        #[expect(
                            clippy::cast_possible_truncation,
                            clippy::cast_possible_wrap,
                            reason = "bridges are shorter than 30 cells"
                        )]
                        let len = self.between(a, b).len() as i32 + 1;
                        let dir = direction(self.islands[a], self.islands[b]);
                        let parallel = self.parallel_factor(self.islands[a], dir, len);
                        let weight = length_weight(len) * parallel;
                        ((a, b), weight)
                    })
                    .collect::<Vec<_>>();
                if let Some((a, b)) = pick_weighted(rng, &edges) {
                    let count = if rng.random_bool(p_double) { 2 } else { 1 };
                    let count = count.min(8 - self.degree(a)).min(8 - self.degree(b));
                    self.build(a, b, count);
                    continue;
                }
            }
            options.clear();
            for (i, &p) in self.islands.iter().enumerate() {
                if self.degree(i) < 8 {
                    self.collect_options(p, &DIRECTIONS, Source::Island(i), 1.0, &mut options);
                }
            }
            // Growing from the middle of a bridge (splitting it with a new island) reaches
            // regions that are walled in by bridges
            for y in 0..self.h {
                for x in 0..self.w {
                    let p = Pos::new(x, y);
                    if let Cell::Bridge(horizontal) =
                        self.cells[self.index(usize::from(x), usize::from(y))]
                    {
                        if !self.no_island_around(i32::from(x), i32::from(y)) {
                            continue;
                        }
                        let dirs: &[(i32, i32)] = if horizontal {
                            &[(0, 1), (0, -1)]
                        } else {
                            &[(1, 0), (-1, 0)]
                        };
                        self.collect_options(p, dirs, Source::Split(p), 0.5, &mut options);
                    }
                }
            }
            let Some((target_pos, source)) = pick_weighted(rng, &options) else {
                break;
            };
            let from = match source {
                Source::Island(i) => i,
                Source::Split(cell) => self.split(cell),
            };
            let new = self.add_island(target_pos);
            let count = if rng.random_bool(p_double) { 2 } else { 1 };
            self.build(from, new, count);
        }
    }

    /// Places for a new island seen from `p` in the given directions, weighted by how empty
    /// the area is, the length of the bridge and parallel bridges next to it.
    fn collect_options(
        &self,
        p: Pos,
        dirs: &[(i32, i32)],
        source: Source,
        factor: f64,
        options: &mut Vec<((Pos, Source), f64)>,
    ) {
        for &dir @ (dx, dy) in dirs {
            for len in 1.. {
                let (cx, cy) = (i32::from(p.x) + dx * len, i32::from(p.y) + dy * len);
                if self.cell(cx, cy) != Some(Cell::Water) {
                    break;
                }
                if len >= 2 && self.can_place(cx, cy) {
                    let crowd = self.crowd(cx, cy);
                    let parallel = self.parallel_factor(p, dir, len);
                    let weight = factor * length_weight(len) / (1.0 + crowd).powi(2) * parallel;
                    options.push(((pos(cx, cy), source), weight));
                }
            }
        }
    }

    /// The two islands joined by the bridge over `cell`.
    fn bridge_at(&self, cell: Pos) -> (usize, usize) {
        let Some(Cell::Bridge(horizontal)) = self.cell(i32::from(cell.x), i32::from(cell.y)) else {
            panic!("no bridge at {cell:?}");
        };
        let (dx, dy) = if horizontal { (1, 0) } else { (0, 1) };
        let end = |sign: i32| {
            (1..)
                .find_map(|k| {
                    match self.cell(
                        i32::from(cell.x) + sign * dx * k,
                        i32::from(cell.y) + sign * dy * k,
                    ) {
                        Some(Cell::Island(i)) => Some(i),
                        _ => None,
                    }
                })
                .unwrap()
        };
        (end(-1), end(1))
    }

    /// Replaces the bridge over `cell` by an island with two bridges of the same count.
    fn split(&mut self, cell: Pos) -> usize {
        let (a, b) = self.bridge_at(cell);
        let count = self.bridges[&(a.min(b), a.max(b))];
        self.unbuild(a, b);
        let new = self.add_island(cell);
        self.build(a, new, count);
        self.build(new, b, count);
        new
    }

    /// Possible bridges of the current islands that aren't built and don't cross a bridge.
    fn free_edges(&self) -> Vec<(usize, usize)> {
        let mut edges = Vec::new();
        for (i, &p) in self.islands.iter().enumerate() {
            for (dx, dy) in [(1, 0), (0, 1)] {
                for len in 1.. {
                    let (cx, cy) = (i32::from(p.x) + dx * len, i32::from(p.y) + dy * len);
                    match self.cell(cx, cy) {
                        Some(Cell::Water) => continue,
                        Some(Cell::Island(j))
                            if len >= 2 && !self.bridges.contains_key(&(i.min(j), i.max(j))) =>
                        {
                            edges.push((i, j));
                        }
                        _ => {}
                    }
                    break;
                }
            }
        }
        edges
    }

    fn add_cycles(&mut self, rng: &mut StdRng, p_cycle: f64, p_double: f64) {
        let mut edges = self.free_edges();
        edges.shuffle(rng);
        for (a, b) in edges {
            let free = self
                .between(a, b)
                .iter()
                .all(|p| self.cell(i32::from(p.x), i32::from(p.y)) == Some(Cell::Water));
            if free && self.degree(a) < 8 && self.degree(b) < 8 && rng.random_bool(p_cycle) {
                let count = if rng.random_bool(p_double) { 2 } else { 1 };
                let count = count.min(8 - self.degree(a)).min(8 - self.degree(b));
                self.build(a, b, count);
            }
        }
    }

    /// The puzzle, its board and the intended solution indexed like `board.edges`.
    fn puzzle(&self) -> (Puzzle, Board, Vec<u8>) {
        let islands = self
            .islands
            .iter()
            .enumerate()
            .map(|(i, &pos)| Island {
                pos,
                required: self.degree(i),
            })
            .collect();
        let puzzle = Puzzle::new(self.w, self.h, islands);
        let board = Board::new(puzzle.clone());
        let solution = self.solution(&board);
        (puzzle, board, solution)
    }

    fn solution(&self, board: &Board) -> Vec<u8> {
        (0..board.edges.len())
            .map(|e| {
                let (ga, gb) = self.grid_edge(board, e);
                self.bridges
                    .get(&(ga.min(gb), ga.max(gb)))
                    .copied()
                    .unwrap_or(0)
            })
            .collect()
    }

    /// Grid island indices of a board edge.
    fn grid_edge(&self, board: &Board, e: usize) -> (usize, usize) {
        let (pa, pb) = board.endpoints(e);
        (self.island_at(pa).unwrap(), self.island_at(pb).unwrap())
    }

    /// Whether the built bridges stay connected without the bridge between a and b.
    fn connected_without(&self, a: usize, b: usize) -> bool {
        let n = self.islands.len();
        let mut adj = vec![Vec::new(); n];
        for &(x, y) in self.bridges.keys() {
            if (x, y) != (a.min(b), a.max(b)) {
                adj[x].push(y);
                adj[y].push(x);
            }
        }
        let mut seen = vec![false; n];
        let mut stack = vec![0];
        seen[0] = true;
        while let Some(i) = stack.pop() {
            for &j in &adj[i] {
                if !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        seen.iter().all(|&s| s)
    }

    /// Ways to change the intended solution at the given edges (see [`Repair`]), weighted.
    fn moves_at(
        &self,
        board: &Board,
        solution: &[u8],
        other: Option<&[u8]>,
        edges: &[usize],
        p: &Profile,
    ) -> Vec<(Repair, f64)> {
        let mut moves = Vec::new();
        #[expect(clippy::cast_precision_loss, reason = "at most 30x30 islands")]
        let too_dense =
            self.islands.len() as f64 > 1.15 * p.density * (f64::from(self.w) * f64::from(self.h));
        for &e in edges {
            let s = solution[e];
            let (a, b) = self.grid_edge(board, e);
            if s > 0 {
                let cells = self.between(a, b);
                // Prefer the middle of the bridge
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "bridges are shorter than 30 cells"
                )]
                let mid = cells.len() as f64 / 2.0;
                #[expect(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    reason = "indices and distances within a bridge, shorter than 30 cells"
                )]
                let split = cells
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| self.no_island_around(i32::from(c.x), i32::from(c.y)))
                    .min_by_key(|(i, _)| ((*i as f64 + 0.5 - mid).abs() * 2.0) as i32);
                if let Some((_, &c)) = split {
                    moves.push((Repair::Split(c), if too_dense { 0.5 } else { 3.0 }));
                }
                if self.connected_without(a, b) {
                    moves.push((Repair::Remove(a, b), p.remove));
                }
                let count = 3 - s;
                if self.degree(a) - s + count <= 8 && self.degree(b) - s + count <= 8 {
                    moves.push((Repair::SetCount(a, b, count), 1.0));
                }
            } else {
                let count = other.map_or(1, |o| o[e].max(1));
                let free = self
                    .between(a, b)
                    .iter()
                    .all(|c| self.cell(i32::from(c.x), i32::from(c.y)) == Some(Cell::Water));
                if free && self.degree(a) + count <= 8 && self.degree(b) + count <= 8 {
                    moves.push((Repair::Add(a, b, count), 0.5));
                }
            }
        }
        moves
    }

    fn apply(&mut self, repair: Repair) {
        match repair {
            Repair::Split(c) => {
                self.split(c);
            }
            Repair::Remove(a, b) => self.unbuild(a, b),
            Repair::SetCount(a, b, count) | Repair::Add(a, b, count) => self.build(a, b, count),
        }
    }

    /// Changes the intended solution where it differs from `other`, so that `other` stops
    /// being a solution. Returns false if there is nothing to repair.
    fn repair(
        &mut self,
        rng: &mut StdRng,
        board: &Board,
        solution: &[u8],
        other: &[u8],
        p: &Profile,
    ) -> bool {
        let diff = (0..board.edges.len())
            .filter(|&e| solution[e] != other[e])
            .collect::<Vec<_>>();
        let moves = self.moves_at(board, solution, Some(other), &diff, p);
        match pick_weighted_index(rng, &moves) {
            Some(i) => {
                self.apply(moves.into_iter().nth(i).unwrap().0);
                true
            }
            None => false,
        }
    }

    /// Makes the puzzle easier around an edge the deduction solver couldn't decide.
    fn ease(
        &mut self,
        rng: &mut StdRng,
        board: &Board,
        solution: &[u8],
        undecided: &[usize],
        p: &Profile,
    ) -> bool {
        let Some(&e) = undecided.get(rng.random_range(0..undecided.len().max(1))) else {
            return false;
        };
        // The undecided edge itself and the edges next to it
        let mut edges = vec![e];
        for island in [board.edges[e].a, board.edges[e].b] {
            edges.extend(board.incident[island].iter().filter(|&&f| f != e));
        }
        // Adding bridges rarely makes a puzzle easier
        let moves = self
            .moves_at(board, solution, None, &edges, p)
            .into_iter()
            .filter(|(m, _)| !matches!(m, Repair::Add(..)))
            .collect::<Vec<_>>();
        match pick_weighted_index(rng, &moves) {
            Some(i) => {
                self.apply(moves.into_iter().nth(i).unwrap().0);
                true
            }
            None => false,
        }
    }

    /// Makes the puzzle harder: a new bridge between two islands that see each other.
    fn harden(&mut self, rng: &mut StdRng, p: &Profile) -> bool {
        let edges = self
            .free_edges()
            .into_iter()
            .filter(|&(a, b)| self.degree(a) < 8 && self.degree(b) < 8)
            .collect::<Vec<_>>();
        let Some(&(a, b)) = edges.get(rng.random_range(0..edges.len().max(1))) else {
            return false;
        };
        let count = if rng.random_bool(p.double) { 2 } else { 1 };
        let count = count.min(8 - self.degree(a)).min(8 - self.degree(b));
        self.build(a, b, count);
        true
    }
}

/// Like [`pick_weighted`], but returns the index.
fn pick_weighted_index<T>(rng: &mut StdRng, items: &[(T, f64)]) -> Option<usize> {
    let indexed = items
        .iter()
        .enumerate()
        .map(|(i, it)| (i, it.1))
        .collect::<Vec<_>>();
    pick_weighted(rng, &indexed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_puzzle_matches_solution() {
        let mut rng = StdRng::seed_from_u64(1);
        let mut grid = Grid::new(10, 10);
        grid.grow(&mut rng, 20, 0.4, 0.3);
        grid.add_cycles(&mut rng, 0.5, 0.4);
        let (_, board, solution) = grid.puzzle();
        assert!(board.check(&solution).solved);
        for (i, &p) in grid.islands.iter().enumerate() {
            assert!(
                grid.no_island_around(i32::from(p.x), i32::from(p.y)),
                "island {i} has a neighbour"
            );
        }
    }
}
