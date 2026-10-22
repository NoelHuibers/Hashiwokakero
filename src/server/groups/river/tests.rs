//! Tests of the rivers against a fresh database file.

use libsql::{Connection, params};

use super::*;
use crate::model::{MS_PER_DAY, Run, Stamp, level_target};
use crate::server::auth::{cleanup_guests, erase};
use crate::server::db::TestConn;
use crate::server::groups::{create, join, leave, merge_guest, remove};

/// 2026-10-21, a Wednesday.
const TODAY: i64 = 20_747;
/// The day the groups of the tests were made.
const MADE: i64 = TODAY - 20;

async fn db() -> TestConn {
    TestConn::new("river").await
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

async fn guest(conn: &Connection, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO users (username, name, created_at) VALUES (NULL, ?1, 0)",
        params![name],
    )
    .await
    .unwrap();
    conn.last_insert_rowid()
}

/// A group of `members` (the first owns it), all joined on [`MADE`], an hour apart.
async fn group(conn: &Connection, members: &[i64]) -> i64 {
    let id = create(conn, members[0], "River crew").await.unwrap();
    let code = invite(conn, id).await;
    for &m in &members[1..] {
        join(conn, &code, m).await.unwrap();
    }
    for (i, &m) in members.iter().enumerate() {
        conn.execute(
            "UPDATE group_members SET joined_at = ?3 WHERE group_id = ?1 AND user_id = ?2",
            params![
                id,
                m,
                MADE * MS_PER_DAY + i64::try_from(i).unwrap() * 3_600_000
            ],
        )
        .await
        .unwrap();
    }
    id
}

/// The daily of `day` solved with `bridges` (a double bridge counts twice), at noon.
async fn solved(conn: &Connection, user: i64, day: i64, bridges: i64) {
    let at = day * MS_PER_DAY + MS_PER_DAY / 2;
    let run = serde_json::to_string(&Run {
        bridges,
        ..Run::default()
    })
    .unwrap();
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms, run)
         VALUES (?1, ?2, ?3, ?4, 60000, ?5)",
        params![user, day, at - 60_000, at, run],
    )
    .await
    .unwrap();
}

async fn invite(conn: &Connection, group: i64) -> String {
    let mut rows = conn
        .query("SELECT invite FROM groups WHERE id = ?1", params![group])
        .await
        .unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

/// The river's bridges, as `user` sees them.
async fn total(conn: &Connection, group: i64, user: i64) -> i64 {
    river(conn, group, user, TODAY).await.unwrap().total
}

async fn crew(conn: &Connection, group: i64, level: u32) -> Vec<i64> {
    let mut rows = conn
        .query(
            "SELECT user_id FROM group_river_crew WHERE group_id = ?1 AND level = ?2 ORDER BY user_id",
            params![group, level],
        )
        .await
        .unwrap();
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        out.push(row.get(0).unwrap());
    }
    out
}

async fn has_stamp(conn: &Connection, user: i64) -> bool {
    let mut rows = conn
        .query(
            "SELECT 1 FROM stamps WHERE user_id = ?1 AND stamp = ?2",
            params![user, Stamp::RiverCrew.id()],
        )
        .await
        .unwrap();
    rows.next().await.unwrap().is_some()
}

async fn number(conn: &Connection, sql: &str) -> i64 {
    let mut rows = conn.query(sql, ()).await.unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

/// Every bridge of a solved daily since joining counts, double bridges twice (the run has them
/// so); dailies given up, flagged or from before joining don't.
#[tokio::test]
async fn the_river_counts_the_bridges_of_solved_dailies_since_joining() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let id = group(&conn, &[aiko, bram]).await;
    solved(&conn, aiko, MADE - 1, 500).await;
    solved(&conn, aiko, MADE + 1, 40).await;
    solved(&conn, bram, MADE + 1, 25).await;
    solved(&conn, bram, MADE + 2, 30).await;
    solved(&conn, aiko, MADE + 3, 70).await;
    conn.execute(
        "UPDATE attempts SET revealed = 1 WHERE user_id = ?1 AND day = ?2",
        params![bram, MADE + 2],
    )
    .await
    .unwrap();
    conn.execute(
        "UPDATE attempts SET flag = 'pace' WHERE user_id = ?1 AND day = ?2",
        params![aiko, MADE + 3],
    )
    .await
    .unwrap();
    // Not finished
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at) VALUES (?1, ?2, ?3)",
        params![bram, MADE + 4, (MADE + 4) * MS_PER_DAY],
    )
    .await
    .unwrap();

    let seen = river(&conn, id, aiko, TODAY).await.unwrap();
    assert_eq!((seen.total, seen.progress, seen.base), (65, 65, 0));
    assert_eq!((seen.level, seen.target), (0, level_target(0, 2)));
    assert_eq!(seen.completed, []);
    // Only members see it
    let chen = player(&conn, "chen").await;
    let e = river(&conn, id, chen, TODAY).await.unwrap_err();
    assert_eq!(
        crate::model::ApiError::from_message(&e.to_string()),
        Some(crate::model::ApiError::GroupNotFound)
    );
}

