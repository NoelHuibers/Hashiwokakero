use clap::{command, Arg, ArgAction};

#[derive(Debug)]
pub struct CliArgs {
    pub solver: String,
    pub inputpath: String,
    pub outputpath: Option<String>,
    pub heuristic: Heuristic,
    pub depth: bool,
    pub flamegraph: bool,
    pub subsumed_clauses: bool,
    pub k: usize,
    pub m: usize,
    pub threshold: Option<u32>,
    pub luby: bool,
    pub factor: Option<u32>,
    pub drup: bool,
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
    VMTF,
    BerkMin,
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
            Arg::new("output")
                .help("Specify the output file or directory.")
                .required(false)
                .index(3),
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
        ).arg(
            Arg::new("k")
                .help("Argument for k-bounded learning")
                .long("k")
                .short('k')
                .default_value("10")
        ).arg(
            Arg::new("m")
                .help("Argument for m-size relevance based learning")
                .long("m")
                .short('m')
                .default_value("10")
        )
        .arg(
            Arg::new("subsumed_clauses")
                .help("Eliminates subsumed clauses")
                .long("subsumed_clauses")
                .short('s')
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("flamegraph")
                .help("Specify whether to create a flamegraph")
                .long("flamegraph")
                .short('f')
                .action(ArgAction::SetTrue),
        ).arg(Arg::new("restarts_threshold")
                .help("Specify the conflict threshold for a restart")
                .long("restarts_threshold")
                .short('r')
        ).arg(Arg::new("luby")
                .help("Use luby sequence for restarts")
                .long("luby")
                .short('l')
                .action(ArgAction::SetTrue),
        ).arg(
            Arg::new("restart_factor")
                .help("Specify the factor for restarts in percent")
                .long("restart_factor")
                .short('F'))
            .arg(Arg::new("DRUP")
                .help("Output in DRUP format")
                .long("DRUP")
                .short('D')
                .action(ArgAction::SetTrue))
        .get_matches();

    let solver = match arguments.get_one::<String>("solver") {
        Some(solver) => solver.to_string(),
        None => panic!("Solver is required"),
    };

    let inputpath = match arguments.get_one::<String>("input") {
        Some(path) => path.to_string(),
        None => panic!("Input is required"),
    };

    let outputpath = arguments.get_one::<String>("output").cloned();

    let heuristic = match arguments.get_one::<String>("heuristic") {
        Some(heuristic) => match heuristic.as_str() {
            "arbitrary" => Heuristic::Arbitrary,
            "dlis" => Heuristic::DLIS,
            "dlcs" => Heuristic::DLCS,
            "mom" => Heuristic::MOM,
            "boehm" => Heuristic::Boehm,
            "jeroslaw_wang" => Heuristic::JeroslawWang,
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

    let k = match arguments.get_one::<String>("k") {
        Some(k) => match k.parse::<usize>() {
            Ok(k) => k,
            Err(_) => panic!("K must be a number"),
        },
        None => panic!("K is required"),
    };

    let m = match arguments.get_one::<String>("m") {
        Some(m) => match m.parse::<usize>() {
            Ok(m) => m,
            Err(_) => panic!("M must be a number"),
        },
        None => panic!("M is required"),
    };

    let threshold = match arguments.get_one::<String>("restarts_threshold") {
        Some(threshold) => Some(match threshold.parse::<u32>() {
            Ok(threshold) => threshold,
            Err(_) => panic!("Threshold must be a number"),
        }),
        None => None,
    };

    let luby = arguments.get_flag("luby");

    let factor = match arguments.get_one::<String>("restart_factor") {
        Some(factor) => match factor.parse::<u32>() {
            Ok(factor) => Some(factor),
            Err(_) => panic!("Factor must be a number"),
        },
        None => None,
    };

    let drup = arguments.get_flag("DRUP");

    return CliArgs {
        solver,
        inputpath,
        outputpath,
        heuristic,
        depth: arguments.get_flag("depth"),
        subsumed_clauses: arguments.get_flag("subsumed_clauses"),
        flamegraph: arguments.get_flag("flamegraph"),
        k,
        m,
        threshold,
        luby,
        factor,
        drup,
    };
}
