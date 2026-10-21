//! The SAT Dojo command line.
//!
//! ```text
//! dojo run <solver.wasm> [--seed S [--hidden] | --set SET.tar.gz] [--budget FUEL] [--threads N]
//!          [--family F] [--stderr] [--checker verified|builtin] [--proofs DIR]
//!     Scores a solver on the development set, exactly like the leaderboard does: the runs go
//!     on to ten times the budget (--budget, the leaderboard's by default), the score counts
//!     what happened within it and the end shows it at other budgets too. --seed takes
//!     another seed, with --hidden for a hidden set like a season's (a past season's seed is
//!     revealed on hashi.rs/dojo/seasons), --set an archived set as downloaded from there. UNSAT proofs are checked with drat-trim and
//!     cake_lpr when they are installed (DOJO_DRAT_TRIM and DOJO_CAKE_LPR, or the PATH), like
//!     on the leaderboard, with the built-in checker otherwise; --checker insists on one.
//!     --proofs keeps the proof files there (a directory per run) instead of a temp directory.
//! dojo export <dir | file.tar.gz> [--seed S] [--hidden] [--season N]
//!     Writes the set as DIMACS files, with the expected answers in answers.txt and models in
//!     witnesses.txt; into a directory, or a tarball like the season archive's. A season's set
//!     (--season) is a hidden one.
//! dojo serve
//!     The worker service for hashi.rs (see dojo::worker for its settings).
//! dojo calibrate fuel --solver NAME=WASM... [--set SET.tar.gz... | --dev] [--cap FUEL]
//!                     [--threads N] [--checker verified|builtin] [--out fuel.csv]
//! dojo calibrate time --solver NAME=WASM:NATIVE... [--set ... | --dev] [--fuel fuel.csv]
//!                     [--runs K] [--timeout SECS] [--cap FUEL] [--scratch DIR] [--out time.csv]
//! dojo calibrate analyze fuel.csv time.csv [--budget FUEL] [--min-ms MS]
//!     The calibration study of fuel against native time (dojo/CALIBRATION.md,
//!     dojo::calibrate). `time` runs one thing at a time: run it pinned to a core
//!     (taskset -c N) on a quiet machine. With --fuel it times only what `fuel` measured, the
//!     runs that didn't finish within the cap once.
//! ```

use std::process::ExitCode;
use std::time::Instant;

use dojo::archive::{self, SetInfo};
use dojo::bench::{self, Instance};
#[cfg(feature = "runner")]
use dojo::eval::{Options, evaluate};
#[cfg(feature = "runner")]
use dojo::proof::{CheckLimits, Checker, Tools};
#[cfg(feature = "runner")]
use dojo::sandbox::Sandbox;
use dojo::{DEV_SEED, Family};
#[cfg(feature = "runner")]
use dojo::{Outcome, RunResult, Score};

