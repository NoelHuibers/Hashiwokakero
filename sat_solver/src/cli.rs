use clap::{command, Arg, ArgAction};

#[derive(Debug)]
pub struct CliArgs {
    pub solver: String,
    pub inputpath: String,
    pub heuristic: Option<String>,
    pub depth: bool,
    pub flamegraph: bool,
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
                .short('e')
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

    let mut heuristic = None;
    if arguments.contains_id("heuristic") {
        heuristic = match arguments.get_one::<String>("heuristic") {
            Some(heuristic) => Some(heuristic.to_string()),
            None => Some(String::from("arbirtray")),
        };
    }

    return CliArgs {
        solver,
        inputpath,
        heuristic,
        depth: arguments.get_flag("depth"),
        flamegraph: arguments.get_flag("flamegraph"),
    };
}
