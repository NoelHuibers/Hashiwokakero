//! Encoding, solving and generating Hashiwokakero puzzles.
//!
//! The web server only uses [`api`]; the other modules are public for the CLI, the tests and
//! the benchmarks.

pub mod api;
pub mod deduce;
pub mod encoding;
pub mod generate_clauses;
pub mod generator;
pub mod modes;
pub mod parse_input;
pub mod reconstruct;
pub mod sat;
pub mod solver;
pub mod writer;
