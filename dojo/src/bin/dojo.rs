//! The SAT Dojo command line.
//!
//! ```text
//! dojo run <solver.wasm> [--seed S [--hidden] | --set SET.tar.gz] [--budget FUEL] [--threads N]
//!          [--family F] [--stderr] [--checker verified|builtin] [--proofs DIR]
//!     Scores a solver on the development set, exactly like the leaderboard does. --seed takes
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
use dojo::{Outcome, Score};

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
    if let Some(family) = flag(args, "--family") {
        let family = Family::from_id(&family).unwrap_or_else(|| {
            let ids: Vec<_> = Family::ALL.iter().map(|f| f.id()).collect();
            panic!("unknown family {family}, one of {}", ids.join(", "))
        });
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
    if let Some(budget) = flag(args, "--budget") {
        options.limits.fuel = budget
            .replace('_', "")
            .parse()
            .map_err(|_| "--budget takes a number")?;
    }
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
        eprintln!(
            "[{done:>2}/{}] {:<16} {:<5} {:<9} {:>8.3} Gfuel {:>7} ms",
            set.len(),
            r.name,
            r.expected.id(),
            match r.outcome {
                Outcome::Wrong => "WRONG",
                Outcome::Unverified => "no proof",
                other => other.id(),
            },
            gfuel,
            r.wall_ms,
        );
    });
    let score = Score::from_results(&results, options.limits.fuel);
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
