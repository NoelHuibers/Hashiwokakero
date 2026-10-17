//! splr used to report UNSAT for every puzzle when its clause eliminator was disabled (a bug in
//! splr 0.19). splr has to agree with our own solvers on generated puzzles.

use backend::api;
use common::{Board, Difficulty, GenerateParams, Puzzle, SolveOutcome, SolverConfig, SolverKind};

fn check_agreement(puzzle: &Puzzle, label: &str) -> bool {
    let board = Board::new(puzzle.clone());
    let outcome = |config: &SolverConfig| {
        let report = api::solve_unique(puzzle, config).unwrap();
        match report.outcome {
            SolveOutcome::Solved { bridges, unique } => {
                assert!(board.check(&bridges).solved, "{label}: invalid solution");
                Some(unique.expect("uniqueness timed out"))
            }
            SolveOutcome::Unsatisfiable => None,
            SolveOutcome::Timeout => panic!("{label}: timeout"),
        }
    };
    let splr = outcome(&SolverConfig::splr());
    let cdcl = outcome(&SolverConfig {
        kind: SolverKind::Cdcl,
        ..Default::default()
    });
    assert_eq!(splr, cdcl, "{label}: splr and CDCL disagree (None = UNSAT)");
    splr.is_some()
}

fn generated(difficulty: Difficulty, seed: u64) -> Puzzle {
    let (width, height) = difficulty.size();
    let g = api::generate(&GenerateParams {
        width: width.min(14),
        height: height.min(14),
        difficulty,
        seed,
    });
    assert!(g.unique);
    g.puzzle
}

#[test]
fn splr_agrees_with_cdcl_on_generated_puzzles() {
    for difficulty in Difficulty::ALL {
        for seed in 1..=3 {
            let puzzle = generated(difficulty, seed);
            let label = format!("{difficulty:?} seed {seed}");
            assert!(
                check_agreement(&puzzle, &label),
                "{label}: generated puzzle unsolvable"
            );
        }
    }
}

/// Changes island numbers of generated puzzles, which gives unsatisfiable puzzles and puzzles
/// with several solutions.
#[test]
#[ignore = "slow in debug builds, run with --release"]
fn splr_agrees_with_cdcl_on_perturbed_puzzles() {
    let (mut sat, mut unsat) = (0, 0);
    for difficulty in Difficulty::ALL {
        for seed in 1..=8 {
            let puzzle = generated(difficulty, seed);
            for k in 0..6 {
                let mut p = puzzle.clone();
                let i = (usize::try_from(seed).unwrap() * 7 + k * 13) % p.islands.len();
                let n = p.islands[i].required;
                p.islands[i].required = if k % 2 == 0 {
                    (n % 8) + 1
                } else {
                    n.max(2) - 1
                };
                let label = format!("{difficulty:?} seed {seed} change {k}");
                if check_agreement(&p, &label) {
                    sat += 1;
                } else {
                    unsat += 1;
                }
            }
        }
    }
    println!("{sat} satisfiable, {unsat} unsatisfiable");
}
