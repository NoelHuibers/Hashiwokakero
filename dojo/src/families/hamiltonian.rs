//! Hamiltonian cycles in bipartite graphs.
//!
//! A cycle through every vertex of a bipartite graph alternates between the two sides, so there
//! is none when they differ in size. Satisfiable: two sides of `n/2` vertices with a planted
//! cycle through all of them. Unsatisfiable: sides of `n/2 + 1` and `n/2 - 1` vertices with a
//! planted path that covers all but one vertex, so that every vertex has two neighbours and
//! the graph is connected. Both get random extra edges between the sides up to
//! [`EDGES_PER_VERTEX`] per vertex. Counting the sides settles it at once, but a resolution proof
//! of that count is a pigeonhole argument in disguise.
//!
//! The encoding puts every vertex on one position of the cycle (one variable per vertex and
//! position, vertex 0 first): each vertex on exactly one position, each position exactly one
//! vertex, and the vertex after `v` is a neighbour of `v`. GBD's `hamiltonian` family.

use rand::RngExt;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;

use super::{Edges, exactly_one, lit};
use crate::Answer;
use crate::bench::Generated;
use crate::cnf::Cnf;

pub const EDGES_PER_VERTEX: f64 = 1.5;

/// The formula that the graph on `n` vertices has a Hamiltonian cycle.
pub fn encode(n: usize, edges: &Edges) -> Cnf {
    let var = |v: usize, p: usize| lit(v * n + p + 1);
    let mut clauses = vec![vec![var(0, 0)]];
    for v in 0..n {
        exactly_one(&mut clauses, &(0..n).map(|p| var(v, p)).collect::<Vec<_>>());
    }
    for p in 0..n {
        exactly_one(&mut clauses, &(0..n).map(|v| var(v, p)).collect::<Vec<_>>());
    }
    let adj = edges.adjacency(n);
    for (v, neighbours) in adj.iter().enumerate() {
        for p in 0..n {
            let mut clause = vec![-var(v, p)];
            clause.extend(neighbours.iter().map(|&u| var(u, (p + 1) % n)));
            clauses.push(clause);
        }
    }
    Cnf::new(u32::try_from(n * n).expect("a small graph"), clauses)
}

/// A bipartite graph on `n` vertices (even, 8 to 200) with a Hamiltonian cycle (SAT) or with
/// sides of different sizes (UNSAT).
pub fn generate(n: u32, want: Answer, rng: &mut StdRng) -> Option<Generated> {
    let n = (n.clamp(8, 200) / 2 * 2) as usize;
    let half = n / 2;
    let mut vertices: Vec<usize> = (0..n).collect();
    vertices.shuffle(rng);
    let left = if want == Answer::Sat { half } else { half + 1 };
    let (a, b) = vertices.split_at(left);
    let mut side = vec![false; n];
    for &v in a {
        side[v] = true;
    }
    let mut edges = Edges::default();
    // Alternating: a0 b0 a1 b1 ...; closed into a cycle, or a path through all of b
    let path: Vec<usize> = (0..b.len())
        .flat_map(|i| [a[i], b[i]])
        .chain((want == Answer::Unsat).then(|| a[b.len()]))
        .collect();
    for pair in path.windows(2) {
        edges.add(pair[0], pair[1]);
    }
    if want == Answer::Sat {
        edges.add(path[path.len() - 1], path[0]);
    } else {
        // The vertex the path misses, and the path's ends, get a second neighbour
        let rest = a[left - 1];
        for v in [rest, rest, path[0], path[path.len() - 1]] {
            while !edges.add(v, b[rng.random_range(0..b.len())]) {}
        }
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "at most 200 vertices"
    )]
    let target = (n as f64 * EDGES_PER_VERTEX).round() as usize;
    if !edges.fill(n, target, rng, |u, v| side[u] != side[v]) {
        return None;
    }
    let cnf = encode(n, &edges);
    if want == Answer::Unsat {
        return Some((cnf, Answer::Unsat, None));
    }
    // The planted cycle, starting at vertex 0
    let start = path
        .iter()
        .position(|&v| v == 0)
        .expect("every vertex is on it");
    let mut position = vec![0; n];
    for (p, &v) in path[start..].iter().chain(&path[..start]).enumerate() {
        position[v] = p;
    }
    let witness: Vec<i32> = (0..n)
        .flat_map(|v| {
            let at = position[v];
            (0..n).map(move |p| {
                let l = lit(v * n + p + 1);
                if p == at { l } else { -l }
            })
        })
        .collect();
    (cnf.check(&witness) == Some(true)).then_some((cnf, Answer::Sat, Some(witness)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn labels_are_right() {
        let mut rng = StdRng::seed_from_u64(8);
        for want in [Answer::Sat, Answer::Unsat] {
            let (cnf, answer, witness) = generate(8, want, &mut rng).expect("a formula");
            assert_eq!(answer, want);
            assert_eq!(cnf.vars, 64);
            if let Some(w) = &witness {
                assert_eq!(cnf.check(w), Some(true));
            }
            crate::bench::tests::assert_label(&cnf, answer, "8 vertices");
        }
    }
}
