use backend::{api, encoding, sat, solver};
use common::{
    Board, Heuristic, Puzzle, Restarts, SolveOutcome, SolveReport, SolverConfig, SolverKind,
};

fn configs() -> Vec<SolverConfig> {
    vec![
        SolverConfig::splr(),
        SolverConfig {
            kind: SolverKind::Cdcl,
            heuristic: Heuristic::Vsids,
            restarts: Restarts::Luby { unit: 32 },
            ..Default::default()
        },
        SolverConfig {
            kind: SolverKind::Dpll,
            heuristic: Heuristic::Dlis,
            timeout_ms: 5_000,
            ..Default::default()
        },
    ]
}

fn inputs() -> Vec<(String, Puzzle)> {
    let mut inputs = std::fs::read_dir("input")
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect::<Vec<_>>();
    inputs.sort();
    inputs
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            // Some inputs are intentionally malformed
            let puzzle = Puzzle::parse_txt(&text).ok()?;
            Some((path.display().to_string(), puzzle))
        })
        .collect()
}

/// (satisfiable, unique) of a report, `None` for parts that timed out.
fn summary(report: &SolveReport, board: &Board, label: &str) -> Option<(bool, Option<bool>)> {
    match &report.outcome {
        SolveOutcome::Solved { bridges, unique } => {
            assert!(board.check(bridges).solved, "{label}: invalid solution");
            Some((true, *unique))
        }
        SolveOutcome::Unsatisfiable => Some((false, None)),
        SolveOutcome::Timeout => None,
    }
}

#[test]
fn all_solvers_agree_on_inputs() {
    let mut checked = 0;
    for (name, puzzle) in inputs() {
        let board = Board::new(puzzle.clone());
        let mut expected = None;
        for config in configs() {
            let label = format!("{name} with {}", config.describe());
            let report = api::solve_unique(&puzzle, &config).unwrap();
            let stats = &report.stats;
            assert_eq!(stats.variables, 2 * board.edges.len());
            assert!(stats.sat_calls >= 1);
            assert!(stats.config.describe().starts_with(config.kind.label()));
            let Some(result) = summary(&report, &board, &label) else {
                assert_eq!(config.kind, SolverKind::Dpll, "{label} timed out");
                continue;
            };
            match expected {
                None => expected = Some(result),
                Some((sat, unique)) => {
                    assert_eq!(sat, result.0, "{label}: disagrees on satisfiability");
                    // Uniqueness only means something for satisfiable puzzles, and DPLL may
                    // run out of time proving it
                    match (unique, result.1) {
                        _ if !sat => {}
                        (Some(a), Some(b)) => {
                            assert_eq!(a, b, "{label}: disagrees on uniqueness");
                        }
                        _ => assert_eq!(config.kind, SolverKind::Dpll, "{label}: unknown"),
                    }
                }
            }
        }
        checked += 1;
    }
    assert!(checked >= 20);
}

#[test]
fn solve_returns_valid_solution() {
    let puzzle = Puzzle::parse_txt(&std::fs::read_to_string("input/test8.txt").unwrap()).unwrap();
    let report = api::solve(&puzzle, &SolverConfig::default()).unwrap();
    let SolveOutcome::Solved { bridges, unique } = report.outcome else {
        panic!("expected a solution, got {:?}", report.outcome);
    };
    assert_eq!(unique, None);
    assert!(Board::new(puzzle).check(&bridges).solved);
    assert!(report.stats.decisions.is_some());
    assert!(report.stats.restarts.is_some());
}

// Every island needs exactly its bridges, but 8 bridge ends make only 4 bridges, too few to
// connect 6 islands. The static encoding is satisfiable, the connectivity cuts make it UNSAT.
const ONLY_DISCONNECTED: &str = "5 5\n1...1\n.....\n2...2\n.....\n1...1\n";

#[test]
fn only_disconnected_solutions_is_unsatisfiable() {
    let puzzle = Puzzle::parse_txt(ONLY_DISCONNECTED).unwrap();
    let (board, enc) = encoding::encode_puzzle(&puzzle);
    let (static_result, _) =
        sat::solve_clauses(&enc.clauses, enc.num_vars, &SolverConfig::splr(), None).unwrap();
    let sat::SatOutcome::Sat(model) = static_result else {
        panic!("the static encoding should be satisfiable");
    };
    let bridges = encoding::decode(&board, &model);
    assert!(board.check(&bridges).complete);
    assert!(board.check(&bridges).components > 1);

    for config in configs() {
        let report = api::solve(&puzzle, &config).unwrap();
        assert_eq!(
            report.outcome,
            SolveOutcome::Unsatisfiable,
            "{}",
            config.describe()
        );
        assert!(report.stats.sat_calls >= 2);
        assert!(report.stats.connectivity_cuts >= 1);
    }
}

#[test]
fn detects_several_solutions() {
    // top + left + double right, or double top + right + bottom
    let puzzle = Puzzle::parse_txt("3 3\n2.3\n...\n1.2\n").unwrap();
    for config in configs() {
        let report = api::solve_unique(&puzzle, &config).unwrap();
        assert!(
            matches!(
                report.outcome,
                SolveOutcome::Solved {
                    unique: Some(false),
                    ..
                }
            ),
            "{}: {:?}",
            config.describe(),
            report.outcome
        );
        assert!(report.stats.sat_calls >= 2);
    }
    let unique = Puzzle::parse_txt("3 3\n1.2\n...\n..1\n").unwrap();
    let report = api::solve_unique(&unique, &SolverConfig::default()).unwrap();
    assert!(matches!(
        report.outcome,
        SolveOutcome::Solved {
            unique: Some(true),
            ..
        }
    ));
}

