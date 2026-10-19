//! CNF formulas: DIMACS text, checking models and scrambling.

use std::fmt::Write as _;

use rand::seq::SliceRandom;
use rand::{Rng, RngExt};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Cnf {
    pub vars: u32,
    pub clauses: Vec<Vec<i32>>,
}

impl Cnf {
    pub fn new(vars: u32, clauses: Vec<Vec<i32>>) -> Self {
        Self { vars, clauses }
    }

    pub fn dimacs(&self, comment: &str) -> String {
        let mut out = String::with_capacity(self.clauses.len() * 12);
        for line in comment.lines() {
            out.push_str("c ");
            out.push_str(line);
            out.push('\n');
        }
        let _ = writeln!(out, "p cnf {} {}", self.vars, self.clauses.len());
        for clause in &self.clauses {
            for lit in clause {
                out.push_str(&lit.to_string());
                out.push(' ');
            }
            out.push_str("0\n");
        }
        out
    }

    /// Reads DIMACS CNF, as [`Cnf::dimacs`] writes it: comments, the `p cnf` line, clauses ended
    /// by 0. The header's numbers have to match the clauses.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut header = None;
        let mut clauses = Vec::new();
        let mut clause = Vec::new();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('c') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("p cnf ") {
                let numbers: Vec<usize> = rest
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| format!("bad header: {line}"))?;
                let &[vars, count] = numbers.as_slice() else {
                    return Err(format!("bad header: {line}"));
                };
                let vars = u32::try_from(vars).map_err(|_| "too many variables".to_string())?;
                header = Some((vars, count));
                continue;
            }
            let (vars, _) = header.ok_or("clauses before the p cnf line")?;
            for token in line.split_whitespace() {
                let lit: i32 = token
                    .parse()
                    .map_err(|_| format!("not a literal: {token}"))?;
                if lit == 0 {
                    clauses.push(std::mem::take(&mut clause));
                } else if lit.unsigned_abs() > vars {
                    return Err(format!("literal {lit} beyond {vars} variables"));
                } else {
                    clause.push(lit);
                }
            }
        }
        let (vars, count) = header.ok_or("no p cnf line")?;
        if !clause.is_empty() {
            return Err("the last clause isn't ended by 0".into());
        }
        if clauses.len() != count {
            return Err(format!(
                "{} clauses, the header says {count}",
                clauses.len()
            ));
        }
        Ok(Self::new(vars, clauses))
    }

    /// Whether the literals (a full or partial assignment) satisfy every clause.
    /// `None` if they assign a variable both ways or name one that doesn't exist.
    pub fn check(&self, literals: &[i32]) -> Option<bool> {
        // 0 unassigned, 1 true, 2 false
        let mut value = vec![0u8; self.vars as usize + 1];
        for &lit in literals {
            let v = lit.unsigned_abs() as usize;
            if lit == 0 || v > self.vars as usize {
                return None;
            }
            let want = if lit > 0 { 1 } else { 2 };
            if value[v] != 0 && value[v] != want {
                return None;
            }
            value[v] = want;
        }
        Some(self.clauses.iter().all(|c| {
            c.iter()
                .any(|&l| value[l.unsigned_abs() as usize] == if l > 0 { 1 } else { 2 })
        }))
    }

    /// Renames and negates variables at random and shuffles clauses and literals, so the
    /// formula can't be recognised by its layout. Returns the new formula and the literal
    /// that each old variable became, to carry a model over.
    pub fn scramble<R: Rng + ?Sized>(&self, rng: &mut R) -> (Self, Vec<i32>) {
        let vars = i32::try_from(self.vars).expect("variables are i32 literals");
        let mut names: Vec<i32> = (1..=vars).collect();
        names.shuffle(rng);
        let mut map = vec![0i32; self.vars as usize + 1];
        for (old, new) in (1..=self.vars as usize).zip(names) {
            map[old] = if rng.random_bool(0.5) { new } else { -new };
        }
        let rename = |l: i32| {
            let renamed = map[l.unsigned_abs() as usize];
            if l > 0 { renamed } else { -renamed }
        };
        let mut clauses: Vec<Vec<i32>> = self
            .clauses
            .iter()
            .map(|c| {
                let mut c: Vec<i32> = c.iter().map(|&l| rename(l)).collect();
                c.shuffle(rng);
                c
            })
            .collect();
        clauses.shuffle(rng);
        (Self::new(self.vars, clauses), map)
    }
}

/// A model (`model[v]` for v in 1..) as literals.
pub fn literals(model: &[bool]) -> Vec<i32> {
    (1..model.len())
        .map(|v| {
            let l = i32::try_from(v).expect("variables are i32 literals");
            if model[v] { l } else { -l }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn check_models() {
        let cnf = Cnf::new(3, vec![vec![1, -2], vec![2, 3], vec![-1, -3]]);
        assert_eq!(cnf.check(&[1, 2, -3]), Some(true));
        assert_eq!(cnf.check(&[1, -2, 3]), Some(false));
        // Partial assignments are fine when they satisfy every clause
        assert_eq!(cnf.check(&[1, 2, -3]), Some(true));
        assert_eq!(cnf.check(&[1, 2]), Some(false));
        assert_eq!(cnf.check(&[1, -1]), None);
        assert_eq!(cnf.check(&[4]), None);
    }

    #[test]
    fn scrambled_models_carry_over() {
        let cnf = Cnf::new(3, vec![vec![1, -2], vec![2, 3], vec![-1, -3]]);
        let (scrambled, map) = cnf.scramble(&mut StdRng::seed_from_u64(7));
        let model: Vec<i32> = [1, 2, -3]
            .iter()
            .map(|&l: &i32| {
                let renamed = map[l.unsigned_abs() as usize];
                if l > 0 { renamed } else { -renamed }
            })
            .collect();
        assert_eq!(scrambled.check(&model), Some(true));
        assert!(scrambled.dimacs("x").starts_with("c x\np cnf 3 3\n"));
    }

    #[test]
    fn dimacs_round_trip() {
        let cnf = Cnf::new(3, vec![vec![1, -2], vec![2, 3], vec![-1, -3], vec![]]);
        assert_eq!(Cnf::parse(&cnf.dimacs("two\nlines")), Ok(cnf));
        // Clauses can span lines
        assert_eq!(
            Cnf::parse("p cnf 2 1\n1\n-2 0\n"),
            Ok(Cnf::new(2, vec![vec![1, -2]]))
        );
        for bad in [
            "",
            "1 0\np cnf 1 1\n",
            "p cnf 1\n",
            "p cnf x 1\n",
            "p cnf 1 1\n2 0\n",
            "p cnf 1 1\n1\n",
            "p cnf 1 2\n1 0\n",
            "p cnf 1 1\na 0\n",
        ] {
            assert!(Cnf::parse(bad).is_err(), "{bad:?}");
        }
    }
}