/// Every value of a flag that can be given more than once.
#[cfg(feature = "runner")]
fn flags(args: &[String], name: &str) -> Vec<String> {
    args.windows(2)
        .filter(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .collect()
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Whether the arguments ask for a hidden set rather than a dev set.
fn hidden(args: &[String]) -> bool {
    args.iter().any(|a| a == "--hidden" || a == "--season")
}

/// The set the arguments ask for (`--seed`, `--hidden`, `--set`, `--family`), with its seed.
fn set(args: &[String]) -> Result<(String, Vec<Instance>), String> {
    // Before the set is generated, which takes a while
    let family = flag(args, "--family")
        .map(|id| {
            Family::from_id(&id).ok_or_else(|| {
                let ids: Vec<_> = Family::ALL.iter().map(|f| f.id()).collect();
                format!("unknown family {id}, one of {}", ids.join(", "))
            })
        })
        .transpose()?;
    let started = Instant::now();
    let (seed, mut set) = if let Some(path) = flag(args, "--set") {
        let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
        let (info, set) = archive::read_tar_gz(&bytes).map_err(|e| format!("{path}: {e}"))?;
        (info.seed, set)
    } else {
        let seed = flag(args, "--seed").unwrap_or_else(|| DEV_SEED.to_string());
        let set = if hidden(args) {
            bench::generate_hidden(&seed)
        } else {
            bench::generate(&seed)
        };
        (seed, set)
    };
    if let Some(family) = family {
        set.retain(|i| i.family == family);
    }
    eprintln!(
        "set {seed}: {} formulas, ready in {:.1?}",
        set.len(),
        started.elapsed()
    );
    Ok((seed, set))
}

#[cfg(feature = "runner")]
fn run(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or("usage: dojo run <solver.wasm>")?;
    let wasm = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let sandbox = Sandbox::new()?;
    let solver = sandbox
        .load(&wasm)
        .map_err(|e| format!("rejected: {e:?}"))?;
    let mut options = Options {
        stderr: args.iter().any(|a| a == "--stderr"),
        ..Default::default()
    };
    // The ranking budget; the runs go on to BUDGET_FACTOR times it, like on the leaderboard
    let budget: u64 = match flag(args, "--budget") {
        Some(budget) => budget
            .replace('_', "")
            .parse()
            .map_err(|_| "--budget takes a number")?,
        None => dojo::DEFAULT_BUDGET,
    };
    options.limits.fuel = budget.saturating_mul(dojo::BUDGET_FACTOR);
    if let Some(threads) = flag(args, "--threads") {
        options.threads = threads.parse().map_err(|_| "--threads takes a number")?;
    }
    let tools = match flag(args, "--checker").as_deref() {
        None => Tools::find_or_builtin(),
        Some("builtin") => Tools::Builtin,
        Some("verified") => Tools::find().ok_or(
            "drat-trim and cake_lpr weren't found: put them on the PATH or point \
             DOJO_DRAT_TRIM and DOJO_CAKE_LPR at them (see the starter's README)",
        )?,
        Some(_) => return Err("--checker takes verified or builtin".into()),
    };
    eprintln!("proofs are checked with {}", tools.describe());
    options.checker = Checker::new(tools, CheckLimits::default(), options.threads);
    let kept = flag(args, "--proofs").map(std::path::PathBuf::from);
    if let Some(dir) = &kept {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        options.proof_root.clone_from(dir);
        options.keep_proofs = true;
    }
    let (seed, set) = set(args)?;
    options.set_seed = seed;
    let started = Instant::now();
    let results = evaluate(&sandbox, &solver, &set, &options, |done, r| {
        #[expect(
            clippy::cast_precision_loss,
            reason = "fuel is at most the budget, far below 2^53, and only printed"
        )]
        let gfuel = r.fuel as f64 / 1e9;
        let id = |outcome| match outcome {
            Outcome::Wrong => "WRONG",
            Outcome::Unverified => "no proof",
            other => other.id(),
        };
        // What counts is the outcome within the budget; how the run ended past it, too
        let at_budget = r.outcome_at(budget);
        let later = if at_budget == r.outcome {
            String::new()
        } else {
            format!("  (past the budget: {})", id(r.outcome))
        };
        eprintln!(
            "[{done:>2}/{}] {:<16} {:<5} {:<9} {:>8.3} Gfuel {:>7} ms{later}",
            set.len(),
            r.name,
            r.expected.id(),
            id(at_budget),
            gfuel,
            r.wall_ms,
        );
    });
    let score = Score::from_results(&results, budget);
    println!();
    for f in &score.families {
        let unverified = if f.unverified > 0 {
            format!("  {} UNSAT without an accepted proof", f.unverified)
        } else {
            String::new()
        };
        println!(
            "{:<14} {:>2}/{:<2} solved  PAR-2 {:>7.3}{unverified}",
            f.family.id(),
            f.solved,
            f.total,
            f.par2
        );
    }
    println!(
        "\nsolved {}/{}  PAR-2 {:.3} Gfuel  ({:.1?})",
        score.solved,
        score.total,
        score.par2,
        started.elapsed()
    );
    // Split by answer, as the SAT Competition reports it
    let published: Vec<_> = results.iter().map(RunResult::published).collect();
    let split = dojo::metrics::breakdown(&published, budget, true).total;
    for (label, part) in [("SAT", split.sat), ("UNSAT", split.unsat)] {
        if let Some(part) = part.filter(|p| p.total > 0) {
            println!(
                "{label:<6} {}/{}  PAR-2 {:.3}",
                part.solved, part.total, part.par2
            );
        }
    }
    // The leaderboard's budget view: the same runs with a tenth of the budget and all of them
    println!(
        "\nwith other budgets (the runs went on to {} times the budget):",
        dojo::BUDGET_FACTOR
    );
    for (label, at) in [
        ("1/10", budget / 10),
        ("1x", budget),
        ("max", options.limits.fuel),
    ] {
        let score = dojo::metrics::score_at(&published, at, true);
        #[expect(
            clippy::cast_precision_loss,
            reason = "budgets are far below 2^53, only printed"
        )]
        let gfuel = at as f64 / 1e9;
        println!(
            "{label:<5} {gfuel:>8.3} Gfuel  solved {}/{}  PAR-2 {:.3}",
            score.solved, score.total, score.par2
        );
    }
    if score.unverified > 0 {
        println!(
            "{} UNSAT answers without an accepted proof count as unsolved: write a DRAT or LRAT \
             proof to the file named by the first argument",
            score.unverified
        );
    }
    if score.wrong > 0 {
        println!(
            "{} WRONG answers (a wrong model, UNSAT on a satisfiable formula or a rejected \
             proof): the leaderboard would disqualify this solver",
            score.wrong
        );
    }
    Ok(())
}

