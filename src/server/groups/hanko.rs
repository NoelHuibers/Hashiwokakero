//! Hanko 判子: seals members press on each other's daily results in a group (`model::hanko`).
//!
//! A seal is stored with the group, the day, whose result it is on (`target_id`), who pressed it
//! (`giver_id`) and its kind, once per kind and giver. Pressing needs both results finished (the
//! presser's own too, the spoiler rule of the boards), and never on one's own. The one who got
//! it sees it on the group page and on their daily result, until then it counts as unread.
//!
//! Seals go with the membership of either side (a trigger on `group_members`, see the
//! migration), with the group, and with an account.

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::{members, require_member};
use crate::model::{
    ApiError, GroupMember, HANKO_DAYS, HankoKind, HankoMark, HankoReceived, UnreadHanko,
};
use crate::server::db::db_err;
use crate::server::now_ms;

/// What deleting a user does to their seals, for `auth::erase` (user `?1`): the ones they pressed
/// and the ones they got. Leaving their groups takes them along too, this is for certain.
pub const ERASE: [&str; 1] = ["DELETE FROM group_hanko WHERE giver_id = ?1 OR target_id = ?1"];

/// What signing in does to the seals of the guest who played in this browser before.
///
/// For `auth::merge_guest` (`?1` the account, `?2` the guest), before their memberships move: the
/// account takes them over, given and got. Seals the account had already, and seals it would
/// have pressed on its own result (both were in the group), go.
pub const MERGE_GUEST: [&str; 3] = [
    "UPDATE OR IGNORE group_hanko SET giver_id = ?1 WHERE giver_id = ?2",
    "UPDATE OR IGNORE group_hanko SET target_id = ?1 WHERE target_id = ?2",
    "DELETE FROM group_hanko WHERE giver_id = target_id OR giver_id = ?2 OR target_id = ?2",
];

/// The days whose results take seals and show the ones got: from this one on.
fn first_day(today: i64) -> i64 {
    today - HANKO_DAYS + 1
}

/// The seals on the results of a group's board of `day`, of and by its current `members` (so a
/// banned member's are hidden), in the order they were pressed.
pub async fn marks(
    conn: &Connection,
    group: i64,
    day: i64,
    members: &[GroupMember],
) -> Result<Vec<HankoMark>, ServerFnError> {
    let ids: HashSet<i64> = members.iter().map(|m| m.user_id).collect();
    let mut rows = conn
        .query(
            "SELECT target_id, giver_id, kind FROM group_hanko
             WHERE group_id = ?1 AND day = ?2 ORDER BY created_at, rowid",
            params![group, day],
        )
        .await
        .map_err(db_err)?;
    let mut marks = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let target: i64 = row.get(0).map_err(db_err)?;
        let giver: i64 = row.get(1).map_err(db_err)?;
        let kind: String = row.get(2).map_err(db_err)?;
        if let Some(kind) = HankoKind::from_id(&kind)
            && ids.contains(&target)
            && ids.contains(&giver)
        {
            marks.push(HankoMark {
                target,
                giver,
                kind,
            });
        }
    }
    Ok(marks)
}

/// Whether `user` finished the daily of `day`: solved or given up.
async fn finished(conn: &Connection, user: i64, day: i64) -> Result<bool, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT 1 FROM attempts WHERE user_id = ?1 AND day = ?2
               AND (finished_at IS NOT NULL OR revealed != 0)",
            params![user, day],
        )
        .await
        .map_err(db_err)?;
    Ok(rows.next().await.map_err(db_err)?.is_some())
}

