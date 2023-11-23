use criterion::{criterion_group, criterion_main, Criterion};
use backend::{modes::{encode_mode, solve_mode, esr_mode}};


const TEST_FILES: [&str; 3] = [
    "backend/input/test1.txt",
    "backend/input/test8.txt",
    "backend/input/test20.txt",
];

// benches modes
fn encode_benchmark(c: &mut Criterion) {
    for input_file in &TEST_FILES {
        let benchmark_name = format!("encode_{}", input_file);
        c.bench_function(&benchmark_name, |b| {
            b.iter(|| {
                if !std::path::Path::new(input_file).exists() {
                    return; 
                }
                encode_mode(input_file.to_string());
            })
        });
    }
}

fn solve_benchmark(c: &mut Criterion) {
    for input_file in &TEST_FILES {
        let benchmark_name = format!("solve_{}", input_file);
        c.bench_function(&benchmark_name, |b| {
            b.iter(|| {
                if !std::path::Path::new(input_file).exists() {
                    return; 
                }
                let output_file = Some("/output/solution.txt");
                solve_mode(input_file.to_string(), output_file.map(|s| s.to_string()));
            })
        });
    }
}

//total performance of 
fn esr_benchmark(c: &mut Criterion) {
    for input_file in &TEST_FILES {
        let benchmark_name = format!("esr_{}", input_file);
        c.bench_function(&benchmark_name, |b| {
            b.iter(|| {
                if !std::path::Path::new(input_file).exists() {
                    return; 
                }
                let output_file = Some("/output/solution.txt");
                esr_mode(input_file.to_string(), output_file.map(|s| s.to_string()));
            })
        });
    }
}

criterion_group!(
    benches,
    encode_benchmark,
    solve_benchmark,
    esr_benchmark
);

criterion_main!(benches);