/// The formulas of the study: `--set` archives and `--dev`, named `<set>/<formula>`.
#[cfg(feature = "runner")]
fn calibration_set(args: &[String]) -> Result<Vec<Instance>, String> {
    let mut set = Vec::new();
    let mut add = |label: &str, instances: Vec<Instance>| {
        set.extend(instances.into_iter().map(|mut i| {
            i.name = format!("{label}/{}", i.name);
            i
        }));
    };
    if args.iter().any(|a| a == "--dev") {
        add("dev", bench::generate(DEV_SEED));
    }
    for path in flags(args, "--set") {
        let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
        let (_, instances) = archive::read_tar_gz(&bytes).map_err(|e| format!("{path}: {e}"))?;
        let label = std::path::Path::new(&path)
            .file_name()
            .map(|f| f.to_string_lossy().trim_end_matches(".tar.gz").to_string())
            .unwrap_or_default();
        add(&label, instances);
    }
    if set.is_empty() {
        return Err("no formulas: give --set SET.tar.gz or --dev".into());
    }
    Ok(set)
}

/// `--solver NAME=WASM[:NATIVE]`, every one.
#[cfg(feature = "runner")]
fn calibration_solvers(args: &[String]) -> Result<Vec<dojo::calibrate::Solver>, String> {
    let solvers = flags(args, "--solver")
        .iter()
        .map(|spec| {
            let (name, paths) = spec
                .split_once('=')
                .ok_or_else(|| format!("--solver NAME=WASM[:NATIVE], not {spec}"))?;
            let (wasm, native) = match paths.split_once(':') {
                Some((wasm, native)) => (wasm, Some(std::path::PathBuf::from(native))),
                None => (paths, None),
            };
            Ok(dojo::calibrate::Solver {
                name: name.to_string(),
                wasm: std::fs::read(wasm).map_err(|e| format!("{wasm}: {e}"))?,
                native,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if solvers.is_empty() {
        return Err("no solvers: give --solver NAME=WASM[:NATIVE]".into());
    }
    Ok(solvers)
}

#[cfg(feature = "runner")]
fn number(args: &[String], name: &str, default: u64) -> Result<u64, String> {
    flag(args, name).map_or(Ok(default), |v| {
        v.replace('_', "")
            .parse()
            .map_err(|_| format!("{name} takes a number"))
    })
}

#[cfg(feature = "runner")]
fn write_out(args: &[String], text: &str) -> Result<(), String> {
    if let Some(path) = flag(args, "--out") {
        return std::fs::write(&path, text).map_err(|e| format!("{path}: {e}"));
    }
    print!("{text}");
    Ok(())
}

#[cfg(feature = "runner")]
fn calibrate(args: &[String]) -> Result<(), String> {
    use dojo::calibrate::{self, FuelRow, Plan, TimeLimits, TimeRow};
    let rest = args.get(1..).unwrap_or_default();
    let cap = number(rest, "--cap", calibrate::CAP)?;
    match args.first().map(String::as_str) {
        Some("fuel") => {
            let solvers = calibration_solvers(rest)?;
            let set = calibration_set(rest)?;
            let mut options = Options::default();
            options.limits.fuel = cap;
            options.set_seed = calibrate::SEED.into();
            if let Some(threads) = flag(rest, "--threads") {
                options.threads = threads.parse().map_err(|_| "--threads takes a number")?;
            }
            let tools = match flag(rest, "--checker").as_deref() {
                Some("builtin") => Tools::Builtin,
                Some("verified") => Tools::find().ok_or("drat-trim and cake_lpr weren't found")?,
                _ => Tools::find_or_builtin(),
            };
            options.checker = Checker::new(tools, CheckLimits::default(), options.threads);
            let total = solvers.len() * set.len();
            let done = std::sync::atomic::AtomicUsize::new(0);
            let rows = calibrate::measure_fuel(&solvers, &set, &options, |row| {
                let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                eprintln!(
                    "[{n}/{total}] {} {} fuel {}",
                    row.solver, row.instance, row.fuel
                );
            })?;
            let mut text = calibrate::fuel_header() + "\n";
            for row in rows {
                text += &(row.csv() + "\n");
            }
            write_out(rest, &text)
        }
        Some("time") => {
            let solvers = calibration_solvers(rest)?;
            let set = calibration_set(rest)?;
            let runs = usize::try_from(number(rest, "--runs", 3)?).unwrap_or(3);
            let measured: Option<Vec<FuelRow>> = flag(rest, "--fuel")
                .map(|path| {
                    let text =
                        std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                    calibrate::read_csv(&text, FuelRow::parse)
                })
                .transpose()?;
            let plan = |solver: &calibrate::Solver, instance: &Instance| match &measured {
                None => Some(Plan { runs, wasm: true }),
                Some(rows) => rows
                    .iter()
                    .find(|r| r.solver == solver.name && r.instance == instance.name)
                    .map(|r| {
                        let finished = r.counts.is_some();
                        Plan {
                            runs: if finished { runs } else { 1 },
                            wasm: finished,
                        }
                    }),
            };
            let limits = TimeLimits {
                timeout: std::time::Duration::from_secs(number(rest, "--timeout", 10)?),
                sandbox: dojo::sandbox::Limits {
                    fuel: cap,
                    ..Default::default()
                },
            };
            let scratch = flag(rest, "--scratch").map_or_else(
                || std::env::temp_dir().join("dojo-calibrate"),
                std::path::PathBuf::from,
            );
            let rows = calibrate::measure_time(&solvers, &set, &limits, &plan, &scratch, |row| {
                eprintln!(
                    "{} {} native {:.1} ms {} wasm {:?}",
                    row.solver,
                    row.instance,
                    row.median_ms(),
                    if row.native_solved {
                        "solved"
                    } else {
                        "unsolved"
                    },
                    row.wasm_ms
                );
            })?;
            let mut text = String::from(calibrate::TIME_HEADER) + "\n";
            for row in rows {
                text += &(row.csv() + "\n");
            }
            write_out(rest, &text)
        }
        Some("analyze") => {
            let (Some(fuel), Some(time)) = (args.get(1), args.get(2)) else {
                return Err("usage: dojo calibrate analyze fuel.csv time.csv".into());
            };
            let read =
                |path: &String| std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"));
            let fuel = calibrate::read_csv(&read(fuel)?, FuelRow::parse)?;
            let time = calibrate::read_csv(&read(time)?, TimeRow::parse)?;
            let budget = number(rest, "--budget", calibrate::UNIFORM_BUDGET)?;
            let min_ms = flag(rest, "--min-ms").map_or(Ok(20.0), |v| {
                v.parse().map_err(|_| "--min-ms takes a number")
            })?;
            let analysis = calibrate::analyze(&fuel, &time, budget, min_ms)?;
            print!("{}", analysis.markdown());
            Ok(())
        }
        _ => Err("usage: dojo calibrate fuel | time | analyze (see dojo --help)".into()),
    }
}

fn export(args: &[String]) -> Result<(), String> {
    let path = std::path::PathBuf::from(
        args.first()
            .ok_or("usage: dojo export <dir | file.tar.gz>")?,
    );
    let season = flag(args, "--season")
        .map(|s| s.parse().map_err(|_| "--season takes a number"))
        .transpose()?;
    let (seed, set) = set(args)?;
    let info = SetInfo {
        hidden: hidden(args),
        ..SetInfo::new(&seed, season)
    };
    if path.to_string_lossy().ends_with(".tar.gz") {
        archive::save(&path, &archive::write_tar_gz(&set, &info))?;
    } else {
        archive::write_dir(&path, &set, &info)?;
    }
    eprintln!("written to {}", path.display());
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        #[cfg(feature = "runner")]
        Some("run") => run(&args[1..]),
        Some("export") => export(&args[1..]),
        #[cfg(feature = "worker")]
        Some("serve") => dojo::worker::serve(),
        #[cfg(feature = "runner")]
        Some("calibrate") => calibrate(&args[1..]),
        _ => Err("usage: dojo run <solver.wasm> | dojo export <dir> | dojo serve".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
