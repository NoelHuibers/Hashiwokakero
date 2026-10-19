//! Checking DRAT proofs of unsatisfiability.
//!
//! A DRAT proof is a list of steps: lemmas (clauses to add) and deletions (`d` in front). It
//! proves the formula unsatisfiable if every lemma is redundant when it is added and the empty
//! clause is derived in the end:
//!
//! - RUP (reverse unit propagation): assigning the negation of the lemma and propagating units
//!   gives a conflict, so the lemma follows from the clauses so far.
//! - RAT (resolution asymmetric tautology) on the lemma's first literal `p`: for every clause
//!   `D` containing `¬p`, the resolvent `C ∪ (D \ {¬p})` is RUP. Adding such a lemma keeps the
//!   formula satisfiable if it was before (it may introduce new variables).
//!
//! Checking runs forward over the proof, every lemma is checked (no backward trimming), with a
//! two-watched-literal propagation engine. The top-level assignment (what follows from the
//! clauses by unit propagation alone) is kept between steps; a lemma is checked by assigning its
//! negation on top of it and undoing that afterwards. As soon as the top level runs into a
//! conflict the empty clause follows by RUP and the proof is done; an explicit empty lemma
//! (`0`) is checked the same way.
//!
//! Soundness: every clause the checker holds is either an original clause, a RUP lemma (implied
//! by the clauses before it) or a RAT lemma (checked against all clauses held at that moment),
//! and deleting clauses only makes the set weaker. So if the clauses held in the end are
//! unsatisfiable, so is the formula. A deletion that is ignored (see [`Checker::delete`]) keeps
//! more clauses than the proof asks for, which can't make the argument wrong either: the RAT
//! check always looks at every clause actually held.
//!
//! The format is textual DRAT (what splr writes): literals as decimal integers, each step ends
//! with `0`, deletions start with `d`, lines starting with `c` are comments. Steps may span
//! lines or share one, only the tokens matter.

use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

use crate::cnf::Cnf;

/// Why a proof was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DratError {
    /// The proof (or the formula) isn't well-formed; `line` counts from 1 (0 for the formula).
    Parse { line: usize, message: String },
    /// Lemma number `lemma` (counting only additions, from 1), starting on `line`, is neither
    /// RUP nor RAT.
    NotRedundant { lemma: usize, line: usize },
    /// The proof ended without deriving the empty clause.
    NoEmptyClause,
    /// Checking took longer than the budget.
    BudgetExceeded,
}

impl fmt::Display for DratError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { line, message } => write!(f, "proof line {line}: {message}"),
            Self::NotRedundant { lemma, line } => {
                write!(f, "lemma {lemma} (line {line}) is neither RUP nor RAT")
            }
            Self::NoEmptyClause => write!(f, "the proof doesn't derive the empty clause"),
            Self::BudgetExceeded => write!(f, "checking the proof took too long"),
        }
    }
}

impl std::error::Error for DratError {}

/// Variables beyond this are rejected, so a corrupt proof can't make us allocate gigabytes.
const MAX_VAR: u32 = 1 << 20;

