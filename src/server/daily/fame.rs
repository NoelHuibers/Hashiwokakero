//! The long run rankings: hall of fame (solved, medals, streaks) and the monthly points race.

use std::collections::HashMap;

use leptos::prelude::ServerFnError;
use libsql::{Connection, TransactionBehavior, params};

use super::stamps::shown_stamps;
use super::today;
use crate::model::{
    AllTimeEntry, HallOfFame, LAUNCH_DAY, MonthBoard, MonthEntry, month_days, month_of, points,
};
use crate::server::count;
use crate::server::db::db_err;

/// Players per hall of fame tab and on the monthly board.
const BOARD_SIZE: usize = 20;

/// Every ranked finish with its place among the day's ranked finishers (faster times first,
/// shared hundredth places from 2026-10-26, like `daily::rank`), the number of them, whether it was the
/// first of the day to finish, the time and the start. `filter` picks the attempts before
/// ranking, it has to keep whole days.
pub(super) fn placed(filter: &str) -> String {
    format!(
        "SELECT a.user_id, a.day, a.hints, a.checks, a.finished_at,
            CASE WHEN a.day >= 20752 THEN
                RANK() OVER (PARTITION BY a.day ORDER BY a.rank_ms, a.rank_finish)
            ELSE ROW_NUMBER() OVER (PARTITION BY a.day ORDER BY a.rank_ms, a.rank_finish) END AS place,
            COUNT(*) OVER (PARTITION BY a.day) AS finishers,
            a.finished_at = MIN(a.finished_at) OVER (PARTITION BY a.day) AS first_finish,
            a.time_ms, a.started_at
         FROM attempts a
         WHERE a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL AND ({filter})"
    )
}

/// Solved dailies, medals and streaks of every player who solved one, or of `only` those
/// players, with the time of their first ranked finish (for tie-breaks). `rank` is 0 and `stamp`
/// `None`, for the caller to fill in.
pub(super) async fn all_time(
    conn: &Connection,
    today: i64,
    only: Option<&[i64]>,
) -> Result<Vec<(AllTimeEntry, i64)>, ServerFnError> {
    // Ids are integers from the database
    let only = only.map_or_else(
        || "1".to_string(),
        |ids| {
            let ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
            format!("p.user_id IN ({})", ids.join(","))
        },
    );
    // Streaks: consecutive days share `day - row_number`, each group is one streak
    let mut rows = conn
        .query(
            &format!(
                "WITH p AS ({placed}),
                 runs AS (
                    SELECT user_id, COUNT(*) AS len, MAX(day) AS last FROM (
                        SELECT user_id, day, day - ROW_NUMBER() OVER (PARTITION BY user_id ORDER BY day) AS run
                        FROM p
                    ) GROUP BY user_id, run
                 ),
                 streaks AS (
                    SELECT user_id, MAX(len) AS best,
                        MAX(CASE WHEN last >= ?1 - 1 THEN len ELSE 0 END) AS current
                    FROM runs GROUP BY user_id
                 )
                 SELECT p.user_id, u.name, u.username, COUNT(*),
                    SUM(p.place = 1 AND p.day < ?1), SUM(p.place = 2 AND p.day < ?1),
                    SUM(p.place = 3 AND p.day < ?1), s.best, s.current, MIN(p.finished_at)
                 FROM p JOIN users u ON u.id = p.user_id JOIN streaks s ON s.user_id = p.user_id
                 WHERE {only}
                 GROUP BY p.user_id",
                placed = placed("1")
            ),
            params![today],
        )
        .await
        .map_err(db_err)?;
    let mut all = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let entry = AllTimeEntry {
            rank: 0,
            user_id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            guest: row.get::<Option<String>>(2).map_err(db_err)?.is_none(),
            solved: row.get(3).map_err(db_err)?,
            gold: row.get(4).map_err(db_err)?,
            silver: row.get(5).map_err(db_err)?,
            bronze: row.get(6).map_err(db_err)?,
            best_streak: row.get(7).map_err(db_err)?,
            current_streak: row.get(8).map_err(db_err)?,
            stamp: None,
        };
        let first: i64 = row.get(9).map_err(db_err)?;
        all.push((entry, first));
    }
    Ok(all)
}

