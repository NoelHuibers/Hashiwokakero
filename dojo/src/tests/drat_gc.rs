//! The built-in DRAT checker after it dropped deleted clauses from memory (`collect_garbage`,
//! once more than 2^16 literals are deleted): clause ids are renumbered, and the watches, the
//! reasons and the index of deletable clauses must follow, or later steps check against the
//! wrong clauses. The leaderboard's checkers have to agree where they are installed.

use std::fmt::Write as _;
use std::time::Duration;

use crate::cnf::Cnf;
use crate::drat::{DratError, check};
use crate::proof::{CheckLimits, Checker, Format, PROOF_FILE, TempDir, Tools, Verdict};

/// Padding clauses, three literals each: deleting them all is well past the 2^16 literals
/// that start a collection.
const PADDING: i32 = 22_000;

/// Padding first (so every clause after it gets a new id), then the unit `p`, `¬p ∨ q` (the
/// reason for `q`) and the four clauses over `1` and `2` that are unsatisfiable once `q` holds.
fn formula() -> Cnf {
    let (p, q) = (3, 4);
    let mut clauses: Vec<Vec<i32>> = (0..PADDING)
        .map(|i| vec![5 + 3 * i, 6 + 3 * i, 7 + 3 * i])
        .collect();
    clauses.push(vec![p]);
    clauses.push(vec![-p, q]);
    for (a, b) in [(1, 2), (-1, 2), (1, -2), (-1, -2)] {
        clauses.push(vec![-q, a, b]);
    }
    Cnf::new((5 + 3 * PADDING).unsigned_abs(), clauses)
}

/// Deletes every padding clause (in another literal order than the formula's), then `steps`.
fn proof(steps: &str) -> Vec<u8> {
    let mut proof = String::new();
    for i in 0..PADDING {
        let _ = writeln!(proof, "d {} {} {} 0", 7 + 3 * i, 5 + 3 * i, 6 + 3 * i);
    }
    proof.push_str(steps);
    proof.into_bytes()
}

/// The verdicts of the built-in checker and, if installed, of drat-trim and `cake_lpr`.
fn verdicts(steps: &str) -> (Result<(), DratError>, Option<Verdict>) {
    let cnf = formula();
    let bytes = proof(steps);
    let builtin = check(&cnf, &bytes, Duration::from_secs(60));
    let verified = Tools::find().map(|tools| {
        let dir = TempDir::new(&std::env::temp_dir(), "dojo-drat-gc").unwrap();
        let path = dir.path().join(PROOF_FILE);
        std::fs::write(&path, &bytes).unwrap();
        let mut checker = Checker::new(tools, CheckLimits::default(), 1);
        checker.scratch = dir.path().to_path_buf();
        checker.check(&cnf, &path, Format::Drat)
    });
    if verified.is_none() {
        assert!(
            std::env::var_os("DOJO_REQUIRE_CHECKERS").is_none(),
            "DOJO_REQUIRE_CHECKERS is set but drat-trim and cake_lpr weren't found"
        );
    }
    (builtin, verified)
}

/// After the collection the refutation still checks: propagation finds the core clauses
/// through their renumbered watches. Deleting the reason for `q` is ignored like before, and
/// deleting a clause that isn't there changes nothing.
#[test]
fn proofs_check_after_a_collection() {
    let (builtin, verified) = verdicts("d 4 -3 0\nd 1 2 3 4 0\n-1 0\n0\n");
    assert_eq!(builtin, Ok(()));
    assert!(
        matches!(verified, None | Some(Verdict::Verified)),
        "{verified:?}"
    );
}

/// Deleting a core clause after the collection finds it through the renumbered index: the
/// empty clause no longer follows. (Missed, the deletion would let a wrong proof pass.)
#[test]
fn deletions_after_a_collection_find_their_clause() {
    let (builtin, verified) = verdicts("d 2 1 -4 0\n-1 0\n0\n");
    assert_eq!(
        builtin,
        Err(DratError::NotRedundant {
            lemma: 2,
            line: usize::try_from(PADDING).unwrap() + 3
        })
    );
    assert!(
        matches!(verified, None | Some(Verdict::Rejected(_))),
        "{verified:?}"
    );
}
