//! The river 川 of a group (see `model::river`): the bridges of the members' solved dailies,
//! laid as planks of one bridge the group builds together.
//!
//! A member's dailies count from the day they joined. When they leave (or delete their account),
//! what they laid stays in the river without their name: it's added to the group's
//! `river_banked`. Levels are found complete when a member looks at the river and kept in
//! `group_river`, once each, with who laid planks of them in `group_river_crew` (their
//! [`Stamp::RiverCrew`](crate::model::Stamp::RiverCrew)).

#[cfg(test)]
mod tests;

use leptos::prelude::ServerFnError;
use libsql::{Connection, TransactionBehavior, params};

use super::{Group, require_member};
use crate::model::{
    Contribution, GroupRiver, Laid, RiverLevel, crossings, level_target, monday_of, ranking,
};
use crate::server::daily::sync_stamps;
use crate::server::db::db_err;
use crate::server::now_ms;

/// What a member `?1` laid in the group of the statement (`groups.id`): the bridges of their
/// solved dailies (ranked: finished, not given up, not flagged) since they joined. A double
/// bridge counts twice, like for the stamp book (`model::Run::bridges`).
macro_rules! laid_by_member {
    () => {
        "SELECT COALESCE(SUM(json_extract(a.run, '$.bridges')), 0)
         FROM group_members m JOIN attempts a ON a.user_id = m.user_id
         WHERE m.group_id = groups.id AND m.user_id = ?1
           AND a.finished_at >= m.joined_at AND a.revealed = 0 AND a.flag IS NULL"
    };
}

/// For `auth::erase` (see `groups::ERASE`), before the memberships go: what `?1` laid stays in
/// the rivers of all their groups, without their name.
pub const BANK: &str = concat!(
    "UPDATE groups SET river_banked = river_banked + (",
    laid_by_member!(),
    ") WHERE id IN (SELECT group_id FROM group_members WHERE user_id = ?1)"
);

/// [`BANK`] for one group `?2`, when `?1` leaves it.
const BANK_ONE: &str = concat!(
    "UPDATE groups SET river_banked = river_banked + (",
    laid_by_member!(),
    ") WHERE id = ?2"
);

/// What `user` laid in `group` stays in its river when they leave.
pub async fn bank(conn: &Connection, group: i64, user: i64) -> Result<(), ServerFnError> {
    conn.execute(BANK_ONE, params![user, group])
        .await
        .map_err(db_err)?;
    Ok(())
}

/// The solved dailies of the members in `group`'s river, as the statements below count them.
const LAID: &str = "FROM group_members m JOIN attempts a ON a.user_id = m.user_id
     WHERE m.group_id = ?1 AND a.finished_at >= m.joined_at AND a.revealed = 0 AND a.flag IS NULL";

/// A river as stored: the crew it counts, every bridge in it, and the levels completed.
struct Stored {
    members: usize,
    total: i64,
    levels: Vec<RiverLevel>,
}

impl Stored {
    fn base(&self) -> i64 {
        self.levels.iter().map(|l| l.target).sum()
    }

    fn level(&self) -> u32 {
        u32::try_from(self.levels.len()).unwrap_or(u32::MAX)
    }

    fn target(&self) -> i64 {
        level_target(self.level(), self.members)
    }
}

async fn stored(conn: &Connection, group: i64) -> Result<Stored, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT g.river_banked
                     + COALESCE((SELECT SUM(json_extract(a.run, '$.bridges')) {LAID}), 0),
                     (SELECT COUNT(*) FROM group_members m JOIN users u ON u.id = m.user_id
                      WHERE m.group_id = ?1 AND u.banned_at IS NULL)
                 FROM groups g WHERE g.id = ?1"
            ),
            params![group],
        )
        .await
        .map_err(db_err)?;
    let (total, members) = match rows.next().await.map_err(db_err)? {
        Some(row) => (
            row.get::<i64>(0).map_err(db_err)?,
            row.get::<i64>(1).map_err(db_err)?,
        ),
        None => (0, 0),
    };
    drop(rows);
    let mut rows = conn
        .query(
            "SELECT level, target, completed_at FROM group_river WHERE group_id = ?1 ORDER BY level",
            params![group],
        )
        .await
        .map_err(db_err)?;
    let mut levels = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        levels.push(RiverLevel {
            level: u32::try_from(row.get::<i64>(0).map_err(db_err)?).unwrap_or(0),
            target: row.get(1).map_err(db_err)?,
            completed_at: row.get(2).map_err(db_err)?,
        });
    }
    Ok(Stored {
        members: usize::try_from(members).unwrap_or(0),
        total,
        levels,
    })
}

