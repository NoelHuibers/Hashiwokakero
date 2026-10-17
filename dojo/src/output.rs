//! Reading a solver's answer (SAT competition format) and judging it.
//!
//! ```text
//! c comments are ignored
//! s SATISFIABLE
//! v 1 -2 3 0
//! ```
//! or `s UNSATISFIABLE` / `s UNKNOWN`.

use crate::cnf::Cnf;
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

/// Judges a claim.
///
/// Models are always checked; UNSAT is accepted when the formula is known to
/// be unsatisfiable. A model for a formula we believed unsatisfiable still counts (it is
/// checked, so the solver is right and our label was wrong).
pub fn judge(cnf: &Cnf, expected: Answer, claim: &Claim) -> Outcome {
    match claim {
        Claim::Sat(model) => match cnf.check(model) {
            Some(true) => Outcome::Solved(Answer::Sat),
            _ => Outcome::Wrong,
        },
        Claim::Unsat if expected == Answer::Unsat => Outcome::Solved(Answer::Unsat),
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

    #[test]
    fn judges() {
        let cnf = Cnf::new(2, vec![vec![1, 2], vec![-1]]);
        assert_eq!(
            judge(&cnf, Answer::Sat, &Claim::Sat(vec![-1, 2])),
            Outcome::Solved(Answer::Sat)
        );
        assert_eq!(
            judge(&cnf, Answer::Sat, &Claim::Sat(vec![1, 2])),
            Outcome::Wrong
        );
        assert_eq!(judge(&cnf, Answer::Sat, &Claim::Unsat), Outcome::Wrong);
        assert_eq!(
            judge(&cnf, Answer::Unsat, &Claim::Unsat),
            Outcome::Solved(Answer::Unsat)
        );
        assert_eq!(
            judge(&cnf, Answer::Unsat, &Claim::Sat(vec![-1, 2])),
            Outcome::Solved(Answer::Sat)
        );
        assert_eq!(judge(&cnf, Answer::Sat, &Claim::Unknown), Outcome::Unknown);
    }
}
