//! The daily puzzle: one per UTC day, the same for everyone, timed by the server.

mod attempt;
mod fame;
mod leaderboard;
mod rivals;
mod stamps;
mod stats;
#[cfg(test)]
mod stored_tests;
#[cfg(test)]
mod tests;

pub use attempt::{
    attempt, check, finishers, hint, info, locked, owned_info, paid_action, recover_actions,
    reveal, set_flag, start, submit, submit_owned, submit_received,
};
pub use fame::{hall_of_fame, month_board};
pub use leaderboard::leaderboard;
pub use rivals::{
    board as rivals_board, duels_of_day as rival_duels, list as rivals, record as rival_record,
    set as set_rival,
};
pub use stamps::{
    show as show_stamp, shown_stamps, start_syncing as start_stamp_sync, sync as sync_stamps,
};
pub(crate) use stats::invalidate as invalidate_profiles;
pub use stats::{cached_profile, profile, stats};

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{LazyLock, Mutex, MutexGuard};

use common::{Difficulty, GenerateParams, GeneratedPuzzle, Puzzle};
use leptos::prelude::ServerFnError;
use libsql::{Connection, params};
use sha2::{Digest, Sha256};

use super::coalesce::Flights;
use super::db::db_err;
use super::{now_ms, run_blocking};
use crate::model::{ApiError, daily_difficulty, day_of};

pub fn today() -> i64 {
    day_of(now_ms())
}

/// Seeds depend on a server secret, so upcoming puzzles can't be generated in advance with the
/// open source generator.
fn seed(day: i64) -> u64 {
    let secret = std::env::var("HASHI_DAILY_SECRET").unwrap_or_else(|_| "hashi-daily".into());
    seed_with(&secret, day)
}

fn seed_with(secret: &str, day: i64) -> u64 {
    let digest = Sha256::digest(format!("{secret}:{day}").as_bytes());
    u64::from_le_bytes(digest[..8].try_into().expect("8 bytes"))
}

/// What the generator gets for the daily of `day`.
fn params(day: i64, seed: u64) -> GenerateParams {
    let difficulty = daily_difficulty(day);
    let (width, height) = difficulty.size();
    GenerateParams {
        width,
        height,
        difficulty,
        seed,
    }
}

#[derive(Clone, Debug)]
pub struct Daily {
    pub difficulty: Difficulty,
    pub puzzle: Puzzle,
    pub solution: Vec<u8>,
}

fn parse_difficulty(id: &str) -> Difficulty {
    Difficulty::from_id(id).unwrap_or_default()
}

fn json<T: serde::Serialize>(value: &T) -> Result<String, ServerFnError> {
    serde_json::to_string(value).map_err(ServerFnError::new)
}

fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, ServerFnError> {
    serde_json::from_str(s).map_err(ServerFnError::new)
}

async fn load(conn: &Connection, day: i64) -> Result<Option<Daily>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT difficulty, puzzle, solution FROM dailies WHERE day = ?1",
            params![day],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    let difficulty: String = row.get(0).map_err(db_err)?;
    let puzzle: String = row.get(1).map_err(db_err)?;
    let solution: String = row.get(2).map_err(db_err)?;
    let puzzle: Puzzle = from_json(&puzzle)?;
    Ok(Some(Daily {
        difficulty: parse_difficulty(&difficulty),
        // Stored JSON skips Puzzle::new, keep the islands in reading order
        puzzle: Puzzle::new(puzzle.width, puzzle.height, puzzle.islands),
        solution: from_json(&solution)?,
    }))
}

type Generation = Result<GeneratedPuzzle, ServerFnError>;

/// The dailies being generated. The first requests of a day (everybody starting the new daily
/// after midnight) wait for one generation instead of each queueing their own: the same
/// parameters give the same puzzle (see `backend::generator`). Only days somebody plays get
/// here, so there are few keys.
static GENERATIONS: LazyLock<Flights<GenerateParams, Generation>> =
    LazyLock::new(|| Flights::new(16));

/// How many days [`Stored`] keeps: yesterday's, today's and tomorrow's (a player whose day
/// differs from the server's around midnight, the HTTP tests).
const STORED_DAYS: usize = 3;

/// The dailies of one database as it stores them, by day.
///
/// A stored daily never changes (the only write is the `INSERT OR IGNORE` of
/// [`get_or_create_with`]), so the requests that need it again skip the query: on Turso about
/// 6 ms (a `describe` round trip and the query). Only the latest [`STORED_DAYS`] are kept: an
/// older day (a replay) goes again at once.
#[derive(Debug, Default)]
pub struct Stored(Mutex<BTreeMap<i64, Daily>>);

