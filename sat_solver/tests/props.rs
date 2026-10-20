//! Property tests: our CDCL and DPLL solvers in every configuration agree with trying every
//! assignment on random small formulas, their models satisfy the formula, and the DIMACS
//! parser answers any file with clauses or an error.

use proptest::prelude::*;
use proptest::test_runner::{RngSeed, contextualize_config};
use sat_solver::cdcl::CDCL;
use sat_solver::cli::Heuristic;
use sat_solver::dpll::{DIMACSOutput, DPLL};

/// The same cases on every run, so CI never fails at random. `PROPTEST_RNG_SEED` (other cases)
/// and `PROPTEST_CASES` (more of them) search further, a failure prints its smallest case.
fn config(cases: u32) -> ProptestConfig {
    contextualize_config(ProptestConfig {
        cases,
        rng_seed: RngSeed::Fixed(0x4a5e),
        // The fixed seed finds a failure again, no files in the source tree needed
        failure_persistence: None,
        ..ProptestConfig::default()
    })
}

/// Random formulas over up to 10 variables, clauses of 1 to 5 literals: some repeat a literal
/// or contain one with its negation, as real inputs do.
fn cnf() -> impl Strategy<Value = (usize, Vec<Vec<i32>>)> {
    (1..=10i32).prop_flat_map(|vars| {
        let lit = (1..=vars, any::<bool>()).prop_map(|(v, neg)| if neg { -v } else { v });
        let clause = prop::collection::vec(lit, 1..=5);
        (
            Just(vars.unsigned_abs() as usize),
            // Small formulas often, they reach the corners (one clause, every variable negated)
            prop_oneof![
                prop::collection::vec(clause.clone(), 1..=4),
                prop::collection::vec(clause, 1..=45)
            ],
        )
    })
}

fn satisfiable(vars: usize, clauses: &[Vec<i32>]) -> bool {
    (0..1u32 << vars).any(|bits| {
        clauses.iter().all(|c| {
            c.iter()
                .any(|&l| (bits >> (l.unsigned_abs() - 1) & 1 == 1) == (l > 0))
        })
    })
}

/// Checks a solver's answer against the brute-force one.
fn check(
    vars: usize,
    clauses: &[Vec<i32>],
    result: &Result<DIMACSOutput, sat_solver::dpll::Error>,
) -> Result<(), TestCaseError> {
    match result {
        Ok(DIMACSOutput::Sat(model)) => {
            let mut value = vec![None; vars + 1];
            for &l in model.iter().filter(|&&l| l != 0) {
                let v = l.unsigned_abs() as usize;
                prop_assert!(v <= vars, "unknown variable {}", l);
                prop_assert!(value[v].is_none(), "variable {} assigned twice", v);
                value[v] = Some(l > 0);
            }
            for clause in clauses {
                prop_assert!(
                    clause
                        .iter()
                        .any(|&l| value[l.unsigned_abs() as usize] == Some(l > 0)),
                    "model {:?} violates {:?}",
                    model,
                    clause
                );
            }
        }
        Ok(DIMACSOutput::Unsat) => prop_assert!(!satisfiable(vars, clauses), "wrongly UNSAT"),
        Ok(DIMACSOutput::Timeout) => prop_assert!(false, "timeout without a deadline"),
        Err(e) => prop_assert!(false, "{}", e),
    }
    Ok(())
}

fn heuristic(i: usize) -> Heuristic {
    match i {
        0 => Heuristic::Arbitrary,
        1 => Heuristic::DLIS,
        2 => Heuristic::DLCS,
        3 => Heuristic::MOM,
        4 => Heuristic::Boehm,
        5 => Heuristic::JeroslawWang,
        6 => Heuristic::VSIDS,
        _ => Heuristic::Custom,
    }
}

