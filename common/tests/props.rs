//! Property tests: the text format, the geometry of a board and checking solutions, on random
//! puzzles.

use common::{
    Board, Difficulty, Heuristic, MAX_BRIDGES, Orientation, Pos, Puzzle, Restarts, SolverConfig,
    SolverKind,
};
use proptest::prelude::*;
use proptest::test_runner::{RngSeed, contextualize_config};

/// The same cases on every run, so CI never fails at random. `PROPTEST_RNG_SEED` (other cases)
/// and `PROPTEST_CASES` (more of them) search further, a failure prints its smallest case.
fn config(cases: u32) -> ProptestConfig {
    contextualize_config(ProptestConfig {
        cases,
        rng_seed: RngSeed::Fixed(0x4a5e),
        // The fixed seed finds a failure again, no files in the source tree needed
        failure_persistence: None,
        ..ProptestConfig::default()
    })
}

/// A grid of 2 to 12 cells each way, about a quarter of them islands with numbers 1 to 8.
fn puzzle() -> impl Strategy<Value = Puzzle> {
    (2..=12usize, 2..=12usize).prop_flat_map(|(w, h)| {
        let cell = prop_oneof![3 => Just(0u8), 1 => 1..=8u8];
        prop::collection::vec(prop::collection::vec(cell, w), h)
            .prop_map(|grid| Puzzle::from_grid(&grid))
    })
}

/// A puzzle and some bridge counts on its edges, crossing or not.
fn puzzle_with_bridges() -> impl Strategy<Value = (Board, Vec<u8>)> {
    puzzle().prop_flat_map(|p| {
        let board = Board::new(p);
        let n = board.edges.len();
        (Just(board), prop::collection::vec(0..=MAX_BRIDGES, n))
    })
}

/// Text that looks like a puzzle file, with mistakes: wrong sizes, characters and row counts.
fn almost_puzzle_text() -> impl Strategy<Value = String> {
    let header = prop_oneof![
        (0..=40u8, 0..=40u8).prop_map(|(r, c)| format!("{r} {c}")),
        (0..=40u8).prop_map(|n| n.to_string()),
        "[ 0-9x-]{0,8}",
    ];
    let rows = prop::collection::vec("[.0-9a ]{0,14}", 0..14);
    (header, rows).prop_map(|(header, rows)| format!("{header}\n{}", rows.join("\n")))
}

