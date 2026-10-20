//! Checking LRAT proofs of unsatisfiability: the fallback for local runs, the leaderboard checks
//! them with `cake_lpr` (see [`crate::proof`]).
//!
//! LRAT is DRAT with hints: every clause has a number (the formula's clauses 1, 2, ... in
//! order) and every lemma lists the clauses that make it follow by unit propagation, in the
//! order they become unit. Checking needs no search, just one pass over the hints:
//!
//! ```text
//! 9 1 2 0 1 5 0        lemma 9 is (1 ∨ 2): assume ¬1 and ¬2, clause 1 becomes unit, clause 5
//!                      then has no true literal left, a conflict
//! 9 d 1 5 0            deletes clauses 1 and 5 (the 9 in front means nothing)
//! 10 0 9 7 3 0         lemma 10 is the empty clause, the end of the proof
//! ```
//!
//! A hint clause whose literals are all false is the conflict that proves the lemma; with one
//! literal left that isn't false, that literal becomes true; with more, the hint is wrong and
//! the proof is rejected (like `cake_lpr` does). If the hints before the first negative one end
//! without a conflict, the lemma must be RAT on its first literal `p`: every clause `D`
//! containing `¬p` needs a group `-D h h ...` whose hints refute the resolvent, unless the
//! resolvent is already true. Binary LRAT is the same with each step starting with `a` or `d`
//! and every number (clause ids too) written like a binary DRAT literal (see
//! [`crate::drat::write_varint`]).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::cnf::Cnf;
use crate::drat::{DratError, MAX_VAR, Parser, is_binary, read_varint};

/// One step of an LRAT proof.
enum Step {
    Add {
        id: u64,
        lits: Vec<i32>,
        hints: Vec<i64>,
    },
    Delete(Vec<u64>),
}

/// Checks that `proof` (text or binary LRAT) shows `cnf` unsatisfiable, within `budget`.
pub fn check(cnf: &Cnf, proof: &[u8], budget: Duration) -> Result<(), DratError> {
    let deadline = Instant::now().checked_add(budget);
    let over_budget = || deadline.is_some_and(|d| Instant::now() >= d);
    let mut clauses: HashMap<u64, Vec<i32>> = HashMap::with_capacity(cnf.clauses.len());
    for (i, clause) in cnf.clauses.iter().enumerate() {
        if clause.iter().any(|&l| l == 0 || l.unsigned_abs() > MAX_VAR) {
            return Err(DratError::Parse {
                line: 0,
                message: "the formula has an invalid literal".into(),
            });
        }
        if clause.is_empty() {
            return Ok(());
        }
        clauses.insert(i as u64 + 1, clause.clone());
    }
    let mut parser = Parser {
        bytes: proof,
        pos: 0,
        line: 1,
        binary: is_binary(proof),
    };
    let mut checker = Hints::new(cnf.vars);
    let mut lemmas = 0;
    let mut steps = 0u32;
    while let Some((step, line)) = next_step(&mut parser)? {
        steps = steps.wrapping_add(1);
        if steps.is_multiple_of(64) && over_budget() {
            return Err(DratError::BudgetExceeded);
        }
        match step {
            Step::Delete(ids) => {
                for id in ids {
                    clauses.remove(&id);
                }
            }
            Step::Add { id, lits, hints } => {
                lemmas += 1;
                if clauses.contains_key(&id) {
                    return Err(DratError::Parse {
                        line,
                        message: format!("clause {id} exists already"),
                    });
                }
                if !checker.implied(&clauses, &lits, &hints) {
                    return Err(DratError::NotRedundant {
                        lemma: lemmas,
                        line,
                    });
                }
                if lits.is_empty() {
                    return Ok(());
                }
                clauses.insert(id, lits);
            }
        }
    }
    Err(DratError::NoEmptyClause)
}

/// Unit propagation along hints, over an assignment that is reset after every lemma.
struct Hints {
    /// Per variable: 1 true, -1 false, 0 unassigned.
    values: Vec<i8>,
    assigned: Vec<usize>,
}