#[test]
fn trivial_and_impossible_puzzles() {
    let single = Puzzle::parse_txt("3 3\n...\n.2.\n...\n").unwrap();
    let isolated = Puzzle::parse_txt("3 3\n1.1\n...\n.1.\n").unwrap();
    let too_many = Puzzle::parse_txt("3 3\n5.1\n...\n...\n").unwrap();
    for puzzle in [single, isolated, too_many] {
        for config in configs() {
            let report = api::solve_unique(&puzzle, &config).unwrap();
            assert_eq!(report.outcome, SolveOutcome::Unsatisfiable, "{puzzle:?}");
        }
        assert!(api::to_dimacs(&puzzle).is_ok());
    }
}

#[test]
fn rejects_invalid_puzzles() {
    let ok = Puzzle::parse_txt("3 3\n1.1\n...\n...\n").unwrap();
    assert!(api::validate(&ok).is_ok());
    let mut empty = ok.clone();
    empty.islands.clear();
    let mut small = ok.clone();
    small.width = 2;
    let mut large = ok.clone();
    large.height = 31;
    let mut zero = ok.clone();
    zero.islands[0].required = 0;
    let mut nine = ok.clone();
    nine.islands[0].required = 9;
    let mut outside = ok.clone();
    outside.islands[1].pos.x = 3;
    let mut unsorted = ok.clone();
    unsorted.islands.reverse();
    let mut duplicate = ok;
    duplicate.islands[1] = duplicate.islands[0].clone();
    for puzzle in [
        empty, small, large, zero, nine, outside, unsorted, duplicate,
    ] {
        let config = SolverConfig::default();
        assert!(api::validate(&puzzle).is_err(), "{puzzle:?}");
        assert!(api::solve(&puzzle, &config).is_err());
        assert!(api::solve_unique(&puzzle, &config).is_err());
        assert!(api::to_dimacs(&puzzle).is_err());
    }
}

#[test]
fn dimacs_round_trip() {
    let puzzle = Puzzle::parse_txt(&std::fs::read_to_string("input/test1.txt").unwrap()).unwrap();
    let text = api::to_dimacs(&puzzle).unwrap();
    assert!(text.starts_with("c "));
    assert!(text.contains("c var 1 = bridge (0,0)-(2,0) #1"));
    let (clauses, vars) = solver::parse_dimacs(&text).unwrap();
    let (board, enc) = encoding::encode_puzzle(&puzzle);
    assert_eq!(vars, 2 * board.edges.len());
    assert_eq!(clauses, enc.clauses);
}

#[test]
fn all_solver_options_work() {
    let puzzle = Puzzle::parse_txt(&std::fs::read_to_string("input/test2.txt").unwrap()).unwrap();
    let board = Board::new(puzzle.clone());
    let restarts = [
        Restarts::None,
        Restarts::Fixed { threshold: 5 },
        Restarts::Geometric {
            threshold: 5,
            factor_percent: 150,
        },
        Restarts::Luby { unit: 2 },
    ];
    for kind in SolverKind::ALL {
        for &heuristic in kind
            .heuristics()
            .iter()
            .chain(std::iter::once(&Heuristic::Mom))
        {
            for restarts in restarts {
                for subsumption in [false, true] {
                    let config = SolverConfig {
                        kind,
                        heuristic,
                        restarts,
                        subsumption,
                        timeout_ms: 10_000,
                    };
                    let report = api::solve(&puzzle, &config).unwrap();
                    let SolveOutcome::Solved { bridges, .. } = report.outcome else {
                        panic!("{config:?}: {:?}", report.outcome);
                    };
                    assert!(board.check(&bridges).solved);
                    assert_eq!(report.stats.config, config.normalized());
                }
            }
        }
    }
}

/// Every CDCL mode the web app offers, on every input: the solver used to return models
/// violating the formula after subsumption (a conflict caused directly by a decision was
/// ignored). That showed up in some runs only, until runs became deterministic (see
/// `sat_solver/src/hash.rs`); `tests/props.rs` tries many formulas instead.
#[test]
fn cdcl_modes_agree_with_splr_on_inputs() {
    let restarts = ["none", "fixed", "geometric", "luby"].map(|id| Restarts::from_id(id).unwrap());
    for (label, puzzle) in inputs() {
        if api::validate(&puzzle).is_err() {
            continue;
        }
        let board = Board::new(puzzle.clone());
        let expected = summary(
            &api::solve(&puzzle, &SolverConfig::splr()).unwrap(),
            &board,
            &label,
        );
        for restarts in restarts {
            for subsumption in [false, true] {
                let config = SolverConfig {
                    kind: SolverKind::Cdcl,
                    heuristic: Heuristic::Vsids,
                    restarts,
                    subsumption,
                    timeout_ms: 10_000,
                };
                let report = api::solve(&puzzle, &config)
                    .unwrap_or_else(|e| panic!("{label} {}: {e}", config.describe()));
                let got = summary(&report, &board, &label);
                assert_eq!(got, expected, "{label} {}", config.describe());
            }
        }
    }
}

#[test]
fn api_is_thread_safe() {
    let puzzle = Puzzle::parse_txt(&std::fs::read_to_string("input/test20.txt").unwrap()).unwrap();
    let handles = (0..8)
        .map(|i| {
            let puzzle = puzzle.clone();
            std::thread::spawn(move || {
                let config = configs()[i % 2];
                api::solve_unique(&puzzle, &config).unwrap().outcome
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        assert!(matches!(
            handle.join().unwrap(),
            SolveOutcome::Solved {
                unique: Some(true),
                ..
            }
        ));
    }
}
