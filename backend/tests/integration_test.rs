use std::env::current_dir;

use backend::{
    generate_clauses::generate, parse_input::parse_input, reconstruct, solver,
    writer::generate_dimacs,
};

/*
#[test]
fn test_integration() {
    // Define input data
    let input = "1 2 3\n4 5 6\n7 8 9\n";

    // Parse input data
    let input = "1 2 3\n4 5 6\n7 8 9\n";
    let matrix = parse_input(input).unwrap();

    // TODO Solve

    // TODO Write

    // Define expected output
    let expected_output = "1 2 3\n4 5 6\n7 8 9\n";

    // Compare actual and expected output
    //assert_eq!(output, expected_output);
}
*/
fn get_current_working_dir() -> std::io::Result<std::path::PathBuf> {
    std::env::current_dir()
}

#[test]
fn test_integration() {
    for i in 1..=24 {
        println!("Test {}", i);
        let input_file = format!("./input/test{}.txt", i);
        let output_file = format!("../solutions/test{}.txt", i);
        match parse_input(&input_file) {
            Ok(game_board) => {
                let (clauses, var_map) = generate(&game_board);
                let out_file = &format!("../solutions/test{}.cnf", i);
                let dimacs_generated = generate_dimacs(&clauses, var_map.keys().len(), out_file);
                match dimacs_generated {
                    Ok(_) => match solver::solve(&out_file) {
                        Ok(certificate) => {
                            match solver::write_solution(certificate, &output_file) {
                                Ok(_) => {
                                    reconstruct::reconstruct_puzzle(
                                        &output_file,
                                        &var_map,
                                        &game_board,
                                    );
                                }
                                Err(err) => {
                                    eprintln!("Error: {}", err);
                                }
                            }
                        }
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
}