/// The members' solved dailies with bridges finished after `after`, in the order they were.
async fn laid_after(conn: &Connection, group: i64, after: i64) -> Result<Vec<Laid>, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT a.finished_at, a.user_id, json_extract(a.run, '$.bridges') {LAID}
                   AND a.finished_at > ?2 AND json_extract(a.run, '$.bridges') > 0
                 ORDER BY a.finished_at, a.user_id"
            ),
            params![group, after],
        )
        .await
        .map_err(db_err)?;
    let mut laid = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        laid.push(Laid {
            at: row.get(0).map_err(db_err)?,
            user_id: row.get(1).map_err(db_err)?,
            bridges: row.get(2).map_err(db_err)?,
        });
    }
    Ok(laid)
}

/// Keeps the levels the river completed since it was looked at last, each once (two members
/// looking at the same time find them once), with their crew. Returns the crew, whose stamps
/// change.
async fn complete(conn: &Connection, group: i64, now: i64) -> Result<Vec<i64>, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let river = stored(&tx, group).await?;
    let base = river.base();
    if river.total - base < river.target() {
        tx.commit().await.map_err(db_err)?;
        return Ok(Vec::new());
    }
    let last = river.levels.last().map_or(i64::MIN, |l| l.completed_at);
    let laid = laid_after(&tx, group, last).await?;
    let before = river.total - laid.iter().map(|l| l.bridges).sum::<i64>();
    let mut crew = Vec::new();
    for c in crossings(river.level(), base, before, &laid, river.members, now) {
        let added = tx
            .execute(
                "INSERT OR IGNORE INTO group_river (group_id, level, target, completed_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![group, c.level, c.target, c.at],
            )
            .await
            .map_err(db_err)?;
        if added == 0 {
            continue;
        }
        for &user in &c.crew {
            tx.execute(
                "INSERT OR IGNORE INTO group_river_crew (group_id, level, user_id) VALUES (?1, ?2, ?3)",
                params![group, c.level, user],
            )
            .await
            .map_err(db_err)?;
        }
        crew.extend(c.crew);
    }
    tx.commit().await.map_err(db_err)?;
    crew.sort_unstable();
    crew.dedup();
    Ok(crew)
}

/// What each member laid in the week of `today` (its dailies, Monday to Sunday), most first.
async fn week(
    conn: &Connection,
    group: i64,
    today: i64,
) -> Result<Vec<Contribution>, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT a.user_id, SUM(json_extract(a.run, '$.bridges')) {LAID}
                   AND a.day >= ?2 GROUP BY a.user_id"
            ),
            params![group, monday_of(today)],
        )
        .await
        .map_err(db_err)?;
    let mut laid = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        laid.push(Contribution {
            user_id: row.get(0).map_err(db_err)?,
            bridges: row.get::<Option<i64>>(1).map_err(db_err)?.unwrap_or(0),
        });
    }
    Ok(ranking(laid))
}

/// The river of a group, for a member.
///
/// Completes the levels that are across, and remembers how far the member saw it: the next look
/// shows the planks laid since as new, and a level completed in between is celebrated once.
pub async fn river(
    conn: &Connection,
    group: i64,
    user: i64,
    today: i64,
) -> Result<GroupRiver, ServerFnError> {
    let Group { id, .. } = require_member(conn, group, user).await?;
    for member in complete(conn, id, now_ms()).await? {
        // Their stamp book has the river's stamp now
        sync_stamps(conn, member, false).await?;
    }
    let river = stored(conn, id).await?;
    let base = river.base();

    let mut rows = conn
        .query(
            "SELECT river_seen, river_levels_seen, joined_at FROM group_members
             WHERE group_id = ?1 AND user_id = ?2",
            params![id, user],
        )
        .await
        .map_err(db_err)?;
    let (seen, levels_seen, joined) = match rows.next().await.map_err(db_err)? {
        Some(row) => (
            row.get::<i64>(0).map_err(db_err)?,
            row.get::<i64>(1).map_err(db_err)?,
            row.get::<i64>(2).map_err(db_err)?,
        ),
        None => (0, 0, 0),
    };
    drop(rows);
    let celebrate = river
        .levels
        .iter()
        .rfind(|l| i64::from(l.level) >= levels_seen && l.completed_at >= joined)
        .copied();
    let completed = i64::try_from(river.levels.len()).unwrap_or(i64::MAX);
    if (seen, levels_seen) != (river.total, completed) {
        conn.execute(
            "UPDATE group_members SET river_seen = ?3, river_levels_seen = ?4
             WHERE group_id = ?1 AND user_id = ?2",
            params![id, user, river.total, completed],
        )
        .await
        .map_err(db_err)?;
    }
    let target = river.target();
    Ok(GroupRiver {
        group_id: id,
        level: river.level(),
        base,
        // Below the base when a result was taken back after a level was completed
        progress: (river.total - base).clamp(0, target),
        target,
        total: river.total,
        week: week(conn, id, today).await?,
        seen,
        celebrate,
        completed: river.levels,
    })
}
