use serde::{Deserialize, Serialize};

use crate::{MAX_BRIDGES, Pos, Puzzle};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// A possible bridge between two islands that see each other.
///
/// `a` and `b` are island indices into [`Puzzle::islands`], `a` is left of / above `b`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    pub orientation: Orientation,
}

/// The geometry of a puzzle: every possible bridge and which of them cross each other.
///
/// Bridge counts of a play state or a solution are stored as `Vec<u8>` indexed like
/// [`Board::edges`]. The order is deterministic, so the backend and the frontend agree on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    pub puzzle: Puzzle,
    pub edges: Vec<Edge>,
    /// For every edge the indices of the edges it crosses.
    pub crossings: Vec<Vec<usize>>,
    /// For every island the indices of its edges.
    pub incident: Vec<Vec<usize>>,
}

/// The state of a (partial) solution.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Check {
    /// Bridges currently ending at each island.
    pub degrees: Vec<u8>,
    /// Pairs of edge indices that are both built and cross each other.
    pub crossing: Vec<(usize, usize)>,
    /// Number of connected components among islands (built bridges only).
    pub components: usize,
    /// Every island has exactly its number of bridges.
    pub complete: bool,
    /// Complete, connected and without crossings.
    pub solved: bool,
}

impl Board {
    pub fn new(puzzle: Puzzle) -> Self {
        let mut edges = Vec::new();
        for (a, island) in puzzle.islands.iter().enumerate() {
            // Nearest island to the right
            if let Some(b) = (island.pos.x + 1..puzzle.width)
                .find_map(|x| puzzle.island_at(Pos::new(x, island.pos.y)))
            {
                edges.push(Edge {
                    a,
                    b,
                    orientation: Orientation::Horizontal,
                });
            }
            // Nearest island below
            if let Some(b) = (island.pos.y + 1..puzzle.height)
                .find_map(|y| puzzle.island_at(Pos::new(island.pos.x, y)))
            {
                edges.push(Edge {
                    a,
                    b,
                    orientation: Orientation::Vertical,
                });
            }
        }

        let mut crossings = vec![Vec::new(); edges.len()];
        for (i, e) in edges.iter().enumerate() {
            if e.orientation != Orientation::Horizontal {
                continue;
            }
            let (h_a, h_b) = (puzzle.islands[e.a].pos, puzzle.islands[e.b].pos);
            for (j, f) in edges.iter().enumerate() {
                if f.orientation != Orientation::Vertical {
                    continue;
                }
                let (v_a, v_b) = (puzzle.islands[f.a].pos, puzzle.islands[f.b].pos);
                if h_a.x < v_a.x && v_a.x < h_b.x && v_a.y < h_a.y && h_a.y < v_b.y {
                    crossings[i].push(j);
                    crossings[j].push(i);
                }
            }
        }

        let mut incident = vec![Vec::new(); puzzle.islands.len()];
        for (i, e) in edges.iter().enumerate() {
            incident[e.a].push(i);
            incident[e.b].push(i);
        }

        Self {
            puzzle,
            edges,
            crossings,
            incident,
        }
    }

    /// The edge between two islands, in any order.
    pub fn edge_between(&self, a: usize, b: usize) -> Option<usize> {
        let (a, b) = (a.min(b), a.max(b));
        self.incident
            .get(a)?
            .iter()
            .copied()
            .find(|&e| self.edges[e].a == a && self.edges[e].b == b)
    }

    /// The other end of `edge` seen from `island`.
    pub fn other(&self, edge: usize, island: usize) -> usize {
        let e = self.edges[edge];
        if e.a == island { e.b } else { e.a }
    }

    pub fn endpoints(&self, edge: usize) -> (Pos, Pos) {
        let e = self.edges[edge];
        (self.puzzle.islands[e.a].pos, self.puzzle.islands[e.b].pos)
    }

    /// Water cells a bridge on `edge` passes over.
    pub fn cells(&self, edge: usize) -> Vec<Pos> {
        let (a, b) = self.endpoints(edge);
        match self.edges[edge].orientation {
            Orientation::Horizontal => (a.x + 1..b.x).map(|x| Pos::new(x, a.y)).collect(),
            Orientation::Vertical => (a.y + 1..b.y).map(|y| Pos::new(a.x, y)).collect(),
        }
    }

    pub fn empty_bridges(&self) -> Vec<u8> {
        vec![0; self.edges.len()]
    }

