//! CNF formulas: DIMACS text, checking models and scrambling.

use rand::{Rng, RngExt};
use rand::seq::SliceRandom;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Cnf {
    pub vars: u32,
    pub clauses: Vec<Vec<i32>>,
}

impl Cnf {
    pub fn new(vars: u32, clauses: Vec<Vec<i32>>) -> Self {
        Cnf { vars, clauses }
    }

    pub fn dimacs(&self, comment: &str) -> String {
        let mut out = String::with_capacity(self.clauses.len() * 12);
        for line in comment.lines() {
            out.push_str("c ");
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(&format!("p cnf {} {}\n", self.vars, self.clauses.len()));
        for clause in &self.clauses {
            for lit in clause {
                out.push_str(&lit.to_string());
                out.push(' ');
            }
            out.push_str("0\n");
        }
        out
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
            c.iter().any(|&l| value[l.unsigned_abs() as usize] == if l > 0 { 1 } else { 2 })
        }))
    }

    /// Renames and negates variables at random and shuffles clauses and literals, so the
    /// formula can't be recognised by its layout. Returns the new formula and the literal
    /// that each old variable became, to carry a model over.
    pub fn scramble<R: Rng + ?Sized>(&self, rng: &mut R) -> (Cnf, Vec<i32>) {
        let mut names: Vec<i32> = (1..=self.vars as i32).collect();
        names.shuffle(rng);
        let mut map = vec![0i32; self.vars as usize + 1];
        for (old, new) in (1..=self.vars as usize).zip(names) {
            map[old] = if rng.random_bool(0.5) { new } else { -new };
        }
        let rename = |l: i32| if l > 0 { map[l as usize] } else { -map[(-l) as usize] };
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
        (Cnf::new(self.vars, clauses), map)
    }
}

/// A model (`model[v]` for v in 1..) as literals.
pub fn literals(model: &[bool]) -> Vec<i32> {
    (1..model.len()).map(|v| if model[v] { v as i32 } else { -(v as i32) }).collect()
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
            .map(|&l: &i32| if l > 0 { map[l as usize] } else { -map[(-l) as usize] })
            .collect();
        assert_eq!(scrambled.check(&model), Some(true));
        assert!(scrambled.dimacs("x").starts_with("c x\np cnf 3 3\n"));
    }
}
