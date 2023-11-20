use criterion::{black_box, criterion_group, criterion_main, Criterion};
use backend::parse_input::parse_input;
use backend::generate_clauses::generate;
use backend::{solver, reconstruct};
use backend::writer::generate_dimacs;
use std::fs;

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
                match black_box(parse_input(input_file)) {
                    Ok(game_board) => {
                        let (clauses, var_map) = generate(&game_board);
                        let out_file = &format!("{}.cnf", input_file);
                        let dimacs_generated = generate_dimacs(&clauses, var_map.keys().len(), out_file);
                        match dimacs_generated {
                            Ok(_) => {
                                println!("Successfully generated {}", out_file);
                                // Delete the generated file
                                if let Err(e) = fs::remove_file(out_file) {
                                    eprintln!("Failed to delete file: {}", e);
                                }
                            }
                            Err(e) => eprint!("{}", e),
                        }
                    }
                    Err(err) => eprintln!("Error: {}", err),
                }
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
                    return; // Abbruch, wenn die Datei nicht gefunden wird
                }
                let output_file = Some("/output/solution.txt");
                match black_box(solver::solve(input_file)) {
                    Ok(certificate) => match output_file {
                        Some(output_file) => match solver::write_solution(certificate, output_file) {
                            Ok(_) => {
                                println!("Solution written to {}", output_file);
                            }
                            Err(err) => {
                                eprintln!("Error: {}", err);
                            }
                        },
                        None => {
                            println!("Solution: {:?}", certificate);
                        }
                    },
                    Err(err) => {
                        eprintln!("Error: {}", err);
                    }
                }
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
                    return; // Abbruch, wenn die Datei nicht gefunden wird
                }
                let output_file = Some("/output/solution.txt");
                match parse_input(input_file) {
                    Ok(game_board) => {
                        let (clauses, var_map) = generate(&game_board);
                        let out_file = &format!("{}.cnf", input_file);
                        let dimacs_generated = generate_dimacs(&clauses, var_map.keys().len(), out_file);
                        match dimacs_generated {
                            Ok(_) => match solver::solve(&out_file) {
                                Ok(certificate) => match output_file {
                                    Some(output_file) => {
                                        match solver::write_solution(certificate, output_file) {
                                            Ok(_) => {
                                                reconstruct::reconstruct_puzzle(
                                                    &output_file.to_string(),
                                                    &var_map,
                                                    &game_board,
                                                );
                                            }
                                            Err(err) => {
                                                eprintln!("Error: {}", err);
                                            }
                                        }
                                    }
                                    None => {
                                        println!("Solution: {:?}", certificate);
                                    }
                                },
                                Err(err) => {
                                    eprintln!("Error: {}", err);
                                }
                            },
                            Err(e) => eprint!("{}", e),
                        }
                    }
                    Err(err) => eprintln!("Error: {}", err),
                }
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