impl Hints {
    fn new(vars: u32) -> Self {
        Self {
            values: vec![0; vars.min(MAX_VAR) as usize + 1],
            assigned: Vec::new(),
        }
    }

    fn value(&self, l: i32) -> i8 {
        let v = self
            .values
            .get(l.unsigned_abs() as usize)
            .copied()
            .unwrap_or(0);
        if l < 0 { -v } else { v }
    }

    /// Makes `l` true; false if it was false (a conflict).
    fn assign(&mut self, l: i32) -> bool {
        match self.value(l) {
            1 => true,
            -1 => false,
            _ => {
                let v = l.unsigned_abs() as usize;
                if v >= self.values.len() {
                    self.values.resize(v + 1, 0);
                }
                self.values[v] = if l < 0 { -1 } else { 1 };
                self.assigned.push(v);
                true
            }
        }
    }

    fn reset(&mut self, to: usize) {
        for v in self.assigned.drain(to..) {
            self.values[v] = 0;
        }
    }

    /// Follows `hints` (positive clause ids); `Some(true)` on a conflict, `Some(false)` if they
    /// end without one, `None` if a hint is missing or isn't unit.
    fn propagate(&mut self, clauses: &HashMap<u64, Vec<i32>>, hints: &[i64]) -> Option<bool> {
        for &hint in hints {
            let clause = clauses.get(&hint.unsigned_abs())?;
            let mut open = None;
            for &l in clause {
                if self.value(l) != -1 {
                    if open.is_some() {
                        return None;
                    }
                    open = Some(l);
                }
            }
            match open {
                None => return Some(true),
                Some(l) => {
                    self.assign(l);
                }
            }
        }
        Some(false)
    }

    /// Whether `lits` follows from `clauses` with `hints`: by unit propagation (RUP), or as RAT
    /// on its first literal.
    fn implied(&mut self, clauses: &HashMap<u64, Vec<i32>>, lits: &[i32], hints: &[i64]) -> bool {
        let result = self.check(clauses, lits, hints);
        self.reset(0);
        result
    }

    fn check(&mut self, clauses: &HashMap<u64, Vec<i32>>, lits: &[i32], hints: &[i64]) -> bool {
        // Assume the lemma false; a tautology is implied by anything
        if !lits.iter().all(|&l| self.assign(-l)) {
            return true;
        }
        let rup = hints.iter().position(|&h| h < 0).unwrap_or(hints.len());
        match self.propagate(clauses, &hints[..rup]) {
            Some(true) => return true,
            None => return false,
            Some(false) => {}
        }
        // RAT on the first literal: every clause with its negation needs a group of hints
        let Some(&pivot) = lits.first() else {
            return false;
        };
        let mut groups: HashMap<u64, &[i64]> = HashMap::new();
        let mut rest = &hints[rup..];
        while let Some((&head, tail)) = rest.split_first() {
            let end = tail.iter().position(|&h| h < 0).unwrap_or(tail.len());
            groups.insert(head.unsigned_abs(), &tail[..end]);
            rest = &tail[end..];
        }
        let base = self.assigned.len();
        // In id order, so the outcome doesn't depend on the order of a hash map
        let mut candidates: Vec<(&u64, &Vec<i32>)> = clauses
            .iter()
            .filter(|(_, c)| c.contains(&-pivot))
            .collect();
        candidates.sort_unstable_by_key(|(id, _)| **id);
        for (id, clause) in candidates {
            // The resolvent is true already when the clause has a literal true now
            let blocked = clause.iter().any(|&l| l != -pivot && self.value(l) == 1);
            if !blocked {
                let refuted = clause
                    .iter()
                    .filter(|&&l| l != -pivot)
                    .all(|&l| self.assign(-l))
                    .then(|| {
                        let hints = groups.get(id).copied().unwrap_or_default();
                        self.propagate(clauses, hints)
                    });
                if !matches!(refuted, None | Some(Some(true))) {
                    return false;
                }
            }
            self.reset(base);
        }
        true
    }
}

/// Reads the next step: `None` at the end of the proof.
fn next_step(parser: &mut Parser<'_>) -> Result<Option<(Step, usize)>, DratError> {
    if parser.binary {
        binary_step(parser)
    } else {
        text_step(parser)
    }
}

