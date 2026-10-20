//! Miters: equivalence checking of two multipliers, the bread and butter of hardware
//! verification.
//!
//! Two `n`-bit multipliers of different architecture (an array multiplier and a Wallace tree
//! wired in a random order, see [`super::circuit`]) get the same inputs, with the operands
//! swapped in one of them; the formula asks for an input on which some output bit differs.
//! Both multiply correctly, so it is unsatisfiable: proving `a·b = b·a` for a commuted
//! multiplier pair is famously hard for CDCL. For the satisfiable kind the second multiplier
//! hides a bug, like a hardware trojan: one output bit flips on the inputs of a random cube
//! (`n`-ish input bits fixed), so random simulation hardly ever hits it, and any input of the
//! cube is a model.
//!
//! The SAT Competition's instances of this kind are in GBD's `miter` family.

use rand::RngExt;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;

use super::circuit::{Circuit, FALSE, array_multiplier, tree_multiplier};
use crate::Answer;
use crate::bench::Generated;
use crate::cnf::literals;

/// Two `n`-bit multipliers (2 to 16 bits) that agree (UNSAT) or don't on one input cube (SAT).
pub fn generate(n: u32, want: Answer, rng: &mut StdRng) -> Option<Generated> {
    let n = n.clamp(2, 16) as usize;
    let mut circuit = Circuit::new();
    let a = circuit.inputs(n);
    let b = circuit.inputs(n);
    let (first, mut second) = if rng.random_bool(0.5) {
        let first = array_multiplier(&mut circuit, &a, &b);
        (first, tree_multiplier(&mut circuit, &b, &a, rng))
    } else {
        let first = tree_multiplier(&mut circuit, &a, &b, rng);
        (first, array_multiplier(&mut circuit, &b, &a))
    };
    let cube = (want == Answer::Sat).then(|| {
        // The bug fires when these inputs have these values: all but a few of the 2n inputs
        let mut inputs: Vec<i32> = a.iter().chain(&b).copied().collect();
        inputs.shuffle(rng);
        let free = rng.random_range(1..=n.min(4));
        inputs.truncate(2 * n - free);
        let cube: Vec<i32> = inputs
            .into_iter()
            .map(|v| if rng.random_bool(0.5) { v } else { -v })
            .collect();
        let fires = cube
            .iter()
            .fold(super::circuit::TRUE, |all, &l| circuit.and(all, l));
        let bit = rng.random_range(0..second.len());
        second[bit] = circuit.xor(second[bit], fires);
        cube
    });
    let differences: Vec<i32> = first
        .iter()
        .zip(&second)
        .map(|(&x, &y)| circuit.xor(x, y))
        .filter(|&d| d != FALSE)
        .collect();
    circuit.clause(&differences);
    let witness = cube.map(|cube| {
        let mut inputs: Vec<(i32, bool)> = a
            .iter()
            .chain(&b)
            .map(|&v| (v, rng.random_bool(0.5)))
            .collect();
        for l in cube {
            inputs.push((l.abs(), l > 0));
        }
        literals(&circuit.eval(&inputs))
    });
    let cnf = circuit.into_cnf();
    match witness {
        Some(w) => (cnf.check(&w) == Some(true)).then_some((cnf, Answer::Sat, Some(w))),
        None => Some((cnf, Answer::Unsat, None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn labels_are_right() {
        let mut rng = StdRng::seed_from_u64(9);
        for want in [Answer::Sat, Answer::Unsat] {
            for n in [2, 3, 4] {
                let (cnf, answer, witness) = generate(n, want, &mut rng).expect("a formula");
                assert_eq!(answer, want);
                if let Some(w) = &witness {
                    assert_eq!(cnf.check(w), Some(true));
                }
                crate::bench::tests::assert_label(&cnf, answer, &format!("{n} bits"));
            }
        }
    }
}
