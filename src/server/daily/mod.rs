//! The daily puzzle: one per UTC day, the same for everyone, timed by the server.

mod attempt;
mod fame;
mod leaderboard;
mod stamps;
mod stats;
#[cfg(test)]
mod tests;

pub use attempt::{attempt, check, finishers, hint, info, locked, reveal, set_flag, start, submit};
pub use fame::{hall_of_fame, month_board};
pub use leaderboard::leaderboard;
pub use stamps::{start_syncing as start_stamp_sync, sync as sync_stamps};
pub use stats::{profile, stats};

use common::{Difficulty, GenerateParams, Puzzle};
use leptos::prelude::ServerFnError;
use libsql::{Connection, params};
use sha2::{Digest, Sha256};

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
    let digest = Sha256::digest(format!("{secret}:{day}").as_bytes());
    u64::from_le_bytes(digest[..8].try_into().expect("8 bytes"))
}

#[derive(Debug)]
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

/// The puzzle of a day, generated and stored on first use.
pub async fn get_or_create(conn: &Connection, day: i64) -> Result<Daily, ServerFnError> {
    if let Some(daily) = load(conn, day).await? {
        return Ok(daily);
    }
    let difficulty = daily_difficulty(day);
    let (width, height) = difficulty.size();
    let params = GenerateParams {
        width,
        height,
        difficulty,
        seed: seed(day),
    };
    let generated = run_blocking(move || Ok(backend::api::generate(&params))).await?;
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
