use clap::{command, Arg, ArgAction};

#[derive(Debug)]
pub struct CliArgs {
    pub solver: String,
    pub inputpath: String,
    pub heuristic: Heuristic,
    pub depth: bool,
    pub flamegraph: bool,
}

#[derive(Debug)]
pub enum Heuristic {
    Arbitrary,
    DLIS,
    DLCS,
    MOM,
    Boehm,
    JeroslawWang,
    VSIDS,
    Custom,
}

pub fn cli() -> CliArgs {
    let arguments = command!("Sat Solver")
        .version("1.0")
        .about("Solves SAT problems using DPLL or CDCL. Give the solver as first argument, the input file as second argument.")
        .arg(
            Arg::new("solver")
                .help("Specify the solver to use: CDCL or DPLL")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("input")
                .help("Specify the input file or directory.")
                .required(true)
                .index(2),
        )
        .arg(
            Arg::new("heuristic")
                .help("Specify the heuristic to use")
                .long("heuristic")
                .short('H')
                .default_value("arbitrary")
        )
        .arg(
            Arg::new("depth")
                .help("Shows the depth of the search tree")
                .long("depth")
                .short('d')
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("flamegraph")
                .help("Specify whether to create a flamegraph")
                .long("flamegraph")
                .short('f')
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    let solver = match arguments.get_one::<String>("solver") {
        Some(solver) => solver.to_string(),
        None => panic!("Solver is required"),
    };

    let inputpath = match arguments.get_one::<String>("input") {
        Some(path) => path.to_string(),
        None => panic!("Input is required"),
    };

    let heuristic = match arguments.get_one::<String>("heuristic") {
        Some(heuristic) => match heuristic.as_str() {
            "arbitrary" => Heuristic::Arbitrary,
            "dlis" => Heuristic::DLIS,
            "dlcs" => Heuristic::DLCS,
            "mom" => Heuristic::MOM,
            "boehm" => Heuristic::Boehm,
            "jeroslawwang" => Heuristic::JeroslawWang,
            "vsids" => Heuristic::VSIDS,
            "custom" => Heuristic::Custom,
            _ => {
                println!("Heuristic not found using arbitrary");
                Heuristic::Arbitrary
            }
        },
        None => {
            println!("No heuristic specified, using arbitrary");
            Heuristic::Arbitrary
        }
    };

    return CliArgs {
        solver,
        inputpath,
        heuristic,
        depth: arguments.get_flag("depth"),
        flamegraph: arguments.get_flag("flamegraph"),
    };
}
