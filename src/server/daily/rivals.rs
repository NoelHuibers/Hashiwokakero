//! Rivals ライバル: players someone marked to compare themselves with. One-sided, no requests:
//! everything compared is on the public leaderboards anyway. The list is private, only the one
//! who marked them sees it, and the rival isn't told.
//!
//! Deleted and banned players drop out of every list (a ban keeps the row, lifting it brings
//! them back); deleting an account removes its list and its place in everyone else's
//! (`auth::erase`).

use std::collections::HashMap;

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::fame::all_time;
use super::stamps::shown_stamps;
use super::{attempt, today};
use crate::model::{
    AllTimeEntry, ApiError, AttemptInfo, Duel, HeadToHead, MAX_RIVALS, RivalDay, RivalRow,
    RivalsBoard, RivalsToday,
};
use crate::server::auth::user_by_id;
use crate::server::db::db_err;
use crate::server::{count, now_ms};

/// Whether the attempt `a` is ranked: finished, not given up and not flagged (as on the
/// leaderboards).
fn ranked(a: &str) -> String {
    format!("{a}.finished_at IS NOT NULL AND {a}.revealed = 0 AND {a}.flag IS NULL")
}

/// Rivals that still play: not deleted, not banned.
const VISIBLE: &str = "u.deleted_at IS NULL AND u.banned_at IS NULL";

#[derive(Debug)]
struct Rival {
    id: i64,
    name: String,
    guest: bool,
}

/// The rivals of `user` with their names, in the order they were marked.
async fn rivals(conn: &Connection, user: i64) -> Result<Vec<Rival>, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT u.id, u.name, u.username FROM rivals r JOIN users u ON u.id = r.rival_id
                 WHERE r.user_id = ?1 AND {VISIBLE} ORDER BY r.created_at, u.id"
            ),
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut list = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        list.push(Rival {
            id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            guest: row.get::<Option<String>>(2).map_err(db_err)?.is_none(),
        });
    }
    Ok(list)
}

/// The ids of the rivals of `user`, in the order they were marked.
pub async fn list(conn: &Connection, user: i64) -> Result<Vec<i64>, ServerFnError> {
    Ok(rivals(conn, user)
        .await?
        .into_iter()
        .map(|r| r.id)
        .collect())
}

/// Marks `rival` as a rival of `user` (`on`) or not any more, and returns the rivals then.
///
/// Nobody is their own rival, and only players who still play can be marked, at most
/// [`MAX_RIVALS`]. Unmarking always works, also a rival who is gone.
pub async fn set(
    conn: &Connection,
    user: i64,
    rival: i64,
    on: bool,
) -> Result<Vec<i64>, ServerFnError> {
    if !on {
        conn.execute(
            "DELETE FROM rivals WHERE user_id = ?1 AND rival_id = ?2",
            params![user, rival],
        )
        .await
        .map_err(db_err)?;
        return list(conn, user).await;
    }
    if rival == user {
        return Err(ApiError::RivalSelf.into());
    }
    let mut rows = conn
        .query(
            &format!("SELECT 1 FROM users u WHERE u.id = ?1 AND {VISIBLE}"),
            params![rival],
        )
        .await
        .map_err(db_err)?;
    if rows.next().await.map_err(db_err)?.is_none() {
        return Err(ApiError::NoSuchPlayer.into());
    }
    drop(rows);
    // The limit in the same statement, so two taps at once can't pass it
    let added = conn
        .execute(
            &format!(
                "INSERT OR IGNORE INTO rivals (user_id, rival_id, created_at)
                 SELECT ?1, ?2, ?3 WHERE (
                    SELECT COUNT(*) FROM rivals r JOIN users u ON u.id = r.rival_id
                    WHERE r.user_id = ?1 AND {VISIBLE}
                 ) < ?4"
            ),
            params![user, rival, now_ms(), count(MAX_RIVALS)],
        )
        .await
        .map_err(db_err)?;
    let now = list(conn, user).await?;
    if added == 0 && !now.contains(&rival) {
        return Err(ApiError::TooManyRivals.into());
    }
    Ok(now)
}

/// Every day `user` and each of `others` both have a ranked time, oldest first.
async fn duels(
    conn: &Connection,
    user: i64,
    others: &[i64],
) -> Result<HashMap<i64, Vec<Duel>>, ServerFnError> {
    let mut duels: HashMap<i64, Vec<Duel>> = HashMap::new();
    if others.is_empty() {
        return Ok(duels);
    }
    // Ids are integers from the database
    let ids: Vec<String> = others.iter().map(ToString::to_string).collect();
    let mut rows = conn
        .query(
            &format!(
                "SELECT b.user_id, a.day, a.time_ms, b.time_ms
                 FROM attempts a JOIN attempts b ON b.day = a.day
                 WHERE a.user_id = ?1 AND b.user_id IN ({})
                   AND {} AND {}
                 ORDER BY a.day",
                ids.join(","),
                ranked("a"),
                ranked("b"),
            ),
            params![user],
        )
        .await
        .map_err(db_err)?;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let other: i64 = row.get(0).map_err(db_err)?;
        duels.entry(other).or_default().push(Duel {
            day: row.get(1).map_err(db_err)?,
            mine: row.get(2).map_err(db_err)?,
            theirs: row.get(3).map_err(db_err)?,
        });
    }
    Ok(duels)
}