/// Everyone who solved a daily, best first in each of the three rankings.
pub async fn hall_of_fame(conn: &Connection) -> Result<HallOfFame, ServerFnError> {
    // (entry, first finish) for the tie-breaks
    let all = all_time(conn, today(), None).await?;

    let top = |key: &dyn Fn(&AllTimeEntry) -> (i64, i64, i64, i64)| {
        let mut list: Vec<_> = all.iter().filter(|(e, _)| key(e).0 > 0).cloned().collect();
        // Higher keys first, whoever got there first breaks ties
        list.sort_by(|(a, fa), (b, fb)| key(b).cmp(&key(a)).then(fa.cmp(fb)));
        list.into_iter()
            .take(BOARD_SIZE)
            .enumerate()
            .map(|(i, (mut e, _))| {
                e.rank = count(i + 1);
                e
            })
            .collect::<Vec<_>>()
    };
    let mut fame = HallOfFame {
        solved: top(&|e| (e.solved, e.best_streak, 0, 0)),
        // Olympic order: golds first, then silvers, then bronzes
        medals: top(&|e| {
            (
                i64::from(e.gold + e.silver + e.bronze > 0),
                e.gold,
                e.silver,
                e.bronze,
            )
        }),
        streaks: top(&|e| (e.best_streak, e.current_streak, e.solved, 0)),
    };

    let ids: Vec<i64> = [&fame.solved, &fame.medals, &fame.streaks]
        .into_iter()
        .flatten()
        .map(|e| e.user_id)
        .collect();
    let stamps = shown_stamps(conn, &ids).await?;
    for e in fame
        .solved
        .iter_mut()
        .chain(&mut fame.medals)
        .chain(&mut fame.streaks)
    {
        e.stamp = stamps.get(&e.user_id).copied();
    }
    Ok(fame)
}

/// Points of every player in a month, best first: `(user, points, solved, first finish)`.
pub(super) async fn month_points(
    conn: &Connection,
    month: i64,
) -> Result<Vec<(i64, i64, i64, i64)>, ServerFnError> {
    if month < month_of(today()) {
        return closed_month(conn, month, None).await;
    }
    count_points(conn, month).await
}

/// [`month_points`] from every ranked time of the month.
async fn count_points(
    conn: &Connection,
    month: i64,
) -> Result<Vec<(i64, i64, i64, i64)>, ServerFnError> {
    let (from, to) = month_days(month);
    let mut rows = conn
        .query(&placed("a.day BETWEEN ?1 AND ?2"), params![from, to])
        .await
        .map_err(db_err)?;
    let mut players: HashMap<i64, (i64, i64, i64)> = HashMap::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let user: i64 = row.get(0).map_err(db_err)?;
        let finished_at: i64 = row.get(4).map_err(db_err)?;
        let place: i64 = row.get(5).map_err(db_err)?;
        let finishers: i64 = row.get(6).map_err(db_err)?;
        let p = players.entry(user).or_insert((0, 0, i64::MAX));
        p.0 += points(place, finishers);
        p.1 += 1;
        p.2 = p.2.min(finished_at);
    }
    let mut list: Vec<_> = players
        .into_iter()
        .map(|(user, (points, solved, first))| (user, points, solved, first))
        .collect();
    // More points first, then whoever got there first. The lower id only breaks a tie down to
    // the millisecond, which used to fall to the order of the map, a different one each time
    list.sort_by(|a, b| b.1.cmp(&a.1).then(a.3.cmp(&b.3)).then(a.0.cmp(&b.0)));
    Ok(list)
}

/// The first `limit` (`None`: all) of [`month_points`] of a month that is over.
///
/// Its ranked times hardly ever change any more, so the standings are worked out once and
/// stored, the first read after the month is over does it. Whatever changes a ranked time of the
/// month still drops them again, triggers on `attempts` take care of that (see the migration of
/// `closed_months`), so they always are what [`count_points`] would say. Also on another server,
/// and on one whose clock thinks the month is over before the others do.
async fn closed_month(
    conn: &Connection,
    month: i64,
    limit: Option<i64>,
) -> Result<Vec<(i64, i64, i64, i64)>, ServerFnError> {
    if let Some(list) = stored_month(conn, month, limit).await? {
        return Ok(list);
    }
    // Under the write lock: a change of the month's ranked times can't come between counting
    // and storing, it waits and then drops what was stored
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    // Another request (or server) may have been faster
    if let Some(list) = stored_month(&tx, month, limit).await? {
        tx.commit().await.map_err(db_err)?;
        return Ok(list);
    }
    let mut list = count_points(&tx, month).await?;
    let (from, to) = month_days(month);
    tx.execute(
        "INSERT INTO closed_months (month, first_day, last_day) VALUES (?1, ?2, ?3)",
        params![month, from, to],
    )
    .await
    .map_err(db_err)?;
    // Numbers from the database and our own count, nothing to escape
    let values: Vec<String> = list
        .iter()
        .zip(1..)
        .map(|(&(user, points, solved, first), place)| {
            format!("({month}, {place}, {user}, {points}, {solved}, {first})")
        })
        .collect();
    for chunk in values.chunks(STORE_CHUNK) {
        tx.execute(
            &format!(
                "INSERT INTO month_standings (month, place, user_id, points, solved, first_finish) VALUES {}",
                chunk.join(",")
            ),
            (),
        )
        .await
        .map_err(db_err)?;
    }
    tx.commit().await.map_err(db_err)?;
    if let Some(limit) = limit {
        list.truncate(usize::try_from(limit).unwrap_or(0));
    }
    Ok(list)
}

