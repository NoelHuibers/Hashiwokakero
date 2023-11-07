extern crate clap;

use clap::{command, Arg};

mod parse_input;
mod generate_clauses;
mod printer;
mod solver;
mod writer;

use parse_input::{parse_input, print_infos};

// To run an example from root: cargo run --package backend -- --input [FILE PATH]
fn main() {
    let matches = command!()
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .value_name("FILE")
                .required(true),
        )
        .get_matches();

    let input_file = matches.get_one::<String>("input").unwrap();

    match parse_input(input_file) {
        Ok(game_board) => {
            print_infos(&game_board);
        }
        Err(err) => eprintln!("Error: {}", err),
    }
}