fn binary_step(parser: &mut Parser<'_>) -> Result<Option<(Step, usize)>, DratError> {
    let Some(&kind) = parser.bytes.get(parser.pos) else {
        return Ok(None);
    };
    parser.pos += 1;
    let line = parser.line;
    parser.line += 1;
    let number = |parser: &mut Parser<'_>| {
        read_varint(parser.bytes, &mut parser.pos)
            .ok_or_else(|| parser.error("the last step doesn't end with 0"))
    };
    // A clause id: positive, as `2 id`
    let id = |parser: &Parser<'_>, n: u64| {
        if n & 1 == 1 || n == 0 {
            Err(parser.error("expected a clause id"))
        } else {
            Ok(n >> 1)
        }
    };
    let signed = |n: u64| {
        let value = i64::try_from(n >> 1).unwrap_or(i64::MAX);
        if n & 1 == 1 { -value } else { value }
    };
    match kind {
        b'a' => {
            let first = number(parser)?;
            let id = id(parser, first)?;
            let mut lits = Vec::new();
            loop {
                match number(parser)? {
                    0 => break,
                    n => lits.push(parser.binary_literal(n)?),
                }
            }
            let mut hints = Vec::new();
            loop {
                match number(parser)? {
                    0 => break,
                    n => hints.push(signed(n)),
                }
            }
            Ok(Some((Step::Add { id, lits, hints }, line)))
        }
        b'd' => {
            let mut ids = Vec::new();
            loop {
                match number(parser)? {
                    0 => break,
                    n => ids.push(id(parser, n)?),
                }
            }
            Ok(Some((Step::Delete(ids), line)))
        }
        _ => Err(parser.error("a step starts with neither a nor d")),
    }
}

fn text_step(parser: &mut Parser<'_>) -> Result<Option<(Step, usize)>, DratError> {
    loop {
        let start = parser.pos;
        let Some(len) = parser.bytes[start..].iter().position(|&b| b == b'\n') else {
            if start == parser.bytes.len() {
                return Ok(None);
            }
            parser.pos = parser.bytes.len();
            return parse_line(parser, &parser.bytes[start..]);
        };
        parser.pos = start + len + 1;
        let step = parse_line(parser, &parser.bytes[start..start + len])?;
        parser.line += 1;
        if step.is_some() {
            return Ok(step);
        }
    }
}

