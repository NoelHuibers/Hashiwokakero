//! Types shared by the backend (encoding, solving, generating) and the frontend (playing).
//!
//! Coordinates follow the backend convention: `x` is the column, `y` is the row.

pub mod board;
pub mod config;
pub mod puzzle;

pub use board::{Board, Check, Edge, Orientation};
pub use config::*;
pub use puzzle::{Island, Pos, Puzzle};

/// At most two bridges may connect the same pair of islands.
pub const MAX_BRIDGES: u8 = 2;
