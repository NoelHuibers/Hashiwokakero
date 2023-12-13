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
        ).arg(
            Arg::new("depth")
                .short('d')
                .long("depth")
                .value_name("DEPTH")
                .default_value("false"),
        )
        .get_matches();

    let input_file = matches.get_one::<String>("input").unwrap();
    let mode = matches.get_one::<String>("mode").unwrap();
    let heuristic = matches.get_one::<String>("heuristic").unwrap();
    let (vars, v_count, c_count) = parse::parse(input_file.as_str()).unwrap();
    let depth = matches.get_one::<String>("depth").unwrap();
    let cert = match mode.as_str() {
        "dpll" => dpll::solve(vars, v_count, c_count, heuristic.as_str(), depth.parse::<bool>().unwrap()),
        "cdcl" => todo!(),
        otherwise => panic!("{} is not a valid mode.", otherwise)
    };
    println!("{:?}", cert);
}
