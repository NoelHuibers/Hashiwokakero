//! Blocks: a player blocks another one, whose name then never reaches them.
//!
//! Wherever other players' names go out (the daily, monthly and all time boards, rivals, groups
//! with their boards, banzuke, week, replays, hanko and puzzles), the server functions pass what
//! they send through [`hidden`], which puts [`BLOCKED_NAME`] in the place of a blocked player's
//! name. The rows stay, so places and ranks don't change; the pages show "blocked player" without
//! a link. A puzzle of theirs loses its title too.
//!
//! One-sided and private, like rivals: the blocked player isn't told and sees nothing change.
//! Blocking unmarks them as a rival, and a blocked player can't be marked as one. Their own
//! profile still shows their name, it's where to unblock them (also on the account page).

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::db::db_err;
use super::now_ms;
use crate::model::{
    AllTimeEntry, ApiError, BLOCKED_NAME, Banzuke, BanzukeRow, BlockedPlayer, GroupMember,
    GroupPage, GroupPuzzles, GroupToday, GroupWeek, HallOfFame, HankoReceived, InvitePreview,
    Kudos, Lane, Leaderboard, LeaderboardEntry, MonthBoard, MonthEntry, PuzzlePage, PuzzleSummary,
    ReplayData, RivalDay, RivalRow, RivalsBoard, RivalsToday, TodayEntry, WeekRow,
};

/// `user` blocks `player` (`on`) or unblocks them, and returns the players blocked then.
/// Blocking unmarks them as a rival.
pub async fn set(
    conn: &Connection,
    user: i64,
    player: i64,
    on: bool,
) -> Result<Vec<BlockedPlayer>, ServerFnError> {
    if !on {
        conn.execute(
            "DELETE FROM blocks WHERE user_id = ?1 AND blocked_id = ?2",
            params![user, player],
        )
        .await
        .map_err(db_err)?;
        return list(conn, user).await;
    }
    if player == user {
        return Err(ApiError::NoSuchPlayer.into());
    }
    let tx = conn.transaction().await.map_err(db_err)?;
    // Only a player who is there, the statement says if they are
    let added = tx
        .execute(
            "INSERT OR IGNORE INTO blocks (user_id, blocked_id, created_at)
             SELECT ?1, id, ?3 FROM users WHERE id = ?2 AND deleted_at IS NULL",
            params![user, player, now_ms()],
        )
        .await
        .map_err(db_err)?;
    tx.execute(
        "DELETE FROM rivals WHERE user_id = ?1 AND rival_id = ?2",
        params![user, player],
    )
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    let now = list(conn, user).await?;
    if added == 0 && !now.iter().any(|b| b.id == player) {
        return Err(ApiError::NoSuchPlayer.into());
    }
    Ok(now)
}

/// The players `user` blocked, the latest first.
pub async fn list(conn: &Connection, user: i64) -> Result<Vec<BlockedPlayer>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT u.id, u.name, u.username, b.created_at FROM blocks b
             JOIN users u ON u.id = b.blocked_id
             WHERE b.user_id = ?1 AND u.deleted_at IS NULL
             ORDER BY b.created_at DESC, u.id",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        out.push(BlockedPlayer {
            id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            guest: row.get::<Option<String>>(2).map_err(db_err)?.is_none(),
            blocked_at: row.get(3).map_err(db_err)?,
        });
    }
    Ok(out)
}

/// Whether `user` blocked `player`.
pub async fn is_blocked(conn: &Connection, user: i64, player: i64) -> Result<bool, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT 1 FROM blocks WHERE user_id = ?1 AND blocked_id = ?2",
            params![user, player],
        )
        .await
        .map_err(db_err)?;
    Ok(rows.next().await.map_err(db_err)?.is_some())
}

/// For `auth::erase`, `?1` the user: whom they blocked, and who blocked them (their name is gone
/// anyway).
pub const ERASE: [&str; 1] = ["DELETE FROM blocks WHERE user_id = ?1 OR blocked_id = ?1"];

/// For `auth::merge_guest`, after the rivals: `?1` the account, `?2` the guest of the browser.
///
/// The account keeps the guest's blocks, and who blocked the guest blocks the account (the same
/// player). Nobody blocks themselves, and nobody has a player they blocked as a rival.
pub const MERGE_GUEST: [&str; 5] = [
    "DELETE FROM blocks WHERE (user_id = ?1 AND blocked_id = ?2) OR (user_id = ?2 AND blocked_id = ?1)",
    "UPDATE OR IGNORE blocks SET user_id = ?1 WHERE user_id = ?2",
    "UPDATE OR IGNORE blocks SET blocked_id = ?1 WHERE blocked_id = ?2",
    "DELETE FROM blocks WHERE user_id = ?2 OR blocked_id = ?2",
    "DELETE FROM rivals WHERE (user_id IN (?1, ?2) OR rival_id IN (?1, ?2))
        AND EXISTS (SELECT 1 FROM blocks b WHERE b.user_id = rivals.user_id AND b.blocked_id = rivals.rival_id)",
];

/// The players a viewer blocked: their ids, and their names for the few places that only have a
/// name (the winner of a closed group puzzle).
#[derive(Debug, Default)]
pub struct Blocked {
    ids: HashSet<i64>,
    names: HashSet<String>,
}

