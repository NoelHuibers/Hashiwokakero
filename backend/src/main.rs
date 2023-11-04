extern crate clap;

use clap::{App, Arg};

mod parse_input;
mod printer;

use parse_input::parse_input;
use printer::print_infos;

// To run an example from root: cargo run --package backend -- --input [FILE PATH]
fn main() {
    let matches = App::new("File Reader")
        .arg(
            Arg::with_name("input")
                .short("i")
                .long("input")
                .value_name("FILE")
                .takes_value(true)
                .required(true),
        )
        .get_matches();

    let input_file = matches.value_of("input").unwrap();

    match parse_input(input_file) {
        Ok(game_board) => {
            print_infos(&game_board);
        }
        Err(err) => eprintln!("Error: {}", err),
    }
}
