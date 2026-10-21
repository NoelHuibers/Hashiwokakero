//! Groups (仲間 nakama): players who compare their dailies with each other.
//!
//! Anyone with a user (guests too) can make a group and share its invite link, `/g/<code>`.
//! Opening the link only shows the group; joining takes a click ([`join`]). The owner can replace
//! the code, which ends the old links, and remove members. Everyone can leave; when the owner
//! does, the longest standing member takes over, and the last one to leave takes the group with
//! them.
//!
//! Members see each other's names and dailies, nobody else sees anything of a group: every
//! request for a group's data goes through [`require_member`], which answers a group the player
//! isn't in like one that doesn't exist.

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use leptos::prelude::ServerFnError;
use libsql::{Connection, TransactionBehavior, params};

use super::daily::shown_stamps;
use super::db::db_err;
use super::{names, now_ms};
use crate::model::{
    ApiError, BRIDGE_DAYS, GroupMember, GroupPage, GroupSummary, GroupToday, GroupWeek,
    INVITE_ALPHABET, INVITE_LEN, InvitePreview, MAX_GROUPS, MAX_MEMBERS, SharedDays, TodayEntry,
    TodayState, clean_group_name, free_colour, group_day, monday_of, spoiler_safe, valid_invite,
    week_rows,
};

/// A group as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub invite: String,
    pub owner_id: i64,
    pub created_at: i64,
}

/// What deleting a user does to their groups.
///
/// Statements for `auth::erase`'s transaction with the user as `?1`: groups only they are in go,
/// groups they own pass to the longest standing other member, and their memberships go.
pub const ERASE: [&str; 4] = [
    "DELETE FROM groups WHERE id IN (SELECT group_id FROM group_members WHERE user_id = ?1)
       AND NOT EXISTS (SELECT 1 FROM group_members m WHERE m.group_id = groups.id AND m.user_id != ?1)",
    "UPDATE groups SET owner_id = (
         SELECT m.user_id FROM group_members m WHERE m.group_id = groups.id AND m.user_id != ?1
         ORDER BY m.joined_at, m.user_id LIMIT 1)
     WHERE owner_id = ?1
       AND EXISTS (SELECT 1 FROM group_members m WHERE m.group_id = groups.id AND m.user_id != ?1)",
    // Owned without being in it, which never happens: nobody to pass it to
    "DELETE FROM groups WHERE owner_id = ?1",
    "DELETE FROM group_members WHERE user_id = ?1",
];

/// What signing in does to the groups of the guest who played in this browser before.
///
/// Statements for `auth::merge_guest`, `?1` the account, `?2` the guest: the account takes their
/// memberships and the groups they own. Where both were members, the account's membership stays.
pub const MERGE_GUEST: [&str; 3] = [
    "UPDATE OR IGNORE group_members SET user_id = ?1 WHERE user_id = ?2",
    "UPDATE groups SET owner_id = ?1 WHERE owner_id = ?2",
    "DELETE FROM group_members WHERE user_id = ?2",
];

/// A new invite code, [`INVITE_LEN`] characters of [`INVITE_ALPHABET`].
pub fn new_invite() -> String {
    let mut bytes = [0u8; INVITE_LEN];
    getrandom::fill(&mut bytes).expect("no system randomness");
    // 256 is a multiple of the alphabet's 32 characters: every one is as likely
    bytes
        .iter()
        .map(|b| char::from(INVITE_ALPHABET[usize::from(*b) % INVITE_ALPHABET.len()]))
        .collect()
}

/// A group name as stored, or why it can't be one. The same words aren't welcome as in
/// usernames (`server/names.rs`).
pub fn check_name(name: &str) -> Result<String, ServerFnError> {
    let name = clean_group_name(name).ok_or_else(|| ApiError::GroupNameInvalid.fail())?;
    if names::blocked(&name).is_some() {
        return Err(ApiError::GroupNameNotAllowed.into());
    }
    Ok(name)
}

fn row_to_group(row: &libsql::Row) -> Result<Group, libsql::Error> {
    Ok(Group {
        id: row.get(0)?,
        name: row.get(1)?,
        invite: row.get(2)?,
        owner_id: row.get(3)?,
        created_at: row.get(4)?,
    })
}

const GROUP_COLUMNS: &str = "g.id, g.name, g.invite, g.owner_id, g.created_at";

