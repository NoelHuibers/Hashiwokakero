use std::hint::black_box;

use backend::{api, encoding};
use common::{Difficulty, GenerateParams, Puzzle, SolverConfig, SolverKind};
use criterion::{Criterion, criterion_group, criterion_main};

// Benchmarks run in the package directory
const TEST_FILES: [&str; 3] = ["input/test1.txt", "input/test8.txt", "input/test20.txt"];

fn puzzles() -> Vec<(&'static str, Puzzle)> {
    TEST_FILES
        .iter()
        .map(|f| {
            let text = std::fs::read_to_string(f).unwrap_or_else(|e| panic!("{f}: {e}"));
            (*f, Puzzle::parse_txt(&text).unwrap())
        })
        .collect()
}

fn encode_benchmark(c: &mut Criterion) {
    for (name, puzzle) in puzzles() {
        c.bench_function(&format!("encode {name}"), |b| {
            b.iter(|| encoding::encode_puzzle(black_box(&puzzle)))
        });
    }
}

fn solve_benchmark(c: &mut Criterion) {
    for kind in SolverKind::ALL {
        let config = SolverConfig {
            kind,
            ..Default::default()
        };
        for (name, puzzle) in puzzles() {
            c.bench_function(&format!("solve_unique {} {name}", kind.id()), |b| {
                b.iter(|| api::solve_unique(black_box(&puzzle), &config).unwrap())
            });
        }
    }
}

fn generate_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate");
    group.sample_size(10);
    for difficulty in Difficulty::ALL {
        let (width, height) = difficulty.size();
        group.bench_function(difficulty.id(), |b| {
            let mut seed = 0;
            b.iter(|| {
                seed += 1;
                api::generate(&GenerateParams {
                    width,
                    height,
                    difficulty,
                    seed,
                })
            })
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    encode_benchmark,
    solve_benchmark,
    generate_benchmark
);
criterion_main!(benches);
