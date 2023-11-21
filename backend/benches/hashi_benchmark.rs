use criterion::{black_box, criterion_group, criterion_main, Criterion};

use backend::modes::{encode_mode, solve_mode, esr_mode};

fn encode_benchmark(c: &mut Criterion) {
    let test_files = vec![
        "backend/input/test1.txt",
        "backend/input/test8.txt",
        "backend/input/test20.txt",
        // Add more test files with different/larger grid sizes
    ];

    for input_file in &test_files {
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
    let test_files = vec![
        "backend/input/test1.txt",
        "backend/input/test8.txt",
        "backend/input/test20.txt",
        // Add more test files with different/larger grid sizes
    ];

    for input_file in &test_files {
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

fn esr_benchmark(c: &mut Criterion) {
    let test_files = vec![
        "backend/input/test1.txt",
        "backend/input/test8.txt",
        "backend/input/test20.txt",
        // Add more test files with different/larger grid sizes
    ];

    for input_file in &test_files {
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


fn main() {
    let mut criterion = Criterion::default();
    encode_benchmark(&mut criterion);
    solve_benchmark(&mut criterion);
    esr_benchmark(&mut criterion);
    criterion.final_summary();
}

// Path: backend/benches/hashi_benchmark.rs