impl Stored {
    pub const fn new() -> Self {
        Self(Mutex::new(BTreeMap::new()))
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<i64, Daily>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn get(&self, day: i64) -> Option<Daily> {
        self.lock().get(&day).cloned()
    }

    fn keep(&self, day: i64, daily: &Daily) {
        let mut days = self.lock();
        days.insert(day, daily.clone());
        while days.len() > STORED_DAYS {
            days.pop_first();
        }
        drop(days);
    }

    /// The days kept.
    #[cfg(test)]
    pub fn days(&self) -> Vec<i64> {
        self.lock().keys().copied().collect()
    }
}

/// The dailies of the installed database (`db::install`), which every request uses.
static INSTALLED: Stored = Stored::new();

tokio::task_local! {
    /// The dailies of the database the request's connections belong to.
    static REQUEST_STORED: &'static Stored;
}

/// Runs a request with the dailies of the installed database.
///
/// Its connections come from `db::conn`. Elsewhere (the tests' own databases, tasks spawned by
/// the request) nothing is kept, so no database sees another one's dailies.
pub async fn with_installed<F: Future>(request: F) -> F::Output {
    with_stored(&INSTALLED, request).await
}

/// Runs `work` with `stored` as the dailies of its connections' database.
async fn with_stored<F: Future>(stored: &'static Stored, work: F) -> F::Output {
    REQUEST_STORED.scope(stored, work).await
}

/// The dailies of the installed database, see [`with_installed`].
#[cfg(test)]
pub fn installed() -> &'static Stored {
    &INSTALLED
}

/// The puzzle of a day, generated and stored on first use. In a request kept in memory once
/// read (see [`Stored`]).
pub async fn get_or_create(conn: &Connection, day: i64) -> Result<Daily, ServerFnError> {
    get_or_create_with(conn, day, &GENERATIONS, |params| {
        run_blocking(move || Ok(backend::api::generate(&params)))
    })
    .await
}

/// [`get_or_create`] with the generations of `flights`, made by `generate`.
async fn get_or_create_with<F, Fut>(
    conn: &Connection,
    day: i64,
    flights: &Flights<GenerateParams, Generation>,
    generate: F,
) -> Result<Daily, ServerFnError>
where
    F: FnOnce(GenerateParams) -> Fut + Send,
    Fut: Future<Output = Generation> + Send,
{
    let stored = REQUEST_STORED.try_with(|stored| *stored).ok();
    if let Some(daily) = stored.and_then(|stored| stored.get(day)) {
        return Ok(daily);
    }
    // Outside a transaction what is read is committed. In one it may be the transaction's own
    // insert (here or earlier), which a rollback takes back: then nothing is kept.
    let committed = conn.is_autocommit();
    let daily = load_or_create(conn, day, flights, generate).await?;
    if let Some(stored) = stored
        && committed
        && conn.is_autocommit()
    {
        stored.keep(day, &daily);
    }
    Ok(daily)
}

/// [`get_or_create_with`] from the database.
async fn load_or_create<F, Fut>(
    conn: &Connection,
    day: i64,
    flights: &Flights<GenerateParams, Generation>,
    generate: F,
) -> Result<Daily, ServerFnError>
where
    F: FnOnce(GenerateParams) -> Fut + Send,
    Fut: Future<Output = Generation> + Send,
{
    if let Some(daily) = load(conn, day).await? {
        return Ok(daily);
    }
    let params = params(day, seed(day));
    let difficulty = params.difficulty;
    // Every request stores it itself: the owner's connection may be in a transaction
    let generated = flights.run(params, || generate(params)).await?;
    if !generated.unique {
        return Err(ApiError::Internal.fail());
    }
    // Another server may have been faster, then its puzzle wins
    conn.execute(
        "INSERT OR IGNORE INTO dailies (day, difficulty, puzzle, solution, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            day,
            difficulty.id(),
            json(&generated.puzzle)?,
            json(&generated.solution)?,
            now_ms()
        ],
    )
    .await
    .map_err(db_err)?;
    load(conn, day)
        .await?
        .ok_or_else(|| ApiError::Internal.fail())
}

/// A puzzle of an earlier day, for practice.
pub async fn archived(conn: &Connection, day: i64) -> Result<Option<Puzzle>, ServerFnError> {
    if day >= today() {
        return Err(ApiError::ArchiveOnlyPast.into());
    }
    Ok(load(conn, day).await?.map(|d| d.puzzle))
}