/// Players stored per statement, which keeps each statement small.
const STORE_CHUNK: usize = 500;

/// The stored standings of a month, `None` if there are none (yet, or any more).
async fn stored_month(
    conn: &Connection,
    month: i64,
    limit: Option<i64>,
) -> Result<Option<Vec<(i64, i64, i64, i64)>>, ServerFnError> {
    // One statement, one snapshot: the month and its standings together. A month without
    // standings is one row of NULLs
    let mut rows = conn
        .query(
            "SELECT s.user_id, s.points, s.solved, s.first_finish FROM closed_months c
             LEFT JOIN month_standings s ON s.month = c.month AND s.place <= ?2
             WHERE c.month = ?1 ORDER BY s.place",
            params![month, limit.unwrap_or(i64::MAX)],
        )
        .await
        .map_err(db_err)?;
    let mut list = Vec::new();
    let mut stored = false;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        stored = true;
        if let Some(user) = row.get::<Option<i64>>(0).map_err(db_err)? {
            list.push((
                user,
                row.get(1).map_err(db_err)?,
                row.get(2).map_err(db_err)?,
                row.get(3).map_err(db_err)?,
            ));
        }
    }
    Ok(stored.then_some(list))
}

/// The winner of a month that is over, nobody before that.
pub(super) async fn champion(conn: &Connection, month: i64) -> Result<Option<i64>, ServerFnError> {
    if month >= month_of(today()) {
        return Ok(None);
    }
    Ok(closed_month(conn, month, Some(1))
        .await?
        .first()
        .map(|p| p.0))
}

/// The monthly board, `month` defaults to the running one.
pub async fn month_board(
    conn: &Connection,
    month: Option<i64>,
    me: Option<i64>,
) -> Result<MonthBoard, ServerFnError> {
    let current = month_of(today());
    let month = month
        .unwrap_or(current)
        .clamp(month_of(LAUNCH_DAY), current);
    let list = month_points(conn, month).await?;
    let mine = me.and_then(|me| list.iter().position(|p| p.0 == me));
    let ids: Vec<i64> = (0..list.len().min(BOARD_SIZE))
        .chain(mine)
        .map(|i| list[i].0)
        .collect();
    let names = names(conn, &ids).await?;
    let stamps = shown_stamps(conn, &ids).await?;
    let entry = |i: usize| {
        let (user, points, solved, _) = list[i];
        let (name, guest) = names.get(&user).cloned().unwrap_or_default();
        MonthEntry {
            rank: count(i + 1),
            user_id: user,
            name,
            guest,
            points,
            solved,
            stamp: stamps.get(&user).copied(),
        }
    };
    Ok(MonthBoard {
        month,
        entries: (0..list.len().min(BOARD_SIZE)).map(entry).collect(),
        me: mine.map(entry),
        players: count(list.len()),
    })
}

/// Name and guest status of some users.
pub(super) async fn names(
    conn: &Connection,
    ids: &[i64],
) -> Result<HashMap<i64, (String, bool)>, ServerFnError> {
    let mut names = HashMap::new();
    if ids.is_empty() {
        return Ok(names);
    }
    // Ids are integers from the database
    let ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
    let mut rows = conn
        .query(
            &format!(
                "SELECT id, name, username FROM users WHERE id IN ({})",
                ids.join(",")
            ),
            (),
        )
        .await
        .map_err(db_err)?;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        names.insert(
            row.get(0).map_err(db_err)?,
            (
                row.get(1).map_err(db_err)?,
                row.get::<Option<String>>(2).map_err(db_err)?.is_none(),
            ),
        );
    }
    Ok(names)
}

#[cfg(test)]
#[path = "closed_month_tests.rs"]
mod tests;
