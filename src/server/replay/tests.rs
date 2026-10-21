//! Tests of replays against a fresh database file: who may watch which day, what the lanes hold.

use common::{Pos, Puzzle};
use libsql::{Connection, params};

use super::*;
use crate::api::PaidAction;
use crate::model::{ApiError, MS_PER_DAY};
use crate::server::auth::erase;
use crate::server::daily;
use crate::server::db::TestConn;
use crate::server::groups::{create, join};

/// 2026-10-21, a Wednesday.
const TODAY: i64 = 20_747;
const YESTERDAY: i64 = TODAY - 1;

async fn db() -> TestConn {
    TestConn::new("replay").await
}

async fn user(conn: &Connection, name: &str, username: Option<&str>) -> i64 {
    conn.execute(
        "INSERT INTO users (username, name, created_at) VALUES (?1, ?2, 0)",
        params![username, name],
    )
    .await
    .unwrap();
    conn.last_insert_rowid()
}

/// Two islands joined by a double bridge (edge 0), a third below the second by one (edge 1).
fn puzzle() -> Puzzle {
    let mut puzzle = Puzzle::empty(5, 5);
    puzzle.set(Pos::new(0, 0), 2);
    puzzle.set(Pos::new(4, 0), 3);
    puzzle.set(Pos::new(4, 4), 1);
    puzzle
}

/// The daily of `day` is the small puzzle, so nothing has to be generated.
async fn small_daily(conn: &Connection, day: i64) {
    conn.execute(
        "INSERT INTO dailies (day, difficulty, puzzle, solution, created_at) VALUES (?1, 'easy', ?2, '[2,1]', 0)",
        params![day, serde_json::to_string(&puzzle()).unwrap()],
    )
    .await
    .unwrap();
}

fn mv(t: u32, edge: u16, count: u8, tap: bool) -> MoveEvent {
    MoveEvent {
        t,
        edge,
        count,
        tap,
    }
}

/// An honest solve of the small puzzle, finished at `t` ms.
fn solve(t: u32) -> Vec<MoveEvent> {
    vec![
        mv(t / 4, 0, 1, true),
        mv(t / 2, 0, 2, true),
        mv(t, 1, 1, true),
    ]
}

/// A finished attempt of `day` with its log (as stored, unsorted and all) and receipts.
async fn finished(conn: &Connection, user: i64, day: i64, log: &[MoveEvent], receipts: &str) {
    let started = day * MS_PER_DAY + 1000;
    let end = log.iter().map(|e| i64::from(e.t)).max().unwrap_or(0);
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, hints, checks, time_ms, log, paid_actions)
         VALUES (?1, ?2, ?3, ?4, 1, 0, ?5, ?6, ?7)",
        params![
            user,
            day,
            started,
            started + end,
            end + 30_000,
            serde_json::to_string(log).unwrap(),
            receipts
        ],
    )
    .await
    .unwrap();
}

async fn started(conn: &Connection, user: i64, day: i64) {
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at) VALUES (?1, ?2, ?3)",
        params![user, day, day * MS_PER_DAY],
    )
    .await
    .unwrap();
}

async fn gave_up(conn: &Connection, user: i64, day: i64) {
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, revealed, bridges) VALUES (?1, ?2, ?3, 1, '[2,1]')",
        params![user, day, day * MS_PER_DAY],
    )
    .await
    .unwrap();
}

/// A group of `owner` and `others`, made `days` before today.
async fn group_of(conn: &Connection, owner: i64, others: &[i64], days: i64) -> i64 {
    let id = create(conn, owner, "Replay Club").await.unwrap();
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
        params![id, (TODAY - days) * MS_PER_DAY],
    )
    .await
    .unwrap();
    id
}

#[track_caller]
fn error<T: std::fmt::Debug>(result: Result<T, ServerFnError>) -> ApiError {
    let e = result.expect_err("should have failed");
    ApiError::from_message(&e.to_string()).unwrap_or_else(|| panic!("{e}"))
}

fn outcomes(data: &ReplayData) -> Vec<(i64, LaneOutcome, usize)> {
    data.lanes
        .iter()
        .map(|l| (l.user_id, l.outcome, l.log.len()))
        .collect()
}

