//! Replays (再生): the move logs of a day's daily for the replay theatre.
//!
//! A group's members see each other's logs, nobody else does: [`group_replay`] goes through
//! `groups::require_member`. And not before the viewer finished the day themselves, as long as
//! it is today: the logs, like the times, would spoil it ([`ReplayData::locked`]). Past days are
//! open, back to the day the group was made. [`own_replay`] is a player's own solve of any day.

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use common::Board;
use leptos::prelude::ServerFnError;
use libsql::{Connection, params};
use serde::Deserialize;

use super::daily::get_or_create;
use super::db::db_err;
use super::groups::{members, require_member};
use crate::api::{PaidAction, PaidOutcome};
use crate::model::{
    Lane, LaneOutcome, Mark, MarkKind, MoveEvent, ReplayData, ReplayGroup, clean_log, group_day,
};

/// Who a lane is for, before their attempt is looked up.
#[derive(Clone, Debug)]
struct Person {
    user_id: i64,
    name: String,
    guest: bool,
    colour: u8,
}

/// A paid help as `attempt::paid_action` stores it, the fields a replay needs.
#[derive(Debug, Deserialize)]
pub(crate) struct Receipt {
    action: PaidAction,
    bridges: Vec<u8>,
    outcome: PaidOutcome,
    /// Server time of the purchase. Receipts from before replays don't have it.
    #[serde(default)]
    at: Option<i64>,
}

impl Receipt {
    /// Whether it cost a penalty: a hint that found nothing to fix didn't.
    fn kind(&self) -> Option<MarkKind> {
        match (&self.action, &self.outcome) {
            (PaidAction::Hint, PaidOutcome::Hint(Some(_))) => Some(MarkKind::Hint),
            (PaidAction::Check, _) => Some(MarkKind::Check),
            _ => None,
        }
    }
}

/// When the paid helps of an attempt were taken, ms after the start like the log's times.
///
/// Receipts with their time (`at`) say so. For older ones the board they were bought on tells:
/// it is the board after some move of the log, the first such one after the previous help. A
/// help whose board never comes up gets no marker.
pub(crate) fn marks(
    receipts: &[Receipt],
    started_at: i64,
    log: &[MoveEvent],
    edges: usize,
) -> Vec<Mark> {
    let mut out = Vec::new();
    // The board after the first `k` moves, and how many edges differ from what a help is after
    let mut state = vec![0u8; edges];
    let mut k = 0;
    for r in receipts {
        let Some(kind) = r.kind() else { continue };
        if let Some(at) = r.at {
            let t = u32::try_from((at - started_at).max(0)).unwrap_or(u32::MAX);
            out.push(Mark { t, kind });
            continue;
        }
        if r.bridges.len() != edges {
            continue;
        }
        let mut s = state.clone();
        let mut diff = s.iter().zip(&r.bridges).filter(|(a, b)| a != b).count();
        let mut j = k;
        while diff > 0 && j < log.len() {
            let e = log[j];
            let slot = usize::from(e.edge);
            let was = s[slot] != r.bridges[slot];
            s[slot] = e.count;
            let is = s[slot] != r.bridges[slot];
            diff = diff + usize::from(is) - usize::from(was);
            j += 1;
        }
        if diff > 0 {
            continue;
        }
        // Bought after move j: a hint's own move is the next one, made by the help (no tap)
        let next = log.get(j).filter(|e| {
            !e.tap
                && matches!(r.outcome, PaidOutcome::Hint(Some((edge, count)))
                    if edge == usize::from(e.edge) && count == e.count)
        });
        let t = match (kind, next) {
            (MarkKind::Hint, Some(e)) => e.t,
            _ => j.checked_sub(1).map_or(0, |i| log[i].t),
        };
        out.push(Mark { t, kind });
        (state, k) = (s, j);
    }
    out.sort_by_key(|m| m.t);
    out
}

/// What the attempts table says about a member's day.
struct Row {
    started_at: i64,
    finished: bool,
    revealed: bool,
    flagged: bool,
    time_ms: i64,
    hints: i64,
    checks: i64,
    log: Option<String>,
    receipts: String,
}

async fn attempts(
    conn: &Connection,
    people: &[Person],
    day: i64,
) -> Result<HashMap<i64, Row>, ServerFnError> {
    let mut out = HashMap::new();
    if people.is_empty() {
        return Ok(out);
    }
    // Ids are integers from the database
    let ids: Vec<String> = people.iter().map(|p| p.user_id.to_string()).collect();
    let mut rows = conn
        .query(
            &format!(
                "SELECT user_id, started_at, finished_at, revealed, flag, time_ms, hints, checks, log, paid_actions
                 FROM attempts WHERE day = ?1 AND user_id IN ({})",
                ids.join(",")
            ),
            params![day],
        )
        .await
        .map_err(db_err)?;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        out.insert(
            row.get::<i64>(0).map_err(db_err)?,
            Row {
                started_at: row.get(1).map_err(db_err)?,
                finished: row.get::<Option<i64>>(2).map_err(db_err)?.is_some(),
                revealed: row.get::<i64>(3).map_err(db_err)? != 0,
                flagged: row.get::<Option<String>>(4).map_err(db_err)?.is_some(),
                time_ms: row.get::<Option<i64>>(5).map_err(db_err)?.unwrap_or(0),
                hints: row.get(6).map_err(db_err)?,
                checks: row.get(7).map_err(db_err)?,
                log: row.get(8).map_err(db_err)?,
                receipts: row.get(9).map_err(db_err)?,
            },
        );
    }
    Ok(out)
}

