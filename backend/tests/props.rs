//! Property tests on random puzzles and formulas, checked against a brute-force search that
//! shares nothing with the encoding, the SAT solvers or the deduction rules (only the geometry
//! of `common::Board`): the generator, the CNF encoding and the solvers, the DIMACS text and
//! the command line's file formats.

use std::time::Instant;

use backend::sat::{SatOutcome, solve_clauses};
use backend::{api, encoding, generate_clauses, parse_input, solver, writer};
use common::{
    Board, Difficulty, GenerateParams, Heuristic, Island, Pos, Puzzle, Restarts, SolveOutcome,
    SolverConfig, SolverKind,
};
use proptest::prelude::*;
use proptest::test_runner::{RngSeed, contextualize_config};

/// The same cases on every run, so CI never fails at random. `PROPTEST_RNG_SEED` (other cases)
/// and `PROPTEST_CASES` (more of them) search further, a failure prints its smallest case.
fn config(cases: u32) -> ProptestConfig {
    contextualize_config(ProptestConfig {
        cases,
        rng_seed: RngSeed::Fixed(0x4a5e),
        // The fixed seed finds a failure again, no files in the source tree needed
        failure_persistence: None,
        ..ProptestConfig::default()
    })
}

/// Up to `limit` solutions, by backtracking over the edges in order.
fn solutions(board: &Board, limit: usize) -> Vec<Vec<u8>> {
    struct Search<'a> {
        board: &'a Board,
        required: Vec<u8>,
        /// The islands whose last edge each edge is: they must be complete once it is set.
        closes: Vec<Vec<usize>>,
        bridges: Vec<u8>,
        degrees: Vec<u8>,
        found: Vec<Vec<u8>>,
        limit: usize,
    }

    impl Search<'_> {
        fn run(&mut self, e: usize) {
            if self.found.len() >= self.limit {
                return;
            }
            if e == self.bridges.len() {
                if self.board.check(&self.bridges).solved {
                    self.found.push(self.bridges.clone());
                }
                return;
            }
            let edge = self.board.edges[e];
            let crosses = self.board.crossings[e]
                .iter()
                .any(|&f| f < e && self.bridges[f] > 0);
            for count in 0..=2 {
                if count > 0 && crosses {
                    break;
                }
                if self.degrees[edge.a] + count > self.required[edge.a]
                    || self.degrees[edge.b] + count > self.required[edge.b]
                {
                    break;
                }
                self.bridges[e] = count;
                self.degrees[edge.a] += count;
                self.degrees[edge.b] += count;
                if self.closes[e]
                    .iter()
                    .all(|&i| self.degrees[i] == self.required[i])
                {
                    self.run(e + 1);
                }
                self.degrees[edge.a] -= count;
                self.degrees[edge.b] -= count;
            }
            self.bridges[e] = 0;
        }
    }

    // An island without any possible bridge can't get its number
    if board.incident.iter().any(Vec::is_empty) {
        return vec![];
    }
    let mut closes = vec![Vec::new(); board.edges.len()];
    for (i, incident) in board.incident.iter().enumerate() {
        closes[*incident.iter().max().expect("not empty")].push(i);
    }
    let mut search = Search {
        board,
        required: board.puzzle.islands.iter().map(|i| i.required).collect(),
        closes,
        bridges: board.empty_bridges(),
        degrees: vec![0; board.puzzle.islands.len()],
        found: vec![],
        limit,
    };
    search.run(0);
    search.found
}

/// The model of a bridge configuration in the encoding's variables (index 0 unused).
fn model_of(bridges: &[u8]) -> Vec<bool> {
    let mut model = vec![false; 2 * bridges.len() + 1];
    for (e, &count) in bridges.iter().enumerate() {
        model[2 * e + 1] = count >= 1;
        model[2 * e + 2] = count == 2;
    }
    model
}

fn satisfies(model: &[bool], clause: &[i32]) -> bool {
    clause
        .iter()
        .any(|&l| model[l.unsigned_abs() as usize] == (l > 0))
}

/// Puzzles of 3 to 6 cells each way with at most 12 islands. Half of them get the numbers of
/// random non-crossing bridges (solvable when those happen to connect all islands, often in
/// other ways too), the other half random numbers (mostly unsolvable).
fn puzzle() -> impl Strategy<Value = Puzzle> {
    (3..=6usize, 3..=6usize)
        .prop_flat_map(|(w, h)| {
            let cells = w * h;
            (
                Just((w, h)),
                prop::collection::vec(prop::bool::weighted(0.35), cells),
                prop::collection::vec(prop_oneof![1 => Just(0u8), 2 => 1..=2u8], 2 * cells),
                prop::collection::vec(1..=8u8, cells),
                any::<bool>(),
            )
        })
        .prop_map(|((w, h), cells, counts, numbers, from_bridges)| {
            let size = |n: usize| u8::try_from(n).expect("small grid");
            let islands: Vec<Island> = (0..w * h)
                .filter(|&c| cells[c])
                .take(12)
                .map(|c| Island {
                    pos: Pos::new(size(c % w), size(c / w)),
                    required: numbers[c],
                })
                .collect();
            let puzzle = Puzzle::new(size(w), size(h), islands);
            if !from_bridges {
                return puzzle;
            }
            let board = Board::new(puzzle);
            let mut bridges = board.empty_bridges();
            for e in 0..bridges.len() {
                if !board.is_blocked(&bridges, e) {
                    bridges[e] = counts[e];
                }
            }
            let degrees = board.check(&bridges).degrees;
            // Islands without bridges go, the bridges stay valid: they only cross water
            let islands = board
                .puzzle
                .islands
                .iter()
                .zip(degrees)
                .filter(|(_, d)| *d > 0)
                .map(|(i, d)| Island {
                    pos: i.pos,
                    required: d,
                })
                .collect();
            Puzzle::new(board.puzzle.width, board.puzzle.height, islands)
        })
        .prop_filter("the solver needs an island", |p| !p.islands.is_empty())
}