/// A day that's over: every member's lane, the solves with their (cleaned) logs.
#[tokio::test]
async fn members_watch_each_others_solves_of_a_past_day() {
    let conn = db().await;
    let aiko = user(&conn, "aiko", Some("aiko")).await;
    let bram = user(&conn, "bram", Some("bram")).await;
    let otter = user(&conn, "Drowsy Otter", None).await;
    let chen = user(&conn, "chen", Some("chen")).await;
    let emre = user(&conn, "emre", Some("emre")).await;
    let group = group_of(&conn, aiko, &[bram, otter, chen, emre], 10).await;
    small_daily(&conn, YESTERDAY).await;
    finished(&conn, aiko, YESTERDAY, &solve(60_000), "[]").await;
    // Out of order, with a move on an edge the board doesn't have
    let mut log = solve(40_000);
    log.reverse();
    log.push(mv(20_000, 7, 1, true));
    finished(&conn, otter, YESTERDAY, &log, "[]").await;
    conn.execute(
        "UPDATE attempts SET flag = 'pace' WHERE user_id = ?1",
        params![otter],
    )
    .await
    .unwrap();
    gave_up(&conn, bram, YESTERDAY).await;
    // Started, never finished
    started(&conn, emre, YESTERDAY).await;

    // Even a member who didn't play may watch a past day
    let data = group_replay(&conn, group, chen, YESTERDAY, TODAY)
        .await
        .unwrap();
    assert!(!data.locked);
    assert_eq!(data.me, chen);
    assert_eq!(data.puzzle, Some(puzzle()));
    let info = data.group.clone().unwrap();
    assert_eq!(
        (info.id, info.name.as_str(), info.first_day),
        (group, "Replay Club", TODAY - 10)
    );
    let solved = |time_ms, ranked| LaneOutcome::Solved {
        time_ms,
        hints: 1,
        checks: 0,
        ranked,
    };
    assert_eq!(
        outcomes(&data),
        [
            (aiko, solved(90_000, true), 3),
            (bram, LaneOutcome::GaveUp, 0),
            (otter, solved(70_000, false), 3),
            (chen, LaneOutcome::Missing, 0),
            (emre, LaneOutcome::Missing, 0),
        ]
    );
    let otter_lane = data.lane(otter).unwrap();
    assert_eq!(otter_lane.log, solve(40_000));
    assert!(otter_lane.guest);
    assert_eq!(otter_lane.finish(), Some(40_000));
    assert_eq!(data.end(), 60_000);
    // Every member in their colour
    let colours: Vec<u8> = data.lanes.iter().map(|l| l.colour).collect();
    assert_eq!(colours, [0, 1, 2, 3, 4]);
}

#[tokio::test]
async fn strangers_get_no_logs() {
    let conn = db().await;
    let aiko = user(&conn, "aiko", Some("aiko")).await;
    let bram = user(&conn, "bram", Some("bram")).await;
    let group = group_of(&conn, aiko, &[], 10).await;
    small_daily(&conn, YESTERDAY).await;
    finished(&conn, aiko, YESTERDAY, &solve(60_000), "[]").await;
    assert_eq!(
        error(group_replay(&conn, group, bram, YESTERDAY, TODAY).await),
        ApiError::GroupNotFound
    );
    // Nor of a group that doesn't exist
    assert_eq!(
        error(group_replay(&conn, group + 1, aiko, YESTERDAY, TODAY).await),
        ApiError::GroupNotFound
    );
}

