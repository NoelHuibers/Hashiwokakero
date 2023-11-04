extern crate clap;

use clap::{App, Arg};
use std::fs::File;
use std::io::{self, Read};

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

    match read_file(input_file) {
        //TODO: Parse the content @Laura
        Ok(contents) => println!("File contents:\n{}", contents),
        Err(err) => eprintln!("Error: {}", err),
    }
}

fn read_file(file_path: &str) -> io::Result<String> {
    let mut file = File::open(file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    Ok(contents)
}
