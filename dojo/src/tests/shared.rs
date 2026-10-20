//! Tests of the types and rules the website shares with the worker: budgets of archived rules,
//! the codes it stores, copy detection by fingerprint.

use crate::community::Refusal;
use crate::{
    Answer, DEFAULT_BUDGET, FINGERPRINT_GAVE_UP, FINGERPRINT_TIMEOUT, Family, Outcome,
    RULES_VERSION, Rejection, Report, RunResult, Score, budget_of, fingerprint, similar,
};

/// Archived seasons are shown with the budget they were scored with.
#[test]
fn budgets_of_rules_versions() {
    for version in 1..=3 {
        assert_eq!(budget_of(version), 10_000_000_000, "rules {version}");
    }
    assert_eq!(budget_of(4), 7_000_000_000);
    assert_eq!(budget_of(RULES_VERSION), DEFAULT_BUDGET);
}

/// The website stores these codes with refused benchmarks and looks up its texts by them.
#[test]
fn refusal_codes_are_stable() {
    assert_eq!(
        [
            Refusal::Invalid("x".into()),
            Refusal::TooLarge,
            Refusal::TooEasy,
            Refusal::NoAnswer,
        ]
        .map(|r| r.id()),
        ["invalid", "too_large", "too_easy", "no_answer"]
    );
}

/// Every family's name in the Global Benchmark Database, as the archives' READMEs print it;
/// the mystery and community families have none of their own.
#[test]
fn families_in_the_benchmark_database() {
    let gbd: Vec<(&str, Option<&str>)> = Family::ALL
        .into_iter()
        .chain([Family::Mystery, Family::Community])
        .map(|f| (f.id(), f.gbd()))
        .collect();
    assert_eq!(
        gbd,
        [
            ("hashi", Some("puzzle")),
            ("hashi-parity", Some("puzzle")),
            ("random-3sat", Some("random")),
            ("pigeonhole", Some("pigeon-hole")),
            ("queens", Some("puzzle")),
            ("factoring", Some("prime-factoring")),
            ("miter", Some("miter")),
            ("coloring", Some("coloring")),
            ("tseitin", Some("tseitin-formulas")),
            ("quasigroup", Some("quasigroup-completion")),
            ("hamiltonian", Some("hamiltonian")),
            ("mystery", None),
            ("community", None),
        ]
    );
    // Neither is generated, so neither can be asked for by id
    assert_eq!(Family::from_id("mystery"), None);
    assert_eq!(Family::from_id("community"), None);
}

#[test]
fn reports_name_their_submission() {
    let reports = [
        Report::Progress {
            id: 1,
            season: 1,
            done: 0,
            total: 1,
        },
        Report::Finished {
            id: 2,
            season: 1,
            score: Score::from_results(&[], DEFAULT_BUDGET),
            fingerprint: Vec::new(),
            results: Vec::new(),
            rules_version: RULES_VERSION,
        },
        Report::Rejected {
            id: 3,
            season: 1,
            reason: Rejection::NoStart,
        },
        Report::Failed {
            id: 4,
            season: 1,
            message: "lost".into(),
        },
    ];
    assert_eq!(reports.map(|r| r.id()), [1, 2, 3, 4]);
}

/// No results: nothing solved and a PAR-2 of 0, not NaN (the website stores and sorts it).
#[test]
fn empty_scores() {
    let score = Score::from_results(&[], DEFAULT_BUDGET);
    assert_eq!((score.solved, score.total, score.wrong), (0, 0, 0));
    assert!(score.par2 == 0.0 && score.fuel == 0.0);
    assert_eq!(score.families, []);
}

fn run(outcome: Outcome, fuel: u64) -> RunResult {
    RunResult {
        name: String::new(),
        family: Family::Hashi,
        expected: Answer::Sat,
        outcome,
        fuel,
        wall_ms: 0,
    }
}

/// A crash and an unknown answer both gave up, a timeout didn't: copies are told apart by it.
#[test]
fn fingerprints_mark_what_happened() {
    let print = fingerprint(&[
        run(Outcome::Crashed, 5),
        run(Outcome::Unknown, 6),
        run(Outcome::Timeout, 7),
        run(Outcome::Solved(Answer::Sat), 8),
    ]);
    assert_eq!(
        print,
        [
            FINGERPRINT_GAVE_UP,
            FINGERPRINT_GAVE_UP,
            FINGERPRINT_TIMEOUT,
            8
        ]
    );
}

/// Copy detection: the same outcomes everywhere and fuel within 2 % on at least 8 solved
/// formulas.
#[test]
fn copies_by_fingerprint() {
    let solved = |n: u64, scale: f64| -> Vec<u64> {
        (1..=n)
            .map(|i| {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss,
                    reason = "small test numbers"
                )]
                let fuel = (i as f64 * 1e6 * scale).round() as u64;
                fuel
            })
            .collect()
    };
    let original = solved(8, 1.0);
    // Within 2 %, at the edge too
    assert!(similar(&solved(8, 1.02), &original));
    assert!(similar(&original, &solved(8, 1.02)));
    // Beyond it on a single formula
    let mut changed = original.clone();
    changed[3] = changed[3] * 103 / 100;
    assert!(!similar(&changed, &original));
    // Fewer than 8 solved formulas in common prove nothing
    let few = [solved(7, 1.0), vec![FINGERPRINT_TIMEOUT]].concat();
    assert!(!similar(&few, &few));
    let enough = [solved(8, 1.0), vec![FINGERPRINT_TIMEOUT]].concat();
    assert!(similar(&enough, &enough));
    // Solved against unsolved, other sets, nothing
    let mut timed_out = enough.clone();
    timed_out[0] = FINGERPRINT_TIMEOUT;
    assert!(!similar(&timed_out, &enough));
    assert!(!similar(&original, &enough));
    assert!(!similar(&[], &[]));
}
