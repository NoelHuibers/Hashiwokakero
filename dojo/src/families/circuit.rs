//! Boolean circuits as CNF (the Tseitin encoding), with multipliers for the factoring and miter
//! families.
//!
//! Every gate gets a variable defined by clauses that say it equals the gate's function of its
//! inputs. Constants ([`TRUE`], [`FALSE`]) are folded away as the circuit is built, so they
//! never appear in a clause. The circuit remembers how each variable is computed, so a model is
//! found by evaluating it on an input (see [`Circuit::eval`]).

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, RngExt};

use crate::cnf::Cnf;

/// The constant true, as a literal; [`FALSE`] is its negation.
pub const TRUE: i32 = i32::MAX;
pub const FALSE: i32 = -i32::MAX;

fn is_const(lit: i32) -> bool {
    lit.unsigned_abs() == TRUE.unsigned_abs()
}

/// How a variable is computed from earlier ones.
#[derive(Clone, Copy, Debug)]
enum Def {
    Input,
    And(i32, i32),
    Xor(i32, i32),
    Xor3(i32, i32, i32),
    Maj(i32, i32, i32),
}

#[derive(Clone, Debug, Default)]
pub struct Circuit {
    clauses: Vec<Vec<i32>>,
    /// `defs[v - 1]` computes variable `v`.
    defs: Vec<Def>,
}

impl Circuit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vars(&self) -> u32 {
        u32::try_from(self.defs.len()).expect("circuits have far fewer than 2^31 gates")
    }

    fn fresh(&mut self, def: Def) -> i32 {
        self.defs.push(def);
        i32::try_from(self.defs.len()).expect("circuits have far fewer than 2^31 gates")
    }

    pub fn input(&mut self) -> i32 {
        self.fresh(Def::Input)
    }

    pub fn inputs(&mut self, n: usize) -> Vec<i32> {
        (0..n).map(|_| self.input()).collect()
    }

    /// Adds a clause; false literals are dropped, a true one satisfies it.
    pub fn clause(&mut self, lits: &[i32]) {
        if lits.contains(&TRUE) {
            return;
        }
        let clause: Vec<i32> = lits.iter().copied().filter(|&l| l != FALSE).collect();
        self.clauses.push(clause);
    }

    /// Says that `lit` is true.
    pub fn assert(&mut self, lit: i32) {
        self.clause(&[lit]);
    }

    pub fn and(&mut self, a: i32, b: i32) -> i32 {
        match (a, b) {
            (FALSE, _) | (_, FALSE) => FALSE,
            (TRUE, x) | (x, TRUE) => x,
            _ if a == b => a,
            _ if a == -b => FALSE,
            _ => {
                let v = self.fresh(Def::And(a, b));
                self.clauses.push(vec![-v, a]);
                self.clauses.push(vec![-v, b]);
                self.clauses.push(vec![v, -a, -b]);
                v
            }
        }
    }

    pub fn or(&mut self, a: i32, b: i32) -> i32 {
        -self.and(-a, -b)
    }

    pub fn xor(&mut self, a: i32, b: i32) -> i32 {
        match (a, b) {
            (FALSE, x) | (x, FALSE) => x,
            (TRUE, x) | (x, TRUE) => -x,
            _ if a == b => FALSE,
            _ if a == -b => TRUE,
            _ => {
                let v = self.fresh(Def::Xor(a, b));
                self.clauses.push(vec![-v, a, b]);
                self.clauses.push(vec![-v, -a, -b]);
                self.clauses.push(vec![v, -a, b]);
                self.clauses.push(vec![v, a, -b]);
                v
            }
        }
    }

    pub fn xor3(&mut self, a: i32, b: i32, c: i32) -> i32 {
        if is_const(a) || is_const(b) || is_const(c) || a.abs() == b.abs() {
            let ab = self.xor(a, b);
            return self.xor(ab, c);
        }
        if a.abs() == c.abs() {
            let ac = self.xor(a, c);
            return self.xor(ac, b);
        }
        if b.abs() == c.abs() {
            let bc = self.xor(b, c);
            return self.xor(bc, a);
        }
        let v = self.fresh(Def::Xor3(a, b, c));
        for mask in 0..8u8 {
            // The clause that rules out this assignment of a, b, c with the wrong v
            let lits = [a, b, c];
            let ones = mask.count_ones();
            let mut clause: Vec<i32> = (0..3)
                .map(|i| {
                    if (mask >> i) & 1 == 1 {
                        -lits[i]
                    } else {
                        lits[i]
                    }
                })
                .collect();
            clause.push(if ones % 2 == 1 { v } else { -v });
            self.clauses.push(clause);
        }
        v
    }

    /// The majority of three, the carry of a full adder.
    pub fn maj(&mut self, a: i32, b: i32, c: i32) -> i32 {
        for (x, y, z) in [(a, b, c), (b, c, a), (c, a, b)] {
            match x {
                TRUE => return self.or(y, z),
                FALSE => return self.and(y, z),
                _ if x == y => return x,
                _ if x == -y => return z,
                _ => {}
            }
        }
        let v = self.fresh(Def::Maj(a, b, c));
        for (x, y) in [(a, b), (a, c), (b, c)] {
            self.clauses.push(vec![-x, -y, v]);
            self.clauses.push(vec![x, y, -v]);
        }
        v
    }

    /// Sum and carry.
    pub fn full_add(&mut self, a: i32, b: i32, c: i32) -> (i32, i32) {
        (self.xor3(a, b, c), self.maj(a, b, c))
    }

    pub fn half_add(&mut self, a: i32, b: i32) -> (i32, i32) {
        (self.xor(a, b), self.and(a, b))
    }

    /// The sum of two numbers (least significant bit first) in `width` bits, the carry out of
    /// the top dropped.
    pub fn ripple_add(&mut self, x: &[i32], y: &[i32], width: usize) -> Vec<i32> {
        let bit = |v: &[i32], i: usize| v.get(i).copied().unwrap_or(FALSE);
        let mut carry = FALSE;
        (0..width)
            .map(|i| {
                let (sum, next) = self.full_add(bit(x, i), bit(y, i), carry);
                carry = next;
                sum
            })
            .collect()
    }

    /// The value of every variable (index `v`, index 0 unused) when the inputs are `inputs`
    /// (inputs not given are false).
    pub fn eval(&self, inputs: &[(i32, bool)]) -> Vec<bool> {
        let mut value = vec![false; self.defs.len() + 1];
        for &(v, b) in inputs {
            value[v.unsigned_abs() as usize] = b == (v > 0);
        }
        let lit = |value: &[bool], l: i32| match l {
            TRUE => true,
            FALSE => false,
            _ => value[l.unsigned_abs() as usize] == (l > 0),
        };
        for (i, def) in self.defs.iter().enumerate() {
            value[i + 1] = match *def {
                Def::Input => value[i + 1],
                Def::And(a, b) => lit(&value, a) && lit(&value, b),
                Def::Xor(a, b) => lit(&value, a) != lit(&value, b),
                Def::Xor3(a, b, c) => lit(&value, a) ^ lit(&value, b) ^ lit(&value, c),
                Def::Maj(a, b, c) => {
                    u8::from(lit(&value, a)) + u8::from(lit(&value, b)) + u8::from(lit(&value, c))
                        >= 2
                }
            };
        }
        value
    }

    /// The value of a literal under [`Circuit::eval`]'s values.
    pub fn value(values: &[bool], lit: i32) -> bool {
        match lit {
            TRUE => true,
            FALSE => false,
            _ => values[lit.unsigned_abs() as usize] == (lit > 0),
        }
    }

    pub fn into_cnf(self) -> Cnf {
        let vars = self.vars();
        Cnf::new(vars, self.clauses)
    }
}

