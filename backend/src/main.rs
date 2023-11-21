extern crate clap;

use backend::modes;
use clap::{command, Arg};

mod dfs;
mod generate_clauses;
mod parse_input;
mod reconstruct;
mod solver;
mod writer;

use modes::{encode_mode, solve_mode, esr_mode};

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
                .value_name("INPUTFILE")
                .required(true),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("OUTPUTFILE"),
        )
        .get_matches();

    let input_file = matches.get_one::<String>("input").unwrap();
    let mode = matches.get_one::<String>("mode").unwrap();
    let output_file: Option<String> = matches.get_one::<String>("output").and_then(|s| Some(s.clone()));

    match mode.as_str() {
        "encode" => encode_mode(input_file.to_string()),
        "solve" => solve_mode(input_file.to_string(), output_file),
        "encodesolvereconstruct" | "esr" => esr_mode(input_file.to_string(), output_file),
        _ => {
            eprint!("Error: Use either 'encode', 'solve' or 'esr' as mode");
        }
    }
}