proptest! {
    #![proptest_config(config(256))]

    #[test]
    fn dpll_agrees_with_brute_force((vars, clauses) in cnf(), h in 0..8usize) {
        let result = DPLL::new(clauses.clone(), vars, clauses.len(), heuristic(h), false).solve();
        check(vars, &clauses, &result)?;
    }

    /// With both of its heuristics, every restart policy, subsumption on or off and clause
    /// deletion starting early (small k and m) or late.
    #[test]
    fn cdcl_agrees_with_brute_force(
        (vars, clauses) in cnf(),
        vsids in any::<bool>(),
        subsumption in any::<bool>(),
        restarts in prop_oneof![
            Just((None, false, None)),
            (1..50u32).prop_map(|t| (Some(t), false, None)),
            (1..50u32, 100..300u32).prop_map(|(t, f)| (Some(t), false, Some(f))),
            (1..50u32).prop_map(|u| (Some(u), true, None)),
        ],
        (k, m) in (1..=10usize, 1..=10usize),
    ) {
        let (threshold, luby, factor) = restarts;
        let heuristic = if vsids { Heuristic::VSIDS } else { Heuristic::Arbitrary };
        let result = CDCL::new(
            clauses.clone(),
            vars,
            clauses.len(),
            heuristic,
            subsumption,
            threshold,
            luby,
            factor,
            k,
            m,
            None,
        )
        .solve();
        check(vars, &clauses, &result)?;
    }

    /// Both solvers decide the same, and the same as before: a run doesn't depend on the run
    /// before it (see `hash.rs`).
    #[test]
    fn solvers_agree_and_repeat((vars, clauses) in cnf()) {
        let dpll = || DPLL::new(clauses.clone(), vars, clauses.len(), Heuristic::Arbitrary, false).solve();
        let cdcl = || {
            CDCL::new(
                clauses.clone(),
                vars,
                clauses.len(),
                Heuristic::VSIDS,
                false,
                Some(10),
                true,
                None,
                10,
                10,
                None,
            )
            .solve()
        };
        let (a, b) = (dpll(), cdcl());
        let sat = |r: &Result<DIMACSOutput, sat_solver::dpll::Error>| matches!(r, Ok(DIMACSOutput::Sat(_)));
        prop_assert_eq!(sat(&a), sat(&b));
        prop_assert_eq!(format!("{a:?}"), format!("{:?}", dpll()));
        prop_assert_eq!(format!("{b:?}"), format!("{:?}", cdcl()));
    }
}

proptest! {
    #![proptest_config(config(128))]

    /// The command line's DIMACS parser reads written formulas back, and answers anything else
    /// with an error instead of a crash.
    #[test]
    fn parser_reads_formulas_and_rejects_the_rest(
        (vars, clauses) in cnf(),
        garbage in prop_oneof![
            "c [a-z ]{0,10}\np( cnf)?( [0-9x]{1,3}){0,3}\n([-0-9 ]{0,12}\n){0,5}",
            "p[ a-z0-9]{0,12}",
            any::<String>(),
        ],
    ) {
        // Removed with the files in it, also when the parser panics
        let dir = tempfile::tempdir().expect("temp dir");
        let path = |name: &str| {
            dir.path()
                .join(format!("{name}.cnf"))
                .to_string_lossy()
                .into_owned()
        };
        let mut text = format!("c random\np cnf {vars} {}\n", clauses.len());
        for clause in &clauses {
            for lit in clause {
                text.push_str(&lit.to_string());
                text.push(' ');
            }
            text.push_str("0\n");
        }
        let written = path("written");
        std::fs::write(&written, text).expect("write");
        let parsed = sat_solver::parse::parse(&written);
        let (read, read_vars, read_count) = parsed.map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        prop_assert_eq!((read, read_vars, read_count), (clauses.clone(), vars, clauses.len()));

        let file = path("garbage");
        std::fs::write(&file, garbage).expect("write");
        let _ = sat_solver::parse::parse(&file);
    }
}
