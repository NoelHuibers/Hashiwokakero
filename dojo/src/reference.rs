//! The reference solvers: `MiniSat`, `CaDiCaL` and Kissat as WebAssembly.
//!
//! `dojo/reference/build.sh` compiles them from pinned sources with a pinned wasi-sdk. The build
//! is reproducible and CI checks that the committed `.wasm` files are what it builds.
//!
//! They are house entries on the leaderboard (see `scripts/dojo-house.sh`), so every
//! submission is compared with the state of the art, run under exactly the same rules. `MiniSat`
//! also decides which community benchmarks are too easy ([`too_easy`]), like it does for the SAT
//! Competition's "bring your own benchmark" rule.

use std::time::Duration;

use crate::cnf::Cnf;
use crate::output::{Claim, parse};
use crate::sandbox::{End, Limits, Sandbox};

/// `MiniSat` (the last commit of its repository, 2013, with DRAT proofs added), see
/// `dojo/reference/`.
pub const MINISAT: &[u8] = include_bytes!("../reference/minisat.wasm");

/// The fuel `MiniSat` gets in the "too easy" filter: a tenth of what an entry gets per formula.
///
/// The SAT Competition calls a benchmark interesting when `MiniSat` can't solve it within a
/// minute and the submitter's own solver can within an hour. Here a community benchmark has
/// to stay out of `MiniSat`'s reach with a tenth of the budget, so a solver that is better than
/// `MiniSat` still has room to solve it within the full budget.
pub const EASY_FUEL: u64 = crate::DEFAULT_BUDGET / 10;

