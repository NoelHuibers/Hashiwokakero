//! Data exchanged between the server functions and the pages: users, daily puzzles, results,
//! groups, replays, group puzzles, races, shared puzzles, reports, blocks, feedback and
//! notifications.

mod achievements;
mod admin;
mod all_time;
mod banzuke;
mod calendar;
mod daily;
mod dojo;
mod error;
mod fame;
mod feedback;
mod groups;
mod hanko;
mod hints;
mod kadai;
mod moderation;
mod moves;
mod notify;
mod race;
mod replay;
mod rivals;
mod river;
mod scoring;
mod shares;
mod stamps;
#[cfg(test)]
mod tests;

pub use achievements::*;
pub use admin::*;
pub use all_time::*;
pub use banzuke::*;
pub use calendar::*;
pub use daily::*;
pub use dojo::*;
pub use error::*;
pub use fame::*;
pub use feedback::*;
pub use groups::*;
pub use hanko::*;
pub use hints::*;
pub use kadai::*;
pub use moderation::*;
pub use moves::*;
pub use notify::*;
pub use race::*;
pub use replay::*;
pub use rivals::*;
pub use river::*;
pub use scoring::*;
pub use shares::*;
pub use stamps::*;