/// One line of a text proof: `None` for blank lines and comments.
fn parse_line(parser: &Parser<'_>, line: &[u8]) -> Result<Option<(Step, usize)>, DratError> {
    let text = std::str::from_utf8(line).map_err(|_| parser.error("not text"))?;
    let mut tokens = text.split_ascii_whitespace();
    let Some(first) = tokens.next() else {
        return Ok(None);
    };
    if first.starts_with('c') {
        return Ok(None);
    }
    let bad = || parser.error("expected a number");
    let id: u64 = first.parse().map_err(|_| bad())?;
    let mut tokens = tokens.peekable();
    if tokens.peek() == Some(&"d") {
        tokens.next();
        let mut ids = Vec::new();
        for token in tokens.by_ref() {
            match token.parse::<u64>().map_err(|_| bad())? {
                0 => return Ok(Some((Step::Delete(ids), parser.line))),
                n => ids.push(n),
            }
        }
        return Err(parser.error("the deletion doesn't end with 0"));
    }
    if id == 0 {
        return Err(parser.error("clause ids start at 1"));
    }
    let mut lits = Vec::new();
    loop {
        let token = tokens
            .next()
            .ok_or_else(|| parser.error("the clause doesn't end with 0"))?;
        let lit: i32 = token.parse().map_err(|_| bad())?;
        if lit == 0 {
            break;
        }
        if lit.unsigned_abs() > MAX_VAR {
            return Err(parser.error("variable out of range"));
        }
        lits.push(lit);
    }
    let mut hints = Vec::new();
    loop {
        let token = tokens
            .next()
            .ok_or_else(|| parser.error("the hints don't end with 0"))?;
        match token.parse::<i64>().map_err(|_| bad())? {
            0 => break,
            h => hints.push(h),
        }
    }
    if tokens.next().is_some() {
        return Err(parser.error("more after the final 0"));
    }
    Ok(Some((Step::Add { id, lits, hints }, parser.line)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::drat::write_varint;

    const BUDGET: Duration = Duration::from_secs(60);

    /// All eight clauses over three variables (ids 1 to 8, see `drat::tests::all_eight`).
    fn all_eight() -> Cnf {
        let clauses = (0..8)
            .map(|signs: i32| {
                (1..=3)
                    .map(|v| if signs >> (v - 1) & 1 == 1 { -v } else { v })
                    .collect()
            })
            .collect();
        Cnf::new(3, clauses)
    }

    /// What drat-trim -L writes for the DRAT proof `1 2 0, d 3 2 1 0, 1 0, 2 0, 0`.
    const PROOF: &str = "8 d 0\n9 1 2 0 1 5 0\n9 d 1 5 0\n10 1 0 9 7 3 0\n10 d 9 7 3 0\n\
                         11 2 0 10 2 6 0\n11 d 2 6 0\n13 0 10 11 4 8 0\n";

    /// A text LRAT proof in binary.
    pub(crate) fn to_binary(text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for line in text.lines() {
            let numbers: Vec<&str> = line.split_whitespace().collect();
            if numbers.get(1) == Some(&"d") {
                out.push(b'd');
                for n in &numbers[2..] {
                    write_varint(&mut out, n.parse().unwrap());
                }
            } else {
                out.push(b'a');
                for n in &numbers {
                    write_varint(&mut out, n.parse().unwrap());
                }
            }
        }
        out
    }

    #[test]
    fn valid_proofs_are_accepted() {
        assert_eq!(check(&all_eight(), PROOF.as_bytes(), BUDGET), Ok(()));
        let binary = to_binary(PROOF);
        assert!(is_binary(&binary));
        assert_eq!(check(&all_eight(), &binary, BUDGET), Ok(()));
        // Comments, blank lines and no final line break
        let commented = format!("c by hand\n\n{}", PROOF.trim_end());
        assert_eq!(check(&all_eight(), commented.as_bytes(), BUDGET), Ok(()));
    }

    #[test]
    fn wrong_hints_are_rejected() {
        let cnf = all_eight();
        // Clause 5 isn't unit after clause 2 here: 1 2 isn't implied by these hints
        let proof = "9 1 2 0 2 5 0\n";
        assert_eq!(
            check(&cnf, proof.as_bytes(), BUDGET),
            Err(DratError::NotRedundant { lemma: 1, line: 1 })
        );
        // Hints that end without a conflict
        assert_eq!(
            check(&cnf, b"9 1 2 0 1 0\n", BUDGET),
            Err(DratError::NotRedundant { lemma: 1, line: 1 })
        );
        // A hint to a deleted clause
        let deleted = "9 d 1 0\n9 1 2 0 1 5 0\n";
        assert_eq!(
            check(&cnf, deleted.as_bytes(), BUDGET),
            Err(DratError::NotRedundant { lemma: 1, line: 2 })
        );
        // No empty clause in the end
        let partial: String = PROOF.lines().take(4).flat_map(|l| [l, "\n"]).collect();
        assert_eq!(
            check(&cnf, partial.as_bytes(), BUDGET),
            Err(DratError::NoEmptyClause)
        );
        // The same proof for a weaker formula
        let mut weaker = cnf.clone();
        weaker.clauses[4] = vec![1, 2, -3, 4];
        assert!(check(&weaker, PROOF.as_bytes(), BUDGET).is_err());
        // An id twice
        assert!(matches!(
            check(&cnf, b"3 1 2 0 1 5 0\n", BUDGET),
            Err(DratError::Parse { .. })
        ));
    }

    #[test]
    fn rat_lemmas_need_a_group_per_clause() {
        // 4 is new: 4 -1 is RAT on 4 (no clause has -4), then -4 1 is RAT on -4: its only
        // resolvent, with 4 -1, is a tautology
        let cnf = Cnf::new(2, vec![vec![1, 2], vec![-1, 2], vec![1, -2], vec![-1, -2]]);
        let proof = "5 4 -1 0 0\n6 -4 1 0 0\n7 1 0 1 3 0\n8 0 7 2 4 0\n";
        assert_eq!(check(&cnf, proof.as_bytes(), BUDGET), Ok(()));
        // 1 isn't RUP but RAT: its resolvents with -1 2 and -1 -2 are refuted by clause 1 and
        // 3, and each needs its own group
        let rat = "5 1 0 -2 1 -4 3 0\n6 0 5 2 4 0\n";
        assert_eq!(check(&cnf, rat.as_bytes(), BUDGET), Ok(()));
        assert_eq!(check(&cnf, &to_binary(rat), BUDGET), Ok(()));
        let missing = "5 1 0 -2 1 0\n6 0 5 2 4 0\n";
        assert_eq!(
            check(&cnf, missing.as_bytes(), BUDGET),
            Err(DratError::NotRedundant { lemma: 1, line: 1 })
        );
        // -1 isn't RAT: its resolvents with 1 2 and 1 -2 have no hints
        assert_eq!(
            check(&cnf, b"5 -1 0 0\n", BUDGET),
            Err(DratError::NotRedundant { lemma: 1, line: 1 })
        );
        // The empty clause can't be RAT
        assert!(check(&cnf, b"5 0 0\n", BUDGET).is_err());
    }

    #[test]
    fn malformed_proofs_are_rejected() {
        let cnf = all_eight();
        for proof in [
            &b"9 1 2 0 1 5"[..],
            b"9 1 2",
            b"9 x 0 0",
            b"0 1 0 0",
            b"9 1 2 0 1 5 0 7",
            b"9 d 1",
            b"9 99999999999 0 0",
            b"a\x13",
            b"a\x12\x02\x00\x02",
            b"d\x03\x00",
            b"x\x00",
            b"\xff\xfe",
        ] {
            assert!(
                matches!(check(&cnf, proof, BUDGET), Err(DratError::Parse { .. })),
                "{proof:?}"
            );
        }
    }

    #[test]
    fn trivial_and_slow_cases() {
        // A formula with the empty clause needs no proof
        assert_eq!(
            check(&Cnf::new(1, vec![vec![1], vec![]]), b"", BUDGET),
            Ok(())
        );
        assert!(check(&Cnf::new(1, vec![vec![0]]), b"", BUDGET).is_err());
        // Tautologies are implied by anything, but a hint with two open literals is wrong
        assert_eq!(
            check(
                &all_eight(),
                b"9 1 -1 0 0\n10 0 1 2 3 4 5 6 7 8 0\n",
                BUDGET
            ),
            Err(DratError::NotRedundant { lemma: 2, line: 2 })
        );
        let steps = (0..200).fold(String::new(), |mut s, i| {
            use std::fmt::Write as _;
            let _ = writeln!(s, "{} d 0", i + 9);
            s
        });
        assert_eq!(
            check(&all_eight(), steps.as_bytes(), Duration::ZERO),
            Err(DratError::BudgetExceeded)
        );
    }

    /// Negated decision prefixes (the starter's proofs), with hints written by hand. LRAT from
    /// real solvers is tested in `proof.rs`, with drat-trim when it is installed.
    #[test]
    fn decision_prefixes_with_hints() {
        // (1 ∨ 2), (¬1 ∨ 2), (1 ∨ ¬2), (¬1 ∨ ¬2): deciding 1 conflicts via 2 and 4, deciding
        // -1 via 1 and 3
        let cnf = Cnf::new(2, vec![vec![1, 2], vec![-1, 2], vec![1, -2], vec![-1, -2]]);
        let proof = "5 -1 0 2 4 0\n6 0 5 1 3 0\n";
        assert_eq!(check(&cnf, proof.as_bytes(), BUDGET), Ok(()));
        assert_eq!(check(&cnf, &to_binary(proof), BUDGET), Ok(()));
    }
}
