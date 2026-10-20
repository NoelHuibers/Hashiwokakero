//! Quasigroup with holes: completing a Latin square.
//!
//! A random Latin square of order `n` is built row by row, each row a random perfect matching
//! of columns to the symbols their column still lacks (one always exists, the bipartite graph
//! is regular). Then [`HOLES`] of its cells are emptied: around 42% holes the completion problem
//! is hardest (Achlioptas, Gomes, Kautz and Selman 2000). The square itself is a model, so the
//! formula is always satisfiable.
//!
//! The encoding is the usual one with the domains already reduced: a variable for each hole and
//! symbol that its row and column don't have yet, each hole exactly one symbol, each missing
//! symbol exactly once per row and column. GBD's `quasigroup-completion` family.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;

use super::{exactly_one, lit};
use crate::Answer;
use crate::bench::Generated;
use crate::cnf::Cnf;

/// The share of empty cells.
pub const HOLES: f64 = 0.42;

/// Matches column `c` to a symbol by an augmenting path (Kuhn's algorithm).
fn augment(
    c: usize,
    options: &[Vec<usize>],
    owner: &mut [Option<usize>],
    seen: &mut [bool],
) -> bool {
    for &s in &options[c] {
        if seen[s] {
            continue;
        }
        seen[s] = true;
        if owner[s].is_none_or(|other| augment(other, options, owner, seen)) {
            owner[s] = Some(c);
            return true;
        }
    }
    false
}

/// A random Latin square of order `n`: `square[r][c]` is a symbol in `0..n`.
pub fn random_latin(n: usize, rng: &mut StdRng) -> Vec<Vec<usize>> {
    let mut square: Vec<Vec<usize>> = Vec::with_capacity(n);
    for _ in 0..n {
        let options: Vec<Vec<usize>> = (0..n)
            .map(|c| {
                let mut free: Vec<usize> = (0..n)
                    .filter(|&s| square.iter().all(|row| row[c] != s))
                    .collect();
                free.shuffle(rng);
                free
            })
            .collect();
        let mut order: Vec<usize> = (0..n).collect();
        order.shuffle(rng);
        let mut owner: Vec<Option<usize>> = vec![None; n];
        for c in order {
            let matched = augment(c, &options, &mut owner, &mut vec![false; n]);
            assert!(matched, "a regular bipartite graph has a perfect matching");
        }
        let mut row = vec![0; n];
        for (s, c) in owner.into_iter().enumerate() {
            row[c.expect("every symbol is matched")] = s;
        }
        square.push(row);
    }
    square
}

/// A Latin square of order `n` (5 to 60) with holes: always satisfiable.
pub fn generate(n: u32, rng: &mut StdRng) -> Option<Generated> {
    let n = n.clamp(5, 60) as usize;
    let square = random_latin(n, rng);
    let mut cells: Vec<(usize, usize)> = (0..n).flat_map(|r| (0..n).map(move |c| (r, c))).collect();
    cells.shuffle(rng);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "at most 3600 cells"
    )]
    let holes = ((n * n) as f64 * HOLES).round() as usize;
    let mut empty: Vec<(usize, usize)> = cells[..holes].to_vec();
    empty.sort_unstable();
    let mut hole = vec![vec![false; n]; n];
    for &(r, c) in &empty {
        hole[r][c] = true;
    }
    let in_row = |r: usize, s: usize| (0..n).any(|c| !hole[r][c] && square[r][c] == s);
    let in_col = |c: usize, s: usize| (0..n).any(|r| !hole[r][c] && square[r][c] == s);
    let mut var = std::collections::HashMap::new();
    for &(r, c) in &empty {
        for s in 0..n {
            if !in_row(r, s) && !in_col(c, s) {
                let v = lit(var.len() + 1);
                var.insert((r, c, s), v);
            }
        }
    }
    let mut clauses = Vec::new();
    let group = |cells: &mut dyn Iterator<Item = (usize, usize, usize)>| -> Vec<i32> {
        cells.filter_map(|key| var.get(&key).copied()).collect()
    };
    for &(r, c) in &empty {
        exactly_one(&mut clauses, &group(&mut (0..n).map(|s| (r, c, s))));
    }
    for s in 0..n {
        for r in 0..n {
            if !in_row(r, s) {
                exactly_one(&mut clauses, &group(&mut (0..n).map(|c| (r, c, s))));
            }
        }
        for c in 0..n {
            if !in_col(c, s) {
                exactly_one(&mut clauses, &group(&mut (0..n).map(|r| (r, c, s))));
            }
        }
    }
    let mut witness: Vec<i32> = var
        .iter()
        .map(|(&(r, c, s), &v)| if square[r][c] == s { v } else { -v })
        .collect();
    witness.sort_by_key(|l| l.abs());
    let cnf = Cnf::new(u32::try_from(var.len()).expect("a small square"), clauses);
    (cnf.check(&witness) == Some(true)).then_some((cnf, Answer::Sat, Some(witness)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn latin_squares() {
        let mut rng = StdRng::seed_from_u64(6);
        for n in [1, 2, 5, 12] {
            let square = random_latin(n, &mut rng);
            for i in 0..n {
                let mut row: Vec<usize> = square[i].clone();
                let mut col: Vec<usize> = square.iter().map(|r| r[i]).collect();
                row.sort_unstable();
                col.sort_unstable();
                let all: Vec<usize> = (0..n).collect();
                assert_eq!((row, col), (all.clone(), all), "order {n}");
            }
        }
        // Not always the same square
        assert_ne!(random_latin(6, &mut rng), random_latin(6, &mut rng));
    }

    #[test]
    fn labels_are_right() {
        let mut rng = StdRng::seed_from_u64(7);
        let (cnf, answer, witness) = generate(10, &mut rng).expect("a formula");
        assert_eq!(answer, Answer::Sat);
        assert_eq!(cnf.check(witness.as_ref().unwrap()), Some(true));
        crate::bench::tests::assert_label(&cnf, answer, "order 10");
    }
}