/// `user` presses a seal of `kind` on `target`'s result of `day` in a group (`on`), or takes it
/// back. Returns the seals of the board of that day, for it to show.
///
/// Only a member, on another member's finished result, after finishing that daily themselves,
/// and only for [`HANKO_DAYS`]. Pressing one pressed already, or taking back one that isn't,
/// changes nothing.
#[expect(
    clippy::too_many_arguments,
    reason = "the seal, who presses it where, and the day it is now"
)]
pub async fn press(
    conn: &Connection,
    group: i64,
    user: i64,
    target: i64,
    day: i64,
    kind: HankoKind,
    on: bool,
    today: i64,
) -> Result<Vec<HankoMark>, ServerFnError> {
    let group = require_member(conn, group, user).await?;
    let members = members(conn, &group, today).await?;
    let allowed = target != user
        && (first_day(today)..=today).contains(&day)
        && members.iter().any(|m| m.user_id == target)
        && members.iter().any(|m| m.user_id == user)
        && finished(conn, user, day).await?
        && finished(conn, target, day).await?;
    if !allowed {
        return Err(ApiError::HankoNotAllowed.into());
    }
    if on {
        conn.execute(
            "INSERT OR IGNORE INTO group_hanko (group_id, day, target_id, giver_id, kind, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![group.id, day, target, user, kind.id(), now_ms()],
        )
        .await
        .map_err(db_err)?;
    } else {
        conn.execute(
            "DELETE FROM group_hanko
             WHERE group_id = ?1 AND day = ?2 AND target_id = ?3 AND giver_id = ?4 AND kind = ?5",
            params![group.id, day, target, user, kind.id()],
        )
        .await
        .map_err(db_err)?;
    }
    marks(conn, group.id, day, &members).await
}

/// Seals shown in the list of the ones got at most: more would be a long list.
const RECEIVED_MAX: i64 = 60;

/// The seals `user` got on their results of the last [`HANKO_DAYS`], newest first, in one group
/// (`Some`) or all of theirs. Opening them marks them seen: the ones that weren't are `fresh`.
pub async fn received(
    conn: &Connection,
    user: i64,
    group: Option<i64>,
    today: i64,
) -> Result<Vec<HankoReceived>, ServerFnError> {
    if let Some(group) = group {
        require_member(conn, group, user).await?;
    }
    let first = first_day(today);
    let mut rows = conn
        .query(
            "SELECT h.group_id, g.name, h.day, h.giver_id, u.name, m.colour, h.kind, h.created_at, h.seen
             FROM group_hanko h
             JOIN groups g ON g.id = h.group_id
             JOIN group_members m ON m.group_id = h.group_id AND m.user_id = h.giver_id
             JOIN users u ON u.id = h.giver_id AND u.banned_at IS NULL
             WHERE h.target_id = ?1 AND h.day >= ?2 AND (?3 IS NULL OR h.group_id = ?3)
             ORDER BY h.created_at DESC, h.rowid DESC
             LIMIT ?4",
            params![user, first, group, RECEIVED_MAX],
        )
        .await
        .map_err(db_err)?;
    let mut got = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let kind: String = row.get(6).map_err(db_err)?;
        let Some(kind) = HankoKind::from_id(&kind) else {
            continue;
        };
        got.push(HankoReceived {
            group_id: row.get(0).map_err(db_err)?,
            group_name: row.get(1).map_err(db_err)?,
            day: row.get(2).map_err(db_err)?,
            giver: row.get(3).map_err(db_err)?,
            giver_name: row.get(4).map_err(db_err)?,
            giver_colour: u8::try_from(row.get::<i64>(5).map_err(db_err)?).unwrap_or(0),
            kind,
            at: row.get(7).map_err(db_err)?,
            fresh: row.get::<i64>(8).map_err(db_err)? == 0,
        });
    }
    drop(rows);
    if got.iter().any(|h| h.fresh) {
        conn.execute(
            "UPDATE group_hanko SET seen = 1
             WHERE target_id = ?1 AND seen = 0 AND day >= ?2 AND (?3 IS NULL OR group_id = ?3)",
            params![user, first, group],
        )
        .await
        .map_err(db_err)?;
    }
    Ok(got)
}

/// How many seals `user` got and didn't see yet, per group (only groups with some).
pub async fn unread(
    conn: &Connection,
    user: i64,
    today: i64,
) -> Result<Vec<UnreadHanko>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT h.group_id, COUNT(*) FROM group_hanko h
             JOIN users u ON u.id = h.giver_id AND u.banned_at IS NULL
             WHERE h.target_id = ?1 AND h.seen = 0 AND h.day >= ?2
             GROUP BY h.group_id ORDER BY h.group_id",
            params![user, first_day(today)],
        )
        .await
        .map_err(db_err)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        out.push(UnreadHanko {
            group_id: row.get(0).map_err(db_err)?,
            count: row.get(1).map_err(db_err)?,
        });
    }
    Ok(out)
}
