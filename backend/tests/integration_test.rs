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
        let output_file = format!("./output/test{}.txt", i);
        let game_board = match parse_input(&input_file) {
            Ok(game_board) => game_board,
            Err(err) => {
                eprintln!("Error: {}", err);
                continue;
            }
        };

        let (clauses, var_map) = generate(&game_board);
        let out_file = format!("./output/test{}.cnf", i);
        if let Err(e) = generate_dimacs(&clauses, var_map.keys().len(), &out_file) {
            eprint!("{}", e);
            continue;
        }

        let certificate = match solver::solve(&out_file) {
            Ok(certificate) => certificate,
            Err(err) => {
                eprintln!("Error: {}", err);
                continue;
            }
        };

        if let Err(err) = solver::write_solution(certificate, &output_file) {
            eprintln!("Error: {}", err);
            continue;
        }

        let res = reconstruct::reconstruct_puzzle(&output_file, &var_map, &game_board);
        print!("{}", res);
    }
}

