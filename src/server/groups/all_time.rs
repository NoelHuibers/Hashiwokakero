//! A group's all time table (worked out in `model::all_time`): daily wins, race wins, yokozuna
//! weeks and current streaks of its members.
//!
//! Like the banzuke nothing is stored: the group's current members' ranked dailies since its
//! first banzuke week, the results of its ended races and the members' streaks are read in a
//! query each whenever it's asked for.

#[cfg(test)]
mod tests;

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::banzuke::{finished, first_monday, ranked};
use super::{members, require_member};
use crate::model::{
    Flag, GroupAllTime, RaceResult, all_time_rows, daily_wins, monday_of, race_wins, yokozuna_weeks,
};
use crate::server::daily::streak::{saves_from, states};
use crate::server::db::db_err;

/// The results of a group's ended races, `(race, result)`, of the members still in it.
async fn race_results(
    conn: &Connection,
    group: i64,
) -> Result<Vec<(i64, RaceResult)>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT p.race_id, p.user_id, p.finished_at, p.revealed, p.time_ms, p.flag
             FROM group_race_players p JOIN group_races r ON r.id = p.race_id
             WHERE r.group_id = ?1 AND r.starts_at IS NOT NULL AND r.ended_at IS NOT NULL",
            params![group],
        )
        .await
        .map_err(db_err)?;
    let mut results = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        results.push((
            row.get(0).map_err(db_err)?,
            RaceResult {
                user_id: row.get(1).map_err(db_err)?,
                finished_at: row.get(2).map_err(db_err)?,
                revealed: row.get::<i64>(3).map_err(db_err)? != 0,
                time_ms: row.get(4).map_err(db_err)?,
                flag: row
                    .get::<Option<String>>(5)
                    .map_err(db_err)?
                    .and_then(|f| Flag::from_id(&f)),
            },
        ));
    }
    Ok(results)
}

/// A group's all time table, for a member.
///
/// Until the viewer finished today's daily, today's dailies don't count, like on the banzuke:
/// the wins would tell who was quicker.
pub async fn all_time(
    conn: &Connection,
    group: i64,
    user: i64,
    today: i64,
) -> Result<GroupAllTime, ServerFnError> {
    let group = require_member(conn, group, user).await?;
    let first = first_monday(&group);
    let members = members(conn, &group, today).await?;
    let ids: Vec<i64> = members.iter().map(|m| m.user_id).collect();
    let mut solves = ranked(conn, &ids, first, today).await?;
    let today_hidden = !finished(conn, user, today).await?;
    if today_hidden {
        solves.retain(|s| s.day != today);
    }
    let races = race_results(conn, group.id).await?;
    let streaks = states(conn, &ids, today, saves_from())
        .await?
        .into_iter()
        .map(|(user, state)| (user, state.current))
        .collect();
    let rows = all_time_rows(
        &members,
        &daily_wins(&ids, &solves),
        &race_wins(&ids, &races),
        &yokozuna_weeks(&ids, &solves, first, monday_of(today)),
        &streaks,
    );
    Ok(GroupAllTime {
        rows,
        first,
        today_hidden,
    })
}
