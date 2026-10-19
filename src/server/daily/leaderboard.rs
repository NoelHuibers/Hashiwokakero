//! The leaderboard of a day.

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::attempt::rank;
use super::finishers;
use super::stamps::shown_stamps;
use crate::model::{Leaderboard, LeaderboardEntry, daily_difficulty};
use crate::server::count;
use crate::server::db::db_err;

/// Ranked entries shown on a daily leaderboard.
const LEADERBOARD_SIZE: i64 = 50;

fn entry(row: &libsql::Row, rank: i64) -> Result<LeaderboardEntry, libsql::Error> {
    Ok(LeaderboardEntry {
        rank,
        user_id: row.get(0)?,
        name: row.get(1)?,
        guest: row.get::<Option<String>>(2)?.is_none(),
        time_ms: row.get(3)?,
        hints: row.get(4)?,
        checks: row.get(5)?,
        stamp: None,
    })
}

const ENTRY_COLUMNS: &str =
    "a.user_id, u.name, u.username, a.time_ms, a.hints, a.checks, a.finished_at";

pub async fn leaderboard(
    conn: &Connection,
    day: i64,
    me: Option<i64>,
) -> Result<Leaderboard, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT {ENTRY_COLUMNS} FROM attempts a JOIN users u ON u.id = a.user_id
                 WHERE a.day = ?1 AND a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL
                 ORDER BY a.time_ms, a.finished_at LIMIT ?2"
            ),
            params![day, LEADERBOARD_SIZE],
        )
        .await
        .map_err(db_err)?;
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        entries.push(entry(&row, count(entries.len() + 1)).map_err(db_err)?);
    }

    let mut mine = entries.iter().find(|e| Some(e.user_id) == me).cloned();
    if let (None, Some(me)) = (&mine, me) {
        let mut rows = conn
            .query(
                &format!(
                    "SELECT {ENTRY_COLUMNS} FROM attempts a JOIN users u ON u.id = a.user_id
                     WHERE a.day = ?1 AND a.user_id = ?2 AND a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL"
                ),
                params![day, me],
            )
            .await
            .map_err(db_err)?;
        if let Some(row) = rows.next().await.map_err(db_err)? {
            let time: i64 = row.get(3).map_err(db_err)?;
            let finished: i64 = row.get(6).map_err(db_err)?;
            let r = rank(conn, day, time, finished).await?;
            mine = Some(entry(&row, r).map_err(db_err)?);
        }
    }

    let ids: Vec<i64> = entries.iter().chain(&mine).map(|e| e.user_id).collect();
    let stamps = shown_stamps(conn, &ids).await?;
    for e in entries.iter_mut().chain(&mut mine) {
        e.stamp = stamps.get(&e.user_id).copied();
    }

    Ok(Leaderboard {
        day,
        difficulty: daily_difficulty(day),
        entries,
        me: mine,
        finishers: finishers(conn, day).await?,
    })
}
