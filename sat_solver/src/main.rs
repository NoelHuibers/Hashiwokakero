use clap::{command, Arg};
use flame;

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

    flame::start("main");
    let input_file = matches.get_one::<String>("input").unwrap();
    let mode = matches.get_one::<String>("mode").unwrap();
    let heuristic = matches.get_one::<String>("heuristic").unwrap();
    let (vars, v_count, c_count) = parse::parse(input_file.as_str()).unwrap();
    let depth = matches.get_one::<String>("depth").unwrap();
    flame::start("main_solve"); 
    let cert = match mode.as_str() {
        "dpll" => dpll::DPLL::new(vars, v_count, c_count, heuristic.to_string(), depth.parse::<bool>().unwrap()).solve(),
        "cdcl" => todo!(),
        otherwise => panic!("{} is not a valid mode.", otherwise)
    };
    flame::end("main_solve");
    println!("{:?}", cert);
    flame::end("main");

    //flame::dump_html(std::fs::File::create("visuals/graphs/flamegraph.html").unwrap()).unwrap();
}
