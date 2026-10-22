//! Data exchanged between the server functions and the pages: users, daily puzzles, results,
//! groups, replays.

mod admin;
mod banzuke;
mod calendar;
mod daily;
mod dojo;
mod error;
mod fame;
mod groups;
mod hanko;
mod hints;
mod moves;
mod replay;
mod rivals;
mod river;
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
pub use hanko::*;
pub use hints::*;
pub use moves::*;
pub use replay::*;
pub use rivals::*;
pub use river::*;
pub use stamps::*;
