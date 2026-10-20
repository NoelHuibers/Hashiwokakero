//! Property tests of what judges submissions: the DRAT checker, reading a solver's answer and
//! scrambling formulas, on random small formulas whose answer is known by trying every
//! assignment.

use std::time::Duration;

use proptest::prelude::*;
use proptest::test_runner::{RngSeed, contextualize_config};
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::cnf::Cnf;
use crate::drat::{DratError, check};
use crate::output::{Claim, judge, parse};
use crate::{Answer, Outcome};

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

const BUDGET: Duration = Duration::from_secs(60);

/// Formulas over 1 to 10 variables with up to `max_clauses` clauses of 1 to 4 literals.
fn cnf(max_clauses: usize) -> impl Strategy<Value = Cnf> {
    (1..=10i32).prop_flat_map(move |vars| {
        let lit = (1..=vars, any::<bool>()).prop_map(|(v, neg)| if neg { -v } else { v });
        prop::collection::vec(prop::collection::vec(lit, 1..=4), 0..=max_clauses)
            .prop_map(move |clauses| Cnf::new(vars.unsigned_abs(), clauses))
    })
}

/// Random 3-SAT over 6 to 10 variables far above the threshold: nearly all unsatisfiable.
fn dense_cnf() -> impl Strategy<Value = Cnf> {
    (6..=10i32).prop_flat_map(|vars| {
        let lit = (1..=vars, any::<bool>()).prop_map(|(v, neg)| if neg { -v } else { v });
        let count = usize::try_from(vars * 8).expect("positive");
        prop::collection::vec(prop::collection::vec(lit, 3), count)
            .prop_map(move |clauses| Cnf::new(vars.unsigned_abs(), clauses))
    })
}

/// A model, if there is one, by trying every assignment.
fn model(cnf: &Cnf) -> Option<Vec<i32>> {
    let vars = i32::try_from(cnf.vars).expect("small");
    (0..1u32 << cnf.vars)
        .map(|bits| {
            (1..=vars)
                .map(|v| if bits >> (v - 1) & 1 == 1 { v } else { -v })
                .collect::<Vec<_>>()
        })
        .find(|lits| cnf.check(lits) == Some(true))
}

/// Ways to break a proof, applied to its steps.
#[derive(Clone, Debug)]
enum Mutation {
    Drop(usize),
    /// Negates the `n`-th literal of a step.
    Negate(usize, usize),
    Insert(usize, Vec<i32>),
    Swap(usize, usize),
    Truncate(usize),
}

fn mutation() -> impl Strategy<Value = Mutation> {
    let index = 0..400usize;
    prop_oneof![
        index.clone().prop_map(Mutation::Drop),
        (index.clone(), 0..4usize).prop_map(|(i, n)| Mutation::Negate(i, n)),
        (
            index.clone(),
            prop::collection::vec((1..=11i32, any::<bool>()), 0..=3).prop_map(|lits| lits
                .into_iter()
                .map(|(v, n)| if n { -v } else { v })
                .collect())
        )
            .prop_map(|(i, c)| Mutation::Insert(i, c)),
        (index.clone(), index.clone()).prop_map(|(i, j)| Mutation::Swap(i, j)),
        index.prop_map(Mutation::Truncate),
    ]
}

fn mutate(proof: &[u8], mutations: &[Mutation]) -> Vec<u8> {
    let text = String::from_utf8_lossy(proof);
    let mut steps: Vec<String> = text.lines().map(str::to_string).collect();
    for m in mutations {
        let n = steps.len().max(1);
        match m {
            Mutation::Drop(i) if !steps.is_empty() => {
                steps.remove(i % n);
            }
            Mutation::Negate(i, k) if !steps.is_empty() => {
                let step = &mut steps[i % n];
                let mut tokens: Vec<String> = step.split_whitespace().map(str::to_string).collect();
                let lits: Vec<usize> = (0..tokens.len())
                    .filter(|&t| tokens[t] != "d" && tokens[t] != "0")
                    .collect();
                if let Some(&t) = lits.get(k % lits.len().max(1)) {
                    tokens[t] = match tokens[t].strip_prefix('-') {
                        Some(positive) => positive.to_string(),
                        None => format!("-{}", tokens[t]),
                    };
                    *step = tokens.join(" ");
                }
            }
            Mutation::Insert(i, clause) => {
                let mut step: String = clause.iter().map(|l| l.to_string() + " ").collect();
                step.push('0');
                steps.insert(i % (steps.len() + 1), step);
            }
            Mutation::Swap(i, j) if !steps.is_empty() => steps.swap(i % n, j % n),
            Mutation::Truncate(i) => steps.truncate(i % (steps.len() + 1)),
            _ => {}
        }
    }
    steps.join("\n").into_bytes()
}

