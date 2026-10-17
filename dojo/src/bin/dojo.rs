//! The SAT Dojo command line.
//!
//! ```text
//! dojo run <solver.wasm> [--seed S] [--budget FUEL] [--threads N] [--family F] [--stderr]
//!     Scores a solver on the development set, exactly like the leaderboard does.
//! dojo export <dir> [--seed S]
//!     Writes the set as DIMACS files, with the expected answers in answers.txt.
//! dojo serve
//!     The worker service for hashi.rs (see dojo::worker for its settings).
//! ```

use std::process::ExitCode;
use std::time::Instant;

use dojo::bench::{self, Instance};
#[cfg(feature = "runner")]
use dojo::eval::{Options, evaluate};
#[cfg(feature = "runner")]
use dojo::sandbox::Sandbox;
use dojo::{Answer, DEV_SEED, Family};
#[cfg(feature = "runner")]
use dojo::{Outcome, Score};

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn set(args: &[String]) -> Vec<Instance> {
    let seed = flag(args, "--seed").unwrap_or_else(|| DEV_SEED.to_string());
    let started = Instant::now();
    let mut set = bench::generate(&seed);
    if let Some(family) = flag(args, "--family") {
        let family = Family::from_id(&family).unwrap_or_else(|| {
            let ids: Vec<_> = Family::ALL.iter().map(|f| f.id()).collect();
            panic!("unknown family {family}, one of {}", ids.join(", "))
        });
        set.retain(|i| i.family == family);
    }
    eprintln!("set {seed}: {} formulas, generated in {:.1?}", set.len(), started.elapsed());
    set
}

#[cfg(feature = "runner")]
fn run(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or("usage: dojo run <solver.wasm>")?;
    let wasm = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let sandbox = Sandbox::new()?;
    let solver = sandbox.load(&wasm).map_err(|e| format!("rejected: {e:?}"))?;
    let mut options = Options { stderr: args.iter().any(|a| a == "--stderr"), ..Default::default() };
    if let Some(budget) = flag(args, "--budget") {
        options.limits.fuel = budget.replace('_', "").parse().map_err(|_| "--budget takes a number")?;
    }
    if let Some(threads) = flag(args, "--threads") {
        options.threads = threads.parse().map_err(|_| "--threads takes a number")?;
    }
    let set = set(args);
    let started = Instant::now();
    let results = evaluate(&sandbox, &solver, &set, &options, |done, r| {
        eprintln!(
            "[{done:>2}/{}] {:<16} {:<5} {:<9} {:>8.3} Gfuel {:>7} ms",
            set.len(),
            r.name,
            match r.expected {
                Answer::Sat => "SAT",
                Answer::Unsat => "UNSAT",
            },
            match r.outcome {
                Outcome::Solved(_) => "solved",
                Outcome::Unknown => "unknown",
                Outcome::Timeout => "timeout",
                Outcome::Crashed => "crashed",
                Outcome::Wrong => "WRONG",
            },
            r.fuel as f64 / 1e9,
            r.wall_ms,
        );
    });
    let score = Score::from_results(&results, options.limits.fuel);
    println!();
    for f in &score.families {
        println!("{:<14} {:>2}/{:<2} solved  PAR-2 {:>7.3}", f.family.id(), f.solved, f.total, f.par2);
    }
    println!(
        "\nsolved {}/{}  PAR-2 {:.3} Gfuel  ({:.1?})",
        score.solved,
        score.total,
        score.par2,
        started.elapsed()
    );
    if score.wrong > 0 {
        println!("{} WRONG answers: the leaderboard would disqualify this solver", score.wrong);
    }
    Ok(())
}

const README: &str = "The development set of the Hashi SAT Dojo (https://hashi.rs/dojo).

One DIMACS CNF file per formula, answers.txt has the expected answer of each. The hidden test
set of a season has the same families and sizes, generated from a secret seed.

Score a solver exactly like the leaderboard does:
    dojo run my-solver.wasm
Regenerate this set:
    dojo export dojo-dev-1
";

fn export(args: &[String]) -> Result<(), String> {
    let dir = std::path::PathBuf::from(args.first().ok_or("usage: dojo export <dir>")?);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut answers = String::new();
    for instance in set(args) {
        let file = dir.join(format!("{}.cnf", instance.name));
        let comment = format!("{} from the Hashi SAT Dojo", instance.name);
        std::fs::write(&file, instance.cnf.dimacs(&comment)).map_err(|e| e.to_string())?;
        let answer = match instance.expected {
            Answer::Sat => "SAT",
            Answer::Unsat => "UNSAT",
        };
        answers.push_str(&format!("{} {answer}\n", instance.name));
    }
    std::fs::write(dir.join("answers.txt"), answers).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("README.txt"), README).map_err(|e| e.to_string())?;
    eprintln!("written to {}", dir.display());
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
