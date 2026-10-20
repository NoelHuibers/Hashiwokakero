//! When the proof checkers can't do their work (no scratch directory, a checker that doesn't
//! start or is killed), the UNSAT answer stays unverified, which counts as unsolved. Only a
//! checker that read the proof and found it wrong may make it a wrong answer, which
//! disqualifies.

use std::path::Path;
use std::time::{Duration, Instant};

use super::{
    CheckLimits, Checker, Ended, Format, Tools, Verdict, cake_lpr_verdict, drat_trim_verdict,
    run_limited,
};
use crate::cnf::Cnf;

fn unverified(verdict: &Verdict, message: &str) -> bool {
    matches!(verdict, Verdict::Unverified(m) if m.contains(message))
}

#[test]
fn checkers_that_do_not_run_leave_answers_unverified() {
    let not_started = Ended::NotStarted("no such file".into());
    let killed = Ended::Exited(None, "c parsing\n".into());
    for format in [Format::Drat, Format::Lrat] {
        let verdict = cake_lpr_verdict(&not_started, format);
        assert!(
            unverified(&verdict, "cake_lpr didn't start: no such file"),
            "{verdict:?}"
        );
        let verdict = cake_lpr_verdict(&killed, format);
        assert!(
            unverified(&verdict, "cake_lpr failed (killed): c parsing"),
            "{verdict:?}"
        );
    }
    let verdict = drat_trim_verdict(&not_started);
    assert!(
        unverified(&verdict, "drat-trim didn't start: no such file"),
        "{verdict:?}"
    );
    // Out of memory, drat-trim says so and exits with 0
    let verdict = drat_trim_verdict(&Ended::Exited(Some(0), "c MEMOUT\n".into()));
    assert!(
        unverified(&verdict, "drat-trim failed (exit code 0): c MEMOUT"),
        "{verdict:?}"
    );
}

/// A checker can't run without a place for its output, and the verified pipeline not without a
/// scratch directory: both leave the proof unverified.
#[test]
fn checks_without_a_scratch_directory() {
    let missing = std::env::temp_dir().join(format!("dojo-no-such-dir-{}", std::process::id()));
    let ended = run_limited(
        Path::new("/bin/true"),
        &[],
        64,
        Instant::now() + Duration::from_secs(5),
        &missing,
    );
    assert!(matches!(ended, Ended::NotStarted(_)), "{ended:?}");

    let tools = Tools::Verified {
        drat_trim: "/bin/true".into(),
        cake_lpr: "/bin/true".into(),
    };
    let mut checker = Checker::new(tools, CheckLimits::default(), 1);
    // A file where the scratch directory should be: no directory can be made in it
    let file = crate::proof::TempDir::new(&std::env::temp_dir(), "dojo-scratch-file").unwrap();
    std::fs::write(file.path().join("f"), b"").unwrap();
    checker.scratch = file.path().join("f");
    let cnf = Cnf::new(1, vec![vec![1], vec![-1]]);
    let verdict = checker.check(&cnf, &file.path().join("f"), Format::Drat);
    assert!(unverified(&verdict, "no scratch directory"), "{verdict:?}");
}
