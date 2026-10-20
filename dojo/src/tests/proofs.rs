//! The house solvers' proofs (`backend::sat::solve_clauses_with_proof`, what `dojo/house` runs)
//! and the starter's, checked like the leaderboard checks them: with the built-in checker, and
//! with drat-trim and `cake_lpr` when they are installed.

use std::path::Path;
use std::time::Duration;

use backend::sat::{SatOutcome, solve_clauses_with_proof};
use common::{SolverConfig, SolverKind};
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::Family;
use crate::cnf::Cnf;
use crate::proof::{CheckLimits, Checker, Format, TempDir, Tools, Verdict};

/// Unsatisfiable formulas of the families: pigeonhole, Hashi parity, random 3-SAT.
fn unsat_formulas() -> Vec<(String, Cnf)> {
    let mut formulas = vec![("php-5-4".to_string(), crate::bench::pigeonhole(4, false).0)];
    for (i, (family, size)) in [(Family::HashiParity, 5), (Family::Random3Sat, 40)]
        .into_iter()
        .enumerate()
    {
        let mut found = 0;
        for attempt in 0..40u64 {
            let mut rng = StdRng::seed_from_u64(attempt * 7 + i as u64);
            if let Some((cnf, crate::Answer::Unsat, _)) = crate::bench::generate_one(
                crate::bench::Spec {
                    family,
                    size,
                    want: None,
                },
                &mut rng,
            ) {
                formulas.push((format!("{}-{attempt}", family.id()), cnf));
                found += 1;
                if found == 2 {
                    break;
                }
            }
        }
        assert!(found > 0, "no unsatisfiable {family:?}");
    }
    formulas
}

fn checkers() -> Vec<Checker> {
    let limits = CheckLimits {
        time: Duration::from_secs(60),
        memory_mb: 1024,
    };
    let mut checkers = vec![Checker::new(Tools::Builtin, limits.clone(), 1)];
    if let Some(tools) = Tools::find() {
        checkers.push(Checker::new(tools, limits, 1));
    }
    checkers
}

#[test]
fn house_proofs_verify() {
    let dir = TempDir::new(&std::env::temp_dir(), "dojo-house-proofs").unwrap();
    let path = dir.path().join("proof.out");
    let configs = [
        ("cdcl", SolverConfig::default()),
        (
            "dpll",
            SolverConfig {
                kind: SolverKind::Dpll,
                ..Default::default()
            }
            .normalized(),
        ),
        ("splr", SolverConfig::splr()),
    ];
    let checkers = checkers();
    for (name, cnf) in unsat_formulas() {
        for (solver, config) in &configs {
            let (outcome, _) =
                solve_clauses_with_proof(&cnf.clauses, cnf.vars as usize, config, None, &path)
                    .unwrap();
            assert_eq!(outcome, SatOutcome::Unsat, "{solver} on {name}");
            for checker in &checkers {
                assert_eq!(
                    checker.check(&cnf, &path, Format::Drat),
                    Verdict::Verified,
                    "{solver} on {name}, checked with {:?}",
                    checker.tools
                );
            }
        }
    }
    // Satisfiable: no proof needed, the file is there anyway (empty)
    let sat = crate::bench::pigeonhole(4, true).0;
    for (_, config) in &configs {
        let (outcome, _) =
            solve_clauses_with_proof(&sat.clauses, sat.vars as usize, config, None, &path).unwrap();
        assert!(matches!(outcome, SatOutcome::Sat(_)));
    }
    assert!(Path::new(&path).exists());
}

/// The starter kit built for WASI, `None` if that isn't possible here (no wasm32-wasip1
/// target). CI has the target, so there it must build.
#[cfg(feature = "runner")]
fn starter_wasm() -> Option<Vec<u8>> {
    let starter = Path::new(env!("CARGO_MANIFEST_DIR")).join("starter");
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/dojo-starter");
    let mut cargo = std::process::Command::new(env!("CARGO"));
    cargo
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-wasip1",
            "--manifest-path",
        ])
        .arg(starter.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target);
    // Not with the coverage instrumentation of this build, which doesn't exist for WASI
    for var in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_RUSTFLAGS",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "LLVM_PROFILE_FILE",
        "CARGO_TARGET_DIR",
    ] {
        cargo.env_remove(var);
    }
    let built = cargo.status().is_ok_and(|s| s.success());
    if !built {
        assert!(
            std::env::var_os("CI").is_none(),
            "the starter must build for wasm32-wasip1 in CI"
        );
        eprintln!("the starter doesn't build for wasm32-wasip1 here, skipped");
        return None;
    }
    std::fs::read(target.join("wasm32-wasip1/release/dojo-starter.wasm")).ok()
}

/// The starter answers UNSAT with a proof the checkers accept, in the real sandbox.
#[cfg(feature = "runner")]
#[test]
fn starter_proofs_verify() {
    use crate::bench::Instance;
    use crate::eval::{Options, run_one};
    use crate::sandbox::Sandbox;
    use crate::{Answer, Outcome};

    let Some(wasm) = starter_wasm() else {
        return;
    };
    let sandbox = Sandbox::new().unwrap();
    let solver = sandbox.load(&wasm).unwrap();
    let mut formulas: Vec<(String, Cnf, Answer)> = unsat_formulas()
        .into_iter()
        .map(|(name, cnf)| (name, cnf, Answer::Unsat))
        .collect();
    formulas.push((
        "php-4-4".into(),
        crate::bench::pigeonhole(4, true).0,
        Answer::Sat,
    ));
    for checker in checkers() {
        let options = Options {
            checker: checker.clone(),
            ..Options::default()
        };
        for (name, cnf, expected) in &formulas {
            let instance = Instance {
                name: name.clone(),
                family: Family::Pigeonhole,
                expected: *expected,
                cnf: cnf.clone(),
                witness: None,
            };
            let result = run_one(&sandbox, &solver, &instance, &options);
            assert_eq!(
                result.outcome,
                Outcome::Solved(*expected),
                "{name} with {:?}",
                checker.tools
            );
        }
    }
}
