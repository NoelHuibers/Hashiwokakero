//! Data exchanged between the server functions and the pages: users, daily puzzles, results,
//! groups.

mod admin;
mod banzuke;
mod calendar;
mod daily;
mod dojo;
mod error;
mod fame;
mod groups;
mod hints;
mod moves;
mod rivals;
mod stamps;
#[cfg(test)]
mod tests;

pub use admin::*;
pub use banzuke::*;
pub use calendar::*;
pub use daily::*;
pub use dojo::*;
pub use error::*;
pub use fame::*;
pub use groups::*;
pub use hints::*;
pub use moves::*;
pub use rivals::*;
pub use stamps::*;
