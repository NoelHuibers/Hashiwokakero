//! A player's daily statistics: solved days, streaks, times, recent results, medals, the
//! monthly race and the stamp book. Also on their public profile.

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::fame::month_points;
use super::stamps::{holders, shown_stamp, sync};
use super::today;
use crate::model::{DayResult, PlayerProfile, UserStats, month_of, streaks};
use crate::server::count;
use crate::server::db::db_err;

/// Days shown in the player's recent results.
const RECENT_DAYS: i64 = 35;

async fn solved_days(conn: &Connection, user: i64) -> Result<Vec<i64>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT day FROM attempts WHERE user_id = ?1 AND finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL ORDER BY day",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut days = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        days.push(row.get(0).map_err(db_err)?);
    }
    Ok(days)
}

pub async fn stats(conn: &Connection, user: i64) -> Result<UserStats, ServerFnError> {
    let today = today();
    let days = solved_days(conn, user).await?;
    let (current_streak, best_streak) = streaks(&days, today);

    // (day, time, rank) of every ranked finish, ranks in the same query to save round trips
    let mut rows = conn
        .query(
            "SELECT a.day, a.time_ms, (
                SELECT COUNT(*) + 1 FROM attempts b
                WHERE b.day = a.day AND b.finished_at IS NOT NULL AND b.revealed = 0 AND b.flag IS NULL
                  AND (b.time_ms < a.time_ms OR (b.time_ms = a.time_ms AND b.finished_at < a.finished_at))
             ) FROM attempts a
             WHERE a.user_id = ?1 AND a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut finished = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let day: i64 = row.get(0).map_err(db_err)?;
        let time: i64 = row.get(1).map_err(db_err)?;
        let rank: i64 = row.get(2).map_err(db_err)?;
        finished.push((day, time, rank));
    }
    let best_time_ms = finished.iter().map(|f| f.1).min();
    let average_time_ms = (!finished.is_empty())
        .then(|| finished.iter().map(|f| f.1).sum::<i64>() / count(finished.len()));

    let recent = (today - RECENT_DAYS + 1..=today)
        .map(|day| {
            let found = finished.iter().find(|f| f.0 == day);
            DayResult {
                day,
                time_ms: found.map(|f| f.1),
                rank: found.map(|f| f.2),
            }
        })
        .collect();

    // Medals only count once the day is over
    let medals = |place| {
        count(
            finished
                .iter()
                .filter(|f| f.0 < today && f.2 == place)
                .count(),
        )
    };
    let month = month_points(conn, month_of(today)).await?;
    let month_place = month.iter().position(|p| p.0 == user);
    let mut stamps = sync(conn, user, false).await?.0;
    let (holders, players) = holders(conn).await?;
    for p in &mut stamps {
        p.holders = holders.get(&p.stamp).copied().unwrap_or_default();
    }

    Ok(UserStats {
        solved: count(days.len()),
        current_streak,
        best_streak,
        best_time_ms,
        average_time_ms,
        recent,
        gold: medals(1),
        silver: medals(2),
        bronze: medals(3),
        month_rank: month_place.map(|i| count(i + 1)),
        month_points: month_place.map_or(0, |i| month[i].1),
        stamps,
        players,
        shown: shown_stamp(conn, user).await?,
    })
}

/// The public profile of a player, `None` if there is no such player.
pub async fn profile(conn: &Connection, user: i64) -> Result<Option<PlayerProfile>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT name, username IS NULL, created_at FROM users WHERE id = ?1",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    let name: String = row.get(0).map_err(db_err)?;
    let guest: i64 = row.get(1).map_err(db_err)?;
    let created_at: i64 = row.get(2).map_err(db_err)?;
    Ok(Some(PlayerProfile {
        id: user,
        name,
        guest: guest != 0,
        created_at,
        stats: stats(conn, user).await?,
    }))
}
