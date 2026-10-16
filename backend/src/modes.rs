use crate::parse_input::parse_input;
use crate::generate_clauses::generate;
use crate::solver::SolverKind;
use crate::{reconstruct, solver};
use crate::writer::generate_dimacs;

pub fn encode_mode(input_file: String) {
    let game_board = match parse_input(&input_file) {
        Ok(game_board) => game_board,
        Err(err) => {
            eprintln!("Error: {}", err);
            return;
        }
    };

    let (clauses, var_map) = generate(&game_board);
    let out_file = format!("{}.cnf", input_file);
    if let Err(e) = generate_dimacs(&clauses, var_map.keys().len(), &out_file) {
        eprint!("{}", e);
        return;
    }

    println!("Successfully generated {}", out_file);
}

pub fn solve_mode(input_file: String, output_file: Option<String>, solver_kind: SolverKind) {
    let certificate = match solver::solve_with(&input_file, solver_kind) {
        Ok(certificate) => certificate,
        Err(err) => {
            eprintln!("Error: {}", err);
            return;
        }
    };

    if let Some(output_file) = output_file {
        if let Err(err) = solver::write_solution(certificate, &output_file) {
            eprintln!("Error: {}", err);
            return;
        }

        println!("Solution written to {}", output_file);
    } else {
        println!("Solution: {:?}", certificate);
    }
}

pub fn esr_mode(input_file: String, output_file: Option<String>, solver_kind: SolverKind) {
    let game_board = match parse_input(&input_file) {
        Ok(game_board) => game_board,
        Err(err) => {
            eprintln!("Error: {}", err);
            return;
        }
    };

    let (clauses, var_map) = generate(&game_board);
    let out_file = format!("{}.cnf", input_file);
    if let Err(e) = generate_dimacs(&clauses, var_map.keys().len(), &out_file) {
        eprint!("{}", e);
        return;
    }

    let certificate = match solver::solve_with(&out_file, solver_kind) {
        Ok(certificate) => certificate,
        Err(err) => {
            eprintln!("Error: {}", err);
            return;
        }
    };

    if let Some(output_file) = output_file {
        if let Err(err) = solver::write_solution(certificate, &output_file) {
            eprintln!("Error: {}", err);
            return;
        }

        let contents = reconstruct::get_content(&output_file);
        let res = reconstruct::reconstruct_puzzle(contents, &var_map, &game_board);


        print!("{}", res);
    } else {
        println!("Solution: {:?}", certificate);
    }
}