//! The stamp book: works out a player's stamps from their dailies (and the Dojo) and keeps the `stamps` table
//! (when each was earned) in step with it.

use std::collections::{HashMap, HashSet};

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use common::Puzzle;

use super::fame::{champion, placed};
use super::{from_json, json, today};
use crate::model::{
    ApiError, Daypart, MoveEvent, Record, Solve, Stamp, StampProgress, day_of, month_of, summarize,
};
use crate::server::db::db_err;
use crate::server::now_ms;

/// Works out the [`Run`] of finished dailies from before runs were kept with the attempt.
async fn backfill_runs(conn: &Connection, user: i64) -> Result<(), ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT a.day, a.bridges, a.log, d.puzzle FROM attempts a JOIN dailies d ON d.day = a.day
             WHERE a.user_id = ?1 AND a.finished_at IS NOT NULL AND a.run IS NULL
               AND a.bridges IS NOT NULL AND a.log IS NOT NULL",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut runs = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let day: i64 = row.get(0).map_err(db_err)?;
        let (Ok(bridges), Ok(log), Ok(puzzle)) = (
            from_json::<Vec<u8>>(&row.get::<String>(1).map_err(db_err)?),
            from_json::<Vec<MoveEvent>>(&row.get::<String>(2).map_err(db_err)?),
            from_json::<Puzzle>(&row.get::<String>(3).map_err(db_err)?),
        ) else {
            continue;
        };
        runs.push((day, summarize(&puzzle, &bridges, &log)));
    }
    for (day, run) in runs {
        conn.execute(
            "UPDATE attempts SET run = ?3 WHERE user_id = ?1 AND day = ?2",
            params![user, day, json(&run)?],
        )
        .await
        .map_err(db_err)?;
    }
    Ok(())
}

/// Everything the stamps of a player depend on. `kept` are the stored stamps that stay.
async fn record(conn: &Connection, user: i64, kept: Vec<Stamp>) -> Result<Record, ServerFnError> {
    backfill_runs(conn, user).await?;
    // The gaps to the times just above and below, before only the player's rows are left
    let mut rows = conn
        .query(
            &format!(
                "SELECT p.day, p.hints, p.checks, p.place, p.finishers, p.first_finish, p.time_ms,
                    p.started_at, p.finished_at, p.above, p.below, a.run, a.daypart
                 FROM (
                    SELECT q.*, q.time_ms - LAG(q.time_ms) OVER w AS above,
                        LEAD(q.time_ms) OVER w - q.time_ms AS below
                    FROM ({}) q WINDOW w AS (PARTITION BY q.day ORDER BY q.place)
                 ) p JOIN attempts a ON a.user_id = p.user_id AND a.day = p.day
                 WHERE p.user_id = ?1 ORDER BY p.day",
                placed(
                    "a.day IN (SELECT day FROM attempts WHERE user_id = ?1 AND finished_at IS NOT NULL)"
                )
            ),
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut solves = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let above: Option<i64> = row.get(9).map_err(db_err)?;
        let below: Option<i64> = row.get(10).map_err(db_err)?;
        let run: Option<String> = row.get(11).map_err(db_err)?;
        let daypart: Option<String> = row.get(12).map_err(db_err)?;
        solves.push(Solve {
            day: row.get(0).map_err(db_err)?,
            hints: row.get(1).map_err(db_err)?,
            checks: row.get(2).map_err(db_err)?,
            place: row.get(3).map_err(db_err)?,
            finishers: row.get(4).map_err(db_err)?,
            first_finish: row.get::<i64>(5).map_err(db_err)? != 0,
            time_ms: row.get(6).map_err(db_err)?,
            started_at: row.get(7).map_err(db_err)?,
            finished_at: row.get(8).map_err(db_err)?,
            gap_ms: above.into_iter().chain(below).min(),
            run: run.and_then(|r| from_json(&r).ok()).unwrap_or_default(),
            daypart: daypart.and_then(|d| Daypart::from_id(&d)),
        });
    }

    let mut months: Vec<i64> = solves.iter().map(|s| month_of(s.day)).collect();
    months.dedup();
    let mut champion_months = 0;
    for month in months {
        if champion(conn, month).await? == Some(user) {
            champion_months += 1;
        }
    }

    let mut rows = conn
        .query(
            "SELECT 1 FROM dojo_submissions WHERE user_id = ?1 AND state = 'done' AND similar_to IS NULL LIMIT 1",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let dojo = rows.next().await.map_err(db_err)?.is_some();
    let dojo_rank = if dojo {
        crate::server::dojo::rank_of(conn, user).await?
    } else {
        None
    };

    let mut rows = conn
        .query("SELECT created_at FROM users WHERE id = ?1", params![user])
        .await
        .map_err(db_err)?;
    let joined = match rows.next().await.map_err(db_err)? {
        Some(row) => day_of(row.get(0).map_err(db_err)?),
        None => 0,
    };

    Ok(Record {
        solves,
        champion_months,
        dojo,
        dojo_rank,
        joined,
        kept,
    })
}

/// The stored stamps of a player: when each was earned, and whether the player was told.
async fn stored(
    conn: &Connection,
    user: i64,
) -> Result<HashMap<Stamp, (i64, bool)>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT stamp, earned_at, announced FROM stamps WHERE user_id = ?1",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut stamps = HashMap::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let id: String = row.get(0).map_err(db_err)?;
        if let Some(stamp) = Stamp::from_id(&id) {
            let announced: i64 = row.get(2).map_err(db_err)?;
            stamps.insert(stamp, (row.get(1).map_err(db_err)?, announced != 0));
        }
    }
    Ok(stamps)
}