/// Today's logs stay closed until the viewer finished it themselves (solved or given up).
#[tokio::test]
async fn today_opens_once_you_are_done() {
    let conn = db().await;
    let aiko = user(&conn, "aiko", Some("aiko")).await;
    let bram = user(&conn, "bram", Some("bram")).await;
    let chen = user(&conn, "chen", Some("chen")).await;
    let group = group_of(&conn, aiko, &[bram, chen], 10).await;
    small_daily(&conn, TODAY).await;
    finished(&conn, bram, TODAY, &solve(50_000), "[]").await;

    let closed = |data: &ReplayData| data.locked && data.lanes.is_empty() && data.puzzle.is_none();
    // Not started, then started: nothing to see
    let data = group_replay(&conn, group, aiko, TODAY, TODAY)
        .await
        .unwrap();
    assert!(closed(&data), "{data:?}");
    started(&conn, aiko, TODAY).await;
    assert!(closed(
        &group_replay(&conn, group, aiko, TODAY, TODAY)
            .await
            .unwrap()
    ));
    // Nor the future, not even for who's done today
    assert!(closed(
        &group_replay(&conn, group, bram, TODAY + 1, TODAY)
            .await
            .unwrap()
    ));

    // Done: bram's solve, aiko still playing
    let data = group_replay(&conn, group, bram, TODAY, TODAY)
        .await
        .unwrap();
    assert!(!data.locked);
    assert_eq!(data.lane(bram).unwrap().log.len(), 3);
    assert_eq!(data.lane(aiko).unwrap().outcome, LaneOutcome::Missing);
    // Giving up is done too
    gave_up(&conn, chen, TODAY).await;
    let data = group_replay(&conn, group, chen, TODAY, TODAY)
        .await
        .unwrap();
    assert!(!data.locked);
    assert_eq!(data.lane(bram).unwrap().log, solve(50_000));
}

/// Back to the day the group was made, not before.
#[tokio::test]
async fn days_before_the_group_have_no_logs() {
    let conn = db().await;
    let aiko = user(&conn, "aiko", Some("aiko")).await;
    let group = group_of(&conn, aiko, &[], 3).await;
    for day in [TODAY - 3, TODAY - 4] {
        small_daily(&conn, day).await;
        finished(&conn, aiko, day, &solve(60_000), "[]").await;
    }
    let first = group_replay(&conn, group, aiko, TODAY - 3, TODAY)
        .await
        .unwrap();
    assert_eq!(first.lanes[0].log.len(), 3);
    let before = group_replay(&conn, group, aiko, TODAY - 4, TODAY)
        .await
        .unwrap();
    assert!(!before.locked);
    assert_eq!(outcomes(&before), [(aiko, LaneOutcome::Missing, 0)]);
    assert_eq!(before.puzzle, None);
    // A day nobody played has no puzzle to make
    let empty = group_replay(&conn, group, aiko, TODAY - 2, TODAY)
        .await
        .unwrap();
    assert_eq!(empty.puzzle, None);
}

/// One's own solve, without a group: any day, only oneself.
#[tokio::test]
async fn your_own_solve_alone() {
    let conn = db().await;
    let otter = user(&conn, "Drowsy Otter", None).await;
    let bram = user(&conn, "bram", Some("bram")).await;
    small_daily(&conn, YESTERDAY).await;
    finished(&conn, otter, YESTERDAY, &solve(30_000), "[]").await;
    finished(&conn, bram, YESTERDAY, &solve(20_000), "[]").await;
    let me = crate::server::auth::User {
        id: otter,
        username: None,
        name: "Drowsy Otter".into(),
        created_at: 0,
        renamed_at: None,
    };
    let data = own_replay(&conn, &me, YESTERDAY, TODAY).await.unwrap();
    assert_eq!(data.group, None);
    assert_eq!(outcomes(&data).len(), 1);
    let lane = &data.lanes[0];
    assert_eq!((lane.user_id, lane.guest, lane.log.len()), (otter, true, 3));
    // Today not before it's done
    let today = own_replay(&conn, &me, TODAY, TODAY).await.unwrap();
    assert!(today.locked && today.lanes.is_empty());
}

/// Deleting an account takes its log out of every replay, with the membership.
#[tokio::test]
async fn deleted_players_leave_the_replays() {
    let conn = db().await;
    let aiko = user(&conn, "aiko", Some("aiko")).await;
    let bram = user(&conn, "bram", Some("bram")).await;
    let group = group_of(&conn, aiko, &[bram], 10).await;
    small_daily(&conn, YESTERDAY).await;
    finished(&conn, aiko, YESTERDAY, &solve(60_000), "[]").await;
    finished(&conn, bram, YESTERDAY, &solve(50_000), "[]").await;
    assert_eq!(
        group_replay(&conn, group, aiko, YESTERDAY, TODAY)
            .await
            .unwrap()
            .lanes
            .len(),
        2
    );
    erase(&conn, bram).await.unwrap();
    let data = group_replay(&conn, group, aiko, YESTERDAY, TODAY)
        .await
        .unwrap();
    assert_eq!(
        outcomes(&data).iter().map(|o| o.0).collect::<Vec<_>>(),
        [aiko]
    );
    let mut rows = conn
        .query("SELECT log FROM attempts WHERE user_id = ?1", params![bram])
        .await
        .unwrap();
    let log: Option<String> = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(log, None);
}

