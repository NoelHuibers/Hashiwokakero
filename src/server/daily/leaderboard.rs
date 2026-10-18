//! The leaderboard of a day and the all time list of the most solved dailies.

use std::collections::HashMap;

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::attempt::rank;
use super::{finishers, today};
use crate::model::{AllTimeEntry, Leaderboard, LeaderboardEntry, daily_difficulty, streaks};
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

    Ok(Leaderboard {
        day,
        difficulty: daily_difficulty(day),
        entries,
        me: mine,
        finishers: finishers(conn, day).await?,
    })
}

/// Players with the most solved dailies.
pub async fn all_time(conn: &Connection) -> Result<Vec<AllTimeEntry>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT a.user_id, u.name, u.username, COUNT(*) AS solved FROM attempts a
             JOIN users u ON u.id = a.user_id
             WHERE a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL
             GROUP BY a.user_id ORDER BY solved DESC, MIN(a.finished_at) LIMIT 20",
            (),
        )
        .await
        .map_err(db_err)?;
    let mut list = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        list.push(AllTimeEntry {
            rank: count(list.len() + 1),
            user_id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            guest: row.get::<Option<String>>(2).map_err(db_err)?.is_none(),
            solved: row.get(3).map_err(db_err)?,
            best_streak: 0,
        });
    }
    if list.is_empty() {
        return Ok(list);
    }
    // Solved days of all listed players in one query (ids are integers from the database)
    let ids: Vec<String> = list.iter().map(|e| e.user_id.to_string()).collect();
    let mut rows = conn
        .query(
            &format!(
                "SELECT user_id, day FROM attempts
                 WHERE user_id IN ({}) AND finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL
                 ORDER BY user_id, day",
                ids.join(",")
            ),
            (),
        )
        .await
        .map_err(db_err)?;
    let mut days: HashMap<i64, Vec<i64>> = HashMap::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let user: i64 = row.get(0).map_err(db_err)?;
        days.entry(user)
            .or_default()
            .push(row.get(1).map_err(db_err)?);
    }
    let today = today();
    for entry in &mut list {
        entry.best_streak = streaks(days.get(&entry.user_id).map_or(&[][..], |d| d), today).1;
    }
    Ok(list)
}