async fn count(conn: &Connection, sql: &str, id: i64) -> Result<i64, ServerFnError> {
    let mut rows = conn.query(sql, params![id]).await.map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err),
        None => Ok(0),
    }
}

/// How many groups a player is in.
pub async fn groups_of(conn: &Connection, user: i64) -> Result<i64, ServerFnError> {
    count(
        conn,
        "SELECT COUNT(*) FROM group_members WHERE user_id = ?1",
        user,
    )
    .await
}

async fn member_count(conn: &Connection, group: i64) -> Result<i64, ServerFnError> {
    count(
        conn,
        "SELECT COUNT(*) FROM group_members WHERE group_id = ?1",
        group,
    )
    .await
}

/// The group, if `user` is a member of it. Every request for a group's data starts here: a group
/// the player isn't in is [`ApiError::GroupNotFound`], like one that doesn't exist.
pub async fn require_member(
    conn: &Connection,
    group: i64,
    user: i64,
) -> Result<Group, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT {GROUP_COLUMNS} FROM groups g
                 JOIN group_members m ON m.group_id = g.id AND m.user_id = ?2
                 WHERE g.id = ?1"
            ),
            params![group, user],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row_to_group(&row).map_err(db_err),
        None => Err(ApiError::GroupNotFound.into()),
    }
}

/// The group, if `user` owns it.
async fn require_owner(conn: &Connection, group: i64, user: i64) -> Result<Group, ServerFnError> {
    let group = require_member(conn, group, user).await?;
    if group.owner_id == user {
        Ok(group)
    } else {
        Err(ApiError::GroupOwnerOnly.into())
    }
}

async fn by_invite(conn: &Connection, code: &str) -> Result<Option<Group>, ServerFnError> {
    if !valid_invite(code) {
        return Ok(None);
    }
    let mut rows = conn
        .query(
            &format!("SELECT {GROUP_COLUMNS} FROM groups g WHERE g.invite = ?1"),
            params![code],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => Ok(Some(row_to_group(&row).map_err(db_err)?)),
        None => Ok(None),
    }
}

/// Makes a group with `user` as its owner and first member.
pub async fn create(conn: &Connection, user: i64, name: &str) -> Result<i64, ServerFnError> {
    let name = check_name(name)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    if groups_of(&tx, user).await? >= crate::server::count(MAX_GROUPS) {
        return Err(ApiError::TooManyGroups.into());
    }
    let now = now_ms();
    // A code that is taken already is as likely as guessing one, but try another then
    let mut inserted = None;
    for _ in 0..5 {
        let added = tx
            .execute(
                "INSERT OR IGNORE INTO groups (name, invite, owner_id, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![name.clone(), new_invite(), user, now],
            )
            .await
            .map_err(db_err)?;
        if added > 0 {
            inserted = Some(tx.last_insert_rowid());
            break;
        }
    }
    let id = inserted.ok_or_else(|| ApiError::Internal.fail())?;
    tx.execute(
        "INSERT INTO group_members (group_id, user_id, joined_at, colour) VALUES (?1, ?2, ?3, ?4)",
        params![id, user, now, free_colour(&[])],
    )
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(id)
}

/// What the invite link shows: the group's name and members, `None` if the code doesn't work.
pub async fn preview(
    conn: &Connection,
    code: &str,
    user: Option<i64>,
    today: i64,
) -> Result<Option<InvitePreview>, ServerFnError> {
    let Some(group) = by_invite(conn, code).await? else {
        return Ok(None);
    };
    let members = members(conn, &group, today).await?;
    let joined = user
        .filter(|u| members.iter().any(|m| m.user_id == *u))
        .map(|_| group.id);
    let full = member_count(conn, group.id).await? >= crate::server::count(MAX_MEMBERS);
    Ok(Some(InvitePreview {
        id: group.id,
        name: group.name,
        members,
        joined,
        full,
    }))
}

