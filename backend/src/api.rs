//! The functions the web server calls. All of them are CPU-bound, thread-safe and don't touch
//! the filesystem.
//!
//! Solving works in rounds (lazy connectivity / CEGAR): the static encoding (see
//! [`crate::encoding`]) doesn't force the bridges to connect all islands. Whenever the SAT
//! solver returns a disconnected model, one clause per component ("one of the possible bridges
//! leaving this group of islands is used") is added and the solver runs again, until the model
//! is connected or the formula is unsatisfiable.

use std::time::{Duration, Instant};

use common::{
    Board, GenerateParams, GeneratedPuzzle, MAX_SIZE, MIN_SIZE, Puzzle, SolveOutcome, SolveReport,
    SolveStats, SolverConfig,
};

use crate::encoding::{self, Encoding};
use crate::sat::{self, SatOutcome, SatStats};

/// Used when a config asks for `timeout_ms == 0`.
pub const DEFAULT_TIMEOUT_MS: u32 = 10_000;
/// Upper bound for `timeout_ms`, the server is shared.
pub const MAX_TIMEOUT_MS: u32 = 120_000;

/// Solves the puzzle with the configured solver. `unique` of the solution is `None`.
pub fn solve(puzzle: &Puzzle, config: &SolverConfig) -> Result<SolveReport, String> {
    validate(puzzle)?;
    let config = config.normalized();
    let mut session = Session::new(puzzle, &config, Some(deadline(&config)));
    let outcome = match session.find()? {
        Search::Found(bridges) => SolveOutcome::Solved {
            bridges,
            unique: None,
        },
        Search::Unsat => SolveOutcome::Unsatisfiable,
        Search::Timeout => SolveOutcome::Timeout,
    };
    Ok(SolveReport {
        outcome,
        stats: session.stats(),
    })
}

/// Like [`solve`], but also decides whether the solution is the only one.
///
/// Afterwards it blocks the found solution and solves again (with the connectivity loop):
/// `SolveOutcome::Solved { unique }` is `Some(true)`/`Some(false)`, or `None` if the second part
/// ran out of time.
pub fn solve_unique(puzzle: &Puzzle, config: &SolverConfig) -> Result<SolveReport, String> {
    validate(puzzle)?;
    let config = config.normalized();
    let mut session = Session::new(puzzle, &config, Some(deadline(&config)));
    let outcome = match session.find()? {
        Search::Found(bridges) => {
            session.block(&bridges);
            let unique = match session.find()? {
                Search::Found(_) => Some(false),
                Search::Unsat => Some(true),
                Search::Timeout => None,
            };
            SolveOutcome::Solved { bridges, unique }
        }
        Search::Unsat => SolveOutcome::Unsatisfiable,
        Search::Timeout => SolveOutcome::Timeout,
    };
    Ok(SolveReport {
        outcome,
        stats: session.stats(),
    })
}

/// DIMACS text of the static encoding (without connectivity cuts), with comments naming the
/// variables.
pub fn to_dimacs(puzzle: &Puzzle) -> Result<String, String> {
    validate(puzzle)?;
    let board = Board::new(puzzle.clone());
    let encoding = encoding::encode(&board);
    Ok(encoding::dimacs(&board, &encoding))
}

/// Generates a puzzle with a unique solution, see [`crate::generator`].
pub fn generate(params: &GenerateParams) -> GeneratedPuzzle {
    crate::generator::generate(params)
}

/// Checks that the puzzle can be encoded.
///
/// That is: size within `MIN_SIZE..=MAX_SIZE`, at least one island, numbers within 1..=8,
/// islands inside the grid, sorted in reading order and on distinct cells (as produced by
/// [`Puzzle::new`]).
pub fn validate(puzzle: &Puzzle) -> Result<(), String> {
    for (name, size) in [("width", puzzle.width), ("height", puzzle.height)] {
        if !(MIN_SIZE..=MAX_SIZE).contains(&size) {
            return Err(format!(
                "The {name} must be between {MIN_SIZE} and {MAX_SIZE}, got {size}"
            ));
        }
    }
    if puzzle.islands.is_empty() {
        return Err("The puzzle has no islands".into());
    }
    for island in &puzzle.islands {
        let (x, y) = (island.pos.x, island.pos.y);
        if x >= puzzle.width || y >= puzzle.height {
            return Err(format!("The island at ({x},{y}) is outside of the grid"));
        }
        if !(1..=8).contains(&island.required) {
            return Err(format!(
                "The island at ({x},{y}) has the number {}, allowed are 1 to 8",
                island.required
            ));
        }
    }
    for pair in puzzle.islands.windows(2) {
        let (a, b) = (pair[0].pos, pair[1].pos);
        if (a.y, a.x) >= (b.y, b.x) {
            return Err(if a == b {
                format!("Two islands at ({},{})", a.x, a.y)
            } else {
                "The islands must be sorted row by row".into()
            });
        }
    }
    Ok(())
}