/// Checks that `proof` (textual DRAT) shows `cnf` unsatisfiable, within `budget`.
pub fn check(cnf: &Cnf, proof: &[u8], budget: Duration) -> Result<(), DratError> {
    let deadline = Instant::now().checked_add(budget);
    let over_budget = || deadline.is_some_and(|d| Instant::now() >= d);
    let mut checker = Checker::new(cnf.vars);
    let mut lits = Vec::new();
    for clause in &cnf.clauses {
        if clause.iter().any(|&l| l == 0 || l.unsigned_abs() > MAX_VAR) {
            return Err(DratError::Parse {
                line: 0,
                message: "the formula has an invalid literal".into(),
            });
        }
        checker.normalize(clause, &mut lits);
        checker.add(&lits);
        if checker.refuted {
            return Ok(());
        }
    }
    if over_budget() {
        return Err(DratError::BudgetExceeded);
    }
    let mut parser = Parser {
        bytes: proof,
        pos: 0,
        line: 1,
    };
    let mut raw = Vec::new();
    let mut lemmas = 0;
    let mut steps = 0u32;
    while let Some((delete, line)) = parser.step(&mut raw)? {
        steps = steps.wrapping_add(1);
        if steps.is_multiple_of(64) && over_budget() {
            return Err(DratError::BudgetExceeded);
        }
        checker.normalize(&raw, &mut lits);
        if delete {
            checker.delete(&lits);
            continue;
        }
        lemmas += 1;
        if !checker.rup(&lits) && !checker.rat(&lits, &over_budget)? {
            return Err(DratError::NotRedundant {
                lemma: lemmas,
                line,
            });
        }
        // Not empty: the empty lemma is RUP only if the top level conflicts, and then we have
        // stopped already
        checker.add(&lits);
        if checker.refuted {
            return Ok(());
        }
    }
    Err(DratError::NoEmptyClause)
}

/// Reads the proof one step at a time.
struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> DratError {
        DratError::Parse {
            line: self.line,
            message: message.into(),
        }
    }

    fn at_token_end(&self) -> bool {
        self.bytes.get(self.pos).is_none_or(u8::is_ascii_whitespace)
    }

    /// The next step's literals into `lits`: `Some((is deletion, line it starts on))`, `None`
    /// at the end of the proof.
    fn step(&mut self, lits: &mut Vec<i32>) -> Result<Option<(bool, usize)>, DratError> {
        lits.clear();
        let mut delete = false;
        let mut start = None;
        loop {
            while let Some(&b) = self.bytes.get(self.pos) {
                if !b.is_ascii_whitespace() {
                    break;
                }
                if b == b'\n' {
                    self.line += 1;
                }
                self.pos += 1;
            }
            let Some(&b) = self.bytes.get(self.pos) else {
                return match start {
                    None => Ok(None),
                    Some(_) => Err(self.error("the last step doesn't end with 0")),
                };
            };
            if start.is_none() && b == b'c' {
                while self.bytes.get(self.pos).is_some_and(|&b| b != b'\n') {
                    self.pos += 1;
                }
                continue;
            }
            if start.is_none() && b == b'd' {
                self.pos += 1;
                if !self.at_token_end() {
                    return Err(self.error("expected a space after d"));
                }
                delete = true;
                start = Some(self.line);
                continue;
            }
            let negative = b == b'-';
            if negative {
                self.pos += 1;
            }
            let mut value: u32 = 0;
            let mut digits = 0;
            while let Some(&b) = self.bytes.get(self.pos) {
                if !b.is_ascii_digit() {
                    break;
                }
                value = value * 10 + u32::from(b - b'0');
                if value > MAX_VAR {
                    return Err(self.error("variable too large"));
                }
                digits += 1;
                self.pos += 1;
            }
            if digits == 0 || !self.at_token_end() {
                return Err(self.error("expected a literal"));
            }
            let line = *start.get_or_insert(self.line);
            if value == 0 {
                return Ok(Some((delete, line)));
            }
            let value = i32::try_from(value).expect("MAX_VAR fits an i32");
            lits.push(if negative { -value } else { value });
        }
    }
}

/// A literal: `2·variable`, plus one if negative.
type Lit = u32;

/// No clause (the reason of a decision).
const NONE: u32 = u32::MAX;

struct Clause {
    start: usize,
    len: usize,
    active: bool,
}

/// A clause watching a literal; `blocker` is another of its literals (if that is true the
/// clause is satisfied and needn't be looked at).
#[derive(Clone, Copy)]
struct Watch {
    clause: u32,
    blocker: Lit,
}