/// A solver with options it supports.
fn solver_config() -> impl Strategy<Value = SolverConfig> {
    (
        prop::sample::select(SolverKind::ALL.to_vec()),
        prop::sample::select(Heuristic::ALL.to_vec()),
        prop::sample::select(vec!["none", "fixed", "geometric", "luby"]),
        any::<bool>(),
    )
        .prop_map(|(kind, heuristic, restarts, subsumption)| {
            SolverConfig {
                kind,
                heuristic,
                restarts: Restarts::from_id(restarts).expect("known id"),
                subsumption,
                // Far beyond what these take, a slow CI machine must not turn them into timeouts
                timeout_ms: api::MAX_TIMEOUT_MS,
            }
            .normalized()
        })
}

/// Random formulas over up to 8 variables, clauses of 1 to 4 literals (some repeat a variable).
fn cnf() -> impl Strategy<Value = (usize, Vec<Vec<i32>>)> {
    (1..=8i32).prop_flat_map(|vars| {
        let lit = (1..=vars, any::<bool>()).prop_map(|(v, neg)| if neg { -v } else { v });
        let clause = prop::collection::vec(lit, 1..=4);
        (
            Just(vars.unsigned_abs() as usize),
            // Small formulas often, they reach the corners (one clause, every variable negated)
            prop_oneof![
                prop::collection::vec(clause.clone(), 0..=4),
                prop::collection::vec(clause, 0..=30)
            ],
        )
    })
}

/// Whether some assignment satisfies all clauses, by trying them all.
fn brute_force_sat(vars: usize, clauses: &[Vec<i32>]) -> bool {
    (0..1u32 << vars).any(|bits| {
        let model: Vec<bool> = (0..=vars)
            .map(|v| v > 0 && bits >> (v - 1) & 1 == 1)
            .collect();
        clauses.iter().all(|c| satisfies(&model, c))
    })
}

fn temp_file(content: &str, name: &str) -> tempfile::NamedTempFile {
    let file = tempfile::Builder::new()
        .suffix(name)
        .tempfile()
        .expect("temp file");
    std::fs::write(file.path(), content).expect("write temp file");
    file
}

