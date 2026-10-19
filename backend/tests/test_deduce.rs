//! The deduction solver (`backend::deduce`) on puzzles the generator never makes: without any
//! island, or without a solution.

use backend::deduce::{self, Uniqueness};
use common::{Board, Puzzle};

fn board(txt: &str) -> Board {
    Board::new(Puzzle::parse_txt(txt).expect("a puzzle"))
}

#[test]
fn nothing_to_decide_without_islands() {
    let analysis = deduce::analyze(&board("3 3\n...\n...\n...\n"), 10);
    assert_eq!(analysis.edges, 0);
    assert_eq!(analysis.decided_fraction, 1.0);
    assert_eq!(analysis.undecided, Vec::<usize>::new());
    assert_eq!(analysis.score(), 0.0);
}

#[test]
fn contradictions_leave_the_puzzle_unsolved() {
    // The corners need more bridges than they can get: basic propagation already fails
    let unsat = board("3 3\n8.8\n...\n8.8\n");
    let analysis = deduce::analyze(&unsat, 10);
    assert!(!analysis.solved);
    assert_eq!(
        analysis.undecided,
        (0..unsat.edges.len()).collect::<Vec<_>>()
    );
    // Counted as the hardest kind of puzzle
    assert!(analysis.score() >= 150.0, "{}", analysis.score());

    // Every island is fine on its own, but there are only two separate pairs
    let pairs = board("3 3\n1.1\n...\n1.1\n");
    let analysis = deduce::analyze(&pairs, 10);
    assert!(!analysis.solved);
    assert!(analysis.score() > 0.0);
    // Neither has the search any solution to offer
    assert_eq!(
        deduce::other_solution(&pairs, &vec![0; pairs.edges.len()], 100),
        Uniqueness::Unique
    );
}