/// The bits of `n`, least significant first.
pub fn bits_of(n: u64, width: usize) -> impl Iterator<Item = bool> {
    (0..width).map(move |i| i < 64 && (n >> i) & 1 == 1)
}

/// The inputs `bits` set to the number `n`.
pub fn assign(bits: &[i32], n: u64) -> Vec<(i32, bool)> {
    bits.iter().copied().zip(bits_of(n, bits.len())).collect()
}

/// The partial products `a_i ∧ b_j` by column (their weight `i + j`).
fn partial_products(c: &mut Circuit, a: &[i32], b: &[i32]) -> Vec<Vec<i32>> {
    let mut columns = vec![Vec::new(); a.len() + b.len()];
    for (j, &bj) in b.iter().enumerate() {
        for (i, &ai) in a.iter().enumerate() {
            let p = c.and(ai, bj);
            if p != FALSE {
                columns[i + j].push(p);
            }
        }
    }
    columns
}

/// The product of `a` and `b` by adding one shifted row of partial products after the other
/// with ripple-carry adders (an array multiplier), `a.len() + b.len()` bits.
pub fn array_multiplier(c: &mut Circuit, a: &[i32], b: &[i32]) -> Vec<i32> {
    let width = a.len() + b.len();
    let mut sum = vec![FALSE; width];
    for (j, &bj) in b.iter().enumerate() {
        let mut row = vec![FALSE; j];
        for &ai in a {
            row.push(c.and(ai, bj));
        }
        sum = if j == 0 {
            row.resize(width, FALSE);
            row
        } else {
            c.ripple_add(&sum, &row, width)
        };
    }
    sum
}