fn deadline(config: &SolverConfig) -> Instant {
    let ms = match config.timeout_ms {
        0 => DEFAULT_TIMEOUT_MS,
        ms => ms.min(MAX_TIMEOUT_MS),
    };
    Instant::now() + Duration::from_millis(u64::from(ms))
}

pub(crate) enum Search {
    Found(Vec<u8>),
    Unsat,
    Timeout,
}

/// A puzzle being solved: the static encoding plus the clauses learned so far.
pub(crate) struct Session {
    pub board: Board,
    num_vars: usize,
    static_clauses: usize,
    clauses: Vec<Vec<i32>>,
    config: SolverConfig,
    deadline: Option<Instant>,
    encode_time: Duration,
    solve_time: Duration,
    sat_stats: SatStats,
    sat_calls: u32,
    cuts: u32,
}

impl Session {
    pub(crate) fn new(puzzle: &Puzzle, config: &SolverConfig, deadline: Option<Instant>) -> Self {
        let start = Instant::now();
        let board = Board::new(puzzle.clone());
        let Encoding { num_vars, clauses } = encoding::encode(&board);
        Self {
            board,
            num_vars,
            static_clauses: clauses.len(),
            clauses,
            config: *config,
            deadline,
            encode_time: start.elapsed(),
            solve_time: Duration::ZERO,
            sat_stats: SatStats::default(),
            sat_calls: 0,
            cuts: 0,
        }
    }

    /// Finds a connected solution that satisfies all clauses added so far.
    pub(crate) fn find(&mut self) -> Result<Search, String> {
        let start = Instant::now();
        let result = self.find_inner();
        self.solve_time += start.elapsed();
        result
    }

    fn find_inner(&mut self) -> Result<Search, String> {
        loop {
            let (outcome, stats) =
                sat::solve_clauses(&self.clauses, self.num_vars, &self.config, self.deadline)?;
            self.sat_calls += 1;
            self.sat_stats.add(stats);
            let model = match outcome {
                SatOutcome::Sat(model) => model,
                SatOutcome::Unsat => return Ok(Search::Unsat),
                SatOutcome::Timeout => return Ok(Search::Timeout),
            };
            let bridges = encoding::decode(&self.board, &model);
            let (count, comp) = encoding::components(&self.board, &bridges);
            if count <= 1 {
                if !self.board.check(&bridges).solved {
                    return Err("The solver returned an invalid solution".into());
                }
                return Ok(Search::Found(bridges));
            }
            // Two components share the same cut, otherwise every cut is different
            let cuts = if count == 2 { 1 } else { count };
            for c in 0..cuts {
                self.clauses
                    .push(encoding::cut_clause(&self.board, &comp, c));
                self.cuts += 1;
            }
        }
    }

    /// Excludes a solution from further searches.
    pub(crate) fn block(&mut self, bridges: &[u8]) {
        self.clauses.push(encoding::blocking_clause(bridges));
    }

    pub(crate) fn stats(&self) -> SolveStats {
        SolveStats {
            config: self.config.describe(),
            variables: self.num_vars,
            clauses: self.static_clauses,
            encode_ms: self.encode_time.as_secs_f64() * 1000.0,
            solve_ms: self.solve_time.as_secs_f64() * 1000.0,
            decisions: self.sat_stats.decisions,
            restarts: self.sat_stats.restarts,
            sat_calls: self.sat_calls,
            connectivity_cuts: self.cuts,
        }
    }
}

/// Searches a solution other than `known` (which must be a solution of `puzzle`).
pub(crate) fn other_solution(
    puzzle: &Puzzle,
    known: &[u8],
    config: &SolverConfig,
    deadline: Option<Instant>,
) -> Result<Search, String> {
    let mut session = Session::new(puzzle, &config.normalized(), deadline);
    session.block(known);
    session.find()
}
