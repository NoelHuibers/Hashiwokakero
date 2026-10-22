//! The command line of `sat`, and the heuristics the solvers know. Reading the command line needs
//! the `cli` feature (clap); the backend uses the solvers without it.

#[cfg(feature = "cli")]
use clap::{Arg, ArgAction, command, value_parser};

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

#[cfg(feature = "cli")]
pub fn cli() -> CliArgs {
    cli_from(std::env::args_os())
}

/// [`cli`] on the given arguments, the program name first.
#[cfg(feature = "cli")]
pub fn cli_from<I, T>(args: I) -> CliArgs
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let arguments = command!("Sat Solver")
        .version("1.0")
        .about("Solves SAT problems using DPLL or CDCL. Give the solver as first argument, the input file as second argument.")
        .arg(
            Arg::new("solver")
                .help("Specify the solver to use: CDCL or DPLL")
                .required(true)
                .value_parser(["dpll", "cdcl"])
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
                .value_parser(value_parser!(usize))
                .default_value("10")
        ).arg(
            Arg::new("m")
                .help("Argument for m-size relevance based learning")
                .long("m")
                .short('m')
                .value_parser(value_parser!(usize))
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
                .value_parser(value_parser!(u32))
        ).arg(Arg::new("luby")
                .help("Use luby sequence for restarts")
                .long("luby")
                .short('l')
                .action(ArgAction::SetTrue),
        ).arg(
            Arg::new("restart_factor")
                .help("Specify the factor for restarts in percent")
                .long("restart_factor")
                .short('F')
                .value_parser(value_parser!(u32)))
            .arg(Arg::new("DRUP")
                .help("Output in DRUP format")
                .long("DRUP")
                .short('D')
                .requires("output")
                .action(ArgAction::SetTrue))
            .arg(Arg::new("timeout")
                .help("Give up after this many seconds (prints UNKNOWN)")
                .long("timeout")
                .short('t')
                .value_parser(seconds))
        .get_matches_from(args);

    // Required or with a default, clap made sure they are there
    let solver = arguments
        .get_one::<String>("solver")
        .cloned()
        .unwrap_or_default();
    let inputpath = arguments
        .get_one::<String>("input")
        .cloned()
        .unwrap_or_default();
    let outputpath = arguments.get_one::<String>("output").cloned();

    let heuristic = match arguments
        .get_one::<String>("heuristic")
        .map_or("arbitrary", String::as_str)
    {
        "arbitrary" => Heuristic::Arbitrary,
        "dlis" => Heuristic::DLIS,
        "dlcs" => Heuristic::DLCS,
        "mom" => Heuristic::MOM,
        "boehm" => Heuristic::Boehm,
        "jeroslaw_wang" => Heuristic::JeroslawWang,
        "vsids" => Heuristic::VSIDS,
        "custom" => Heuristic::Custom,
        _ => {
            // Not on stdout, that's where the result goes
            eprintln!("Heuristic not found using arbitrary");
            Heuristic::Arbitrary
        }
    };

    // The value parsers above checked the numbers
    let k = arguments.get_one::<usize>("k").copied().unwrap_or(10);
    let m = arguments.get_one::<usize>("m").copied().unwrap_or(10);
    let threshold = arguments.get_one::<u32>("restarts_threshold").copied();
    let luby = arguments.get_flag("luby");
    let factor = arguments.get_one::<u32>("restart_factor").copied();
    let drup = arguments.get_flag("DRUP");
    let timeout = arguments.get_one::<f64>("timeout").copied();

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

/// A time limit for `--timeout`.
#[cfg(feature = "cli")]
fn seconds(arg: &str) -> Result<f64, String> {
    match arg.parse::<f64>() {
        Ok(t) if t >= 0.0 => Ok(t),
        _ => Err("must be a non-negative number of seconds".into()),
    }
}

#[cfg(all(test, feature = "cli"))]
mod tests {
    use super::*;

    #[test]
    fn reads_the_command_line() {
        let args = cli_from(["sat", "dpll", "in.cnf"]);
        assert_eq!(
            (args.solver.as_str(), args.inputpath.as_str()),
            ("dpll", "in.cnf")
        );
        assert!(matches!(args.heuristic, Heuristic::Arbitrary));
        assert_eq!((args.k, args.m, args.outputpath), (10, 10, None));
        assert!(!args.depth && !args.luby && !args.drup && !args.subsumed_clauses);
        assert_eq!(
            (args.threshold, args.factor, args.timeout),
            (None, None, None)
        );

        let args = cli_from([
            "sat", "cdcl", "in.cnf", "out.txt", "-H", "vsids", "-d", "-k", "3", "-m", "4", "-s",
            "-r", "50", "-l", "-F", "150", "-D", "-t", "2.5",
        ]);
        assert_eq!(args.outputpath.as_deref(), Some("out.txt"));
        assert!(matches!(args.heuristic, Heuristic::VSIDS));
        assert_eq!((args.k, args.m), (3, 4));
        assert!(args.depth && args.luby && args.drup && args.subsumed_clauses);
        assert_eq!(
            (args.threshold, args.factor, args.timeout),
            (Some(50), Some(150), Some(2.5))
        );
        for (name, heuristic) in [
            ("dlis", Heuristic::DLIS),
            ("dlcs", Heuristic::DLCS),
            ("mom", Heuristic::MOM),
            ("boehm", Heuristic::Boehm),
            ("jeroslaw_wang", Heuristic::JeroslawWang),
            ("custom", Heuristic::Custom),
            ("nonsense", Heuristic::Arbitrary),
        ] {
            let args = cli_from(["sat", "dpll", "x.cnf", "--heuristic", name]);
            assert_eq!(format!("{:?}", args.heuristic), format!("{heuristic:?}"));
        }
    }
}
