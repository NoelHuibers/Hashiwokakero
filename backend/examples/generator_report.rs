//! Print generation and difficulty statistics without treating diagnostics as a test.
//!
//! Run with `cargo run --release -p backend --example generator_report`.
//! Optional environment variables: SEEDS (default 6), SIZE, QUIET and SHOW.

use backend::{api, deduce};
use common::{Board, Difficulty, GenerateParams};

fn params(difficulty: Difficulty, seed: u64) -> GenerateParams {
    let (width, height) = difficulty.size();
    GenerateParams {
        width,
        height,
        difficulty,
        seed,
    }
}

fn main() {
    let seeds: u64 = std::env::var("SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&seeds| seeds > 0)
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
