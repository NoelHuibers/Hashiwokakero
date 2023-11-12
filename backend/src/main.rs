extern crate clap;

use clap::{command, Arg};

mod generate_clauses;
mod parse_input;
mod solver;
mod writer;
mod reconstruct;

use generate_clauses::generate;
use parse_input::{parse_input, print_infos};

use crate::writer::generate_dimacs;

// To run an example from root: cargo run --package backend -- --mode [encode/solve] --input [FILE PATH]
// Short: cargo run --package backend -- -m [encode/solve] -i [FILE PATH]
fn main() {
    let matches = command!()
        .arg(
            Arg::new("mode")
                .short('m')
                .long("mode")
                .value_name("MODE")
                .required(true),
        )
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .value_name("FILE")
                .required(true),
        )
        .get_matches();

    let input_file = matches.get_one::<String>("input").unwrap();
    let mode = matches.get_one::<String>("mode").unwrap();

    match mode.as_str() {
        "encode" => match parse_input(input_file) {
            Ok(game_board) => {
                print_infos(&game_board);
                let (clauses, var_map) = generate(&game_board);
                print!("Clauses as Vec<Vec<i32>>: {:?}", clauses);
                let out_file = &format!("{}.sat", input_file);
                let dimacs_generated = generate_dimacs(&clauses, var_map.keys().len(), out_file);
                match dimacs_generated {
                    Ok(_) => println!("Successfully generated {}", out_file),
                    Err(e) => eprint!("{}", e),
                }
            }
            Err(err) => eprintln!("Error: {}", err),
        },
        "solve" => {
            solver::parse(&input_file);
        }
        _ => {
            eprint!("Error: Use either 'encode' or 'solve' as mode");
        }
    }
}
