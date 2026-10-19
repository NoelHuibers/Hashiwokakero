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
            // Default sizes of the higher difficulties are slow in debug builds, see below
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
#[ignore = "default sizes of all difficulties, slow in debug builds; run with --release"]
fn generates_unique_puzzles_default_sizes() {
    for difficulty in Difficulty::ALL {
        for seed in 1..=5 {
            check_generated(params(difficulty, seed));
        }
    }
}

#[test]
#[ignore = "timing, only meaningful with --release"]
fn generation_time() {
    // Budgets of the shared 1-vCPU server, this machine is usually faster
    let budget_ms = |d| match d {
        Difficulty::Easy | Difficulty::Medium => 300.0,
        Difficulty::Hard | Difficulty::Expert => 1_500.0,
        Difficulty::Oni => 4_000.0,
    };
    for difficulty in Difficulty::ALL {
        let mut times = (1..=10)
            .map(|seed| api::generate(&params(difficulty, seed)).generate_ms)
            .collect::<Vec<_>>();
        times.sort_by(f64::total_cmp);
        let median = times[times.len() / 2];
        println!(
            "{difficulty:?}: median {median:.1} ms, max {:.1} ms",
            times[times.len() - 1]
        );
        assert!(median < budget_ms(difficulty), "{difficulty:?} too slow");
    }
}

#[test]
#[ignore = "prints statistics of generated puzzles, run with --release --nocapture"]
fn generator_report() {
    let seeds: u64 = std::env::var("SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);
    let size: Option<u8> = std::env::var("SIZE").ok().and_then(|s| s.parse().ok());
    let quiet = std::env::var("QUIET").is_ok();
    for difficulty in Difficulty::ALL {
        let mut times = vec![];
        let mut scores = vec![];
        let mut analyze_times = vec![];
        let band = backend::generator::score_range(difficulty);
        for seed in 1..=seeds {
            let mut params = params(difficulty, seed);
            if let Some(size) = size {
                params.width = size;
                params.height = size;
            }
            let g = api::generate(&params);
            let board = Board::new(g.puzzle.clone());
            let t0 = std::time::Instant::now();
            let a = deduce::analyze(&board, backend::generator::MAX_TRIALS);
            let analyze_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let mut hist = [0; 9];
            for i in &g.puzzle.islands {
                hist[i.required as usize] += 1;
            }
            let built = g.solution.iter().filter(|&&b| b > 0).count();
            let doubles = g
                .solution
                .iter()
                .map(|&b| usize::from(b == 2))
                .sum::<usize>();
            let cycles = built + 1 - g.puzzle.islands.len();
            let cells = f64::from(g.puzzle.width) * f64::from(g.puzzle.height);
            scores.push(a.score());
            analyze_times.push(analyze_ms);
            if !quiet {
                println!(
                    "{:6} seed {seed:2}: {:7.1} ms unique {} att {:2} | {:3} islands ({:4.1}%) hist {:?} doubles {:2}/{:3} cycles {:2} | basic {:.2} conn {:2} trials {:2} solved {} score {:5.1}",
                    difficulty.id(),
                    g.generate_ms,
                    g.unique,
                    g.attempts,
                    g.puzzle.islands.len(),
                    100.0 * f64::from(u32::try_from(g.puzzle.islands.len()).unwrap()) / cells,
                    &hist[1..],
                    doubles,
                    built,
                    cycles,
                    a.basic_fraction,
                    a.connectivity_steps,
                    a.trial_steps,
                    a.solved,
                    a.score()
                );
            }
            times.push(g.generate_ms);
            if std::env::var("SHOW").is_ok() {
                println!("{}", backend::reconstruct::render(&board, &g.solution));
            }
        }
        times.sort_by(f64::total_cmp);
        analyze_times.sort_by(f64::total_cmp);
        println!(
            "{:6} analysis max {:.1} ms",
            difficulty.id(),
            analyze_times.last().unwrap()
        );
        let in_band = scores
            .iter()
            .filter(|&&s| band.0 <= s && s <= band.1)
            .count();
        scores.sort_by(f64::total_cmp);
        println!(
            "{:6} median {:.1} ms, max {:.1} ms | score min {:.1} median {:.1} max {:.1}, {in_band}/{} in {band:?}",
            difficulty.id(),
            times[times.len() / 2],
            times.last().unwrap(),
            scores[0],
            scores[scores.len() / 2],
            scores.last().unwrap(),
            scores.len(),
        );
    }
}

/// The generator's budget counts work, not time: on a machine busy with other things (tests
/// in parallel, coverage instrumentation) the same parameters give the same puzzle. With the
/// time budget it had, this failed: the busy run stopped earlier and took another candidate.
#[test]
fn busy_machine_gets_the_same_puzzle() {
    use std::sync::atomic::{AtomicBool, Ordering};

    // Takes about half of its old time budget in a debug build
    let params = GenerateParams {
        width: 16,
        height: 16,
        ..params(Difficulty::Medium, 1)
    };
    let calm = api::generate(&params);
    let stop = AtomicBool::new(false);
    // Many more busy threads than cores, the generator gets a small share of a core
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get) * 12;
    let busy = std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    std::hint::spin_loop();
                }
            });
        }
        let busy = api::generate(&params);
        stop.store(true, Ordering::Relaxed);
        busy
    });
    assert_eq!(
        (calm.puzzle, calm.solution),
        (busy.puzzle, busy.solution),
        "another puzzle on a busy machine ({:.0} ms instead of {:.0} ms)",
        busy.generate_ms,
        calm.generate_ms
    );
}
