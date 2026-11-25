//! The all time table against a fresh database file.

use libsql::{Connection, params};

use super::*;
use crate::model::{ApiError, GroupAllTimeRow, MS_PER_DAY};
use crate::server::db::TestConn;
use crate::server::groups::{create, join, leave};

/// 2026-10-19, a Monday...
const MONDAY: i64 = 20_745;
/// ...and the Wednesday of its week.
const TODAY: i64 = MONDAY + 2;

async fn db() -> TestConn {
    TestConn::new("all_time").await
}

async fn player(conn: &Connection, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO users (username, name, created_at) VALUES (?1, ?1, 0)",
        params![name],
    )
    .await
    .unwrap();
    conn.last_insert_rowid()
}

/// An attempt at the daily of `day`, finished in `time_ms`, given up if `revealed`, with a
/// `flag`.
async fn attempt(
    conn: &Connection,
    user: i64,
    day: i64,
    time_ms: i64,
    revealed: bool,
    flag: Option<&str>,
) {
    let at = day * MS_PER_DAY + 8 * 3_600_000;
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, revealed, time_ms, flag)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            user,
            day,
            at - time_ms,
            at,
            i64::from(revealed),
            time_ms,
            flag
        ],
    )
    .await
    .unwrap();
}

async fn solved(conn: &Connection, user: i64, day: i64, time_ms: i64) {
    attempt(conn, user, day, time_ms, false, None).await;
}

/// A group of `owner` and `others`, made on the day `since`.
async fn group_of(conn: &Connection, owner: i64, others: &[i64], since: i64) -> i64 {
    let id = create(conn, owner, "Lunch").await.unwrap();
    let mut rows = conn
        .query("SELECT invite FROM groups WHERE id = ?1", params![id])
        .await
        .unwrap();
    let code: String = rows.next().await.unwrap().unwrap().get(0).unwrap();
    drop(rows);
    for &other in others {
        join(conn, &code, other).await.unwrap();
    }
    conn.execute(
        "UPDATE groups SET created_at = ?2 WHERE id = ?1",
        params![id, since * MS_PER_DAY],
    )
    .await
    .unwrap();
    id
}

/// How a racer's race went: solved in a time, gave up, or ran out of it.
#[derive(Clone, Copy)]
enum Run {
    Solved(i64),
    GaveUp,
    Out,
}

/// A race of `group` with its `racers`, over if `ended`.
async fn race(conn: &Connection, group: i64, ended: bool, racers: &[(i64, Run)]) {
    let start = TODAY * MS_PER_DAY;
    conn.execute(
        "INSERT INTO group_races (group_id, host_id, difficulty, created_at, starts_at, ended_at)
         VALUES (?1, ?2, 'easy', ?3, ?3, ?4)",
        params![group, racers[0].0, start, ended.then_some(start + 600_000)],
    )
    .await
    .unwrap();
    let id = conn.last_insert_rowid();
    for &(user, run) in racers {
        let (finished_at, revealed, time_ms) = match run {
            Run::Solved(ms) => (Some(start + ms), 0, Some(ms)),
            Run::GaveUp => (None, 1, None),
            Run::Out => (None, 0, None),
        };
        conn.execute(
            "INSERT INTO group_race_players
                 (race_id, user_id, joined_at, last_seen, finished_at, revealed, time_ms)
             VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6)",
            params![id, user, start, finished_at, revealed, time_ms],
        )
        .await
        .unwrap();
    }
}

/// `(user, daily wins, race wins, yokozuna weeks, streak)`, in the table's order.
fn lines(table: &GroupAllTime) -> Vec<(i64, i64, i64, i64, i64)> {
    table
        .rows
        .iter()
        .map(|r: &GroupAllTimeRow| (r.user_id, r.daily_wins, r.race_wins, r.yokozuna, r.streak))
        .collect()
}

#[track_caller]
fn error<T: std::fmt::Debug>(result: Result<T, ServerFnError>) -> ApiError {
    let e = result.expect_err("should have failed");
    ApiError::from_message(&e.to_string()).unwrap_or_else(|| panic!("{e}"))
}

