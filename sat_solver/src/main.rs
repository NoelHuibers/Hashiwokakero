use clap::{command, Arg};

pub mod dpll;
pub mod parse;
mod heuristsics;

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
            Arg::new("heuristic")
                .short('H')
                .long("heuristic")
                .value_name("HEURISTIC")
                .default_value("arbitrary"),
        )
        .get_matches();

    let input_file = matches.get_one::<String>("input").unwrap();
    let mode = matches.get_one::<String>("mode").unwrap();
    let heuristic = matches.get_one::<String>("heuristic").unwrap();
    let vars = parse::parse(input_file.as_str()).unwrap();
    let cert = match mode.as_str() {
        "dpll" => dpll::solve(vars, heuristic.as_str()),
        "cdcl" => todo!(),
        otherwise => panic!("{} is not a valid mode.", otherwise)
    };
    println!("{:?}", cert);
}