proptest! {
    #![proptest_config(config(128))]

    /// The static encoding only rules out what the rules rule out: every solution is a model.
    /// (Too many clauses would make solvable puzzles unsolvable, or unique ones "unique" for
    /// the wrong reason.)
    #[test]
    fn every_solution_satisfies_the_encoding(p in puzzle()) {
        let (board, enc) = encoding::encode_puzzle(&p);
        for bridges in solutions(&board, 20) {
            let model = model_of(&bridges);
            prop_assert_eq!(model.len(), enc.num_vars + 1);
            for clause in &enc.clauses {
                prop_assert!(satisfies(&model, clause), "{:?} excludes {:?}", clause, bridges);
            }
            // Models decode to their bridges, and blocking one solution keeps every other
            prop_assert_eq!(encoding::decode(&board, &model), bridges.clone());
            let block = encoding::blocking_clause(&bridges);
            prop_assert!(!satisfies(&model, &block));
            for other in solutions(&board, 5).iter().filter(|o| **o != bridges) {
                prop_assert!(satisfies(&model_of(other), &block));
            }
        }
    }

    /// Every solver finds a solution exactly when there is one, the solution is valid, and
    /// "unique" is right. Unsolvable puzzles stay unsolvable whatever the solver.
    #[test]
    fn solvers_agree_with_brute_force(p in puzzle(), config in solver_config()) {
        let board = Board::new(p.clone());
        let truth = solutions(&board, 2);
        let report = api::solve_unique(&p, &config).map_err(TestCaseError::fail)?;
        match (&report.outcome, truth.len()) {
            (SolveOutcome::Unsatisfiable, 0) => {}
            (SolveOutcome::Solved { bridges, unique }, n) if n > 0 => {
                prop_assert!(board.check(bridges).solved);
                prop_assert!(truth.contains(bridges) || n > 1);
                prop_assert_eq!(*unique, Some(n == 1));
            }
            (outcome, n) => prop_assert!(false, "{} solutions, solver says {:?}", n, outcome),
        }
        prop_assert_eq!(report.stats.config, config);
        let single = api::solve(&p, &config).map_err(TestCaseError::fail)?;
        match single.outcome {
            SolveOutcome::Solved { bridges, unique: None } => {
                prop_assert!(board.check(&bridges).solved);
            }
            SolveOutcome::Unsatisfiable => prop_assert!(truth.is_empty()),
            other => prop_assert!(false, "{:?}", other),
        }
    }

    /// The DIMACS download is the encoding: it reads back to the same clauses.
    #[test]
    fn dimacs_round_trips(p in puzzle()) {
        let (_, enc) = encoding::encode_puzzle(&p);
        let text = api::to_dimacs(&p).map_err(TestCaseError::fail)?;
        prop_assert_eq!(solver::parse_dimacs(&text), Ok((enc.clauses.clone(), enc.num_vars)));
        // The command line's writer too
        let file = tempfile::NamedTempFile::new().expect("temp file");
        let path = file.path().to_str().expect("utf-8 path");
        writer::generate_dimacs(&enc.clauses, enc.num_vars, path).expect("write");
        let written = std::fs::read_to_string(path).expect("read");
        prop_assert_eq!(solver::parse_dimacs(&written), Ok((enc.clauses, enc.num_vars)));
    }

    /// The command line reads exported puzzles as the website does, with the same possible
    /// bridges.
    #[test]
    fn command_line_reads_exported_puzzles(p in puzzle()) {
        let file = temp_file(&p.to_txt(), ".txt");
        let game = parse_input::parse_input(file.path().to_str().expect("utf-8 path"))
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        let board = Board::new(p.clone());
        prop_assert_eq!(generate_clauses::to_puzzle(&game), p);
        let mut expected: Vec<((u8, u8), (u8, u8))> = (0..board.edges.len())
            .map(|e| {
                let (a, b) = board.endpoints(e);
                ((a.x, a.y), (b.x, b.y))
            })
            .collect();
        let mut found: Vec<((u8, u8), (u8, u8))> =
            game.bridges.iter().map(|b| (b.from, b.to)).collect();
        expected.sort_unstable();
        found.sort_unstable();
        prop_assert_eq!(found, expected);
        let (clauses, vars) = generate_clauses::generate(&game);
        prop_assert_eq!(clauses, encoding::encode(&board).clauses);
        prop_assert_eq!(vars.len(), 2 * board.edges.len());
    }

    /// Whatever is in the file, the command line answers with a board or an error. A board it
    /// accepts has its islands inside the grid.
    #[test]
    fn command_line_reads_any_file(
        text in prop_oneof![
            "([0-9]{1,2}( [0-9]{1,2})?)?\n([.1-9 ]{0,6}\n){0,7}",
            "[0-9 ]{0,6}\n(\n|[.1-8]{3}\n){0,8}",
            any::<String>(),
        ],
    ) {
        let file = temp_file(&text, ".txt");
        if let Ok(game) = parse_input::parse_input(file.path().to_str().expect("utf-8 path")) {
            prop_assert!(game.islands.iter().all(|i| i.x < game.cols && i.y < game.rows));
            prop_assert!(game.islands.iter().all(|i| (1..=8).contains(&i.connections)));
        }
    }

    /// The solvers and splr behind `solve_clauses` agree with trying every assignment, and
    /// their models satisfy the formula.
    #[test]
    fn sat_solvers_agree_on_random_formulas((vars, clauses) in cnf(), config in solver_config()) {
        let deadline = Instant::now() + std::time::Duration::from_secs(60);
        let (outcome, _) = solve_clauses(&clauses, vars, &config, Some(deadline))
            .map_err(TestCaseError::fail)?;
        match outcome {
            SatOutcome::Sat(model) => {
                prop_assert!(clauses.iter().all(|c| satisfies(&model, c)));
            }
            SatOutcome::Unsat => prop_assert!(!brute_force_sat(vars, &clauses)),
            SatOutcome::Timeout => prop_assert!(false, "timeout"),
        }
    }
}

proptest! {
    #![proptest_config(config(24))]

    /// Generated puzzles of any small size, difficulty and seed are valid and have exactly one
    /// solution, the one the generator returns.
    #[test]
    fn generated_puzzles_have_one_solution(
        width in 3..=7u8,
        height in 3..=7u8,
        difficulty in prop::sample::select(Difficulty::ALL.to_vec()),
        seed in any::<u64>(),
    ) {
        let params = GenerateParams { width, height, difficulty, seed };
        let generated = api::generate(&params);
        prop_assert_eq!(generated.params, params);
        api::validate(&generated.puzzle).map_err(TestCaseError::fail)?;
        prop_assert!(generated.unique);
        let board = Board::new(generated.puzzle);
        prop_assert!(board.check(&generated.solution).solved);
        prop_assert_eq!(solutions(&board, 2), vec![generated.solution]);
    }
}
