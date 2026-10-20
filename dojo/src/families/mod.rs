//! The generators of the newer families (the first five are in [`crate::bench`]).
//!
//! Every label is certain by construction, none needs a solver:
//!
//! - satisfiable formulas come with a planted model (factors, a triggering input, a colouring,
//!   an edge assignment, the completed square, a cycle), which is checked like every model;
//! - unsatisfiable ones have a mathematical reason: a prime has no factors, two correct
//!   multipliers agree everywhere, a graph containing a Mycielski graph of chromatic number
//!   five has no 4-colouring, the parity constraints of a connected graph with an odd total
//!   charge contradict each other, and a bipartite graph with sides of different sizes has no
//!   Hamiltonian cycle.
//!
//! Solvers still have to prove their UNSAT answers like everywhere else.
//!
//! Each family maps to a family of the Global Benchmark Database that SAT Competition instances
//! are sorted into (see [`crate::Family::gbd`]), so the Dojo's sets resemble the competition's
//! mix of application-like and crafted formulas rather than puzzles alone.

pub mod circuit;
pub mod coloring;
pub mod factoring;
pub mod hamiltonian;
pub mod miter;
pub mod quasigroup;
pub mod tseitin;

use rand::RngExt;
use rand::rngs::StdRng;

/// Undirected edges `(u, v)` with `u < v`, without duplicates, in the order they were added.
#[derive(Clone, Debug, Default)]
pub struct Edges {
    pub list: Vec<(usize, usize)>,
    seen: std::collections::HashSet<(usize, usize)>,
}

impl Edges {
    /// Adds the edge unless it is a loop or there already: whether it was added.
    pub fn add(&mut self, u: usize, v: usize) -> bool {
        if u == v {
            return false;
        }
        let e = (u.min(v), u.max(v));
        let added = self.seen.insert(e);
        if added {
            self.list.push(e);
        }
        added
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Adds random edges between vertices `0..n` that `allowed` accepts until there are
    /// `target`; `false` if that doesn't work out in reasonable time (too dense).
    pub fn fill(
        &mut self,
        n: usize,
        target: usize,
        rng: &mut StdRng,
        allowed: impl Fn(usize, usize) -> bool,
    ) -> bool {
        let mut tries = 0;
        while self.len() < target {
            tries += 1;
            if tries > 100 * target + 1000 {
                return false;
            }
            let (u, v) = (rng.random_range(0..n), rng.random_range(0..n));
            if allowed(u, v) {
                self.add(u, v);
            }
        }
        true
    }

    /// The neighbours of every vertex `0..n`.
    pub fn adjacency(&self, n: usize) -> Vec<Vec<usize>> {
        let mut adj = vec![Vec::new(); n];
        for &(u, v) in &self.list {
            adj[u].push(v);
            adj[v].push(u);
        }
        adj
    }

    /// Whether every vertex `0..n` can reach every other.
    pub fn connected(&self, n: usize) -> bool {
        let adj = self.adjacency(n);
        let mut seen = vec![false; n];
        let mut stack = vec![0];
        seen[0] = true;
        while let Some(u) = stack.pop() {
            for &v in &adj[u] {
                if !seen[v] {
                    seen[v] = true;
                    stack.push(v);
                }
            }
        }
        seen.into_iter().all(|s| s)
    }
}

/// A variable count as a literal.
pub(crate) fn lit(v: usize) -> i32 {
    i32::try_from(v).expect("formulas have far fewer than 2^31 variables")
}

/// Pairwise at-most-one over the literals.
pub(crate) fn at_most_one(clauses: &mut Vec<Vec<i32>>, lits: &[i32]) {
    for (i, &a) in lits.iter().enumerate() {
        for &b in &lits[i + 1..] {
            clauses.push(vec![-a, -b]);
        }
    }
}

/// Exactly one of the literals: one clause that wants one, and at most one pairwise.
pub(crate) fn exactly_one(clauses: &mut Vec<Vec<i32>>, lits: &[i32]) {
    clauses.push(lits.to_vec());
    at_most_one(clauses, lits);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn edges() {
        let mut e = Edges::default();
        assert!(e.is_empty());
        assert!(e.add(2, 1));
        assert!(!e.add(1, 2));
        assert!(!e.add(3, 3));
        assert_eq!(e.list, [(1, 2)]);
        assert!(!e.connected(3));
        assert!(e.add(0, 1));
        assert!(e.connected(3));
        assert_eq!(e.adjacency(3)[1], [2, 0]);
        let mut rng = StdRng::seed_from_u64(1);
        assert!(e.fill(10, 20, &mut rng, |u, v| u % 2 != v % 2));
        assert_eq!(e.len(), 20);
        // Only 25 edges between odd and even vertices exist
        assert!(!e.fill(10, 26, &mut rng, |u, v| u % 2 != v % 2));
        let mut clauses = Vec::new();
        exactly_one(&mut clauses, &[1, 2, 3]);
        assert_eq!(
            clauses,
            [vec![1, 2, 3], vec![-1, -2], vec![-1, -3], vec![-2, -3]]
        );
    }
}
