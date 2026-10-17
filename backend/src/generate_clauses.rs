//! Encoding of the legacy [`GameBoard`] (used by the CLI file modes and the tests).
//!
//! This is a thin adapter around [`crate::encoding`], which works on [`common::Board`].

use std::collections::HashMap;

use common::{Board, Island, Pos, Puzzle};

use crate::encoding::{encode, var};
use crate::parse_input::GameBoard;

/// A bridge variable: (from x, from y, to x, to y, 1 for the first / 2 for the second bridge).
pub type BridgeCoord = (u8, u8, u8, u8, u8);

pub fn to_puzzle(game: &GameBoard) -> Puzzle {
    let islands = game
        .islands
        .iter()
        .map(|i| Island {
            pos: Pos::new(i.x, i.y),
            required: i.connections.min(u8::MAX as u32) as u8,
        })
        .collect();
    Puzzle::new(game.cols, game.rows, islands)
}

/// The static CNF (see `Clauses.md`) and the meaning of each variable.
pub fn generate(game: &GameBoard) -> (Vec<Vec<i32>>, HashMap<i32, BridgeCoord>) {
    let board = Board::new(to_puzzle(game));
    let encoding = encode(&board);
    let mut var_map = HashMap::with_capacity(encoding.num_vars);
    for e in 0..board.edges.len() {
        let (a, b) = board.endpoints(e);
        for k in 1..=2 {
            var_map.insert(var(e, k), (a.x, a.y, b.x, b.y, k));
        }
    }
    (encoding.clauses, var_map)
}
