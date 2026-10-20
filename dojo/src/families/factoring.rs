//! Factoring: is `N` the product of two numbers greater than one?
//!
//! A multiplier circuit (array or tree, see [`super::circuit`]) multiplies an `⌈b/2⌉`-bit `p` by
//! a `(b-1)`-bit `q`, and the output is fixed to the `b`-bit `N`. Every factorisation `N = p·q`
//! with `2 ≤ p ≤ q` fits: `p ≤ √N < 2^⌈b/2⌉` and `q ≤ N/2 < 2^(b-1)`. So the formula is
//! satisfiable exactly when `N` is composite: for a semiprime the factors are a model, for a
//! prime (by a deterministic Miller–Rabin test, exact for 64-bit numbers) there is none.
//!
//! SAT Competition instances of this kind are in GBD's `prime-factoring` family; factoring
//! circuits are the classic hard crafted formulas from cryptography.

use rand::RngExt;
use rand::rngs::StdRng;

use super::circuit::{Circuit, assign, bits_of, random_bits, some_multiplier};
use crate::Answer;
use crate::bench::Generated;
use crate::cnf::literals;

/// `a·b mod m` without overflow.
fn mul_mod(a: u64, b: u64, m: u64) -> u64 {
    let product = u128::from(a) * u128::from(b) % u128::from(m);
    u64::try_from(product).expect("below the modulus")
}

fn pow_mod(mut base: u64, mut exp: u64, m: u64) -> u64 {
    let mut result = 1 % m;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            result = mul_mod(result, base, m);
        }
        base = mul_mod(base, base, m);
        exp >>= 1;
    }
    result
}

/// Whether `n` is prime: Miller–Rabin with the first twelve primes as bases, which is exact
/// for every `n < 3.3·10^24`, so for all of `u64`.
pub fn is_prime(n: u64) -> bool {
    const BASES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    if n < 2 {
        return false;
    }
    for p in BASES {
        if n.is_multiple_of(p) {
            return n == p;
        }
    }
    let mut d = n - 1;
    let mut s = 0;
    while d.is_multiple_of(2) {
        d /= 2;
        s += 1;
    }
    'bases: for a in BASES {
        let mut x = pow_mod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = mul_mod(x, x, n);
            if x == n - 1 {
                continue 'bases;
            }
        }
        return false;
    }
    true
}

/// A random prime of exactly `bits` bits.
fn random_prime(rng: &mut StdRng, bits: u32) -> u64 {
    loop {
        let n = random_bits(rng, bits) | 1;
        if is_prime(n) {
            return n;
        }
    }
}

/// `N` of `bits` bits (6 to 62): a semiprime with two factors of about half the bits for SAT,
/// a prime for UNSAT.
pub fn generate(bits: u32, want: Answer, rng: &mut StdRng) -> Option<Generated> {
    let bits = bits.clamp(6, 62);
    let (number, factors) = match want {
        Answer::Unsat => (random_prime(rng, bits), None),
        Answer::Sat => {
            let small = bits / 2;
            let p = random_prime(rng, small);
            // A product of an a-bit and a b-bit number has a+b-1 or a+b bits
            let extra = u32::from(rng.random_bool(0.5));
            let q = random_prime(rng, bits - small + extra);
            let n = p.checked_mul(q)?;
            if 64 - n.leading_zeros() != bits {
                return None;
            }
            (n, Some((p.min(q), p.max(q))))
        }
    };
    let width = bits as usize;
    let mut circuit = Circuit::new();
    let p = circuit.inputs(width.div_ceil(2));
    let q = circuit.inputs(width - 1);
    let product = some_multiplier(&mut circuit, &p, &q, rng);
    for (k, bit) in bits_of(number, product.len()).enumerate() {
        circuit.assert(if bit { product[k] } else { -product[k] });
    }
    // Neither factor is 0 or 1
    circuit.clause(&p[1..]);
    circuit.clause(&q[1..]);
    let witness = factors.map(|(small, large)| {
        let mut inputs = assign(&p, small);
        inputs.extend(assign(&q, large));
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
    fn primes() {
        let small: Vec<u64> = (0..40).filter(|&n| is_prime(n)).collect();
        assert_eq!(small, [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37]);
        // Strong pseudoprimes to several bases, and large primes
        for composite in [
            3_215_031_751,
            341_550_071_728_321,
            3_825_123_056_546_413_051,
        ] {
            assert!(!is_prime(composite), "{composite}");
        }
        for prime in [1_000_000_007, 18_446_744_073_709_551_557] {
            assert!(is_prime(prime), "{prime}");
        }
        assert_eq!(pow_mod(3, 0, 1), 0);
    }

    /// Small numbers: splr agrees with every label, and proves the primes' formulas
    /// unsatisfiable with a proof the checker accepts.
    #[test]
    fn labels_are_right() {
        let mut rng = StdRng::seed_from_u64(5);
        for want in [Answer::Sat, Answer::Unsat] {
            for bits in [6, 9, 12] {
                let (cnf, answer, witness) = (0..100)
                    .find_map(|_| generate(bits, want, &mut rng))
                    .expect("a formula");
                assert_eq!(answer, want);
                if let Some(w) = &witness {
                    assert_eq!(cnf.check(w), Some(true));
                }
                crate::bench::tests::assert_label(&cnf, answer, &format!("{bits} bits"));
            }
        }
    }
}