impl Blocked {
    /// Whom `viewer` blocked, nobody without a viewer.
    pub async fn of(conn: &Connection, viewer: Option<i64>) -> Result<Self, ServerFnError> {
        let mut blocked = Self::default();
        let Some(viewer) = viewer else {
            return Ok(blocked);
        };
        let mut rows = conn
            .query(
                "SELECT b.blocked_id, u.name FROM blocks b JOIN users u ON u.id = b.blocked_id
                 WHERE b.user_id = ?1",
                params![viewer],
            )
            .await
            .map_err(db_err)?;
        while let Some(row) = rows.next().await.map_err(db_err)? {
            blocked.ids.insert(row.get(0).map_err(db_err)?);
            blocked.names.insert(row.get(1).map_err(db_err)?);
        }
        Ok(blocked)
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Puts [`BLOCKED_NAME`] in place of `name` if `user` is blocked, and says if it did.
    fn name(&self, user: i64, name: &mut String) -> bool {
        let blocked = self.ids.contains(&user);
        if blocked {
            BLOCKED_NAME.clone_into(name);
        }
        blocked
    }

    /// The same for a name without its player. Guests can share a generated name, then both are
    /// hidden: better one too many than one too few.
    fn bare_name(&self, name: &mut String) {
        if self.names.contains(name.as_str()) {
            BLOCKED_NAME.clone_into(name);
        }
    }
}

/// What the server sends that has other players' names in it.
pub trait Hide {
    /// Hides the names (and puzzle titles) of the `blocked` players.
    fn hide(&mut self, blocked: &Blocked);
}

/// `value` with the names of the players `viewer` blocked hidden.
pub async fn hidden<T: Hide>(
    conn: &Connection,
    viewer: Option<i64>,
    mut value: T,
) -> Result<T, ServerFnError> {
    let blocked = Blocked::of(conn, viewer).await?;
    if !blocked.is_empty() {
        value.hide(&blocked);
    }
    Ok(value)
}

impl<T: Hide> Hide for Vec<T> {
    fn hide(&mut self, blocked: &Blocked) {
        for item in self {
            item.hide(blocked);
        }
    }
}

impl<T: Hide> Hide for Option<T> {
    fn hide(&mut self, blocked: &Blocked) {
        if let Some(item) = self {
            item.hide(blocked);
        }
    }
}

/// Types with a player's `user_id` and `name`.
macro_rules! named {
    ($($t:ty),* $(,)?) => {$(
        impl Hide for $t {
            fn hide(&mut self, blocked: &Blocked) {
                blocked.name(self.user_id, &mut self.name);
            }
        }
    )*};
}

named!(
    LeaderboardEntry,
    MonthEntry,
    AllTimeEntry,
    RivalDay,
    GroupMember,
    TodayEntry,
    WeekRow,
    BanzukeRow,
    Lane,
);

impl Hide for Leaderboard {
    fn hide(&mut self, blocked: &Blocked) {
        self.entries.hide(blocked);
        self.me.hide(blocked);
    }
}

impl Hide for MonthBoard {
    fn hide(&mut self, blocked: &Blocked) {
        self.entries.hide(blocked);
        self.me.hide(blocked);
    }
}

impl Hide for HallOfFame {
    fn hide(&mut self, blocked: &Blocked) {
        self.solved.hide(blocked);
        self.medals.hide(blocked);
        self.streaks.hide(blocked);
    }
}

impl Hide for RivalsToday {
    fn hide(&mut self, blocked: &Blocked) {
        self.rivals.hide(blocked);
    }
}

impl Hide for RivalRow {
    fn hide(&mut self, blocked: &Blocked) {
        self.entry.hide(blocked);
    }
}

impl Hide for RivalsBoard {
    fn hide(&mut self, blocked: &Blocked) {
        self.rows.hide(blocked);
    }
}

impl Hide for GroupToday {
    fn hide(&mut self, blocked: &Blocked) {
        self.entries.hide(blocked);
    }
}

impl Hide for GroupPage {
    fn hide(&mut self, blocked: &Blocked) {
        self.members.hide(blocked);
        self.today.hide(blocked);
    }
}

impl Hide for InvitePreview {
    fn hide(&mut self, blocked: &Blocked) {
        self.members.hide(blocked);
    }
}

impl Hide for GroupWeek {
    fn hide(&mut self, blocked: &Blocked) {
        self.rows.hide(blocked);
    }
}

impl Hide for Banzuke {
    fn hide(&mut self, blocked: &Blocked) {
        self.rows.hide(blocked);
    }
}

impl Hide for ReplayData {
    fn hide(&mut self, blocked: &Blocked) {
        self.lanes.hide(blocked);
    }
}

impl Hide for HankoReceived {
    fn hide(&mut self, blocked: &Blocked) {
        blocked.name(self.giver, &mut self.giver_name);
    }
}

/// A puzzle of a blocked author loses its title too: the page names it "a member's puzzle".
impl Hide for PuzzleSummary {
    fn hide(&mut self, blocked: &Blocked) {
        if let Some(author) = &mut self.author
            && blocked.name(author.user_id, &mut author.name)
        {
            self.title = None;
        }
        if let Some((name, _)) = &mut self.winner {
            blocked.bare_name(name);
        }
    }
}

impl Hide for Kudos {
    fn hide(&mut self, blocked: &Blocked) {
        self.puzzle.hide(blocked);
    }
}

impl Hide for GroupPuzzles {
    fn hide(&mut self, blocked: &Blocked) {
        self.open.hide(blocked);
        self.closed.hide(blocked);
        self.kudos.hide(blocked);
    }
}

impl Hide for PuzzlePage {
    fn hide(&mut self, blocked: &Blocked) {
        self.summary.hide(blocked);
        self.board.hide(blocked);
    }
}