/// Brings the stored stamps of a player up to date.
///
/// New ones are added, ones a review took away again (a flagged result) removed. Returns the
/// progress towards every stamp, and with `announce` the stamps the player wasn't told about
/// yet (from now on they are).
pub async fn sync(
    conn: &Connection,
    user: i64,
    announce: bool,
) -> Result<(Vec<StampProgress>, Vec<Stamp>), ServerFnError> {
    let mut stored = stored(conn, user).await?;
    let kept = stored.keys().copied().filter(|s| s.sticky()).collect();
    let progress = record(conn, user, kept).await?.progress(today());
    let now = now_ms();
    for &(stamp, done, target) in &progress {
        let earned = done >= target;
        match (earned, stored.contains_key(&stamp)) {
            (true, false) => {
                conn.execute(
                    "INSERT OR IGNORE INTO stamps (user_id, stamp, earned_at) VALUES (?1, ?2, ?3)",
                    params![user, stamp.id(), now],
                )
                .await
                .map_err(db_err)?;
                stored.insert(stamp, (now, false));
            }
            (false, true) => {
                conn.execute(
                    "DELETE FROM stamps WHERE user_id = ?1 AND stamp = ?2",
                    params![user, stamp.id()],
                )
                .await
                .map_err(db_err)?;
                stored.remove(&stamp);
            }
            _ => {}
        }
    }
    let mut new = Vec::new();
    if announce {
        // In the order of the book
        new = Stamp::ALL
            .into_iter()
            .filter(|s| stored.get(s).is_some_and(|&(_, told)| !told))
            .collect();
        if !new.is_empty() {
            conn.execute(
                "UPDATE stamps SET announced = 1 WHERE user_id = ?1",
                params![user],
            )
            .await
            .map_err(db_err)?;
        }
    }
    let book = progress
        .into_iter()
        .map(|(stamp, progress, target)| StampProgress {
            stamp,
            earned_at: stored.get(&stamp).map(|&(at, _)| at),
            progress,
            target,
            holders: 0,
        })
        .collect();
    Ok((book, new))
}