struct Checker {
    /// The literals of all clauses, back to back. The first two of each clause (with two or
    /// more literals) are its watched ones.
    lits: Vec<Lit>,
    clauses: Vec<Clause>,
    /// Per literal: the clauses watching it, looked at when it becomes false. Watches of
    /// deleted clauses are removed lazily.
    watches: Vec<Vec<Watch>>,
    /// Per literal: 1 true, -1 false, 0 unassigned.
    values: Vec<i8>,
    /// Per variable: the clause that implied it (only meaningful while it is assigned).
    reasons: Vec<u32>,
    trail: Vec<Lit>,
    /// Trail position up to which literals are propagated.
    head: usize,
    /// Active clauses by a hash of their literal set, to find them for deletion.
    index: HashMap<u64, Vec<u32>>,
    /// Per literal, scratch space for deduplicating and comparing clauses.
    marks: Vec<bool>,
    /// Literals of deleted clauses still in `lits`.
    garbage: usize,
    /// The top level has a conflict: the clauses are unsatisfiable.
    refuted: bool,
}

const fn lit_of(l: i32) -> Lit {
    2 * l.unsigned_abs() + if l < 0 { 1 } else { 0 }
}

const fn var_of(l: Lit) -> usize {
    (l >> 1) as usize
}

/// Order independent, so a clause hashes the same whatever the order of its literals.
fn hash_clause(lits: &[Lit]) -> u64 {
    lits.iter().fold(lits.len() as u64, |h, &l| {
        // splitmix64's finaliser
        let mut x = u64::from(l).wrapping_add(0x9e37_79b9_7f4a_7c15);
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        h.wrapping_add(x ^ (x >> 31))
    })
}

impl Checker {
    fn new(vars: u32) -> Self {
        let mut checker = Self {
            lits: Vec::new(),
            clauses: Vec::new(),
            watches: Vec::new(),
            values: Vec::new(),
            reasons: Vec::new(),
            trail: Vec::new(),
            head: 0,
            index: HashMap::new(),
            marks: Vec::new(),
            garbage: 0,
            refuted: false,
        };
        checker.ensure_var(vars.min(MAX_VAR) as usize);
        checker
    }

    fn ensure_var(&mut self, var: usize) {
        let lits = 2 * var + 2;
        if self.values.len() < lits {
            self.values.resize(lits, 0);
            self.marks.resize(lits, false);
            self.watches.resize_with(lits, Vec::new);
            self.reasons.resize(var + 1, NONE);
        }
    }

    /// `clause` as literals into `out`, without duplicates (the first occurrence stays, so
    /// the RAT pivot stays first). Literals are within `MAX_VAR` (checked by the caller).
    fn normalize(&mut self, clause: &[i32], out: &mut Vec<Lit>) {
        out.clear();
        for &l in clause {
            let lit = lit_of(l);
            self.ensure_var(var_of(lit));
            if !self.marks[lit as usize] {
                self.marks[lit as usize] = true;
                out.push(lit);
            }
        }
        for &lit in out.iter() {
            self.marks[lit as usize] = false;
        }
    }

    fn value(&self, l: Lit) -> i8 {
        self.values[l as usize]
    }

    fn assign(&mut self, l: Lit, reason: u32) {
        self.values[l as usize] = 1;
        self.values[(l ^ 1) as usize] = -1;
        self.reasons[var_of(l)] = reason;
        self.trail.push(l);
    }

    fn backtrack(&mut self, len: usize) {
        for &l in &self.trail[len..] {
            self.values[l as usize] = 0;
            self.values[(l ^ 1) as usize] = 0;
        }
        self.trail.truncate(len);
        self.head = len;
    }

    fn clause_id(&self) -> u32 {
        u32::try_from(self.clauses.len()).expect("fewer than 4 billion clauses fit in memory")
    }

