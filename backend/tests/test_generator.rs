use backend::{api, deduce};
use common::{Board, Difficulty, GenerateParams, MAX_SIZE, MIN_SIZE, SolveOutcome, SolverConfig};

fn params(difficulty: Difficulty, seed: u64) -> GenerateParams {
    let (width, height) = difficulty.size();
    GenerateParams {
        width,
        height,
        difficulty,
        seed,
    }
}

/// The generated puzzle is valid, unique (checked again with the SAT solver), solved by its
/// stored solution and the same for the same parameters.
fn check_generated(params: GenerateParams) {
    let g = api::generate(&params);
    let label = format!("{params:?}");
    assert!(g.unique, "{label}: not unique");
    assert_eq!(g.params.difficulty, params.difficulty);
    assert_eq!(g.params.seed, params.seed);
    assert_eq!(
        (g.puzzle.width, g.puzzle.height),
        (
            params.width.clamp(MIN_SIZE, MAX_SIZE),
            params.height.clamp(MIN_SIZE, MAX_SIZE)
        ),
        "{label}: size"
    );
    assert_eq!(
        (g.params.width, g.params.height),
        (g.puzzle.width, g.puzzle.height)
    );
    api::validate(&g.puzzle).unwrap();
    let board = Board::new(g.puzzle.clone());
    assert_eq!(g.solution.len(), board.edges.len());
    assert!(
        board.check(&g.solution).solved,
        "{label}: stored solution is wrong"
    );
    // No orthogonally adjacent islands
    for island in &g.puzzle.islands {
        let (x, y) = (island.pos.x, island.pos.y);
        for (nx, ny) in [(x + 1, y), (x, y + 1)] {
            assert!(
                g.puzzle.island_at(common::Pos::new(nx, ny)).is_none(),
                "{label}: adjacent islands at ({x},{y})"
            );
        }
    }
    let report = api::solve_unique(&g.puzzle, &SolverConfig::splr()).unwrap();
    assert!(
        matches!(
            report.outcome,
            SolveOutcome::Solved {
                unique: Some(true),
                ..
            }
        ),
        "{label}: SAT solver says {:?}",
        report.outcome
    );
    let again = api::generate(&params);
    assert_eq!(again.puzzle, g.puzzle, "{label}: not deterministic");
    assert_eq!(again.solution, g.solution);
}

#[test]
fn generates_unique_puzzles() {
    for difficulty in Difficulty::ALL {
        for seed in 1..=3 {
            let mut params = params(difficulty, seed);
            // Exercise more seeds on compact boards; the full default sizes are tested below.
            params.width = params.width.min(12);
            params.height = params.height.min(12);
            check_generated(params);
        }
    }
}

#[test]
fn clamps_sizes() {
    for (width, height) in [(0, 0), (1, 200), (255, 3), (4, 3)] {
        check_generated(GenerateParams {
            width,
            height,
            difficulty: Difficulty::Easy,
            seed: 7,
        });
    }
}

#[test]
fn different_seeds_give_different_puzzles() {
    let a = api::generate(&params(Difficulty::Medium, 1));
    let b = api::generate(&params(Difficulty::Medium, 2));
    assert_ne!(a.puzzle, b.puzzle);
}

#[test]
fn puzzles_fill_the_grid() {
    let mut max_number = 0;
    for difficulty in [Difficulty::Medium, Difficulty::Hard] {
        for seed in 1..=3 {
            let g = api::generate(&params(difficulty, seed));
            let cells = f64::from(g.puzzle.width) * f64::from(g.puzzle.height);
            let density = f64::from(u32::try_from(g.puzzle.islands.len()).unwrap()) / cells;
            assert!(
                (0.17..=0.32).contains(&density),
                "{difficulty:?} {seed}: {density}"
            );
            let built = g.solution.iter().filter(|&&b| b > 0).count();
            assert!(g.solution.contains(&2), "no double bridges");
            max_number = g
                .puzzle
                .islands
                .iter()
                .map(|i| i.required)
                .max()
                .unwrap()
                .max(max_number);
            // A spanning tree has islands - 1 bridges, the rest close cycles
            assert!(built + 1 >= g.puzzle.islands.len());
        }
    }
    assert!(max_number >= 5, "no high numbers");
}

#[test]
fn easy_is_easier_than_hard() {
    let score = |d| {
        (1..=3)
            .map(|seed| {
                let g = api::generate(&params(d, seed));
                deduce::analyze(&Board::new(g.puzzle), backend::generator::MAX_TRIALS).score()
            })
            .sum::<f64>()
    };
    let (easy, medium, hard) = (
        score(Difficulty::Easy),
        score(Difficulty::Medium),
        score(Difficulty::Hard),
    );
    assert_eq!(easy, 0.0, "easy puzzles are solved by basic propagation");
    assert!(easy < medium && medium < hard, "{easy} {medium} {hard}");
}

#[test]
fn generates_unique_puzzles_default_sizes() {
    for difficulty in Difficulty::ALL {
        for seed in 1..=5 {
            check_generated(params(difficulty, seed));
        }
    }
}