proptest! {
    #![proptest_config(config(64))]

    /// Proofs splr writes for unsatisfiable formulas are accepted.
    #[test]
    fn splr_proofs_are_accepted(cnf in dense_cnf()) {
        prop_assume!(model(&cnf).is_none());
        let proof = crate::bench::splr_proof(&cnf).expect("splr proves it");
        prop_assert_eq!(check(&cnf, &proof, BUDGET), Ok(()));
    }

    /// The checker is sound: whatever the proof (a real one, broken in a few places, for the
    /// formula or a part of it), it never accepts a satisfiable formula.
    #[test]
    fn satisfiable_formulas_have_no_proof(
        full in dense_cnf(),
        keep in prop::collection::vec(any::<bool>(), 80),
        mutations in prop::collection::vec(mutation(), 0..6),
    ) {
        let proof = crate::bench::splr_proof(&full).unwrap_or_default();
        let part = Cnf::new(
            full.vars,
            full.clauses
                .iter()
                .zip(keep.iter().chain(std::iter::repeat(&true)))
                .filter(|(_, keep)| **keep)
                .map(|(c, _)| c.clone())
                .collect(),
        );
        let broken = mutate(&proof, &mutations);
        for (cnf, proof) in [(&full, &broken), (&part, &proof), (&part, &broken)] {
            if check(cnf, proof, BUDGET).is_ok() {
                prop_assert!(model(cnf).is_none(), "accepted a proof for a satisfiable formula");
            }
        }
    }

    /// Bytes that aren't a proof are rejected, with an error instead of a crash.
    #[test]
    fn any_bytes_are_checked_without_panicking(
        cnf in cnf(20),
        proof in prop_oneof![
            prop::collection::vec(any::<u8>(), 0..200),
            "(c [a-z]{0,5}\n|d )?([-]?[0-9]{1,8} ){0,5}0?\n{0,2}".prop_map(String::into_bytes),
            prop::collection::vec("(d )?(-?[1-9] ){0,3}0", 0..20).prop_map(|s| s.join("\n").into_bytes()),
        ],
    ) {
        let result = check(&cnf, &proof, BUDGET);
        if result.is_ok() {
            prop_assert!(model(&cnf).is_none());
        }
        prop_assert!(!matches!(result, Err(DratError::BudgetExceeded)));
    }

    /// A solver's answer reads back as what it said, however the model is split into lines and
    /// whatever comments surround it.
    #[test]
    fn answers_read_back(
        claim in prop_oneof![
            prop::collection::vec(any::<i32>().prop_filter("not 0", |l| *l != 0), 0..20).prop_map(Claim::Sat),
            Just(Claim::Unsat),
            Just(Claim::Unknown),
        ],
        per_line in 1..6usize,
        comments in prop::collection::vec("c[ a-zA-Z0-9]{0,12}", 0..3),
    ) {
        let mut out = comments.join("\n");
        out.push('\n');
        match &claim {
            Claim::Sat(lits) => {
                out.push_str("s SATISFIABLE\n");
                for chunk in lits.chunks(per_line) {
                    let line: Vec<String> = chunk.iter().map(i32::to_string).collect();
                    out.push_str("v ");
                    out.push_str(&line.join(" "));
                    out.push('\n');
                }
                out.push_str("v 0\n");
            }
            Claim::Unsat => out.push_str("s UNSATISFIABLE\n"),
            _ => out.push_str("s UNKNOWN\n"),
        }
        prop_assert_eq!(parse(&out), claim);
    }

    /// Any output is read as some claim, never a crash; a model is only ever accepted if it
    /// satisfies the formula.
    #[test]
    fn any_output_is_judged(
        cnf in cnf(20),
        expected in prop::sample::select(vec![Answer::Sat, Answer::Unsat]),
        stdout in prop_oneof![
            any::<String>(),
            "(s (SATISFIABLE|UNSATISFIABLE|UNKNOWN|x)\n)?(v( -?[0-9]{1,2}){0,6}\n){0,3}",
        ],
    ) {
        let claim = parse(&stdout);
        let outcome = judge(&cnf, expected, &claim, || crate::proof::Verdict::Verified);
        match &claim {
            Claim::Sat(lits) => prop_assert_eq!(
                outcome == Outcome::Solved(Answer::Sat),
                cnf.check(lits) == Some(true)
            ),
            Claim::Unsat => prop_assert_eq!(outcome == Outcome::Solved(Answer::Unsat), expected == Answer::Unsat),
            Claim::Unknown => prop_assert_eq!(outcome, Outcome::Unknown),
            Claim::Malformed => prop_assert_eq!(outcome, Outcome::Wrong),
        }
    }

    /// Scrambling keeps the answer: models carry over, and a scrambled unsatisfiable formula
    /// stays unsatisfiable. The DIMACS text has the formula's size.
    #[test]
    fn scrambling_keeps_the_answer(cnf in cnf(30), seed in any::<u64>()) {
        let (scrambled, map) = cnf.scramble(&mut StdRng::seed_from_u64(seed));
        prop_assert_eq!(scrambled.clauses.len(), cnf.clauses.len());
        match model(&cnf) {
            Some(lits) => {
                let carried: Vec<i32> = lits
                    .iter()
                    .map(|&l| {
                        let renamed = map[l.unsigned_abs() as usize];
                        if l > 0 { renamed } else { -renamed }
                    })
                    .collect();
                prop_assert_eq!(scrambled.check(&carried), Some(true));
            }
            None => prop_assert!(model(&scrambled).is_none()),
        }
        let dimacs = scrambled.dimacs("x");
        let header = format!("c x\np cnf {} {}\n", cnf.vars, cnf.clauses.len());
        prop_assert!(dimacs.starts_with(&header));
        prop_assert_eq!(dimacs.lines().count(), 2 + cnf.clauses.len());
    }
}