    /// Unit propagation of the unpropagated part of the trail; whether there is a conflict.
    fn propagate(&mut self) -> bool {
        while self.head < self.trail.len() {
            let falsified = self.trail[self.head] ^ 1;
            self.head += 1;
            let mut watches = std::mem::take(&mut self.watches[falsified as usize]);
            let (mut i, mut j) = (0, 0);
            let mut conflict = false;
            while i < watches.len() {
                let watch = watches[i];
                i += 1;
                if self.value(watch.blocker) == 1 {
                    watches[j] = watch;
                    j += 1;
                    continue;
                }
                let clause = &self.clauses[watch.clause as usize];
                if !clause.active {
                    continue;
                }
                let (start, len) = (clause.start, clause.len);
                if self.lits[start] == falsified {
                    self.lits.swap(start, start + 1);
                }
                debug_assert_eq!(self.lits[start + 1], falsified);
                let first = self.lits[start];
                let kept = Watch {
                    clause: watch.clause,
                    blocker: first,
                };
                if first != watch.blocker && self.value(first) == 1 {
                    watches[j] = kept;
                    j += 1;
                    continue;
                }
                // Another literal that isn't false takes over the watch
                if let Some(k) = (start + 2..start + len).find(|&k| self.value(self.lits[k]) != -1)
                {
                    self.lits.swap(start + 1, k);
                    // Not `falsified` (that one is false), so not the list taken out above
                    self.watches[self.lits[start + 1] as usize].push(kept);
                    continue;
                }
                watches[j] = kept;
                j += 1;
                match self.value(first) {
                    -1 => {
                        conflict = true;
                        break;
                    }
                    0 => self.assign(first, watch.clause),
                    _ => {}
                }
            }
            if conflict {
                watches.copy_within(i.., j);
                j += watches.len() - i;
            }
            watches.truncate(j);
            self.watches[falsified as usize] = watches;
            if conflict {
                return true;
            }
        }
        false
    }

    /// Adds a clause (duplicate-free) at the top level and propagates what it implies there.
    fn add(&mut self, lits: &[Lit]) {
        debug_assert_eq!(self.head, self.trail.len());
        if lits.is_empty() {
            self.refuted = true;
            return;
        }
        let id = self.clause_id();
        let start = self.lits.len();
        self.lits.extend_from_slice(lits);
        self.clauses.push(Clause {
            start,
            len: lits.len(),
            active: true,
        });
        self.index.entry(hash_clause(lits)).or_default().push(id);
        if lits.len() == 1 {
            // Units aren't watched: the top level is never undone, so they are assigned for good
            match self.value(lits[0]) {
                0 => {
                    self.assign(lits[0], id);
                    self.refuted = self.propagate();
                }
                -1 => self.refuted = true,
                _ => {}
            }
            return;
        }
        // Watch the best two literals: true before unassigned before false
        let rank = |value: i8| match value {
            1 => 2,
            0 => 1,
            _ => 0,
        };
        for pos in 0..2 {
            let best = (start + pos..start + lits.len())
                .max_by_key(|&k| rank(self.value(self.lits[k])))
                .expect("at least two literals");
            self.lits.swap(start + pos, best);
        }
        let (first, second) = (self.lits[start], self.lits[start + 1]);
        self.watches[first as usize].push(Watch {
            clause: id,
            blocker: second,
        });
        self.watches[second as usize].push(Watch {
            clause: id,
            blocker: first,
        });
        match (self.value(first), self.value(second)) {
            // All false
            (-1, _) => self.refuted = true,
            // Unit
            (0, -1) => {
                self.assign(first, id);
                self.refuted = self.propagate();
            }
            // Satisfied, or two unassigned watches
            _ => {}
        }
    }

    /// Whether the clause is RUP: its negation propagates to a conflict.
    fn rup(&mut self, lits: &[Lit]) -> bool {
        debug_assert!(!self.refuted && self.head == self.trail.len());
        let top = self.trail.len();
        let mut conflict = false;
        for &l in lits {
            match self.value(l) {
                0 => self.assign(l ^ 1, NONE),
                // True (at the top level, or the lemma is a tautology): trivially implied
                1 => {
                    conflict = true;
                    break;
                }
                _ => {}
            }
        }
        let conflict = conflict || self.propagate();
        self.backtrack(top);
        conflict
    }