/// Whether `MiniSat` solves `cnf` with [`EASY_FUEL`] in the sandbox: a model that checks out,
/// or an UNSAT answer (its proof isn't checked, the filter only asks how hard the formula is).
///
/// Deterministic like every run in the sandbox: the same formula is too easy on every machine.
/// An error loading the module (it is checked by the tests) counts as not too easy.
pub fn too_easy(sandbox: &Sandbox, cnf: &Cnf, memory_bytes: usize) -> bool {
    let Ok(solver) = sandbox.load(MINISAT) else {
        return false;
    };
    let limits = Limits {
        fuel: EASY_FUEL,
        memory_bytes,
        wall: Duration::from_secs(120),
        ..Limits::default()
    };
    let run = sandbox.run(&solver, &cnf.dimacs(""), 0, &limits, false, None);
    run.end == End::Exited
        && match parse(&run.stdout) {
            Claim::Sat(model) => cnf.check(&model) == Some(true),
            Claim::Unsat => true,
            Claim::Unknown | Claim::Malformed => false,
        }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::bench::{Spec, generate_with};
    use crate::eval::{Options, run_one};
    use crate::proof::{CheckLimits, Checker, Tools};
    use crate::{Answer, Family, Outcome};

    const CADICAL: &[u8] = include_bytes!("../reference/cadical.wasm");
    const KISSAT: &[u8] = include_bytes!("../reference/kissat.wasm");

    fn spec(family: Family, size: u32, want: Answer) -> Spec {
        Spec {
            family,
            size,
            want: Some(want),
        }
    }

    /// The committed modules are the ones `SHA256SUMS` lists (and CI checks that the build
    /// script makes exactly these).
    #[test]
    fn modules_match_their_checksums() {
        let sums = include_str!("../reference/SHA256SUMS");
        for (name, wasm) in [
            ("minisat.wasm", MINISAT),
            ("cadical.wasm", CADICAL),
            ("kissat.wasm", KISSAT),
        ] {
            let hex = Sha256::digest(wasm).iter().fold(String::new(), |mut s, b| {
                use std::fmt::Write as _;
                let _ = write!(s, "{b:02x}");
                s
            });
            assert!(
                sums.lines().any(|l| l == format!("{hex}  {name}")),
                "{name} isn't the one in SHA256SUMS: run dojo/reference/build.sh"
            );
        }
    }

    /// Every reference solver runs in the sandbox, finds models and proves unsatisfiability with
    /// a proof the checker accepts: drat-trim and `cake_lpr` when installed (CI builds them and
    /// sets `DOJO_REQUIRE_CHECKERS`), the built-in checkers otherwise.
    #[test]
    fn reference_solvers_solve_and_prove() {
        let tools = Tools::find().unwrap_or_else(|| {
            assert!(
                std::env::var_os("DOJO_REQUIRE_CHECKERS").is_none(),
                "DOJO_REQUIRE_CHECKERS is set but drat-trim and cake_lpr weren't found"
            );
            Tools::Builtin
        });
        let set = generate_with(
            "reference",
            &[
                spec(Family::Factoring, 14, Answer::Sat),
                spec(Family::Pigeonhole, 6, Answer::Unsat),
                spec(Family::Coloring, 40, Answer::Unsat),
                spec(Family::Miter, 4, Answer::Unsat),
            ],
        );
        let sandbox = Sandbox::new().unwrap();
        let options = Options {
            checker: Checker::new(tools, CheckLimits::default(), 1),
            threads: 1,
            ..Options::default()
        };
        for (name, wasm) in [
            ("minisat", MINISAT),
            ("cadical", CADICAL),
            ("kissat", KISSAT),
        ] {
            let solver = sandbox.load(wasm).unwrap();
            for instance in &set {
                let result = run_one(&sandbox, &solver, instance, &options);
                assert_eq!(
                    result.outcome,
                    Outcome::Solved(instance.expected),
                    "{name} on {}",
                    instance.name
                );
                assert!(result.fuel > 0 && result.fuel < crate::DEFAULT_BUDGET);
            }
        }
    }

    /// `MiniSat` settles small formulas at once, SAT or UNSAT, and runs out of fuel on a large
    /// pigeonhole formula, which resolution can only refute with exponentially many steps.
    #[test]
    fn minisat_decides_what_is_too_easy() {
        let sandbox = Sandbox::new().unwrap();
        let memory = 1 << 30;
        assert!(too_easy(
            &sandbox,
            &Cnf::new(2, vec![vec![1, 2], vec![-1]]),
            memory
        ));
        assert!(too_easy(
            &sandbox,
            &Cnf::new(1, vec![vec![1], vec![-1]]),
            memory
        ));
        let set = generate_with(
            "too easy",
            &[
                spec(Family::Factoring, 14, Answer::Sat),
                spec(Family::Pigeonhole, 12, Answer::Unsat),
            ],
        );
        assert!(too_easy(&sandbox, &set[0].cnf, memory));
        assert!(!too_easy(&sandbox, &set[1].cnf, memory));
        // Without memory to run in it settles nothing
        assert!(!too_easy(&sandbox, &set[0].cnf, 1 << 16));
    }
}

/// What `MiniSat` pays on fixed formulas under the leaderboard's cost model: it changes only
/// when the cost model, the sandbox or the module does, and then [`crate::RULES_VERSION`] has to
/// change too (every stored score was computed with the old fuel).
#[cfg(test)]
mod golden {
    use super::*;
    use crate::bench::{Spec, generate_with};
    use crate::{Answer, Family};

    #[test]
    fn leaderboard_fuel_is_unchanged() {
        let set = generate_with(
            "golden fuel",
            &[
                Spec {
                    family: Family::Pigeonhole,
                    size: 6,
                    want: Some(Answer::Unsat),
                },
                Spec {
                    family: Family::Factoring,
                    size: 14,
                    want: Some(Answer::Sat),
                },
            ],
        );
        let sandbox = Sandbox::new().unwrap();
        let solver = sandbox.load(MINISAT).unwrap();
        let fuel: Vec<u64> = set
            .iter()
            .map(|i| {
                let run = sandbox.run(
                    &solver,
                    &i.cnf.dimacs(""),
                    1,
                    &Limits::default(),
                    false,
                    None,
                );
                assert_eq!(run.end, End::Exited);
                run.fuel
            })
            .collect();
        assert_eq!(
            fuel,
            [34_301_392, 23_972_394],
            "the leaderboard's fuel changed: bump RULES_VERSION (and update these numbers)"
        );
    }
}
