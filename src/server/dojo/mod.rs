//! The SAT Dojo on the website: submissions, the leaderboard and talking to the worker.
//!
//! A submission is stored with its WebAssembly module and queued. A background task sends
//! queued submissions to the worker (`DOJO_URL`, which starts on demand), the worker reports
//! progress and the score to `/dojo/report`, and a heartbeat for every job it holds, waiting
//! or running. Jobs it stops reporting about (the worker is gone) are queued again.
//!
//! Settings: `DOJO_URL` (worker, unset: nothing is evaluated), `DOJO_TOKEN` (authenticates us
//! to the worker; its reports carry a per-job token instead), `HASHI_ADMIN_TOKEN` (house entries),
//! `DOJO_SECRET` (seed of the hidden sets), `DOJO_SEASON` (number of the current season; a
//! new season archives the old one, reveals its seed and re-evaluates every submission on a new
//! hidden set, see [`seasons`]). Community benchmarks: see [`benchmarks`] (`DOJO_GBD_URL` for
//! another copy of the Global Benchmark Database).

#[cfg(test)]
mod benchmark_tests;
mod benchmarks;
mod board;
mod metrics;
mod seasons;
mod submit;
#[cfg(test)]
mod tests;
mod worker;

pub use benchmarks::{
    BenchmarkError, BenchmarkForm, admin_action as benchmark_action,
    admin_list as admin_benchmarks, page as benchmarks_page,
    report_handler as benchmark_report_handler, submit_handler as benchmark_submit_handler,
};
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
/// A job or check at the worker that hasn't reported for this long is queued again. The worker
/// sends heartbeats every minute (`dojo::worker`), so only a worker that is gone stays quiet.
const STALE_MS: i64 = 15 * 60 * 1000;
const MAX_ATTEMPTS: i64 = 3;
/// Evaluations sent to the worker at the same time. It runs one, the other waits in its queue
/// (with heartbeats), so it can start right after.
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