    /// Whether the clause is RAT on its first literal: every resolvent with an active clause
    /// containing the negated pivot is RUP.
    fn rat(&mut self, lits: &[Lit], over_budget: &dyn Fn() -> bool) -> Result<bool, DratError> {
        let Some(&pivot) = lits.first() else {
            return Ok(false);
        };
        let negated = pivot ^ 1;
        let mut resolvent = Vec::new();
        for id in 0..self.clauses.len() {
            let clause = &self.clauses[id];
            let other = &self.lits[clause.start..clause.start + clause.len];
            if !clause.active || !other.contains(&negated) {
                continue;
            }
            resolvent.clear();
            resolvent.extend_from_slice(lits);
            resolvent.extend(other.iter().filter(|&&l| l != negated));
            if !self.rup(&resolvent) {
                return Ok(false);
            }
            if id % 64 == 63 && over_budget() {
                return Err(DratError::BudgetExceeded);
            }
        }
        Ok(true)
    }

    /// Whether the clause implied one of the current (top-level) assignments.
    fn is_reason(&self, id: u32) -> bool {
        let clause = &self.clauses[id as usize];
        self.lits[clause.start..clause.start + clause.len]
            .iter()
            .any(|&l| self.value(l) == 1 && self.reasons[var_of(l)] == id)
    }

    /// Deletes one copy of the clause (duplicate-free, any order). Like drat-trim, deleting a
    /// clause that is the reason for a top-level assignment (a unit clause, or a clause that
    /// became unit) is ignored: the top level would have to be recomputed without it, and
    /// solvers delete such clauses while keeping what they implied. Keeping a clause can't make
    /// the check unsound (see the module docs). Deleting a clause that isn't there is ignored
    /// as well.
    fn delete(&mut self, lits: &[Lit]) {
        let hash = hash_clause(lits);
        let Some(bucket) = self.index.get(&hash) else {
            return;
        };
        for &l in lits {
            self.marks[l as usize] = true;
        }
        let found = bucket.iter().position(|&id| {
            let clause = &self.clauses[id as usize];
            clause.len == lits.len()
                && self.lits[clause.start..clause.start + clause.len]
                    .iter()
                    .all(|&l| self.marks[l as usize])
                && !self.is_reason(id)
        });
        for &l in lits {
            self.marks[l as usize] = false;
        }
        let Some(pos) = found else {
            return;
        };
        let bucket = self.index.get_mut(&hash).expect("looked up above");
        let id = bucket.swap_remove(pos) as usize;
        if bucket.is_empty() {
            self.index.remove(&hash);
        }
        self.clauses[id].active = false;
        self.garbage += self.clauses[id].len;
        if self.garbage > 1 << 16 && 2 * self.garbage > self.lits.len() {
            self.collect_garbage();
        }
    }

