//! Streaks with saves (see `docs/streak-saves.md`): the days of players that
//! [`model::streak_state`](crate::model::streak_state) walks, from `attempts` and `catch_ups`.
//!
//! Ranked days are finished, not revealed and not flagged attempts, as everywhere. Caught-up days
//! are finished catch-ups from [`saves_from`] on; with `None` there are none, and streaks are what
//! they always were.

use std::collections::HashMap;

use leptos::prelude::ServerFnError;
use libsql::Connection;

use crate::model::{SAVES_FROM, StreakState, streak_state};
use crate::server::db::db_err;

/// What a test set with [`SavesFrom::set`].
#[cfg(test)]
#[derive(Clone, Copy)]
enum Here {
    Const,
    Set(Option<i64>),
}

#[cfg(test)]
thread_local! {
    static SAVES_FROM_HERE: std::cell::Cell<Here> = const { std::cell::Cell::new(Here::Const) };
}

/// The first day that earns saves: [`SAVES_FROM`], or what a test set on its thread with
/// [`SavesFrom::set`].
pub(crate) fn saves_from() -> Option<i64> {
    #[cfg(test)]
    if let Here::Set(from) = SAVES_FROM_HERE.with(std::cell::Cell::get) {
        return from;
    }
    SAVES_FROM
}

/// A test's own [`saves_from`] on its thread (`#[tokio::test]` runs on one), back to
/// [`SAVES_FROM`] when dropped.
#[cfg(test)]
#[must_use]
pub(crate) struct SavesFrom;

#[cfg(test)]
impl SavesFrom {
    pub(crate) fn set(from: Option<i64>) -> Self {
        SAVES_FROM_HERE.with(|cell| cell.set(Here::Set(from)));
        Self
    }
}

#[cfg(test)]
impl Drop for SavesFrom {
    fn drop(&mut self) {
        SAVES_FROM_HERE.with(|cell| cell.set(Here::Const));
    }
}

/// SQL: the days that count for the streaks, `(user_id, day)` rows: `ranked` (a query of ranked
/// days) and the finished catch-ups from `saves_from` on. Only `ranked` without saves.
pub(crate) fn counted_days(ranked: &str, saves_from: Option<i64>) -> String {
    match saves_from {
        // A number, nothing to escape
        Some(from) => format!(
            "{ranked} UNION SELECT c.user_id, c.day FROM catch_ups c
             WHERE c.finished_at IS NOT NULL AND c.day >= {from}"
        ),
        None => ranked.to_string(),
    }
}

/// What a player's streaks follow from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Days {
    /// Days with a ranked finish, in order.
    pub(crate) ranked: Vec<i64>,
    /// Finished catch-ups from `saves_from` on: `(day, time_ms)`, in order.
    pub(crate) caught_up: Vec<(i64, i64)>,
    /// Days with a revealed or flagged run, which can't be caught up.
    pub(crate) blocked: Vec<i64>,
}

impl Days {
    pub(crate) fn state(&self, today: i64, saves_from: Option<i64>) -> StreakState {
        let caught_up: Vec<i64> = self.caught_up.iter().map(|c| c.0).collect();
        streak_state(&self.ranked, &caught_up, &self.blocked, today, saves_from)
    }
}

/// The [`Days`] of `users`, in one query. Players without any are left out.
pub(crate) async fn days(
    conn: &Connection,
    users: &[i64],
    saves_from: Option<i64>,
) -> Result<HashMap<i64, Days>, ServerFnError> {
    let mut all: HashMap<i64, Days> = HashMap::new();
    if users.is_empty() {
        return Ok(all);
    }
    // Ids are integers from the database
    let ids: Vec<String> = users.iter().map(ToString::to_string).collect();
    let ids = ids.join(",");
    // A number, nothing to escape
    let caught_up = saves_from.map_or_else(String::new, |from| {
        format!(
            "UNION ALL SELECT user_id, day, 1, time_ms FROM catch_ups
             WHERE user_id IN ({ids}) AND finished_at IS NOT NULL AND day >= {from}"
        )
    });
    let mut rows = conn
        .query(
            &format!(
                "SELECT user_id, day, CASE WHEN finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL
                    THEN 0 ELSE 2 END, NULL
                 FROM attempts WHERE user_id IN ({ids})
                   AND ((finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL) OR revealed != 0 OR flag IS NOT NULL)
                 {caught_up}
                 ORDER BY 1, 2"
            ),
            (),
        )
        .await
        .map_err(db_err)?;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let user: i64 = row.get(0).map_err(db_err)?;
        let day: i64 = row.get(1).map_err(db_err)?;
        let days = all.entry(user).or_default();
        match row.get::<i64>(2).map_err(db_err)? {
            0 => days.ranked.push(day),
            1 => days.caught_up.push((
                day,
                row.get::<Option<i64>>(3)
                    .map_err(db_err)?
                    .unwrap_or_default(),
            )),
            _ => days.blocked.push(day),
        }
    }
    Ok(all)
}

/// The [`Days`] of one player.
pub(crate) async fn days_of(
    conn: &Connection,
    user: i64,
    saves_from: Option<i64>,
) -> Result<Days, ServerFnError> {
    Ok(days(conn, &[user], saves_from)
        .await?
        .remove(&user)
        .unwrap_or_default())
}

/// The streaks of `users` on `today`. Players without any days have none.
pub(crate) async fn states(
    conn: &Connection,
    users: &[i64],
    today: i64,
    saves_from: Option<i64>,
) -> Result<HashMap<i64, StreakState>, ServerFnError> {
    Ok(days(conn, users, saves_from)
        .await?
        .into_iter()
        .map(|(user, days)| (user, days.state(today, saves_from)))
        .collect())
}

/// SQL for `auth::merge_guest`, `?1` the account, `?2` the guest; before their attempts move. A
/// day is never both ranked and caught up: a ranked finish wins over a catch-up of the day, and
/// of two catch-ups the account's stays. The guest's that are left go with the guest.
pub(crate) const MERGE_GUEST: [&str; 3] = [
    "DELETE FROM catch_ups WHERE user_id = ?2 AND day IN (SELECT day FROM attempts
        WHERE user_id = ?1 AND finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL)",
    // The guest's ranked attempt moves only if the account has none of the day
    "DELETE FROM catch_ups WHERE user_id = ?1 AND day IN (SELECT day FROM attempts a
        WHERE a.user_id = ?2 AND a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL
          AND NOT EXISTS (SELECT 1 FROM attempts b WHERE b.user_id = ?1 AND b.day = a.day))",
    "UPDATE OR IGNORE catch_ups SET user_id = ?1 WHERE user_id = ?2",
];

/// SQL for `auth::erase`, `?1` the player: catch-ups are unranked, they go with the account.
pub(crate) const ERASE: [&str; 1] = ["DELETE FROM catch_ups WHERE user_id = ?1"];