/// The head-to-head records of `user` against `others`.
async fn records(
    conn: &Connection,
    user: i64,
    others: &[i64],
) -> Result<HashMap<i64, HeadToHead>, ServerFnError> {
    Ok(duels(conn, user, others)
        .await?
        .into_iter()
        .map(|(other, d)| (other, HeadToHead::of(&d)))
        .collect())
}

/// The head-to-head record of `user` against `other`, if `other` is one of their rivals.
pub async fn record(
    conn: &Connection,
    user: i64,
    other: i64,
) -> Result<Option<HeadToHead>, ServerFnError> {
    if !list(conn, user).await?.contains(&other) {
        return Ok(None);
    }
    Ok(Some(
        records(conn, user, &[other])
            .await?
            .remove(&other)
            .unwrap_or_default(),
    ))
}

/// How `user` did against their rivals on `day`, `None` until they finished it themselves
/// (solved or given up): the rivals' times would give away what a fair time is.
pub async fn duels_of_day(
    conn: &Connection,
    user: i64,
    day: i64,
) -> Result<Option<RivalsToday>, ServerFnError> {
    let Some(mine) = attempt(conn, user, day)
        .await?
        .filter(AttemptInfo::finished)
    else {
        return Ok(None);
    };
    let mine = mine
        .time_ms
        .filter(|_| !mine.revealed && mine.flag.is_none());
    let list = rivals(conn, user).await?;
    let ids: Vec<i64> = list.iter().map(|r| r.id).collect();
    let mut times = HashMap::new();
    if !ids.is_empty() {
        let joined: Vec<String> = ids.iter().map(ToString::to_string).collect();
        let mut rows = conn
            .query(
                &format!(
                    "SELECT a.user_id, a.time_ms FROM attempts a
                     WHERE a.day = ?1 AND a.user_id IN ({}) AND {}",
                    joined.join(","),
                    ranked("a"),
                ),
                params![day],
            )
            .await
            .map_err(db_err)?;
        while let Some(row) = rows.next().await.map_err(db_err)? {
            times.insert(
                row.get::<i64>(0).map_err(db_err)?,
                row.get::<i64>(1).map_err(db_err)?,
            );
        }
    }
    let mut records = records(conn, user, &ids).await?;
    let stamps = shown_stamps(conn, &ids).await?;
    Ok(Some(RivalsToday {
        mine,
        rivals: list
            .into_iter()
            .map(|r| RivalDay {
                user_id: r.id,
                time_ms: times.get(&r.id).copied(),
                record: records.remove(&r.id).unwrap_or_default(),
                stamp: stamps.get(&r.id).copied(),
                name: r.name,
                guest: r.guest,
            })
            .collect(),
    }))
}

/// `user` and their rivals compared: solved dailies, medals, streaks and the head-to-head
/// records. Most solved first, then most medals; `user` is in it even without a result.
pub async fn board(conn: &Connection, user: i64) -> Result<RivalsBoard, ServerFnError> {
    let list = rivals(conn, user).await?;
    let ids: Vec<i64> = std::iter::once(user)
        .chain(list.iter().map(|r| r.id))
        .collect();
    let mut stats: HashMap<i64, (AllTimeEntry, i64)> = all_time(conn, today(), Some(&ids))
        .await?
        .into_iter()
        .map(|(e, first)| (e.user_id, (e, first)))
        .collect();
    let mut records = records(conn, user, &ids[1..]).await?;
    let stamps = shown_stamps(conn, &ids).await?;
    let me = user_by_id(conn, user).await?;
    let names = std::iter::once(me.map(|u| (u.name.clone(), u.username.is_none())))
        .chain(list.iter().map(|r| Some((r.name.clone(), r.guest))));
    let mut rows: Vec<(RivalRow, i64)> = ids
        .iter()
        .zip(names)
        .map(|(&id, name)| {
            let (mut entry, first) = stats.remove(&id).unwrap_or_else(|| {
                (
                    AllTimeEntry {
                        rank: 0,
                        user_id: id,
                        name: String::new(),
                        guest: false,
                        solved: 0,
                        gold: 0,
                        silver: 0,
                        bronze: 0,
                        best_streak: 0,
                        current_streak: 0,
                        stamp: None,
                    },
                    i64::MAX,
                )
            });
            // The name as it is now, also without a result
            if let Some((name, guest)) = name {
                (entry.name, entry.guest) = (name, guest);
            }
            entry.stamp = stamps.get(&id).copied();
            let me = id == user;
            let record = (!me).then(|| records.remove(&id).unwrap_or_default());
            (RivalRow { entry, me, record }, first)
        })
        .collect();
    // Most solved, most medals (gold first), then whoever got there first
    let key = |e: &AllTimeEntry| (e.solved, e.gold, e.silver, e.bronze);
    rows.sort_by(|(a, fa), (b, fb)| {
        key(&b.entry)
            .cmp(&key(&a.entry))
            .then(fa.cmp(fb))
            .then(a.entry.user_id.cmp(&b.entry.user_id))
    });
    Ok(RivalsBoard {
        rows: rows
            .into_iter()
            .enumerate()
            .map(|(i, (mut row, _))| {
                row.entry.rank = count(i + 1);
                row
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests;