    /// Drops deleted clauses from memory, renumbering the rest.
    fn collect_garbage(&mut self) {
        let mut renamed = vec![NONE; self.clauses.len()];
        let mut lits = Vec::with_capacity(self.lits.len() - self.garbage);
        let mut clauses = Vec::with_capacity(self.clauses.len());
        for (id, clause) in self.clauses.iter().enumerate() {
            if clause.active {
                renamed[id] = u32::try_from(clauses.len()).expect("clause ids are u32");
                clauses.push(Clause {
                    start: lits.len(),
                    len: clause.len,
                    active: true,
                });
                lits.extend_from_slice(&self.lits[clause.start..clause.start + clause.len]);
            }
        }
        for watches in &mut self.watches {
            watches.retain_mut(|w| {
                w.clause = renamed[w.clause as usize];
                w.clause != NONE
            });
        }
        // Reasons of unassigned variables may point to deleted clauses, they become NONE
        for reason in &mut self.reasons {
            if *reason != NONE {
                *reason = renamed[*reason as usize];
            }
        }
        for bucket in self.index.values_mut() {
            for id in bucket {
                *id = renamed[*id as usize];
            }
        }
        self.lits = lits;
        self.clauses = clauses;
        self.garbage = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUDGET: Duration = Duration::from_secs(60);

    /// All eight clauses over three variables: unsatisfiable, but not by unit propagation.
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

    /// Five pigeons in four holes.
    fn pigeonhole() -> Cnf {
        crate::bench::pigeonhole(4, false).0
    }

    #[test]
    fn valid_proof_is_accepted() {
        let proof = b"c a comment\n1 2 0\nd 3 2 1 0\n1 0 2 0\n0\n";
        assert_eq!(check(&all_eight(), proof, BUDGET), Ok(()));
    }

    #[test]
    fn bogus_lemma_is_rejected() {
        // -1 contradicts the 1 derived just before it, it is neither RUP nor RAT
        let proof = b"1 2 0\n1 0\n-1 0\n2 0\n0\n";
        assert_eq!(
            check(&all_eight(), proof, BUDGET),
            Err(DratError::NotRedundant { lemma: 3, line: 3 })
        );
        // The empty clause right away
        assert_eq!(
            check(&all_eight(), b"0\n", BUDGET),
            Err(DratError::NotRedundant { lemma: 1, line: 1 })
        );
    }

    #[test]
    fn deletions_take_effect() {
        // Without 1 2 -3, the lemma 1 2 is neither RUP nor RAT any more
        let proof = b"d -3 1 2 0\n1 2 0\n1 0\n2 0\n0\n";
        assert_eq!(
            check(&all_eight(), proof, BUDGET),
            Err(DratError::NotRedundant { lemma: 1, line: 2 })
        );
    }

    #[test]
    fn missing_empty_clause_is_rejected() {
        assert_eq!(
            check(&all_eight(), b"1 2 0\n", BUDGET),
            Err(DratError::NoEmptyClause)
        );
        assert_eq!(
            check(&all_eight(), b"", BUDGET),
            Err(DratError::NoEmptyClause)
        );
        // A satisfiable formula has no proof
        let sat = Cnf::new(3, vec![vec![1, 2], vec![-1, 3]]);
        assert!(check(&sat, b"1 0\n0\n", BUDGET).is_err());
        assert!(check(&sat, b"-2 0\n0\n", BUDGET).is_err());
    }

    #[test]
    fn rat_lemmas_are_accepted() {
        // 1 isn't RUP, but every resolvent on 1 is a clause of the formula
        let cnf = all_eight();
        let mut checker = Checker::new(3);
        let mut lits = Vec::new();
        for clause in &cnf.clauses {
            checker.normalize(clause, &mut lits);
            checker.add(&lits);
        }
        assert!(!checker.rup(&[lit_of(1)]));
        assert_eq!(check(&cnf, b"1 0\n2 0\n0\n", BUDGET), Ok(()));
        // Extended resolution: 4 is a new variable defined as 1 and 2 (-4 1 and -4 2 are RAT on
        // -4, not RUP), then the proof from above
        let proof = b"4 -1 -2 0\n-4 1 0\n-4 2 0\n1 2 0\n1 0\n2 0\n0\n";
        assert_eq!(check(&cnf, proof, BUDGET), Ok(()));
    }

    #[test]
    fn rat_is_on_the_first_literal() {
        // 3 -1 is RAT on the new variable 3, but -1 3 isn't RAT on -1 (its resolvent with 1 2
        // is not RUP)
        let cnf = Cnf::new(2, vec![vec![1, 2]]);
        assert_eq!(
            check(&cnf, b"3 -1 0\n", BUDGET),
            Err(DratError::NoEmptyClause)
        );
        assert_eq!(
            check(&cnf, b"-1 3 0\n", BUDGET),
            Err(DratError::NotRedundant { lemma: 1, line: 1 })
        );
    }

    #[test]
    fn unit_propagation_alone_needs_no_proof() {
        let cnf = Cnf::new(2, vec![vec![1], vec![-1, 2], vec![-2]]);
        assert_eq!(check(&cnf, b"", BUDGET), Ok(()));
        let cnf = Cnf::new(2, vec![vec![1, 2], vec![]]);
        assert_eq!(check(&cnf, b"", BUDGET), Ok(()));
    }

    #[test]
    fn malformed_proofs_are_rejected() {
        let cnf = all_eight();
        for proof in [
            &b"1 2"[..],
            b"1 x 0",
            b"1 2 0\n2d 0",
            b"- 0",
            b"99999999999 0",
        ] {
            assert!(
                matches!(check(&cnf, proof, BUDGET), Err(DratError::Parse { .. })),
                "{}",
                String::from_utf8_lossy(proof)
            );
        }
    }

    #[test]
    fn budget_is_enforced() {
        assert_eq!(
            check(&pigeonhole(), b"0\n", Duration::ZERO),
            Err(DratError::BudgetExceeded)
        );
    }

    /// Found by the property tests (tests/props.rs): splr refutes these while loading them and
    /// reports an error, which counted as "no proof" though unit propagation alone refutes them.
    #[test]
    fn formulas_splr_refutes_on_loading_are_proven() {
        for clauses in [
            vec![vec![1], vec![-1]],
            vec![vec![1, 1, 1], vec![-1, -1, -1], vec![2, 3]],
        ] {
            let cnf = Cnf::new(3, clauses);
            let proof = crate::bench::splr_proof(&cnf).expect("unsatisfiable");
            assert_eq!(check(&cnf, &proof, BUDGET), Ok(()));
        }
        // Satisfiable ones still have none
        assert_eq!(
            crate::bench::splr_proof(&Cnf::new(1, vec![vec![1, 1]])),
            None
        );
    }

    /// A proof splr writes for PHP(5,4), checked, and broken versions of it rejected.
    #[test]
    fn splr_proofs_are_checked() {
        let cnf = pigeonhole();
        let proof = crate::bench::splr_proof(&cnf).expect("unsatisfiable");
        assert_eq!(check(&cnf, &proof, BUDGET), Ok(()));
        let text = String::from_utf8(proof).expect("text DRAT");
        let lemmas = text.lines().filter(|l| !l.starts_with('d')).count();
        assert!(lemmas > 10, "{lemmas} lemmas");

        // The same proof for the formula without one clause, which is satisfiable
        let mut weaker = cnf.clone();
        weaker.clauses.remove(0);
        assert!(check(&weaker, text.as_bytes(), BUDGET).is_err());

        // A wrong lemma slipped in after the first one
        let mut lines: Vec<&str> = text.lines().collect();
        let first = lines
            .iter()
            .position(|l| !l.starts_with('d'))
            .expect("a lemma");
        lines.insert(first + 1, "-1 0");
        let broken = lines.join("\n");
        assert!(matches!(
            check(&cnf, broken.as_bytes(), BUDGET),
            Err(DratError::NotRedundant { .. })
        ));

        // Only the deletions and the final empty clause
        let hollow: String = text
            .lines()
            .filter(|l| l.starts_with('d'))
            .map(|l| format!("{l}\n"))
            .chain(["0\n".to_string()])
            .collect();
        assert!(check(&cnf, hollow.as_bytes(), BUDGET).is_err());
    }

    /// Small random 3-SAT formulas above the threshold: every proof splr gives is accepted.
    #[test]
    fn random_proofs_are_checked() {
        use rand::rngs::StdRng;
        use rand::{RngExt, SeedableRng};
        let mut rng = StdRng::seed_from_u64(1);
        let mut proven = 0;
        for _ in 0..20 {
            let clauses = (0..250)
                .map(|_| {
                    let mut vars: Vec<i32> = Vec::new();
                    while vars.len() < 3 {
                        let v = rng.random_range(1..=50);
                        if !vars.contains(&v) {
                            vars.push(v);
                        }
                    }
                    vars.into_iter()
                        .map(|v| if rng.random_bool(0.5) { v } else { -v })
                        .collect()
                })
                .collect();
            let cnf = Cnf::new(50, clauses);
            if let Some(proof) = crate::bench::splr_proof(&cnf) {
                assert_eq!(check(&cnf, &proof, BUDGET), Ok(()));
                proven += 1;
            }
        }
        assert!(proven >= 10, "only {proven} unsatisfiable");
    }
}