/// Daily wins on days with two members (ties all win, unranked attempts and days before the
/// group's first week left out, today only once the viewer finished it), race wins of ended
/// races with two members, yokozuna weeks over, current streaks; who left takes theirs along.
#[tokio::test]
async fn the_table_counts_wins_weeks_and_streaks() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let dana = player(&conn, "dana").await;
    let id = group_of(&conn, aiko, &[bram, chen, dana], MONDAY - 12).await;
    // Before the group's first week: not counted
    solved(&conn, aiko, MONDAY - 21, 90_000).await;
    solved(&conn, bram, MONDAY - 21, 10_000).await;
    // The first week (from Monday, like the banzuke): aiko wins Monday, bram is alone on
    // Tuesday, aiko and chen tie on Wednesday, bram gave up and dana's run is flagged on
    // Thursday, so aiko is alone on it. aiko is yokozuna
    solved(&conn, aiko, MONDAY - 14, 50_000).await;
    solved(&conn, bram, MONDAY - 14, 60_000).await;
    solved(&conn, bram, MONDAY - 13, 60_000).await;
    solved(&conn, aiko, MONDAY - 12, 40_000).await;
    solved(&conn, chen, MONDAY - 12, 40_000).await;
    solved(&conn, aiko, MONDAY - 11, 70_000).await;
    attempt(&conn, bram, MONDAY - 11, 20_000, true, None).await;
    attempt(&conn, dana, MONDAY - 11, 10_000, false, Some("too_fast")).await;
    // Last week: bram wins the day and the week
    solved(&conn, bram, MONDAY - 7, 30_000).await;
    solved(&conn, aiko, MONDAY - 7, 40_000).await;
    // This week, not over: aiko wins Monday and plays Tuesday alone; chen beats bram today
    solved(&conn, aiko, MONDAY, 20_000).await;
    solved(&conn, bram, MONDAY, 30_000).await;
    solved(&conn, aiko, MONDAY + 1, 20_000).await;
    solved(&conn, chen, TODAY, 10_000).await;
    solved(&conn, bram, TODAY, 20_000).await;

    // bram beats aiko; aiko beats chen, who gave up; aiko and bram tie
    race(
        &conn,
        id,
        true,
        &[(aiko, Run::Solved(60_000)), (bram, Run::Solved(50_000))],
    )
    .await;
    race(
        &conn,
        id,
        true,
        &[(aiko, Run::Solved(40_000)), (chen, Run::GaveUp)],
    )
    .await;
    race(
        &conn,
        id,
        true,
        &[(aiko, Run::Solved(45_000)), (bram, Run::Solved(45_000))],
    )
    .await;
    // Nobody won: dana alone, nobody finished, the race still running
    race(&conn, id, true, &[(dana, Run::Solved(30_000))]).await;
    race(&conn, id, true, &[(chen, Run::Out), (dana, Run::GaveUp)]).await;
    race(
        &conn,
        id,
        false,
        &[(bram, Run::Solved(30_000)), (dana, Run::Out)],
    )
    .await;
    // Another group's race
    let other = group_of(&conn, chen, &[dana], MONDAY - 12).await;
    race(
        &conn,
        other,
        true,
        &[(chen, Run::Solved(30_000)), (dana, Run::Solved(40_000))],
    )
    .await;

    // aiko hasn't played today: chen's win of today doesn't count for her yet
    let seen = all_time(&conn, id, aiko, TODAY).await.unwrap();
    assert_eq!((seen.first, seen.today_hidden), (MONDAY - 14, true));
    assert_eq!(
        lines(&seen),
        [
            (aiko, 3, 2, 1, 2),
            (bram, 1, 2, 1, 1),
            (chen, 1, 0, 0, 1),
            (dana, 0, 0, 0, 0),
        ]
    );
    assert!(!seen.empty());
    // chen played today: it counts
    let theirs = all_time(&conn, id, chen, TODAY).await.unwrap();
    assert!(!theirs.today_hidden);
    assert_eq!(
        lines(&theirs)[..3],
        [(aiko, 3, 2, 1, 2), (chen, 2, 0, 0, 1), (bram, 1, 2, 1, 1)]
    );

    // chen leaves: aiko is alone on their tied day, and on the race chen gave up
    leave(&conn, id, chen).await.unwrap();
    let after = all_time(&conn, id, aiko, TODAY).await.unwrap();
    assert_eq!(
        lines(&after),
        [(aiko, 2, 1, 1, 2), (bram, 1, 2, 1, 1), (dana, 0, 0, 0, 0)]
    );
    // And no longer sees the group
    assert_eq!(
        error(all_time(&conn, id, chen, TODAY).await),
        ApiError::GroupNotFound
    );
}

/// A new group: a line per member, nothing won.
#[tokio::test]
async fn a_new_group_has_won_nothing() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let id = group_of(&conn, aiko, &[bram], TODAY).await;
    solved(&conn, aiko, TODAY, 30_000).await;
    let table = all_time(&conn, id, aiko, TODAY).await.unwrap();
    assert!(table.empty() && !table.today_hidden);
    assert_eq!(lines(&table), [(aiko, 0, 0, 0, 1), (bram, 0, 0, 0, 0)]);
}