/// What a member laid stays when they leave, are removed or delete their account, without
/// their name, and it isn't counted twice when they come back.
#[tokio::test]
async fn bridges_stay_in_the_river_when_members_go() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let emre = player(&conn, "emre").await;
    let tanuki = guest(&conn, "Sleepy Tanuki 27").await;
    let id = group(&conn, &[aiko, bram, chen, emre, tanuki]).await;
    for (user, bridges) in [(aiko, 10), (bram, 20), (chen, 40), (emre, 5), (tanuki, 80)] {
        solved(&conn, user, MADE + 1, bridges).await;
    }
    assert_eq!(total(&conn, id, aiko).await, 155);
    leave(&conn, id, bram).await.unwrap();
    assert_eq!(total(&conn, id, aiko).await, 155);
    remove(&conn, id, aiko, emre).await.unwrap();
    assert_eq!(total(&conn, id, aiko).await, 155);
    erase(&conn, chen).await.unwrap();
    assert_eq!(total(&conn, id, aiko).await, 155);
    // A guest nobody can get into any more goes too
    assert_eq!(cleanup_guests(&conn).await.unwrap(), 1);
    assert_eq!(total(&conn, id, aiko).await, 155);
    assert_eq!(number(&conn, "SELECT river_banked FROM groups").await, 145);
    // Back again: only what bram lays from now on counts for him
    join(&conn, &invite(&conn, id).await, bram).await.unwrap();
    assert_eq!(total(&conn, id, aiko).await, 155);
}

/// A guest who signs in: the river of a group both are in counts their dailies once, a group the
/// guest has to leave (no room for it) keeps what they laid.
#[tokio::test]
async fn signing_in_neither_loses_nor_doubles_bridges() {
    let conn = db().await;
    let tanuki = guest(&conn, "Sleepy Tanuki 27").await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let both = group(&conn, &[bram, aiko, tanuki]).await;
    let left = group(&conn, &[bram, tanuki]).await;
    // No room for the guest's other group
    for i in 1..crate::model::MAX_GROUPS {
        create(&conn, aiko, &format!("Aiko {i}")).await.unwrap();
    }
    solved(&conn, tanuki, MADE + 1, 33).await;
    solved(&conn, aiko, MADE + 2, 7).await;
    assert_eq!(
        (
            total(&conn, both, bram).await,
            total(&conn, left, bram).await
        ),
        (40, 33)
    );
    // What `auth::merge_guest` does: the groups, then the dailies
    merge_guest(&conn, aiko, tanuki).await.unwrap();
    conn.execute(
        "UPDATE OR IGNORE attempts SET user_id = ?1 WHERE user_id = ?2",
        params![aiko, tanuki],
    )
    .await
    .unwrap();
    assert_eq!(
        (
            total(&conn, both, bram).await,
            total(&conn, left, bram).await
        ),
        (40, 33)
    );
}

/// A level completes once, when someone looks: kept with the bridges it took and when the last
/// plank was laid, its crew get their stamp, the next level starts with what was left over, and
/// every member is told once.
#[tokio::test]
async fn a_level_completes_once_with_its_crew() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let id = group(&conn, &[aiko, bram, chen]).await;
    let target = level_target(0, 3);
    solved(&conn, aiko, MADE + 1, target / 2).await;
    solved(&conn, bram, MADE + 2, target - target / 2 - 1).await;
    let first = river(&conn, id, aiko, TODAY).await.unwrap();
    assert_eq!((first.level, first.progress), (0, target - 1));
    assert_eq!(first.celebrate, None);
    assert!(!has_stamp(&conn, aiko).await);

    // The plank that reaches the other side, and 20 more
    solved(&conn, aiko, MADE + 3, 21).await;
    let across = river(&conn, id, bram, TODAY).await.unwrap();
    let level = RiverLevel {
        level: 0,
        target,
        completed_at: (MADE + 3) * MS_PER_DAY + MS_PER_DAY / 2,
    };
    assert_eq!(across.completed, [level]);
    assert_eq!(
        (across.level, across.base, across.progress),
        (1, target, 20)
    );
    assert_eq!(
        (across.total, across.target),
        (target + 20, level_target(1, 3))
    );
    assert_eq!(across.celebrate, Some(level));
    // Who laid planks of it has the stamp, chen didn't lay any
    assert_eq!(crew(&conn, id, 0).await, [aiko, bram]);
    assert!(has_stamp(&conn, aiko).await && has_stamp(&conn, bram).await);
    assert!(!has_stamp(&conn, chen).await);

    // Seen again, by anyone: still the one level, celebrated once with each member
    assert_eq!(river(&conn, id, bram, TODAY).await.unwrap().celebrate, None);
    let aiko_sees = river(&conn, id, aiko, TODAY).await.unwrap();
    assert_eq!(
        (aiko_sees.completed.len(), aiko_sees.celebrate),
        (1, Some(level))
    );
    assert_eq!(river(&conn, id, aiko, TODAY).await.unwrap().celebrate, None);
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM group_river").await, 1);
    // Who joins later isn't told about levels from before
    let emre = player(&conn, "emre").await;
    join(&conn, &invite(&conn, id).await, emre).await.unwrap();
    assert_eq!(river(&conn, id, emre, TODAY).await.unwrap().celebrate, None);
}

