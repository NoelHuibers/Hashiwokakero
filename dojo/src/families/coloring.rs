//! Graph colouring with [`COLORS`] colours.
//!
//! Every vertex gets a random colour first, and random edges only join vertices of different
//! colours, [`EDGES_PER_VERTEX`] per vertex: close to the threshold where random graphs stop
//! being 4-colourable, where planted instances are hardest. That colouring is a model. For the
//! unsatisfiable kind, the edges of the Mycielski graph `M5` join 23 random vertices first: `M5`
//! has chromatic number five (Mycielski 1955), so no graph containing it is 4-colourable. The
//! other edges are drawn the same way for both kinds, so the edge count gives nothing away.
//!
//! One variable per vertex and colour, every vertex exactly one colour, no edge with both ends
//! the same colour: the encoding of the DIMACS colouring benchmarks (GBD's `coloring` family,
//! which has Mycielski graphs too).

use rand::RngExt;
use rand::rngs::StdRng;

use super::{Edges, at_most_one, lit};
use crate::Answer;
use crate::bench::Generated;
use crate::cnf::Cnf;

pub const COLORS: usize = 4;

/// Edges per vertex: average degree 8.6, near the 4-colouring threshold of random graphs.
pub const EDGES_PER_VERTEX: f64 = 4.3;

/// The Mycielski graph `M_k`: chromatic number `k`, no triangles. `M_2` is an edge, `M_3` the
/// 5-cycle, `M_4` the Grötzsch graph (11 vertices), `M_5` has 23.
pub fn mycielski(k: usize) -> (usize, Vec<(usize, usize)>) {
    let mut n = 2;
    let mut edges = vec![(0, 1)];
    for _ in 2..k {
        // A shadow n + v of every vertex v, adjacent to v's neighbours, and an apex 2n
        // adjacent to every shadow
        let mut next = edges.clone();
        for &(u, v) in &edges {
            next.push((u, n + v));
            next.push((v, n + u));
        }
        next.extend((0..n).map(|v| (n + v, 2 * n)));
        n = 2 * n + 1;
        edges = next;
    }
    (n, edges)
}

/// The formula that `edges` on `n` vertices can be coloured with `colors` colours.
pub fn encode(n: usize, edges: &[(usize, usize)], colors: usize) -> Cnf {
    let var = |v: usize, c: usize| lit(v * colors + c + 1);
    let mut clauses = Vec::new();
    for v in 0..n {
        let lits: Vec<i32> = (0..colors).map(|c| var(v, c)).collect();
        clauses.push(lits.clone());
        at_most_one(&mut clauses, &lits);
    }
    for &(u, v) in edges {
        for c in 0..colors {
            clauses.push(vec![-var(u, c), -var(v, c)]);
        }
    }
    Cnf::new(u32::try_from(n * colors).expect("a small graph"), clauses)
}

/// A graph on `n` vertices (at least 30): 4-colourable with a planted colouring (SAT), or
/// containing `M5` (UNSAT).
pub fn generate(n: u32, want: Answer, rng: &mut StdRng) -> Option<Generated> {
    let n = n.clamp(30, 2000) as usize;
    let color: Vec<usize> = (0..n).map(|_| rng.random_range(0..COLORS)).collect();
    let mut edges = Edges::default();
    if want == Answer::Unsat {
        let (core, core_edges) = mycielski(COLORS + 1);
        let mut vertices: Vec<usize> = (0..n).collect();
        rand::seq::SliceRandom::shuffle(vertices.as_mut_slice(), rng);
        for (u, v) in core_edges {
            debug_assert!(u < core && v < core);
            edges.add(vertices[u], vertices[v]);
        }
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a few thousand vertices, far from the limits of f64 and usize"
    )]
    let target = (n as f64 * EDGES_PER_VERTEX).round() as usize;
    if !edges.fill(n, target, rng, |u, v| color[u] != color[v]) {
        return None;
    }
    let cnf = encode(n, &edges.list, COLORS);
    if want == Answer::Unsat {
        return Some((cnf, Answer::Unsat, None));
    }
    let color = &color;
    let witness: Vec<i32> = (0..n)
        .flat_map(|v| {
            (0..COLORS).map(move |c| {
                let l = lit(v * COLORS + c + 1);
                if color[v] == c { l } else { -l }
            })
        })
        .collect();
    (cnf.check(&witness) == Some(true)).then_some((cnf, Answer::Sat, Some(witness)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    /// Whether the graph can be coloured with `k` colours, by trying every colouring.
    fn colourable(n: usize, edges: &[(usize, usize)], k: usize) -> bool {
        fn go(v: usize, color: &mut [usize], edges: &[(usize, usize)], k: usize) -> bool {
            if v == color.len() {
                return true;
            }
            (0..k).any(|c| {
                color[v] = c;
                edges
                    .iter()
                    .all(|&(a, b)| (a.max(b) != v) || color[a] != color[b])
                    && go(v + 1, color, edges, k)
            })
        }
        go(0, &mut vec![0; n], edges, k)
    }

    #[test]
    fn mycielski_graphs() {
        assert_eq!(mycielski(3).0, 5);
        assert_eq!(mycielski(4), (11, mycielski(4).1));
        assert_eq!(mycielski(4).1.len(), 20);
        let (n, edges) = mycielski(5);
        assert_eq!((n, edges.len()), (23, 71));
        // The Grötzsch graph needs four colours
        let (n4, e4) = mycielski(4);
        assert!(!colourable(n4, &e4, 3));
        assert!(colourable(n4, &e4, 4));
        // M5 needs five: splr proves it with a checked proof
        crate::bench::tests::assert_label(&encode(n, &edges, 4), Answer::Unsat, "M5");
        assert!(colourable(n, &edges, 5));
    }

    #[test]
    fn labels_are_right() {
        let mut rng = StdRng::seed_from_u64(4);
        for want in [Answer::Sat, Answer::Unsat] {
            let (cnf, answer, witness) = generate(40, want, &mut rng).expect("a formula");
            assert_eq!(answer, want);
            assert_eq!(cnf.vars, 160);
            if let Some(w) = &witness {
                assert_eq!(cnf.check(w), Some(true));
            }
            crate::bench::tests::assert_label(&cnf, answer, "40 vertices");
        }
    }
}
