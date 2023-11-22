extern crate clap;

use backend::modes;
use clap::{command, Arg};

mod dfs;
mod generate_clauses;
mod generator;
mod parse_input;
mod reconstruct;
mod solver;
mod writer;

use generator::output_to_file;
use modes::{encode_mode, esr_mode, solve_mode};

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
                .value_name("INPUTFILE"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("OUTPUTFILE"),
        )
        .arg(
            Arg::new("grid_x")
                .short('x')
                .long("grid_x")
                .value_name("GRID_X"),
        )
        .arg(
            Arg::new("grid_y")
                .short('y')
                .long("grid_y")
                .value_name("GRID_Y"),
        )
        .get_matches();

    let input_file = matches.get_one::<String>("input").unwrap();
    let mode = matches.get_one::<String>("mode").unwrap();
    let output_file: Option<String> = matches
        .get_one::<String>("output")
        .and_then(|s| Some(s.clone()));
    let x = matches
        .get_one::<String>("grid_x")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let y = matches
        .get_one::<String>("grid_y")
        .unwrap()
        .parse::<usize>()
        .unwrap();

    match mode.as_str() {
        "encode" => encode_mode(input_file.to_string()),
        "solve" => solve_mode(input_file.to_string(), output_file),
        "encodesolvereconstruct" | "esr" => esr_mode(input_file.to_string(), output_file),
        "generate" => match output_file {
            Some(output) => {
                let vec = generator::generator(x, y);
                output_to_file(&vec, &output).unwrap();
            }
            None => eprint!("Invalid"),
        },
        _ => {
            eprint!("Error: Use either 'encode', 'solve' or 'esr' as mode");
        }
    }
}