    /// Whether a bridge could be added to `edge` without crossing a built bridge.
    pub fn is_blocked(&self, bridges: &[u8], edge: usize) -> bool {
        self.crossings[edge].iter().any(|&c| bridges[c] > 0)
    }

    pub fn check(&self, bridges: &[u8]) -> Check {
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }

        if bridges.len() != self.edges.len() || bridges.iter().any(|&b| b > MAX_BRIDGES) {
            return Check {
                degrees: vec![0; self.puzzle.islands.len()],
                components: self.puzzle.islands.len(),
                ..Check::default()
            };
        }
        let n = self.puzzle.islands.len();
        let mut degrees = vec![0u8; n];
        let mut crossing = Vec::new();
        let mut parent: Vec<usize> = (0..n).collect();

        for (i, (&count, edge)) in bridges.iter().zip(&self.edges).enumerate() {
            if count == 0 {
                continue;
            }
            degrees[edge.a] = degrees[edge.a].saturating_add(count);
            degrees[edge.b] = degrees[edge.b].saturating_add(count);
            let (ra, rb) = (find(&mut parent, edge.a), find(&mut parent, edge.b));
            parent[ra] = rb;
            for &j in &self.crossings[i] {
                if i < j && bridges[j] > 0 {
                    crossing.push((i, j));
                }
            }
        }

        let components = (0..n).filter(|&i| find(&mut parent, i) == i).count();
        let complete = self
            .puzzle
            .islands
            .iter()
            .zip(&degrees)
            .all(|(island, &d)| island.required == d);
        let solved = n > 0
            && complete
            && components == 1
            && crossing.is_empty()
            && bridges.iter().all(|&b| b <= MAX_BRIDGES);

        Check {
            degrees,
            crossing,
            components,
            complete,
            solved,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(txt: &str) -> Board {
        Board::new(Puzzle::parse_txt(txt).unwrap())
    }

    #[test]
    fn finds_nearest_neighbours_only() {
        let b = board("3 3\n1.2\n...\n2.1\n");
        // 4 islands on the corners: 2 horizontal, 2 vertical edges
        assert_eq!(b.edges.len(), 4);
        let b = board("2 3\n111\n...\n");
        // The outer islands don't see each other through the middle one
        assert_eq!(b.edges.len(), 2);
    }

    #[test]
    fn detects_crossings() {
        let b = board("3 3\n.1.\n1.1\n.1.\n");
        assert_eq!(b.edges.len(), 2);
        assert_eq!(b.crossings[0], vec![1]);
        assert_eq!(b.crossings[1], vec![0]);
        let mut bridges = vec![1, 0];
        assert!(b.is_blocked(&bridges, 1));
        bridges[1] = 1;
        assert_eq!(b.check(&bridges).crossing, vec![(0, 1)]);
    }

    #[test]
    fn checks_solution() {
        let b = board("3 3\n2.3\n...\n1.2\n");
        let top = b.edge_between(0, 1).unwrap();
        let left = b.edge_between(0, 2).unwrap();
        let right = b.edge_between(1, 3).unwrap();
        let mut bridges = b.empty_bridges();
        bridges[top] = 1;
        bridges[left] = 1;
        bridges[right] = 2;
        let check = b.check(&bridges);
        assert!(check.complete);
        assert!(check.solved);
        assert_eq!(check.components, 1);

        bridges[right] = 1;
        assert!(!b.check(&bridges).solved);
    }

    #[test]
    fn disconnected_is_not_solved() {
        let b = board("3 3\n1.1\n...\n1.1\n");
        let mut bridges = b.empty_bridges();
        bridges[b.edge_between(0, 1).unwrap()] = 1;
        bridges[b.edge_between(2, 3).unwrap()] = 1;
        let check = b.check(&bridges);
        assert!(check.complete);
        assert_eq!(check.components, 2);
        assert!(!check.solved);
    }
}

#[cfg(test)]
mod malformed_tests {
    use super::*;
    #[test]
    fn malformed_bridge_lengths_are_never_solutions_or_panics() {
        let board = Board::new(Puzzle::parse_txt("3 3\n1.1\n...\n...\n").unwrap());
        assert!(board.check(&[1]).solved);
        for bridges in [vec![], vec![1, 0], vec![3]] {
            assert!(!board.check(&bridges).solved);
        }
        let crossing = Board::new(Puzzle::parse_txt("3 3\n.1.\n1.1\n.1.\n").unwrap());
        assert!(!crossing.check(&[1]).solved);
    }
}