/// Joins the group of an invite code, returns its id. Joining a group one is in already does
/// nothing.
pub async fn join(conn: &Connection, code: &str, user: i64) -> Result<i64, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let group = by_invite(&tx, code)
        .await?
        .ok_or_else(|| ApiError::GroupNotFound.fail())?;
    let mut rows = tx
        .query(
            "SELECT user_id, colour FROM group_members WHERE group_id = ?1",
            params![group.id],
        )
        .await
        .map_err(db_err)?;
    let mut taken = Vec::new();
    let mut member = false;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        member |= row.get::<i64>(0).map_err(db_err)? == user;
        taken.push(u8::try_from(row.get::<i64>(1).map_err(db_err)?).unwrap_or(0));
    }
    drop(rows);
    if member {
        return Ok(group.id);
    }
    if taken.len() >= MAX_MEMBERS {
        return Err(ApiError::GroupFull.into());
    }
    if groups_of(&tx, user).await? >= crate::server::count(MAX_GROUPS) {
        return Err(ApiError::TooManyGroups.into());
    }
    tx.execute(
        "INSERT INTO group_members (group_id, user_id, joined_at, colour) VALUES (?1, ?2, ?3, ?4)",
        params![group.id, user, now_ms(), free_colour(&taken)],
    )
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(group.id)
}

/// Takes `user` out of a group. An owner's group passes to the longest standing member, an empty
/// group is deleted.
async fn take_out(conn: &Connection, group: &Group, user: i64) -> Result<(), ServerFnError> {
    conn.execute(
        "DELETE FROM group_members WHERE group_id = ?1 AND user_id = ?2",
        params![group.id, user],
    )
    .await
    .map_err(db_err)?;
    let mut rows = conn
        .query(
            "SELECT user_id FROM group_members WHERE group_id = ?1 ORDER BY joined_at, user_id LIMIT 1",
            params![group.id],
        )
        .await
        .map_err(db_err)?;
    let next = match rows.next().await.map_err(db_err)? {
        Some(row) => Some(row.get::<i64>(0).map_err(db_err)?),
        None => None,
    };
    drop(rows);
    match next {
        None => {
            conn.execute("DELETE FROM groups WHERE id = ?1", params![group.id])
                .await
                .map_err(db_err)?;
        }
        Some(next) if group.owner_id == user => {
            conn.execute(
                "UPDATE groups SET owner_id = ?2 WHERE id = ?1",
                params![group.id, next],
            )
            .await
            .map_err(db_err)?;
        }
        Some(_) => {}
    }
    Ok(())
}

/// Leaves a group.
pub async fn leave(conn: &Connection, group: i64, user: i64) -> Result<(), ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let group = require_member(&tx, group, user).await?;
    take_out(&tx, &group, user).await?;
    tx.commit().await.map_err(db_err)
}

/// The owner removes a member. They can join again with the link, unless the owner replaces it
/// ([`rotate_invite`]). Removing oneself is leaving.
pub async fn remove(
    conn: &Connection,
    group: i64,
    owner: i64,
    member: i64,
) -> Result<(), ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let group = require_owner(&tx, group, owner).await?;
    take_out(&tx, &group, member).await?;
    tx.commit().await.map_err(db_err)
}

/// Replaces a group's invite code: the old links stop working.
pub async fn rotate_invite(
    conn: &Connection,
    group: i64,
    owner: i64,
) -> Result<String, ServerFnError> {
    let group = require_owner(conn, group, owner).await?;
    for _ in 0..5 {
        let code = new_invite();
        let changed = conn
            .execute(
                "UPDATE OR IGNORE groups SET invite = ?2 WHERE id = ?1",
                params![group.id, code.clone()],
            )
            .await
            .map_err(db_err)?;
        if changed > 0 {
            return Ok(code);
        }
    }
    Err(ApiError::Internal.into())
}

/// The members of a group, longest standing first. Banned players are left out, like on the
/// boards.
pub async fn members(
    conn: &Connection,
    group: &Group,
    today: i64,
) -> Result<Vec<GroupMember>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT m.user_id, u.name, u.username, m.colour, m.joined_at,
                 EXISTS (SELECT 1 FROM attempts a WHERE a.user_id = m.user_id AND a.day = ?2
                         AND (a.finished_at IS NOT NULL OR a.revealed != 0))
             FROM group_members m JOIN users u ON u.id = m.user_id
             WHERE m.group_id = ?1 AND u.banned_at IS NULL
             ORDER BY m.joined_at, m.user_id",
            params![group.id, today],
        )
        .await
        .map_err(db_err)?;
    let mut members = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let user_id: i64 = row.get(0).map_err(db_err)?;
        members.push(GroupMember {
            user_id,
            name: row.get(1).map_err(db_err)?,
            guest: row.get::<Option<String>>(2).map_err(db_err)?.is_none(),
            colour: u8::try_from(row.get::<i64>(3).map_err(db_err)?).unwrap_or(0),
            joined_at: row.get(4).map_err(db_err)?,
            owner: user_id == group.owner_id,
            stamp: None,
            done_today: row.get::<i64>(5).map_err(db_err)? != 0,
        });
    }
    let ids: Vec<i64> = members.iter().map(|m| m.user_id).collect();
    let stamps = shown_stamps(conn, &ids).await?;
    for m in &mut members {
        m.stamp = stamps.get(&m.user_id).copied();
    }
    Ok(members)
}