fn receipt(action: &str, bridges: &str, outcome: &str, at: Option<i64>) -> String {
    let at = at.map(|at| format!(r#","at":{at}"#)).unwrap_or_default();
    format!(r#"{{"id":"x","action":"{action}","bridges":{bridges},"outcome":{outcome}{at}}}"#)
}

/// Hints and checks become markers: at their time, or, for receipts without one, where their
/// board comes up in the log.
#[test]
fn paid_helps_become_markers() {
    let log = [
        mv(1000, 0, 1, true),
        // The hint's own move, right after it was bought on the board [1, 0]
        mv(4000, 1, 1, false),
        mv(9000, 0, 2, true),
    ];
    let parse = |json: &str| -> Vec<Receipt> { serde_json::from_str(json).unwrap() };
    let receipts = parse(&format!(
        "[{}]",
        [
            receipt("Check", "[0,0]", r#"{"Check":[]}"#, None),
            receipt("Hint", "[1,0]", r#"{"Hint":[1,1]}"#, None),
            // A hint that found nothing cost nothing
            receipt("Hint", "[2,1]", r#"{"Hint":null}"#, None),
            receipt("Check", "[1,1]", r#"{"Check":[0]}"#, None),
            // A board that never came up
            receipt("Check", "[0,2]", r#"{"Check":[]}"#, None),
            // With its time: 7.5 s after the start
            receipt("Check", "[1,1]", r#"{"Check":[]}"#, Some(17_500)),
        ]
        .join(",")
    ));
    let marks = marks(&receipts, 10_000, &log, 2);
    let seen: Vec<_> = marks.iter().map(|m| (m.t, m.kind)).collect();
    assert_eq!(
        seen,
        [
            (0, MarkKind::Check),
            (4000, MarkKind::Hint),
            (4000, MarkKind::Check),
            (7500, MarkKind::Check),
        ]
    );
    // Boards of another size are someone else's
    let other = parse(&format!(
        "[{}]",
        receipt("Check", "[0]", r#"{"Check":[]}"#, None)
    ));
    assert_eq!(super::marks(&other, 0, &log, 2), []);
}

/// A check bought during a real attempt keeps its time, and shows on the replay.
#[tokio::test]
async fn a_check_is_marked_where_it_was_bought() {
    let conn = db().await;
    let aiko = user(&conn, "aiko", Some("aiko")).await;
    small_daily(&conn, TODAY).await;
    daily::start(&conn, aiko, TODAY, None).await.unwrap();
    let id = daily::attempt(&conn, aiko, TODAY)
        .await
        .unwrap()
        .unwrap()
        .attempt_id;
    daily::paid_action(
        &conn,
        aiko,
        TODAY,
        vec![1, 0],
        PaidAction::Check,
        "check-0000000001",
        &id,
    )
    .await
    .unwrap();
    // The attempt began a minute ago
    conn.execute(
        "UPDATE attempts SET started_at = started_at - 60000 WHERE user_id = ?1",
        params![aiko],
    )
    .await
    .unwrap();
    daily::submit(&conn, aiko, TODAY, vec![2, 1], 3, solve(50_000), None)
        .await
        .unwrap();
    let data = group_replay(
        &conn,
        group_of(&conn, aiko, &[], 0).await,
        aiko,
        TODAY,
        TODAY,
    )
    .await
    .unwrap();
    let lane = &data.lanes[0];
    assert_eq!(lane.log, solve(50_000));
    assert_eq!(lane.marks.len(), 1);
    // Bought just now, a minute after the start
    assert_eq!(lane.marks[0].kind, MarkKind::Check);
    assert!(
        (60_000..65_000).contains(&lane.marks[0].t),
        "{:?}",
        lane.marks
    );
}