/// The product of `a` and `b` by a Wallace tree.
///
/// Full and half adders compress the columns of partial products until two rows are left, which
/// a ripple-carry adder adds. The order in which each column's bits are combined is random, so
/// every tree is wired differently.
pub fn tree_multiplier(circuit: &mut Circuit, a: &[i32], b: &[i32], rng: &mut StdRng) -> Vec<i32> {
    let width = a.len() + b.len();
    let mut columns = partial_products(circuit, a, b);
    while columns.iter().any(|col| col.len() > 2) {
        let mut next = vec![Vec::new(); width + 1];
        for (w, column) in columns.iter_mut().enumerate() {
            column.shuffle(rng);
            let mut bits = column.as_slice();
            while bits.len() >= 3 {
                let (sum, carry) = circuit.full_add(bits[0], bits[1], bits[2]);
                next[w].push(sum);
                next[w + 1].push(carry);
                bits = &bits[3..];
            }
            // Two left over: a half adder now and then, to vary the shape
            if bits.len() == 2 && rng.random_bool(0.5) {
                let (sum, carry) = circuit.half_add(bits[0], bits[1]);
                next[w].push(sum);
                next[w + 1].push(carry);
            } else {
                next[w].extend_from_slice(bits);
            }
        }
        next.truncate(width);
        columns = next;
    }
    let row = |k: usize| -> Vec<i32> {
        columns
            .iter()
            .map(|col| col.get(k).copied().unwrap_or(FALSE))
            .collect()
    };
    let (x, y) = (row(0), row(1));
    circuit.ripple_add(&x, &y, width)
}

/// A multiplier of a random architecture: array or tree, operands possibly swapped.
pub fn some_multiplier(c: &mut Circuit, a: &[i32], b: &[i32], rng: &mut StdRng) -> Vec<i32> {
    let (a, b) = if rng.random_bool(0.5) { (a, b) } else { (b, a) };
    if rng.random_bool(0.5) {
        array_multiplier(c, a, b)
    } else {
        tree_multiplier(c, a, b, rng)
    }
}

/// The number the bits stand for under `values`.
pub fn number(values: &[bool], bits: &[i32]) -> u64 {
    bits.iter()
        .enumerate()
        .filter(|&(_, &l)| Circuit::value(values, l))
        .fold(0, |n, (i, _)| n | 1 << i)
}

/// A random number of exactly `bits` bits (the top one set).
pub fn random_bits<R: Rng + ?Sized>(rng: &mut R, bits: u32) -> u64 {
    debug_assert!((2..64).contains(&bits));
    let top = 1u64 << (bits - 1);
    top | rng.random_range(0..top)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    /// Every multiplier multiplies, and its clauses hold for exactly the computed values.
    #[test]
    fn multipliers_multiply() {
        let mut rng = StdRng::seed_from_u64(1);
        for (wa, wb) in [(3, 3), (4, 2), (2, 5), (1, 3)] {
            for kind in 0..3 {
                let mut circuit = Circuit::new();
                let a = circuit.inputs(wa);
                let b = circuit.inputs(wb);
                let p = match kind {
                    0 => array_multiplier(&mut circuit, &a, &b),
                    1 => tree_multiplier(&mut circuit, &a, &b, &mut rng),
                    _ => some_multiplier(&mut circuit, &a, &b, &mut rng),
                };
                assert_eq!(p.len(), wa + wb);
                let cnf = circuit.clone().into_cnf();
                for x in 0..1u64 << wa {
                    for y in 0..1u64 << wb {
                        let mut inputs = assign(&a, x);
                        inputs.extend(assign(&b, y));
                        let values = circuit.eval(&inputs);
                        assert_eq!(number(&values, &p), x * y, "{wa}x{wb} kind {kind}");
                        assert_eq!(cnf.check(&crate::cnf::literals(&values)), Some(true));
                    }
                }
            }
        }
    }

    #[test]
    fn gates_fold_constants() {
        let mut c = Circuit::new();
        let a = c.input();
        let b = c.input();
        assert_eq!(c.and(a, TRUE), a);
        assert_eq!(c.and(FALSE, b), FALSE);
        assert_eq!(c.and(a, -a), FALSE);
        assert_eq!(c.and(a, a), a);
        assert_eq!(c.or(a, TRUE), TRUE);
        assert_eq!(c.xor(a, TRUE), -a);
        assert_eq!(c.xor(a, FALSE), a);
        assert_eq!(c.xor(a, a), FALSE);
        assert_eq!(c.xor(a, -a), TRUE);
        assert_eq!(c.maj(a, b, TRUE), -c.vars().cast_signed());
        assert_eq!(c.maj(a, a, b), a);
        assert_eq!(c.maj(a, -a, b), b);
        assert_eq!(c.maj(FALSE, a, b), c.vars().cast_signed());
        let before = c.vars();
        assert_eq!(c.xor3(a, a, b), b);
        assert_eq!(c.xor3(a, b, a), b);
        assert_eq!(c.xor3(a, b, b), a);
        assert_eq!(c.vars(), before);
        c.clause(&[TRUE, a]);
        c.clause(&[FALSE, a]);
        let cnf = c.into_cnf();
        assert_eq!(cnf.clauses.last(), Some(&vec![a]));
        assert!(cnf.clauses.iter().flatten().all(|l| !is_const(*l)));
        assert_eq!(
            bits_of(5, 4).collect::<Vec<_>>(),
            [true, false, true, false]
        );
        let n = random_bits(&mut StdRng::seed_from_u64(3), 10);
        assert!((512..1024).contains(&n));
    }
}
