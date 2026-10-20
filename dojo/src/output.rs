//! Reading a solver's answer (SAT competition format) and judging it.
//!
//! ```text
//! c comments are ignored
//! s SATISFIABLE
//! v 1 -2 3 0
//! ```
//! or `s UNSATISFIABLE` / `s UNKNOWN`. UNSAT comes with a proof in a file (see [`crate::proof`]).

use crate::cnf::Cnf;
use crate::proof::Verdict;
use crate::{Answer, Outcome};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Claim {
    Sat(Vec<i32>),
    Unsat,
    Unknown,
    /// Several `s` lines, a model without `s SATISFIABLE`, or numbers that don't parse.
    Malformed,
}

pub fn parse(stdout: &str) -> Claim {
    let mut status = None;
    let mut literals = Vec::new();
    let mut ended = false;
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(s) = line.strip_prefix("s ") {
            if status.is_some() {
                return Claim::Malformed;
            }
            status = Some(s.trim().to_string());
        } else if let Some(v) = line
            .strip_prefix("v ")
            .or_else(|| (line == "v").then_some(""))
        {
            for token in v.split_whitespace() {
                match token.parse::<i32>() {
                    Ok(0) => ended = true,
                    Ok(_) if ended => return Claim::Malformed,
                    Ok(l) => literals.push(l),
                    Err(_) => return Claim::Malformed,
                }
            }
        }
    }
    match status.as_deref() {
        Some("SATISFIABLE") => Claim::Sat(literals),
        Some("UNSATISFIABLE") if literals.is_empty() => Claim::Unsat,
        Some("UNKNOWN") | None if literals.is_empty() => Claim::Unknown,
        _ => Claim::Malformed,
    }
}

/// Judges a claim, like the SAT Competition.
///
/// Models are always checked. UNSAT on a formula known to be satisfiable is wrong; otherwise
/// `proof` checks the solver's proof (it is only called then): verified is solved, rejected is
/// wrong, no verdict (no proof, too large, the checker ran out of time) is unverified, which
/// counts as unsolved. A model for a formula we believed unsatisfiable still counts (it is
/// checked, so the solver is right and our label was wrong).
pub fn judge(
    cnf: &Cnf,
    expected: Answer,
    claim: &Claim,
    proof: impl FnOnce() -> Verdict,
) -> Outcome {
    match claim {
        Claim::Sat(model) => match cnf.check(model) {
            Some(true) => Outcome::Solved(Answer::Sat),
            _ => Outcome::Wrong,
        },
        Claim::Unsat if expected == Answer::Unsat => match proof() {
            Verdict::Verified => Outcome::Solved(Answer::Unsat),
            Verdict::Rejected(_) => Outcome::Wrong,
            Verdict::Unverified(_) => Outcome::Unverified,
        },
        Claim::Unknown => Outcome::Unknown,
        Claim::Unsat | Claim::Malformed => Outcome::Wrong,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_competition_output() {
        assert_eq!(
            parse("c hi\ns SATISFIABLE\nv 1 -2\nv 3 0\n"),
            Claim::Sat(vec![1, -2, 3])
        );
        assert_eq!(parse("s UNSATISFIABLE\n"), Claim::Unsat);
        assert_eq!(parse("s UNKNOWN\n"), Claim::Unknown);
        assert_eq!(parse(""), Claim::Unknown);
        assert_eq!(parse("s SATISFIABLE\ns UNSATISFIABLE\n"), Claim::Malformed);
        assert_eq!(parse("v 1 0\n"), Claim::Malformed);
        assert_eq!(parse("s SATISFIABLE\nv 1 x 0\n"), Claim::Malformed);
        assert_eq!(parse("s SATISFIABLE\nv 1 0 2\n"), Claim::Malformed);
    }

    /// The judging matrix: what each claim and proof verdict comes to.
    #[test]
    fn judges() {
        let cnf = Cnf::new(2, vec![vec![1, 2], vec![-1]]);
        let verified = || Verdict::Verified;
        let unused = || -> Verdict { panic!("no proof to check") };
        assert_eq!(
            judge(&cnf, Answer::Sat, &Claim::Sat(vec![-1, 2]), unused),
            Outcome::Solved(Answer::Sat)
        );
        assert_eq!(
            judge(&cnf, Answer::Sat, &Claim::Sat(vec![1, 2]), unused),
            Outcome::Wrong
        );
        // UNSAT on a known-SAT formula is wrong, whatever the proof
        assert_eq!(
            judge(&cnf, Answer::Sat, &Claim::Unsat, unused),
            Outcome::Wrong
        );
        assert_eq!(
            judge(&cnf, Answer::Unsat, &Claim::Unsat, verified),
            Outcome::Solved(Answer::Unsat)
        );
        assert_eq!(
            judge(&cnf, Answer::Unsat, &Claim::Unsat, || Verdict::Rejected(
                "lemma 3".into()
            )),
            Outcome::Wrong
        );
        assert_eq!(
            judge(&cnf, Answer::Unsat, &Claim::Unsat, || Verdict::Unverified(
                "no proof".into()
            )),
            Outcome::Unverified
        );
        assert_eq!(
            judge(&cnf, Answer::Unsat, &Claim::Sat(vec![-1, 2]), unused),
            Outcome::Solved(Answer::Sat)
        );
        assert_eq!(
            judge(&cnf, Answer::Sat, &Claim::Unknown, unused),
            Outcome::Unknown
        );
        assert_eq!(
            judge(&cnf, Answer::Sat, &Claim::Malformed, unused),
            Outcome::Wrong
        );
    }
}