/// The members' results of `day` and whether `viewer` finished it, before [`spoiler_safe`].
/// Ranked like the global board: finished, not given up, not flagged, by time and then who was
/// first; the others who finished have no time.
async fn day_entries(
    conn: &Connection,
    members: &[GroupMember],
    viewer: i64,
    day: i64,
) -> Result<(Vec<TodayEntry>, bool), ServerFnError> {
    let ids: Vec<String> = members.iter().map(|m| m.user_id.to_string()).collect();
    let mut results = HashMap::new();
    if !ids.is_empty() {
        // Ids are integers from the database
        let mut rows = conn
            .query(
                &format!(
                    "SELECT user_id, finished_at, revealed, flag, time_ms, hints, checks FROM attempts
                     WHERE day = ?1 AND user_id IN ({})",
                    ids.join(",")
                ),
                params![day],
            )
            .await
            .map_err(db_err)?;
        while let Some(row) = rows.next().await.map_err(db_err)? {
            let user: i64 = row.get(0).map_err(db_err)?;
            let finished: Option<i64> = row.get(1).map_err(db_err)?;
            let revealed = row.get::<i64>(2).map_err(db_err)? != 0;
            let flag: Option<String> = row.get(3).map_err(db_err)?;
            let time: Option<i64> = row.get(4).map_err(db_err)?;
            let penalties = (
                row.get::<i64>(5).map_err(db_err)?,
                row.get::<i64>(6).map_err(db_err)?,
            );
            let result = match (finished, time) {
                _ if revealed => Some(None),
                (Some(at), Some(time)) if flag.is_none() => Some(Some((time, at, penalties))),
                (Some(_), _) => Some(None),
                (None, _) => None,
            };
            if let Some(result) = result {
                results.insert(user, result);
            }
        }
    }
    let mut ranked: Vec<(i64, i64, i64)> = results
        .iter()
        .filter_map(|(&user, r)| r.map(|(time, at, _)| (time, at, user)))
        .collect();
    ranked.sort_unstable();
    let places: HashMap<i64, i64> = ranked
        .iter()
        .enumerate()
        .map(|(i, &(_, _, user))| (user, crate::server::count(i + 1)))
        .collect();
    let entries = members
        .iter()
        .map(|m| TodayEntry {
            user_id: m.user_id,
            name: m.name.clone(),
            guest: m.guest,
            colour: m.colour,
            stamp: m.stamp,
            state: match results.get(&m.user_id) {
                None => TodayState::Waiting,
                Some(None) => TodayState::Unranked,
                Some(Some((time_ms, _, (hints, checks)))) => TodayState::Ranked {
                    rank: places.get(&m.user_id).copied().unwrap_or(0),
                    time_ms: *time_ms,
                    hints: *hints,
                    checks: *checks,
                },
            },
        })
        .collect();
    Ok((entries, results.contains_key(&viewer)))
}

/// The group's board of `day` as `viewer` may see it (see [`spoiler_safe`]).
pub async fn today_board(
    conn: &Connection,
    group: &Group,
    members: &[GroupMember],
    viewer: i64,
    day: i64,
) -> Result<GroupToday, ServerFnError> {
    let (entries, open) = day_entries(conn, members, viewer, day).await?;
    Ok(GroupToday {
        group_id: group.id,
        group_name: group.name.clone(),
        day,
        open,
        entries: spoiler_safe(entries, open),
    })
}

/// Solved dailies `(user, day)` of some members between two days: finished, not given up.
async fn solved_days(
    conn: &Connection,
    members: &[i64],
    from: i64,
    to: i64,
) -> Result<Vec<(i64, i64)>, ServerFnError> {
    if members.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = members.iter().map(ToString::to_string).collect();
    let mut rows = conn
        .query(
            &format!(
                "SELECT user_id, day FROM attempts
                 WHERE day BETWEEN ?1 AND ?2 AND finished_at IS NOT NULL AND revealed = 0
                   AND user_id IN ({})",
                ids.join(",")
            ),
            params![from, to],
        )
        .await
        .map_err(db_err)?;
    let mut solved = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        solved.push((row.get(0).map_err(db_err)?, row.get(1).map_err(db_err)?));
    }
    Ok(solved)
}