/// Connected components of the islands by built bridges, with a plain search.
fn components(board: &Board, bridges: &[u8]) -> usize {
    let n = board.puzzle.islands.len();
    let mut seen = vec![false; n];
    let mut count = 0;
    for start in 0..n {
        if seen[start] {
            continue;
        }
        count += 1;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            for (e, edge) in board.edges.iter().enumerate() {
                let j = if edge.a == i {
                    edge.b
                } else if edge.b == i {
                    edge.a
                } else {
                    continue;
                };
                if bridges[e] > 0 && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
    }
    count
}

proptest! {
    #![proptest_config(config(256))]

    /// The editor's export and import (`hashi.txt`) lose nothing.
    #[test]
    fn text_round_trips(p in puzzle()) {
        prop_assert_eq!(Puzzle::parse_txt(&p.to_txt()), Ok(p));
    }

    /// Any file can be imported: it is a puzzle or an error, never a crash. What it reads
    /// round-trips like an exported puzzle.
    #[test]
    fn any_text_parses_or_fails(text in prop_oneof![almost_puzzle_text(), any::<String>()]) {
        if let Ok(p) = Puzzle::parse_txt(&text) {
            prop_assert!(p.width >= 2 && p.height >= 2);
            prop_assert!(p.islands.iter().all(|i| (1..=8).contains(&i.required)));
            prop_assert_eq!(Puzzle::parse_txt(&p.to_txt()), Ok(p));
        }
    }

    /// Islands stay sorted and on distinct cells inside the grid however the editor changes
    /// them.
    #[test]
    fn editing_keeps_islands_in_order(
        p in puzzle(),
        edits in prop::collection::vec((0..14u8, 0..14u8, 0..=8u8), 0..30),
        (w, h) in (2..=14u8, 2..=14u8),
    ) {
        let mut p = p;
        for (x, y, n) in edits {
            let pos = Pos::new(x, y);
            p.set(pos, n);
            if x < p.width && y < p.height {
                let found = p.island_at(pos).map(|i| p.islands[i].required);
                prop_assert_eq!(found, (n > 0).then_some(n));
            }
        }
        let p = p.resized(w, h);
        prop_assert!(p.islands.windows(2).all(|w| (w[0].pos.y, w[0].pos.x) < (w[1].pos.y, w[1].pos.x)));
        prop_assert!(p.islands.iter().all(|i| i.pos.x < w && i.pos.y < h));
        prop_assert_eq!(Puzzle::from_grid(&p.to_grid()), p);
    }

    /// Edges join islands that see each other over open water, crossings are between a
    /// horizontal and a vertical edge and go both ways, `incident` lists exactly the edges of
    /// an island.
    #[test]
    fn board_geometry_is_consistent(p in puzzle()) {
        let board = Board::new(p);
        let islands = &board.puzzle.islands;
        for (e, edge) in board.edges.iter().enumerate() {
            prop_assert!(edge.a < edge.b);
            let (a, b) = board.endpoints(e);
            match edge.orientation {
                Orientation::Horizontal => prop_assert!(a.y == b.y && a.x < b.x),
                Orientation::Vertical => prop_assert!(a.x == b.x && a.y < b.y),
            }
            prop_assert!(board.cells(e).iter().all(|&c| board.puzzle.island_at(c).is_none()));
            prop_assert_eq!(board.edge_between(edge.b, edge.a), Some(e));
            prop_assert_eq!(board.other(e, edge.a), edge.b);
            for &f in &board.crossings[e] {
                prop_assert_ne!(edge.orientation, board.edges[f].orientation);
                prop_assert!(board.crossings[f].contains(&e));
                // They share a water cell
                let cells = board.cells(f);
                prop_assert!(board.cells(e).iter().any(|c| cells.contains(c)));
            }
        }
        for (i, incident) in board.incident.iter().enumerate() {
            let expected: Vec<usize> = (0..board.edges.len())
                .filter(|&e| board.edges[e].a == i || board.edges[e].b == i)
                .collect();
            prop_assert_eq!(incident, &expected);
            // At most one edge per direction
            prop_assert!(incident.len() <= 4);
        }
        prop_assert_eq!(board.incident.len(), islands.len());
    }

    /// `check` agrees with counting by hand: degrees, crossings, components, and solved only
    /// when every rule holds.
    #[test]
    fn checks_agree_with_the_rules((board, bridges) in puzzle_with_bridges()) {
        let check = board.check(&bridges);
        for (i, &degree) in check.degrees.iter().enumerate() {
            let sum: u8 = board.incident[i].iter().map(|&e| bridges[e]).sum();
            prop_assert_eq!(degree, sum);
        }
        let mut crossing = Vec::new();
        for (e, crossings) in board.crossings.iter().enumerate() {
            for &f in crossings {
                if e < f && bridges[e] > 0 && bridges[f] > 0 {
                    crossing.push((e, f));
                }
            }
        }
        crossing.sort_unstable();
        let mut found = check.crossing.clone();
        found.sort_unstable();
        prop_assert_eq!(found, crossing);
        prop_assert_eq!(check.components, components(&board, &bridges));
        let complete = board
            .puzzle
            .islands
            .iter()
            .zip(&check.degrees)
            .all(|(island, &d)| island.required == d);
        prop_assert_eq!(check.complete, complete);
        prop_assert_eq!(
            check.solved,
            !board.puzzle.islands.is_empty()
                && complete
                && check.components == 1
                && check.crossing.is_empty()
        );
        // Rule 2 of the encoding: no bridge where it is blocked
        if check.solved {
            prop_assert!((0..bridges.len()).all(|e| bridges[e] == 0 || !board.is_blocked(&bridges, e)));
        }
    }
}

proptest! {
    #![proptest_config(config(64))]

    /// The solver panel's options: normalizing keeps only what the solver supports, twice
    /// changes nothing, and the description names every option that is kept.
    #[test]
    fn solver_configs_normalize(
        kind in prop::sample::select(SolverKind::ALL.to_vec()),
        heuristic in prop::sample::select(Heuristic::ALL.to_vec()),
        restarts in prop::sample::select(vec!["none", "fixed", "geometric", "luby"]),
        subsumption in any::<bool>(),
        timeout_ms in any::<u32>(),
    ) {
        let config = SolverConfig {
            kind,
            heuristic,
            restarts: Restarts::from_id(restarts).unwrap(),
            subsumption,
            timeout_ms,
        };
        let normal = config.normalized();
        prop_assert_eq!(normal.normalized(), normal);
        prop_assert_eq!((normal.kind, normal.timeout_ms), (kind, timeout_ms));
        let heuristics = kind.heuristics();
        prop_assert!(heuristics.is_empty() || heuristics.contains(&normal.heuristic));
        if heuristics.contains(&heuristic) {
            prop_assert_eq!(normal.heuristic, heuristic);
        }
        prop_assert!(kind.supports_restarts() || normal.restarts == Restarts::None);
        prop_assert!(kind.supports_subsumption() || !normal.subsumption);
        let description = normal.describe();
        prop_assert!(description.starts_with(kind.label()));
        if !heuristics.is_empty() {
            prop_assert!(description.contains(normal.heuristic.label()));
        }
        if kind.supports_restarts() {
            prop_assert!(description.contains(normal.restarts.label()));
        }
        prop_assert_eq!(description.contains("subsumption"), normal.subsumption);
    }
}

/// Ids are what the server, the URLs and the saved settings use: each one names its value.
#[test]
fn ids_round_trip() {
    for kind in SolverKind::ALL {
        assert_eq!(SolverKind::from_id(kind.id()), Some(kind));
        assert_eq!(SolverKind::from_id(&kind.id().to_uppercase()), Some(kind));
    }
    for h in Heuristic::ALL {
        assert_eq!(Heuristic::from_id(h.id()), Some(h));
    }
    for d in Difficulty::ALL {
        assert_eq!(Difficulty::from_id(d.id()), Some(d));
        let (w, h) = d.size();
        assert!((common::MIN_SIZE..=common::MAX_SIZE).contains(&w) && w == h);
    }
    for id in ["none", "fixed", "geometric", "luby"] {
        let restarts = Restarts::from_id(id).unwrap();
        assert_eq!(restarts.id(), id);
    }
    assert_eq!(SolverKind::from_id("minisat"), None);
    assert_eq!(Heuristic::from_id(""), None);
    assert_eq!(Difficulty::from_id("Easy"), None);
    assert_eq!(Restarts::from_id("sometimes"), None);
}
