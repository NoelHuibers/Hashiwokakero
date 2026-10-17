use clap::{Arg, ArgAction, command};

#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one field per command line flag, mirrors the clap arguments"
)]
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
    /// Give up after this many seconds
    pub timeout: Option<f64>,
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
            .arg(Arg::new("timeout")
                .help("Give up after this many seconds (prints UNKNOWN)")
                .long("timeout")
                .short('t'))
        .get_matches();

    let solver = match arguments.get_one::<String>("solver") {
        Some(solver) => solver.clone(),
        None => panic!("Solver is required"),
    };

    let inputpath = match arguments.get_one::<String>("input") {
        Some(path) => path.clone(),
        None => panic!("Input is required"),
    };

    let outputpath = arguments.get_one::<String>("output").cloned();

    let heuristic = if let Some(heuristic) = arguments.get_one::<String>("heuristic") {
        match heuristic.as_str() {
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
        }
    } else {
        println!("No heuristic specified, using arbitrary");
        Heuristic::Arbitrary
    };

    let k = match arguments.get_one::<String>("k") {
        Some(k) => k
            .parse::<usize>()
            .unwrap_or_else(|e| panic!("K must be a number: {e}")),
        None => panic!("K is required"),
    };

    let m = match arguments.get_one::<String>("m") {
        Some(m) => m
            .parse::<usize>()
            .unwrap_or_else(|e| panic!("M must be a number: {e}")),
        None => panic!("M is required"),
    };

    let threshold = arguments
        .get_one::<String>("restarts_threshold")
        .map(|threshold| {
            threshold
                .parse::<u32>()
                .unwrap_or_else(|e| panic!("Threshold must be a number: {e}"))
        });

    let luby = arguments.get_flag("luby");

    let factor = arguments.get_one::<String>("restart_factor").map(|factor| {
        factor
            .parse::<u32>()
            .unwrap_or_else(|e| panic!("Factor must be a number: {e}"))
    });

    let drup = arguments.get_flag("DRUP");

    let timeout = arguments
        .get_one::<String>("timeout")
        .map(|t| match t.parse::<f64>() {
            Ok(t) if t >= 0.0 => t,
            _ => panic!("Timeout must be a non-negative number of seconds"),
        });

    CliArgs {
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
        timeout,
    }
}
