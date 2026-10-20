//! How long formulas take to generate, and how much fuel solvers spend on them (for tuning the
//! sizes in `bench::DEV_SPEC`):
//!
//! ```text
//! gen_time <family> <size>[:sat|:unsat]... [--seeds N] [--solvers a.wasm,b.wasm]
//! ```
//!
//! Every size is generated `--seeds` times (1), each solver runs on each formula with the
//! default budget and the built-in proof checker.
use std::time::Instant;

use dojo::bench::{Spec, generate_with};
use dojo::eval::{Options, run_one};
use dojo::sandbox::Sandbox;
use dojo::{Answer, Family};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let family = Family::from_id(&args[0]).expect("unknown family");
    let seeds: usize = flag("--seeds").map_or(1, |s| s.parse().expect("--seeds N"));
    let sandbox = Sandbox::new().expect("a sandbox");
    let solvers: Vec<(String, dojo::sandbox::Solver)> = flag("--solvers")
        .map(|list| {
            list.split(',')
                .map(|path| {
                    let wasm = std::fs::read(path).expect("a solver");
                    let name = std::path::Path::new(path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    (name, sandbox.load(&wasm).expect("a valid solver"))
                })
                .collect()
        })
        .unwrap_or_default();
    let options = Options {
        threads: 1,
        ..Options::default()
    };
    let sizes = args[1..]
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .map(|s| {
            let (size, want) = s.split_once(':').unwrap_or((s, ""));
            let want = match want {
                "sat" => Some(Answer::Sat),
                "unsat" => Some(Answer::Unsat),
                _ => None,
            };
            (size.parse::<u32>().expect("a size"), want)
        });
    for (size, want) in sizes {
        for seed in 0..seeds {
            let t = Instant::now();
            let spec = [Spec { family, size, want }];
            let set = generate_with(&format!("timing {seed}"), &spec);
            let instance = &set[0];
            print!(
                "{} {size} {:?}: {:.1?} vars={} clauses={}",
                family.id(),
                instance.expected,
                t.elapsed(),
                instance.cnf.vars,
                instance.cnf.clauses.len()
            );
            for (name, solver) in &solvers {
                let r = run_one(&sandbox, solver, instance, &options);
                #[expect(clippy::cast_precision_loss, reason = "only printed")]
                let gfuel = r.fuel as f64 / 1e9;
                print!("  {name} {} {gfuel:.2}G", r.outcome.id());
            }
            println!();
        }
    }
}
