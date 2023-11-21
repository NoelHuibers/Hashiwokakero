use crate::parse_input::parse_input;
use crate::generate_clauses::generate;
use crate::{reconstruct, solver};
use crate::writer::generate_dimacs;

pub fn encode_mode(input_file: String) {
    match parse_input(&&input_file) {
        Ok(game_board) => {
            let (clauses, var_map) = generate(&game_board);
            let out_file = &format!("{}.cnf", input_file);
            let dimacs_generated = generate_dimacs(&clauses, var_map.keys().len(), out_file);
            match dimacs_generated {
                Ok(_) => println!("Successfully generated {}", out_file),
                Err(e) => eprint!("{}", e),
            }
        }
        Err(err) => eprintln!("Error: {}", err),
    }
}

pub fn solve_mode(input_file: String, output_file: Option<String>) {
    match solver::solve(&input_file) {
        Ok(certificate) => match output_file {
            Some(output_file) => match solver::write_solution(certificate, &output_file) {
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
}

pub fn esr_mode(input_file: String, output_file: Option<String>) {
    match parse_input(&input_file) {
        Ok(game_board) => {
            let (clauses, var_map) = generate(&game_board);
            let out_file = &format!("{}.cnf", input_file);
            let dimacs_generated = generate_dimacs(&clauses, var_map.keys().len(), out_file);
            match dimacs_generated {
                Ok(_) => match solver::solve(&out_file) {
                    Ok(certificate) => match output_file {
                        Some(output_file) => {
                            match solver::write_solution(certificate, &output_file) {
                                Ok(_) => {
                                    let res = reconstruct::reconstruct_puzzle(
                                        &output_file,
                                        &var_map,
                                        &game_board,
                                    );
                                    print!("{}", res);
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
}