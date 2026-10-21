//! The weekly banzuke 番付 of a group and its awards 賞 (worked out in `model::banzuke`), and the
//! stamps they bring.
//!
//! Nothing is stored: a week is worked out from the ranked dailies of the group's current
//! members whenever it's asked for. So a member who leaves takes their results off the earlier
//! weeks too, and one who joins brings theirs along. The stamps it earns stay
//! (`Stamp::sticky`).

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::{Group, groups_by_user, members, require_member};
use crate::model::{
    Banzuke, HISTORY_WEEKS, HONOUR_MIN_ACTIVE, HONOUR_WEEKS, WeekSolve, awards, banzuke_rows,
    champions, group_day, honours as week_honours, monday_of, standings,
};
use crate::server::db::db_err;

/// The ranked dailies (finished, not given up, not flagged) of some members between two days.
async fn ranked(
    conn: &Connection,
    members: &[i64],
    from: i64,
    to: i64,
) -> Result<Vec<WeekSolve>, ServerFnError> {
    if members.is_empty() {
        return Ok(Vec::new());
    }
    // Ids are integers from the database
    let ids: Vec<String> = members.iter().map(ToString::to_string).collect();
    let mut rows = conn
        .query(
            &format!(
                "SELECT user_id, day, time_ms, finished_at, hints = 0 AND checks = 0 FROM attempts
                 WHERE day BETWEEN ?1 AND ?2 AND finished_at IS NOT NULL AND revealed = 0
                   AND flag IS NULL AND time_ms IS NOT NULL AND user_id IN ({})",
                ids.join(",")
            ),
            params![from, to],
        )
        .await
        .map_err(db_err)?;
    let mut solves = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        solves.push(WeekSolve {
            user: row.get(0).map_err(db_err)?,
            day: row.get(1).map_err(db_err)?,
            time_ms: row.get(2).map_err(db_err)?,
            finished_at: row.get(3).map_err(db_err)?,
            clean: row.get::<i64>(4).map_err(db_err)? != 0,
        });
    }
    Ok(solves)
}

/// The median time of each day's ranked dailies, of all players: what makes the days'
/// different difficulties comparable (`model::pace`).
async fn medians(
    conn: &Connection,
    from: i64,
    to: i64,
) -> Result<HashMap<i64, i64>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT day, time_ms FROM (
                 SELECT day, time_ms, ROW_NUMBER() OVER (PARTITION BY day ORDER BY time_ms) AS n,
                     COUNT(*) OVER (PARTITION BY day) AS finishers
                 FROM attempts
                 WHERE day BETWEEN ?1 AND ?2 AND finished_at IS NOT NULL AND revealed = 0
                   AND flag IS NULL AND time_ms IS NOT NULL
             ) WHERE n = (finishers + 1) / 2",
            params![from, to],
        )
        .await
        .map_err(db_err)?;
    let mut medians = HashMap::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        medians.insert(row.get(0).map_err(db_err)?, row.get(1).map_err(db_err)?);
    }
    Ok(medians)
}

/// Whether `user` finished the daily of `day`, solved or given up: the same as for the board
/// of today (`model::spoiler_safe`).
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

/// The first Monday of a group.
fn first_monday(group: &Group) -> i64 {
    monday_of(group_day(group.created_at))
}

/// A group's banzuke and awards of a week, for a member.
///
/// The week of `monday` (any day of it), the current one if `None`. Weeks before the group's
/// first and after the current one are the nearest there is, like the week grid.
///
/// Until the viewer finished today's daily, today's results are left out: the points would
/// tell who was quicker.
pub async fn banzuke(
    conn: &Connection,
    group: i64,
    user: i64,
    monday: Option<i64>,
    today: i64,
) -> Result<Banzuke, ServerFnError> {
    let group = require_member(conn, group, user).await?;
    let first = first_monday(&group);
    let current = monday_of(today);
    let monday = monday
        .map_or(current, monday_of)
        .clamp(first.min(current), current);
    let members = members(conn, &group, today).await?;
    let ids: Vec<i64> = members.iter().map(|m| m.user_id).collect();
    // The week, the one before for the movement, and the ones before that for the most improved
    let from = monday - 7 * HISTORY_WEEKS;
    let mut solves = ranked(conn, &ids, from, monday + 6).await?;
    let today_hidden = monday == current && !finished(conn, user, today).await?;
    if today_hidden {
        solves.retain(|s| s.day != today);
    }
    let now = standings(&ids, &solves, monday);
    // The group's first week has nothing before it: everyone is new
    let before = if monday > first {
        standings(&ids, &solves, monday - 7)
    } else {
        Vec::new()
    };
    let awards = awards(
        &ids,
        &solves,
        &medians(conn, from, monday + 6).await?,
        monday,
    );
    // Last week's yokozuna wear the crown, whichever week is shown
    let champions = if current <= first {
        Vec::new()
    } else if monday == current {
        champions(&before)
    } else {
        let last = ranked(conn, &ids, current - 7, current - 1).await?;
        champions(&standings(&ids, &last, current - 7))
    };
    Ok(Banzuke {
        monday,
        today,
        first,
        today_hidden,
        rows: banzuke_rows(&members, &now, &before),
        awards,
        champions,
    })
}

/// The member ids of a group, banned players left out like everywhere.
async fn member_ids(conn: &Connection, group: i64) -> Result<Vec<i64>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT m.user_id FROM group_members m JOIN users u ON u.id = m.user_id
             WHERE m.group_id = ?1 AND u.banned_at IS NULL",
            params![group],
        )
        .await
        .map_err(db_err)?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        ids.push(row.get(0).map_err(db_err)?);
    }
    Ok(ids)
}

/// For the stamps: the weeks `user` was a group's yokozuna, and the weeks they won an award.
///
/// Of the last [`HONOUR_WEEKS`] weeks that are over, only weeks with [`HONOUR_MIN_ACTIVE`]
/// players count (see `model::honours`).
pub async fn honours(
    conn: &Connection,
    user: i64,
    today: i64,
) -> Result<(i64, i64), ServerFnError> {
    let current = monday_of(today);
    let (mut yokozuna, mut awarded) = (0, 0);
    // The days' medians, asked for once and only if needed
    let mut paces = None;
    for group in groups_by_user(conn, user).await? {
        let first = first_monday(&group);
        let weeks: Vec<i64> = (1..=HONOUR_WEEKS)
            .map(|k| current - 7 * k)
            .filter(|&m| m >= first)
            .collect();
        let Some(&oldest) = weeks.last() else {
            continue;
        };
        let ids = member_ids(conn, group.id).await?;
        if ids.len() < HONOUR_MIN_ACTIVE {
            continue;
        }
        let from = oldest - 7 * HISTORY_WEEKS;
        let solves = ranked(conn, &ids, from, current - 1).await?;
        for monday in weeks {
            // Nothing to win without playing
            if !solves
                .iter()
                .any(|s| s.user == user && (monday..monday + 7).contains(&s.day))
            {
                continue;
            }
            if paces.is_none() {
                let oldest = current - 7 * (HONOUR_WEEKS + HISTORY_WEEKS);
                paces = Some(medians(conn, oldest, current - 1).await?);
            }
            let Some(day_medians) = &paces else { continue };
            let week = standings(&ids, &solves, monday);
            let won = awards(&ids, &solves, day_medians, monday);
            let (y, a) = week_honours(user, &week, &won);
            yokozuna += i64::from(y);
            awarded += i64::from(a);
        }
    }
    Ok((yokozuna, awarded))
}
