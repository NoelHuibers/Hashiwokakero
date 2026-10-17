use backend::modes::{encode_mode, esr_mode, generate_mode, solve_mode};
use clap::{Arg, ArgAction, command, value_parser};
use common::{Difficulty, GenerateParams, Heuristic, Restarts, SolverConfig, SolverKind};

// Examples (from the repository root):
//   cargo run -p backend -- -m esr -i backend/input/test1.txt -s cdcl -H vsids -r luby
//   cargo run -p backend -- -m generate -d hard --seed 42
fn main() {
    let matches = command!()
        .arg(
            Arg::new("mode")
                .short('m')
                .long("mode")
                .value_name("MODE")
                .help("encode | solve | esr (encode, solve, reconstruct) | generate")
                .required(true),
        )
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .value_name("INPUTFILE")
                .help("Puzzle (.txt) for encode/esr, DIMACS CNF for solve"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("OUTPUTFILE"),
        )
        .arg(
            Arg::new("solver")
                .short('s')
                .long("solver")
                .value_name("SOLVER")
                .help("SAT solver: splr | cdcl | dpll")
                .default_value("splr"),
        )
        .arg(
            Arg::new("heuristic")
                .short('H')
                .long("heuristic")
                .value_name("HEURISTIC")
                .help("Branching heuristic of the own solvers: vsids | arbitrary (cdcl, dpll), dlis | dlcs | mom | boehm | jeroslaw_wang | custom (dpll)"),
        )
        .arg(
            Arg::new("restarts")
                .short('r')
                .long("restarts")
                .value_name("POLICY")
                .help("Restart policy of the own CDCL: none | fixed | geometric | luby"),
        )
        .arg(
            Arg::new("threshold")
                .long("threshold")
                .value_name("CONFLICTS")
                .value_parser(value_parser!(u32))
                .help("Restart threshold (fixed, geometric) or unit (luby) [default: 100 / 32]"),
        )
        .arg(
            Arg::new("factor")
                .long("factor")
                .value_name("PERCENT")
                .value_parser(value_parser!(u32))
                .help("Growth of the geometric restart threshold in percent [default: 150]"),
        )
        .arg(
            Arg::new("subsumption")
                .long("subsumption")
                .action(ArgAction::SetTrue)
                .help("Eliminate subsumed clauses before solving (own CDCL)"),
        )
        .arg(
            Arg::new("timeout")
                .short('t')
                .long("timeout")
                .value_name("MS")
                .value_parser(value_parser!(u32))
                .default_value("60000")
                .help("Time limit in milliseconds (0: none for solve, default for esr)"),
        )
        .arg(
            Arg::new("grid_x")
                .short('x')
                .long("grid_x")
                .value_name("WIDTH")
                .value_parser(value_parser!(u16))
                .help("Width of a generated puzzle [default: depends on the difficulty]"),
        )
        .arg(
            Arg::new("grid_y")
                .short('y')
                .long("grid_y")
                .value_name("HEIGHT")
                .value_parser(value_parser!(u16))
                .help("Height of a generated puzzle [default: depends on the difficulty]"),
        )
        .arg(
            Arg::new("difficulty")
                .short('d')
                .long("difficulty")
                .value_name("DIFFICULTY")
                .help("easy | medium | hard | expert | oni")
                .default_value("medium"),
        )
        .arg(
            Arg::new("seed")
                .long("seed")
                .value_name("SEED")
                .value_parser(value_parser!(u64))
                .help("Seed of the generator [default: random]"),
        )
        .get_matches();

    let mode = matches.get_one::<String>("mode").unwrap();
    let input_file = matches.get_one::<String>("input").cloned();
    let output_file = matches.get_one::<String>("output").cloned();
    let config = match solver_config(&matches) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(2);
        }
    };

    let needs_input = |f: fn(&str)| match &input_file {
        Some(input) => f(input),
        None => eprintln!("Error: mode '{mode}' needs an input file (-i)"),
    };
    match mode.as_str() {
        "encode" => needs_input(encode_mode),
        "solve" => match input_file {
            Some(input) => solve_mode(&input, output_file, config),
            None => eprintln!("Error: mode 'solve' needs a CNF file (-i)"),
        },
        "encodesolvereconstruct" | "esr" => match input_file {
            Some(input) => esr_mode(&input, output_file, config),
            None => eprintln!("Error: mode 'esr' needs an input file (-i)"),
        },
        "generate" => {
            let difficulty = matches.get_one::<String>("difficulty").unwrap();
            let Some(difficulty) = Difficulty::from_id(&difficulty.to_lowercase()) else {
                eprintln!("Error: unknown difficulty '{difficulty}'");
                std::process::exit(2);
            };
            let (w, h) = difficulty.size();
            // The generator clamps the size to MIN_SIZE..=MAX_SIZE
            let size = |arg: &str, default: u8| {
                matches
                    .get_one::<u16>(arg)
                    .map_or(default, |&s| u8::try_from(s).unwrap_or(u8::MAX))
            };
            let params = GenerateParams {
                width: size("grid_x", w),
                height: size("grid_y", h),
                difficulty,
                seed: matches
                    .get_one::<u64>("seed")
                    .copied()
                    .unwrap_or_else(rand::random),
            };
            generate_mode(params, output_file);
        }
        _ => eprintln!("Error: Use 'encode', 'solve', 'esr' or 'generate' as mode"),
    }
}

fn solver_config(matches: &clap::ArgMatches) -> Result<SolverConfig, String> {
    let kind = matches.get_one::<String>("solver").unwrap();
    let kind = SolverKind::from_id(kind)
        .ok_or_else(|| format!("Unknown solver '{kind}', use splr, cdcl or dpll"))?;
    let mut config = SolverConfig {
        kind,
        timeout_ms: *matches.get_one::<u32>("timeout").unwrap(),
        subsumption: matches.get_flag("subsumption"),
        ..Default::default()
    };
    if let Some(h) = matches.get_one::<String>("heuristic") {
        config.heuristic =
            Heuristic::from_id(h).ok_or_else(|| format!("Unknown heuristic '{h}'"))?;
        if !kind.heuristics().contains(&config.heuristic) {
            return Err(format!(
                "{} doesn't support the heuristic '{h}'",
                kind.label()
            ));
        }
    }
    if let Some(r) = matches.get_one::<String>("restarts") {
        config.restarts = Restarts::from_id(&r.to_lowercase())
            .ok_or_else(|| format!("Unknown restart policy '{r}'"))?;
    }
    let threshold = matches.get_one::<u32>("threshold").copied();
    let factor = matches.get_one::<u32>("factor").copied();
    config.restarts = match config.restarts {
        Restarts::None => Restarts::None,
        Restarts::Fixed { threshold: t } => Restarts::Fixed {
            threshold: threshold.unwrap_or(t),
        },
        Restarts::Geometric {
            threshold: t,
            factor_percent: f,
        } => Restarts::Geometric {
            threshold: threshold.unwrap_or(t),
            factor_percent: factor.unwrap_or(f),
        },
        Restarts::Luby { unit } => Restarts::Luby {
            unit: threshold.unwrap_or(unit),
        },
    };
    Ok(config.normalized())
}
