//! Tseitin formulas on random regular graphs.
//!
//! One variable per edge of a random connected [`DEGREE`]-regular graph, and for every vertex
//! a parity constraint: the XOR of its edges equals its charge (`2^(d-1)` clauses of `d`
//! literals). Every edge is in two constraints, so adding them all up gives `0` on the left and
//! the total charge on the right: with an odd total the formula is unsatisfiable (Tseitin
//! 1968). Random regular graphs are expanders, which makes every resolution proof exponentially
//! long (Urquhart 1987), so CDCL solvers only manage small ones; Gaussian elimination solves them
//! at once. For the satisfiable kind the charges come from a random assignment of the edges,
//! which is the model.
//!
//! GBD's `tseitin-formulas` family.

use rand::RngExt;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;

use super::{Edges, lit};
use crate::Answer;
use crate::bench::Generated;
use crate::cnf::Cnf;

pub const DEGREE: usize = 4;

/// A random simple `d`-regular graph on `n` vertices (`n·d` even), by pairing up `d` stubs
/// per vertex at random until no loop or double edge comes out; `None` after many tries.
pub fn random_regular(n: usize, d: usize, rng: &mut StdRng) -> Option<Edges> {
    let mut stubs: Vec<usize> = (0..n).flat_map(|v| std::iter::repeat_n(v, d)).collect();
    'tries: for _ in 0..10_000 {
        stubs.shuffle(rng);
        let mut edges = Edges::default();
        for pair in stubs.chunks(2) {
            if !edges.add(pair[0], pair[1]) {
                continue 'tries;
            }
        }
        return Some(edges);
    }
    None
}

/// A Tseitin formula on a random connected 4-regular graph with `n` vertices (at least 6).
pub fn generate(n: u32, want: Answer, rng: &mut StdRng) -> Option<Generated> {
    let n = n.clamp(6, 1000) as usize;
    let edges = random_regular(n, DEGREE, rng)?;
    if !edges.connected(n) {
        return None;
    }
    let assignment: Vec<bool> = edges.list.iter().map(|_| rng.random_bool(0.5)).collect();
    let mut incident = vec![Vec::new(); n];
    for (e, &(u, v)) in edges.list.iter().enumerate() {
        incident[u].push(e);
        incident[v].push(e);
    }
    let mut charge: Vec<bool> = match want {
        Answer::Sat => incident
            .iter()
            .map(|es| es.iter().fold(false, |x, &e| x ^ assignment[e]))
            .collect(),
        Answer::Unsat => (0..n).map(|_| rng.random_bool(0.5)).collect(),
    };
    if want == Answer::Unsat && charge.iter().filter(|&&c| c).count() % 2 == 0 {
        let v = rng.random_range(0..n);
        charge[v] = !charge[v];
    }
    let mut clauses = Vec::new();
    for (v, es) in incident.iter().enumerate() {
        for mask in 0u32..1 << es.len() {
            // Rules out the assignment `mask` of the edges when its parity is wrong
            if (mask.count_ones() % 2 == 1) != charge[v] {
                clauses.push(
                    es.iter()
                        .enumerate()
                        .map(|(i, &e)| {
                            if (mask >> i) & 1 == 1 {
                                -lit(e + 1)
                            } else {
                                lit(e + 1)
                            }
                        })
                        .collect(),
                );
            }
        }
    }
    let cnf = Cnf::new(u32::try_from(edges.len()).expect("a small graph"), clauses);
    if want == Answer::Unsat {
        return Some((cnf, Answer::Unsat, None));
    }
    let witness = crate::cnf::literals(
        &std::iter::once(false)
            .chain(assignment.iter().copied())
            .collect::<Vec<_>>(),
    );
    (cnf.check(&witness) == Some(true)).then_some((cnf, Answer::Sat, Some(witness)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn regular_graphs() {
        let mut rng = StdRng::seed_from_u64(2);
        let edges = random_regular(10, 4, &mut rng).unwrap();
        assert_eq!(edges.len(), 20);
        assert!(edges.adjacency(10).iter().all(|a| a.len() == 4));
        // K5 is the only 4-regular graph on 5 vertices; none on 4
        assert_eq!(random_regular(5, 4, &mut rng).unwrap().len(), 10);
        assert!(random_regular(4, 4, &mut rng).is_none());
    }

    #[test]
    fn labels_are_right() {
        let mut rng = StdRng::seed_from_u64(3);
        for want in [Answer::Sat, Answer::Unsat] {
            let (cnf, answer, witness) = (0..10)
                .find_map(|_| generate(10, want, &mut rng))
                .expect("a formula");
            assert_eq!(answer, want);
            // 8 clauses per vertex
            assert_eq!((cnf.vars, cnf.clauses.len()), (20, 80));
            if let Some(w) = &witness {
                assert_eq!(cnf.check(w), Some(true));
            }
            crate::bench::tests::assert_label(&cnf, answer, "10 vertices");
        }
    }
}
