//! The SAT Dojo on the website: submissions, the leaderboard and talking to the worker.
//!
//! A submission is stored with its WebAssembly module and queued. A background task sends
//! queued submissions to the worker (`DOJO_URL`, which starts on demand), the worker reports
//! progress and the score to `/dojo/report`. Runs that go quiet are queued again.
//!
//! Settings: `DOJO_URL` (worker, unset: nothing is evaluated), `DOJO_TOKEN` (authenticates us
//! to the worker; its reports carry a per-job token instead), `HASHI_ADMIN_TOKEN` (house entries),
//! `DOJO_SECRET` (seed of the hidden sets), `DOJO_SEASON` (number of the current season; a
//! new season archives the old one, reveals its seed and re-evaluates every submission on a new
//! hidden set, see [`seasons`]).

mod board;
mod seasons;
mod submit;
#[cfg(test)]
mod tests;
mod worker;

pub use board::{board, me, rank_of, submission};
pub use seasons::{
    ArchivedEntry, ArchivedSeason, MAX_SET_BYTES, SetQuery, archived, commitment, csv, results_csv,
    results_json, seasons, seasons_json, set_download, set_handler,
};
pub use submit::{
    Form, HouseQuery, SubmitError, board_json, code_hash, house_handler, submit, submit_handler,
};
pub use worker::{apply, dispatch, housekeeping, report_handler, start_dispatcher};

use sha2::{Digest, Sha256};

use super::hex;
use crate::model::MS_PER_DAY;

/// Submissions per account and day (house entries don't count).
pub const PER_DAY: u32 = 3;
/// Accounts have to be this old to submit.
const MIN_ACCOUNT_AGE_MS: i64 = MS_PER_DAY;
/// Submissions waiting or running before new ones are refused.
const MAX_PENDING: i64 = 40;
const NAME_MAX: usize = 40;
const DESCRIPTION_MAX: usize = 600;
const URL_MAX: usize = 200;
/// A running evaluation that hasn't reported for this long is queued again.
const STALE_MS: i64 = 15 * 60 * 1000;
const MAX_ATTEMPTS: i64 = 3;
/// Evaluations sent to the worker at the same time.
const MAX_RUNNING: i64 = 2;
const BOARD_SIZE: usize = 100;

pub fn season() -> u32 {
    crate::server::env_var("DOJO_SEASON")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
}

/// The secret seed of a season's hidden set.
pub fn test_seed(season: u32) -> String {
    let secret = std::env::var("DOJO_SECRET").unwrap_or_else(|_| "hashi-dojo-local".into());
    test_seed_with(&secret, season)
}

fn test_seed_with(secret: &str, season: u32) -> String {
    hex(&Sha256::digest(
        format!("{secret}:season:{season}").as_bytes(),
    ))
}

fn token() -> Option<String> {
    std::env::var("DOJO_TOKEN").ok().filter(|t| !t.is_empty())
}
