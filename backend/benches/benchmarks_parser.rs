use criterion::{black_box, criterion_group, criterion_main, Criterion};
use backend::parse_input::{parse_input, GameBoard, build_bridges};
use std::fs;


fn benchmark_parse_input(c: &mut Criterion) {
    let paths = fs::read_dir("input/").unwrap();

    for path in paths {
        let path = path.unwrap().path();
        if path.is_file() && path.extension().unwrap() == "txt" {
            let file_name = path.file_name().unwrap().to_str().unwrap().to_owned();
            match parse_input(black_box(path.to_str().unwrap())) {
                Ok(game_board) => {
                    c.bench_function(&format!("parse_input_{}", file_name), |b| {
                        b.iter(|| {
                            let _ = black_box(game_board.clone());
                        });
                    });
                },
                Err(e) => {
                    println!("Invalid file: {:?} ({})", path, e);
                    continue;
                }
            }
        } else {
            println!("Invalid file: {:?}", path);
        }
    }
}

fn benchmark_build_bridges(c: &mut Criterion) {
    let paths = fs::read_dir("input/").unwrap();

    for path in paths {
        let path = path.unwrap().path();
        if path.is_file() && path.extension().unwrap() == "txt" {
            let file_name = path.file_name().unwrap().to_str().unwrap().to_owned();
            match parse_input(path.to_str().unwrap()) {
                Ok(mut game_board) => {
                    c.bench_function(&format!("build_bridges_{}", file_name), |b| {
                        b.iter(|| {
                            let _ = black_box(build_bridges(&mut game_board));
                        });
                    });
                },
                Err(e) => {
                    println!("Invalid file: {:?} ({})", path, e);
                    continue;
                }
            }
        } else {
            println!("Invalid file: {:?}", path);
        }
    }
}

fn benchmark_total_performance(c: &mut Criterion) {
    let paths = fs::read_dir("input/").unwrap();

    for path in paths {
        let path = path.unwrap().path();
        if path.is_file() && path.extension().unwrap() == "txt" {
            let file_name = path.file_name().unwrap().to_str().unwrap().to_owned();
            match parse_input(path.to_str().unwrap()) {
                Ok(mut game_board) => {
                    c.bench_function(&format!("total_performance_{}", file_name), |b| {
                        b.iter(|| {
                            let _ = black_box(build_bridges(&mut game_board));
                        });
                    });
                },
                Err(e) => {
                    println!("Invalid file: {:?} ({})", path, e);
                    continue;
                }
            }
        } else {
            println!("Invalid file: {:?}", path);
        }
    }
}


criterion_group!(benches, benchmark_parse_input, benchmark_build_bridges, benchmark_total_performance);
criterion_main!(benches);