/// For every pair of members (in the order of `members`), the days both solved, pairs without
/// one left out.
pub fn count_shared(members: &[i64], solved: &[(i64, i64)]) -> Vec<SharedDays> {
    let mut days: HashMap<i64, HashSet<i64>> = HashMap::new();
    for &(user, day) in solved {
        days.entry(user).or_default().insert(day);
    }
    let empty = HashSet::new();
    let mut shared = Vec::new();
    for (i, &a) in members.iter().enumerate() {
        let of_a = days.get(&a).unwrap_or(&empty);
        for &b in &members[i + 1..] {
            let both = days
                .get(&b)
                .map_or(0, |of_b| of_a.intersection(of_b).count());
            if both > 0 {
                shared.push(SharedDays {
                    a,
                    b,
                    days: crate::server::count(both),
                });
            }
        }
    }
    shared
}

/// Everything the group page shows when it opens, for a member.
pub async fn page(
    conn: &Connection,
    group: i64,
    user: i64,
    today: i64,
) -> Result<GroupPage, ServerFnError> {
    let group = require_member(conn, group, user).await?;
    let members = members(conn, &group, today).await?;
    let ids: Vec<i64> = members.iter().map(|m| m.user_id).collect();
    let solved = solved_days(conn, &ids, today - BRIDGE_DAYS + 1, today).await?;
    let today_board = today_board(conn, &group, &members, user, today).await?;
    Ok(GroupPage {
        id: group.id,
        name: group.name,
        created_at: group.created_at,
        owner_id: group.owner_id,
        me: user,
        invite: group.invite,
        shared: count_shared(&ids, &solved),
        members,
        today: today_board,
    })
}

/// The week grid of a group, the week of `monday` (any day of it), the current one if `None`.
/// Weeks before the group's first and after the current one are the nearest there is.
pub async fn week(
    conn: &Connection,
    group: i64,
    user: i64,
    monday: Option<i64>,
    today: i64,
) -> Result<GroupWeek, ServerFnError> {
    let group = require_member(conn, group, user).await?;
    let first = monday_of(group_day(group.created_at));
    let current = monday_of(today);
    let monday = monday
        .map_or(current, monday_of)
        .clamp(first.min(current), current);
    let members = members(conn, &group, today).await?;
    let ids: Vec<i64> = members.iter().map(|m| m.user_id).collect();
    let solved = solved_days(conn, &ids, monday, monday + 6).await?;
    Ok(GroupWeek {
        monday,
        today,
        first,
        rows: week_rows(&members, &solved, monday),
    })
}

/// The groups `user` is in, the ones joined first first.
async fn groups_by_user(conn: &Connection, user: i64) -> Result<Vec<Group>, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT {GROUP_COLUMNS} FROM groups g
                 JOIN group_members m ON m.group_id = g.id AND m.user_id = ?1
                 ORDER BY m.joined_at, g.id"
            ),
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut groups = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        groups.push(row_to_group(&row).map_err(db_err)?);
    }
    Ok(groups)
}

/// The list of the player's groups, with how many finished today.
pub async fn mine(
    conn: &Connection,
    user: i64,
    today: i64,
) -> Result<Vec<GroupSummary>, ServerFnError> {
    let mut out = Vec::new();
    for group in groups_by_user(conn, user).await? {
        let members = members(conn, &group, today).await?;
        out.push(GroupSummary {
            id: group.id,
            owner: group.owner_id == user,
            members: crate::server::count(members.len()),
            done_today: crate::server::count(members.iter().filter(|m| m.done_today).count()),
            colours: members.iter().map(|m| m.colour).collect(),
            name: group.name,
        });
    }
    Ok(out)
}

/// The boards of today of all the player's groups, for the daily page.
pub async fn boards(
    conn: &Connection,
    user: i64,
    today: i64,
) -> Result<Vec<GroupToday>, ServerFnError> {
    let mut out = Vec::new();
    for group in groups_by_user(conn, user).await? {
        let members = members(conn, &group, today).await?;
        out.push(today_board(conn, &group, &members, user, today).await?);
    }
    Ok(out)
}