/// How many players have each stamp, and how many have any (everyone who solved a daily or got
/// onto the Dojo board).
pub(super) async fn holders(
    conn: &Connection,
) -> Result<(HashMap<Stamp, i64>, i64), ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT stamp, COUNT(*), (SELECT COUNT(DISTINCT user_id) FROM stamps) FROM stamps GROUP BY stamp",
            (),
        )
        .await
        .map_err(db_err)?;
    let mut holders = HashMap::new();
    let mut players = 0;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let id: String = row.get(0).map_err(db_err)?;
        if let Some(stamp) = Stamp::from_id(&id) {
            holders.insert(stamp, row.get(1).map_err(db_err)?);
        }
        players = row.get(2).map_err(db_err)?;
    }
    Ok((holders, players))
}

/// Syncs the players who finished a daily in the last days, or who have no stamps yet (from
/// before stamps existed): their medals and the fastest tenth only count once a day is over,
/// and the leaderboards show stamps without the players visiting first.
pub(super) async fn sync_recent(conn: &Connection) -> Result<usize, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT DISTINCT user_id FROM attempts a
             WHERE a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL
               AND (a.day >= ?1 - 2 OR NOT EXISTS (SELECT 1 FROM stamps s WHERE s.user_id = a.user_id))",
            params![today()],
        )
        .await
        .map_err(db_err)?;
    let mut users = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        users.push(row.get::<i64>(0).map_err(db_err)?);
    }
    for &user in &users {
        sync(conn, user, false).await?;
    }
    Ok(users.len())
}

/// Runs [`sync_recent`] now and then every few hours.
pub fn start_syncing() {
    tokio::spawn(async {
        loop {
            match crate::server::db::conn() {
                Ok(conn) => {
                    if let Err(e) = sync_recent(&conn).await {
                        eprintln!("stamps: {e}");
                    }
                }
                Err(e) => eprintln!("stamps: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_hours(4)).await;
        }
    });
}

/// The stamp each of some players chose to show next to their name (those that chose one they
/// still have).
pub(super) async fn shown_stamps(
    conn: &Connection,
    users: &[i64],
) -> Result<HashMap<i64, Stamp>, ServerFnError> {
    let users: HashSet<i64> = users.iter().copied().collect();
    if users.is_empty() {
        return Ok(HashMap::new());
    }
    // Ids are integers from the database
    let ids: Vec<String> = users.iter().map(ToString::to_string).collect();
    let mut rows = conn
        .query(
            &format!(
                "SELECT u.id, u.stamp FROM users u JOIN stamps s ON s.user_id = u.id AND s.stamp = u.stamp
                 WHERE u.id IN ({})",
                ids.join(",")
            ),
            (),
        )
        .await
        .map_err(db_err)?;
    let mut shown = HashMap::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let user: i64 = row.get(0).map_err(db_err)?;
        let id: String = row.get(1).map_err(db_err)?;
        if let Some(stamp) = Stamp::from_id(&id) {
            shown.insert(user, stamp);
        }
    }
    Ok(shown)
}

/// The stamp a player shows next to their name, if they still have it.
pub(super) async fn shown_stamp(
    conn: &Connection,
    user: i64,
) -> Result<Option<Stamp>, ServerFnError> {
    Ok(shown_stamps(conn, &[user]).await?.remove(&user))
}

/// Shows one of the player's stamps next to their name on the leaderboards, or none.
pub async fn show(conn: &Connection, user: i64, stamp: Option<Stamp>) -> Result<(), ServerFnError> {
    if let Some(stamp) = stamp
        && !stored(conn, user).await?.contains_key(&stamp)
    {
        return Err(ApiError::Forbidden.fail());
    }
    conn.execute(
        "UPDATE users SET stamp = ?2 WHERE id = ?1",
        params![user, stamp.map(Stamp::id)],
    )
    .await
    .map_err(db_err)?;
    Ok(())
}