/// Two members looking at the same moment, on connections of their own, find the level once.
#[tokio::test]
async fn members_looking_at_once_complete_a_level_once() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let id = group(&conn, &[aiko, bram]).await;
    solved(&conn, aiko, MADE + 1, level_target(0, 2) + 5).await;
    let other = conn.independent_connection().await;
    other
        .busy_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let (a, b) = tokio::join!(
        river(&conn, id, aiko, TODAY),
        river(&other, id, bram, TODAY)
    );
    assert_eq!(a.unwrap().completed, b.unwrap().completed);
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM group_river").await, 1);
    assert_eq!(crew(&conn, id, 0).await, [aiko]);
}

/// A big day completes more than one level; levels after the strait are oceans that take as
/// long as the first.
#[tokio::test]
async fn one_big_push_crosses_several_waters() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let id = group(&conn, &[aiko]).await;
    let five: i64 = (0..5).map(|l| level_target(l, 1)).sum();
    solved(&conn, aiko, MADE + 1, five + level_target(4, 1) + 1).await;
    let seen = river(&conn, id, aiko, TODAY).await.unwrap();
    assert_eq!(seen.level, 6);
    assert_eq!(seen.progress, 1);
    assert_eq!(
        seen.completed.iter().map(|l| l.level).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5]
    );
    // The newest one is celebrated
    assert_eq!(seen.celebrate.map(|l| l.level), Some(5));
    assert_eq!(crew(&conn, id, 5).await, [aiko]);
}

/// The members who laid the most planks of this week's dailies, most first.
#[tokio::test]
async fn this_weeks_crew_by_bridges() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let id = group(&conn, &[aiko, bram, chen]).await;
    let monday = monday_of(TODAY);
    // Last week's don't count this week
    solved(&conn, aiko, monday - 1, 500).await;
    solved(&conn, aiko, monday, 30).await;
    solved(&conn, bram, monday, 20).await;
    solved(&conn, bram, TODAY, 25).await;
    let week = river(&conn, id, chen, TODAY).await.unwrap().week;
    assert_eq!(
        week,
        [
            Contribution {
                user_id: bram,
                bridges: 45
            },
            Contribution {
                user_id: aiko,
                bridges: 30
            }
        ]
    );
}

/// The planks laid since a member looked last are new to them, once.
#[tokio::test]
async fn new_planks_are_new_once() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let id = group(&conn, &[aiko, bram]).await;
    solved(&conn, bram, MADE + 1, 40).await;
    let first = river(&conn, id, aiko, TODAY).await.unwrap();
    assert_eq!((first.seen, first.total), (0, 40));
    solved(&conn, aiko, MADE + 2, 25).await;
    let back = river(&conn, id, aiko, TODAY).await.unwrap();
    assert_eq!(
        (back.seen, back.seen_progress(), back.progress),
        (40, 40, 65)
    );
    let again = river(&conn, id, aiko, TODAY).await.unwrap();
    assert_eq!(again.seen_progress(), again.progress);
    // bram hasn't looked yet: everything is new to him
    assert_eq!(river(&conn, id, bram, TODAY).await.unwrap().seen, 0);
}

/// A group that goes takes its river along; an account deleted takes its place in the crews.
#[tokio::test]
async fn rivers_go_with_their_groups_and_crews_with_their_members() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let id = group(&conn, &[aiko, bram]).await;
    let other = group(&conn, &[aiko]).await;
    solved(&conn, aiko, MADE + 1, level_target(0, 2)).await;
    river(&conn, id, aiko, TODAY).await.unwrap();
    river(&conn, other, aiko, TODAY).await.unwrap();
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM group_river").await, 2);
    leave(&conn, other, aiko).await.unwrap();
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM group_river").await, 1);
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM group_river_crew").await,
        1
    );
    erase(&conn, aiko).await.unwrap();
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM group_river_crew").await,
        0
    );
    // The group's level stays, its bridges too
    let seen = river(&conn, id, bram, TODAY).await.unwrap();
    assert_eq!((seen.level, seen.total), (1, level_target(0, 2)));
}