/// A member's lane from their attempt (if any), its log cleaned for a board of `edges`.
fn lane(person: Person, row: Option<&Row>, edges: usize) -> Lane {
    let mut lane = Lane {
        user_id: person.user_id,
        name: person.name,
        guest: person.guest,
        colour: person.colour,
        outcome: LaneOutcome::Missing,
        log: Vec::new(),
        marks: Vec::new(),
    };
    let Some(row) = row else { return lane };
    if row.revealed {
        lane.outcome = LaneOutcome::GaveUp;
    } else if row.finished {
        lane.outcome = LaneOutcome::Solved {
            time_ms: row.time_ms,
            hints: row.hints,
            checks: row.checks,
            ranked: !row.flagged,
        };
        // A log that doesn't parse plays as none, the result is still there
        let log: Vec<MoveEvent> = row
            .log
            .as_deref()
            .and_then(|l| serde_json::from_str(l).ok())
            .unwrap_or_default();
        lane.log = clean_log(&log, edges);
        let receipts: Vec<Receipt> = serde_json::from_str(&row.receipts).unwrap_or_default();
        lane.marks = marks(&receipts, row.started_at, &lane.log, edges);
    }
    lane
}

/// Whether `user` is done with the daily of `day`: solved or given up.
async fn done(conn: &Connection, user: i64, day: i64) -> Result<bool, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT 1 FROM attempts WHERE user_id = ?1 AND day = ?2 AND (finished_at IS NOT NULL OR revealed != 0)",
            params![user, day],
        )
        .await
        .map_err(db_err)?;
    Ok(rows.next().await.map_err(db_err)?.is_some())
}

/// The replay of `day` for `people`, as `viewer` may see it on `today`.
async fn replay(
    conn: &Connection,
    group: Option<ReplayGroup>,
    people: Vec<Person>,
    viewer: i64,
    day: i64,
    today: i64,
) -> Result<ReplayData, ServerFnError> {
    let mut data = ReplayData {
        day,
        today,
        group,
        me: viewer,
        locked: false,
        puzzle: None,
        lanes: Vec::new(),
    };
    // The future can't be watched, and today only once it's over for the viewer
    if day > today || (day == today && !done(conn, viewer, day).await?) {
        data.locked = true;
        return Ok(data);
    }
    let first = data.group.as_ref().map_or(i64::MIN, |g| g.first_day);
    let rows = if day < first {
        HashMap::new()
    } else {
        attempts(conn, &people, day).await?
    };
    // Somebody played it: the day's puzzle exists already (or is made the same as it was)
    let board = if rows.is_empty() {
        None
    } else {
        Some(Board::new(get_or_create(conn, day).await?.puzzle))
    };
    let edges = board.as_ref().map_or(0, |b| b.edges.len());
    data.lanes = people
        .into_iter()
        .map(|p| {
            let row = rows.get(&p.user_id);
            lane(p, row, edges)
        })
        .collect();
    data.puzzle = board.map(|b| b.puzzle);
    Ok(data)
}

/// The replay of a group's day, for a member (anyone else: `GroupNotFound`, as for every group
/// request). Every member who solved it gets a lane, in the group's order.
pub async fn group_replay(
    conn: &Connection,
    group: i64,
    viewer: i64,
    day: i64,
    today: i64,
) -> Result<ReplayData, ServerFnError> {
    let group = require_member(conn, group, viewer).await?;
    let people = members(conn, &group, today)
        .await?
        .into_iter()
        .map(|m| Person {
            user_id: m.user_id,
            name: m.name,
            guest: m.guest,
            colour: m.colour,
        })
        .collect();
    let info = ReplayGroup {
        id: group.id,
        name: group.name,
        first_day: group_day(group.created_at),
    };
    replay(conn, Some(info), people, viewer, day, today).await
}

/// A player's own solve of `day`, alone on the board, in the first colour.
pub async fn own_replay(
    conn: &Connection,
    viewer: &super::auth::User,
    day: i64,
    today: i64,
) -> Result<ReplayData, ServerFnError> {
    let me = Person {
        user_id: viewer.id,
        name: viewer.name.clone(),
        guest: viewer.username.is_none(),
        colour: 0,
    };
    replay(conn, None, vec![me], viewer.id, day, today).await
}
