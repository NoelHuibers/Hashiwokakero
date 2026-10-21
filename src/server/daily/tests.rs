//! Tests of the daily puzzle against a fresh database file.

use common::Board;

use super::stamps::sync_recent;
use super::*;
use crate::model::{
    CHECK_PENALTY_MS, Flag, HINT_PENALTY_MS, LAUNCH_DAY, MS_PER_DAY, MoveEvent, Stamp, month_days,
    month_of,
};
use crate::server::db;

/// A fresh database file, removed again when dropped.
struct TestDb {
    /// Kept open for as long as the test runs.
    database: libsql::Database,
    /// Removed after the database is closed: fields are dropped in order.
    _file: db::TempDb,
}

async fn test_db() -> (TestDb, Connection) {
    let file = db::TempDb::new("daily");
    let database = db::open_local(file.as_str()).await.unwrap();
    let conn = database.connect().unwrap();
    db::migrate(&conn).await.unwrap();
    // Running the migrations again is a no-op
    db::migrate(&conn).await.unwrap();
    (
        TestDb {
            database,
            _file: file,
        },
        conn,
    )
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

async fn paid_current(
    conn: &Connection,
    user: i64,
    day: i64,
    bridges: Vec<u8>,
    action: crate::api::PaidAction,
    id: &str,
) -> Result<crate::api::PaidReply, ServerFnError> {
    let identity = attempt(conn, user, day)
        .await?
        .map(|a| a.attempt_id)
        .unwrap_or_default();
    paid_action(conn, user, day, bridges, action, id, &identity).await
}

/// A player's log for `solution`: one click per bridge, 1.5 s apart.
fn played(solution: &[u8]) -> Vec<MoveEvent> {
    let mut log = Vec::new();
    for (edge, &count) in solution.iter().enumerate() {
        for c in 1..=count {
            let t = u32::try_from(log.len() + 1).unwrap() * 1500;
            log.push(MoveEvent {
                t,
                edge: u16::try_from(edge).unwrap(),
                count: c,
                tap: true,
            });
        }
    }
    log
}

/// Moves the start of an attempt `ms` into the past, as if the player had been at it.
async fn age(conn: &Connection, user: i64, day: i64, ms: i64) {
    conn.execute(
        "UPDATE attempts SET started_at = started_at - ?3 WHERE user_id = ?1 AND day = ?2",
        params![user, day, ms],
    )
    .await
    .unwrap();
}

/// Monday dailies are Easy, quick to generate.
const MONDAY: i64 = LAUNCH_DAY + 2;

#[tokio::test]
async fn daily_is_generated_once_and_kept() {
    let (_db, conn) = test_db().await;
    let first = get_or_create(&conn, MONDAY).await.unwrap();
    assert_eq!(first.difficulty, Difficulty::Easy);
    let board = Board::new(first.puzzle.clone());
    assert!(board.check(&first.solution).solved);
    let again = get_or_create(&conn, MONDAY).await.unwrap();
    assert_eq!(again.puzzle, first.puzzle);
    // Different days give different puzzles
    let tuesday = get_or_create(&conn, MONDAY + 1).await.unwrap();
    assert_ne!(tuesday.puzzle, first.puzzle);
}

#[tokio::test]
async fn puzzle_is_only_sent_after_start() {
    let (_db, conn) = test_db().await;
    let id = user(&conn, "Sleepy Crane 12", None).await;
    let before = info(&conn, Some(id), MONDAY).await.unwrap();
    assert!(before.puzzle.is_none() && before.attempt.is_none());
    let started = start(&conn, id, MONDAY, None).await.unwrap();
    assert!(started.puzzle.is_some());
    let started_at = started.attempt.unwrap().started_at;
    // Starting again doesn't reset the clock
    let again = start(&conn, id, MONDAY, None).await.unwrap();
    assert_eq!(again.attempt.unwrap().started_at, started_at);
}

#[tokio::test]
async fn submit_validates_and_ranks() {
    let (_db, conn) = test_db().await;
    let a = user(&conn, "noel", Some("noel")).await;
    let b = user(&conn, "Brave Koi 40", None).await;
    let daily = get_or_create(&conn, MONDAY).await.unwrap();
    let board = Board::new(daily.puzzle.clone());

    // Not started yet
    assert!(
        submit(
            &conn,
            a,
            MONDAY,
            daily.solution.clone(),
            10,
            played(&daily.solution),
            None
        )
        .await
        .is_err()
    );
    start(&conn, a, MONDAY, None).await.unwrap();
    start(&conn, b, MONDAY, None).await.unwrap();
    // Wrong boards are rejected
    assert!(
        submit(&conn, a, MONDAY, board.empty_bridges(), 1, vec![], None)
            .await
            .is_err()
    );
    assert!(
        submit(&conn, a, MONDAY, vec![1], 1, vec![], None)
            .await
            .is_err()
    );

    // b asks for a hint and a check, a doesn't
    let given = hint(&conn, b, MONDAY, board.empty_bridges()).await.unwrap();
    let (edge, value) = given.unwrap();
    assert_eq!(daily.solution[edge], value);
    let mut wrong = board.empty_bridges();
    let extra = (0..wrong.len()).find(|&e| daily.solution[e] < 2).unwrap();
    wrong[extra] = daily.solution[extra] + 1;
    assert_eq!(check(&conn, b, MONDAY, wrong).await.unwrap(), vec![extra]);

    // Ten minutes of playing
    age(&conn, a, MONDAY, 600_000).await;
    age(&conn, b, MONDAY, 600_000).await;
    let ra = submit(
        &conn,
        a,
        MONDAY,
        daily.solution.clone(),
        20,
        played(&daily.solution),
        None,
    )
    .await
    .unwrap();
    let rb = submit(
        &conn,
        b,
        MONDAY,
        daily.solution.clone(),
        25,
        played(&daily.solution),
        None,
    )
    .await
    .unwrap();
    assert!(rb.time_ms >= HINT_PENALTY_MS + CHECK_PENALTY_MS);
    assert_eq!((ra.rank, rb.rank, rb.finishers), (1, 2, 2));
    // Finished is final
    assert!(
        submit(
            &conn,
            a,
            MONDAY,
            daily.solution.clone(),
            20,
            played(&daily.solution),
            None
        )
        .await
        .is_err()
    );
    assert!(hint(&conn, a, MONDAY, board.empty_bridges()).await.is_err());

    let board = leaderboard(&conn, MONDAY, Some(b)).await.unwrap();
    assert_eq!(board.entries.len(), 2);
    assert_eq!(board.entries[0].name, "noel");
    assert!(!board.entries[0].guest && board.entries[1].guest);
    assert_eq!(board.me.unwrap().rank, 2);
}

/// A browser clock that ran ahead of the server's mid-game (2026-10-21: the last click logged
/// 55 s after the finish the server measured) doesn't take a human result off the board.
#[tokio::test]
async fn a_log_ahead_of_the_server_clock_is_ranked() {
    let (_db, conn) = test_db().await;
    let player = user(&conn, "kit", Some("kit")).await;
    let daily = get_or_create(&conn, MONDAY).await.unwrap();
    start(&conn, player, MONDAY, Some("device-of-kit"))
        .await
        .unwrap();
    let mut log = played(&daily.solution);
    let measured = i64::from(log.last().unwrap().t) + 500;
    let half = log.len() / 2;
    for e in &mut log[half..] {
        e.t += 55_000;
    }
    age(&conn, player, MONDAY, measured).await;
    let r = submit(&conn, player, MONDAY, daily.solution.clone(), 9, log, None)
        .await
        .unwrap();
    assert_eq!((r.rank, r.flag), (1, None));
    assert!(
        (measured..measured + 5_000).contains(&r.time_ms),
        "{}",
        r.time_ms
    );
    assert_eq!(
        attempt(&conn, player, MONDAY).await.unwrap().unwrap().flag,
        None
    );
}

#[tokio::test]
async fn implausible_results_are_not_ranked() {
    let (_db, conn) = test_db().await;
    let bot = user(&conn, "bot", Some("bot")).await;
    let fair = user(&conn, "fair", Some("fair")).await;
    let daily = get_or_create(&conn, MONDAY).await.unwrap();
    start(&conn, bot, MONDAY, Some("device-of-bot"))
        .await
        .unwrap();
    start(&conn, fair, MONDAY, Some("device-of-fair"))
        .await
        .unwrap();
    // Solved by a program: no log, or a perfect one in no time
    let r = submit(&conn, bot, MONDAY, daily.solution.clone(), 1, vec![], None)
        .await
        .unwrap();
    assert_eq!((r.rank, r.flag), (0, Some(Flag::Log)));
    assert_eq!(finishers(&conn, MONDAY).await.unwrap(), 0);
    age(&conn, fair, MONDAY, 600_000).await;
    let r = submit(
        &conn,
        fair,
        MONDAY,
        daily.solution.clone(),
        9,
        played(&daily.solution),
        None,
    )
    .await
    .unwrap();
    assert_eq!((r.rank, r.flag), (1, None));
    let board = leaderboard(&conn, MONDAY, None).await.unwrap();
    assert_eq!(board.entries.len(), 1);
    assert_eq!(board.entries[0].name, "fair");
    // The flag shows up when the bot reloads
    let attempt = attempt(&conn, bot, MONDAY).await.unwrap().unwrap();
    assert_eq!(attempt.flag, Some(Flag::Log));
    // A review can take a result off the board, and put it back
    assert!(
        set_flag(&conn, fair, MONDAY, Some(Flag::Review))
            .await
            .unwrap()
    );
    let board = leaderboard(&conn, MONDAY, None).await.unwrap();
    assert_eq!(board.entries.len(), 0);
    set_flag(&conn, fair, MONDAY, None).await.unwrap();
    assert_eq!(
        leaderboard(&conn, MONDAY, None)
            .await
            .unwrap()
            .entries
            .len(),
        1
    );
}

#[tokio::test]
async fn a_second_account_on_the_same_device_is_not_ranked() {
    let (_db, conn) = test_db().await;
    let scout = user(&conn, "Scout Fox 1", None).await;
    let main = user(&conn, "main", Some("main")).await;
    let other = user(&conn, "other", Some("other")).await;
    let daily = get_or_create(&conn, MONDAY).await.unwrap();
    // The scout account looks at the puzzle, the main account plays it on the same device
    start(&conn, scout, MONDAY, Some("shared-device-1"))
        .await
        .unwrap();
    start(&conn, main, MONDAY, Some("shared-device-1"))
        .await
        .unwrap();
    start(&conn, other, MONDAY, Some("own-device-22"))
        .await
        .unwrap();
    age(&conn, main, MONDAY, 600_000).await;
    age(&conn, other, MONDAY, 600_000).await;
    let r = submit(
        &conn,
        main,
        MONDAY,
        daily.solution.clone(),
        9,
        played(&daily.solution),
        None,
    )
    .await
    .unwrap();
    assert_eq!(r.flag, Some(Flag::SecondAccount));
    let r = submit(
        &conn,
        other,
        MONDAY,
        daily.solution.clone(),
        9,
        played(&daily.solution),
        None,
    )
    .await
    .unwrap();
    assert_eq!((r.rank, r.flag), (1, None));
}

#[tokio::test]
async fn solvers_refuse_the_live_daily() {
    let (_db, conn) = test_db().await;
    let player = user(&conn, "player", Some("player")).await;
    let daily = get_or_create(&conn, today()).await.unwrap();
    assert!(locked(&conn, None, &daily.puzzle).await.unwrap());
    assert!(locked(&conn, Some(player), &daily.puzzle).await.unwrap());
    // Mirrored, it's still the daily
    let mirrored = Puzzle::new(
        daily.puzzle.width,
        daily.puzzle.height,
        daily
            .puzzle
            .islands
            .iter()
            .map(|i| common::Island {
                pos: common::Pos::new(daily.puzzle.width - 1 - i.pos.x, i.pos.y),
                required: i.required,
            })
            .collect(),
    );
    assert!(locked(&conn, Some(player), &mirrored).await.unwrap());
    // Another puzzle isn't locked (a Monday that's surely over, MONDAY itself is today once a
    // week), and the daily unlocks once you gave up or finished
    assert!(
        !locked(
            &conn,
            None,
            &get_or_create(&conn, MONDAY - 7).await.unwrap().puzzle
        )
        .await
        .unwrap()
    );
    start(&conn, player, today(), None).await.unwrap();
    reveal(&conn, player, today()).await.unwrap();
    assert!(!locked(&conn, Some(player), &daily.puzzle).await.unwrap());
}

#[tokio::test]
async fn revealed_runs_are_not_ranked() {
    let (_db, conn) = test_db().await;
    let id = user(&conn, "Quiet Owl 77", None).await;
    start(&conn, id, MONDAY, None).await.unwrap();
    let solution = reveal(&conn, id, MONDAY).await.unwrap();
    assert!(
        submit(&conn, id, MONDAY, solution, 5, vec![], None)
            .await
            .is_err()
    );
    let attempt = attempt(&conn, id, MONDAY).await.unwrap().unwrap();
    assert!(attempt.revealed && attempt.finished());
    assert_eq!(finishers(&conn, MONDAY).await.unwrap(), 0);
    assert!(
        leaderboard(&conn, MONDAY, Some(id))
            .await
            .unwrap()
            .me
            .is_none()
    );
}

#[tokio::test]
async fn stats_and_hall_of_fame() {
    let (_db, conn) = test_db().await;
    let a = user(&conn, "alice", Some("alice")).await;
    let b = user(&conn, "bob", Some("bob")).await;
    let today = today();
    // alice: 3 days in a row up to yesterday, bob: one day
    for (u, day, time) in [
        (a, today - 3, 90_000),
        (a, today - 2, 80_000),
        (a, today - 1, 70_000),
        (b, today - 1, 60_000),
    ] {
        conn.execute(
            "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms) VALUES (?1, ?2, 0, ?3, ?3)",
            params![u, day, time],
        )
        .await
        .unwrap();
    }
    let s = stats(&conn, a).await.unwrap();
    assert_eq!((s.solved, s.current_streak, s.best_streak), (3, 3, 3));
    assert_eq!(s.best_time_ms, Some(70_000));
    assert_eq!(s.average_time_ms, Some(80_000));
    let yesterday = s.recent.iter().find(|r| r.day == today - 1).unwrap();
    assert_eq!((yesterday.time_ms, yesterday.rank), (Some(70_000), Some(2)));
    assert_eq!(s.recent.last().unwrap().day, today);

    // Bob won yesterday, alice was second; the days before alice played alone
    assert_eq!((s.gold, s.silver, s.bronze), (2, 1, 0));

    let fame = hall_of_fame(&conn).await.unwrap();
    let all = &fame.solved;
    assert_eq!(all[0].name, "alice");
    assert_eq!((all[0].solved, all[0].best_streak), (3, 3));
    assert_eq!((all[1].solved, all[1].best_streak), (1, 1));
    // Alice's two golds beat bob's one
    let medals: Vec<_> = fame
        .medals
        .iter()
        .map(|e| (e.name.as_str(), e.gold, e.silver))
        .collect();
    assert_eq!(medals, [("alice", 2, 1), ("bob", 1, 0)]);
    assert_eq!(fame.streaks[0].name, "alice");
    assert_eq!(fame.streaks[0].current_streak, 3);
}

/// A finished, ranked daily: started at 0, finished at `time`.
async fn finish(conn: &Connection, user: i64, day: i64, time: i64) {
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms) VALUES (?1, ?2, 0, ?3, ?3)",
        params![user, day, time],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn month_board_counts_points() {
    let (_db, conn) = test_db().await;
    let a = user(&conn, "alice", Some("alice")).await;
    let b = user(&conn, "bob", Some("bob")).await;
    let c = user(&conn, "Calm Heron 7", None).await;
    let month = month_of(today());
    let (first, _) = month_days(month);
    // Day 1: alice, bob, carol; day 2: bob alone; last month: carol alone
    finish(&conn, a, first, 50_000).await;
    finish(&conn, b, first, 60_000).await;
    finish(&conn, c, first, 70_000).await;
    finish(&conn, b, first + 1, 80_000).await;
    finish(&conn, c, first - 1, 80_000).await;

    let board = month_board(&conn, None, Some(c)).await.unwrap();
    assert_eq!(board.month, month);
    let rows: Vec<_> = board
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.points, e.solved))
        .collect();
    // bob 15 + 20, alice 20, carol 10
    assert_eq!(
        rows,
        [("bob", 35, 2), ("alice", 20, 1), ("Calm Heron 7", 10, 1)]
    );
    assert!(board.entries[2].guest);
    assert_eq!(board.me.as_ref().map(|m| m.rank), Some(3));
    assert_eq!(board.players, 3);

    // Last month carol played alone (if there was a last month), earlier ones are clamped to
    // the launch
    if month > month_of(LAUNCH_DAY) {
        let last = month_board(&conn, Some(month - 1), None).await.unwrap();
        assert_eq!(last.entries.len(), 1);
        assert_eq!(last.entries[0].user_id, c);
    }
    assert_eq!(
        month_board(&conn, Some(0), None).await.unwrap().month,
        month_of(LAUNCH_DAY)
    );
    // The future is the running month
    assert_eq!(
        month_board(&conn, Some(month + 5), None)
            .await
            .unwrap()
            .month,
        month
    );
}

#[tokio::test]
async fn stamps_are_earned_kept_and_taken_back() {
    let (_db, conn) = test_db().await;
    let a = user(&conn, "alice", Some("alice")).await;
    let b = user(&conn, "bob", Some("bob")).await;
    let yesterday = today() - 1;
    finish(&conn, a, yesterday, 50_000).await;
    finish(&conn, b, yesterday, 60_000).await;

    let (book, new) = sync_stamps(&conn, a, true).await.unwrap();
    assert_eq!(book.len(), Stamp::ALL.len());
    for stamp in [Stamp::First, Stamp::Gold, Stamp::Podium, Stamp::EarlyBird] {
        assert!(new.contains(&stamp), "{stamp:?}");
    }
    let gold = book.iter().find(|p| p.stamp == Stamp::Gold).unwrap();
    assert!(gold.earned_at.is_some());
    // Nothing new the second time
    assert_eq!(sync_stamps(&conn, a, true).await.unwrap().1, []);

    // No stamp on the leaderboard until alice picks one, and only one she has
    let lb = leaderboard(&conn, yesterday, None).await.unwrap();
    assert_eq!(lb.entries[0].stamp, None);
    assert!(show_stamp(&conn, a, Some(Stamp::Sensei)).await.is_err());
    show_stamp(&conn, a, Some(Stamp::Gold)).await.unwrap();
    let lb = leaderboard(&conn, yesterday, None).await.unwrap();
    assert_eq!(lb.entries[0].stamp, Some(Stamp::Gold));
    assert_eq!(lb.entries[1].stamp, None);
    assert_eq!(stats(&conn, a).await.unwrap().shown, Some(Stamp::Gold));

    // A review takes the result off the board, and its stamps with it; bob moves up
    set_flag(&conn, a, yesterday, Some(Flag::Review))
        .await
        .unwrap();
    let (book, _) = sync_stamps(&conn, a, true).await.unwrap();
    assert!(book.iter().all(|p| p.earned_at.is_none()));
    assert_eq!(stats(&conn, a).await.unwrap().shown, None);
    // Synced in the background first: bob still hears about it on his next visit
    assert_eq!(sync_recent(&conn).await.unwrap(), 1);
    assert!(
        sync_stamps(&conn, b, true)
            .await
            .unwrap()
            .1
            .contains(&Stamp::Gold)
    );
}

#[tokio::test]
async fn stamps_from_the_move_log_and_the_time_of_day() {
    let (_db, conn) = test_db().await;
    // A Monday that is over: places only count then
    let day = MONDAY - 7;
    let a = user(&conn, "alice", Some("alice")).await;
    let b = user(&conn, "bob", Some("bob")).await;
    let daily = get_or_create(&conn, day).await.unwrap();
    for id in [a, b] {
        start(&conn, id, day, None).await.unwrap();
        age(&conn, id, day, 600_000).await;
    }
    // The time zone that makes it half past one at night for alice
    let minute = now_ms().rem_euclid(MS_PER_DAY) / 60_000;
    let offset = i32::try_from((minute - 90 + 840).rem_euclid(1440) - 840).unwrap();
    let log = played(&daily.solution);
    submit(
        &conn,
        a,
        day,
        daily.solution.clone(),
        20,
        log.clone(),
        Some(offset),
    )
    .await
    .unwrap();
    // Bob takes his last bridge away and puts it back, a moment after alice
    let mut wavering = log.clone();
    let last = *log.last().unwrap();
    wavering.push(MoveEvent {
        t: last.t + 1500,
        count: last.count - 1,
        ..last
    });
    wavering.push(MoveEvent {
        t: last.t + 3000,
        ..last
    });
    submit(&conn, b, day, daily.solution.clone(), 22, wavering, None)
        .await
        .unwrap();
    // Three tenths of a second behind her
    conn.execute(
        "UPDATE attempts SET time_ms = (SELECT time_ms + 300 FROM attempts WHERE user_id = ?1)
         WHERE user_id = ?2",
        params![a, b],
    )
    .await
    .unwrap();

    let earned = |book: &[crate::model::StampProgress], stamp| {
        book.iter()
            .any(|p| p.stamp == stamp && p.earned_at.is_some())
    };
    let (book, _) = sync_stamps(&conn, a, false).await.unwrap();
    for stamp in [Stamp::NightOwl, Stamp::StoneBridge, Stamp::PhotoFinish] {
        assert!(earned(&book, stamp), "{stamp:?}");
    }
    let (book, _) = sync_stamps(&conn, b, false).await.unwrap();
    assert!(earned(&book, Stamp::PhotoFinish));
    assert!(!earned(&book, Stamp::StoneBridge) && !earned(&book, Stamp::NightOwl));

    // Only the part of the day is stored, not the time zone
    let mut rows = conn
        .query(
            "SELECT daypart FROM attempts WHERE user_id = ?1",
            params![a],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    assert_eq!(row.get::<String>(0).unwrap(), "night");

    // Dailies from before runs were kept get theirs worked out
    conn.execute("UPDATE attempts SET run = NULL", ())
        .await
        .unwrap();
    conn.execute("DELETE FROM stamps", ()).await.unwrap();
    let (book, _) = sync_stamps(&conn, a, false).await.unwrap();
    assert!(earned(&book, Stamp::StoneBridge));
    let mut rows = conn
        .query("SELECT COUNT(*) FROM attempts WHERE run IS NULL", ())
        .await
        .unwrap();
    let missing: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    // Bob's are worked out on his next sync
    assert_eq!(missing, 1);
}

#[tokio::test]
async fn profiles_show_stamps_and_how_many_have_them() {
    let (_db, conn) = test_db().await;
    let a = user(&conn, "alice", Some("alice")).await;
    let b = user(&conn, "Calm Heron 7", None).await;
    let yesterday = today() - 1;
    finish(&conn, a, yesterday, 50_000).await;
    finish(&conn, b, yesterday, 60_000).await;

    assert_eq!(profile(&conn, a + b + 1).await.unwrap(), None);
    let p = profile(&conn, b).await.unwrap().unwrap();
    assert_eq!((p.name.as_str(), p.guest), ("Calm Heron 7", true));
    assert_eq!(p.stats.solved, 1);
    // Public reads are side-effect free. Awards are reconciled explicitly or in the background.
    assert_eq!(p.stats.players, 0);
    sync_stamps(&conn, b, false).await.unwrap();
    assert_eq!(profile(&conn, b).await.unwrap().unwrap().stats.players, 1);
    sync_stamps(&conn, a, false).await.unwrap();
    let p = profile(&conn, a).await.unwrap().unwrap();
    assert!(!p.guest);
    assert_eq!(p.stats.players, 2);
    let holders = |stamp| {
        p.stats
            .stamps
            .iter()
            .find(|s| s.stamp == stamp)
            .unwrap()
            .holders
    };
    // Both solved one, only alice won
    assert_eq!(holders(Stamp::First), 2);
    assert_eq!(holders(Stamp::Gold), 1);
    assert_eq!(holders(Stamp::Solved10), 0);
}

#[tokio::test]
async fn archive_only_has_earlier_days() {
    let (_db, conn) = test_db().await;
    assert!(archived(&conn, today()).await.is_err());
    assert_eq!(archived(&conn, MONDAY - 7).await.unwrap(), None);
    get_or_create(&conn, MONDAY).await.unwrap();
    if MONDAY < today() {
        assert!(archived(&conn, MONDAY).await.unwrap().is_some());
    }
}

#[tokio::test]
async fn deleting_keeps_the_rankings_but_nothing_personal() {
    use crate::server::auth::erase;
    let (_db, conn) = test_db().await;
    let a = user(&conn, "alice", Some("alice")).await;
    let b = user(&conn, "bob", Some("bob")).await;
    let today = today();
    finish(&conn, a, today - 2, 80_000).await;
    finish(&conn, a, today - 1, 70_000).await;
    finish(&conn, b, today - 1, 60_000).await;
    // Bob's personal bits: a move log and device, a flagged and an unfinished daily, a session,
    // a stamp, a Dojo submission
    conn.execute(
        "UPDATE attempts SET log = '[]', device = 'd', bridges = '[]', paid_actions = '[{\"id\":\"personal-intent-id\"}]' WHERE user_id = ?1",
        params![b],
    )
    .await
    .unwrap();
    for sql in [
        "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms, flag) VALUES (?1, ?2 - 2, 0, 1, 1, 'pace')",
        "INSERT INTO attempts (user_id, day, started_at) VALUES (?1, ?2, 0)",
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES ('t', ?1, 0, ?2 * 86400000)",
        "INSERT INTO stamps (user_id, stamp, earned_at) VALUES (?1, 'first', ?2)",
        "INSERT INTO dojo_submissions (user_id, name, wasm, sha256, size, created_at, updated_at, state, season)
         VALUES (?1, 'x', x'00', 'h', 1, ?2, 0, 'done', 1)",
    ] {
        conn.execute(sql, params![b, today]).await.unwrap();
    }
    let before = hall_of_fame(&conn).await.unwrap();

    erase(&conn, b).await.unwrap();

    // Alice keeps her silver from yesterday, bob's gold stays with a nameless player
    let after = hall_of_fame(&conn).await.unwrap();
    let medals = |f: &crate::model::HallOfFame| {
        f.medals
            .iter()
            .map(|e| (e.user_id, e.gold, e.silver))
            .collect::<Vec<_>>()
    };
    assert_eq!(medals(&after), medals(&before));
    let bob = after.solved.iter().find(|e| e.user_id == b).unwrap();
    assert_eq!((bob.name.as_str(), bob.solved), ("", 1));
    assert!(profile(&conn, b).await.unwrap().is_none());
    assert!(profile(&conn, a).await.unwrap().is_some());

    let left = |sql: &'static str| {
        let conn = conn.clone();
        async move {
            let mut rows = conn.query(sql, params![b]).await.unwrap();
            rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
        }
    };
    assert_eq!(
        left("SELECT COUNT(*) FROM attempts WHERE user_id = ?1").await,
        1
    );
    assert_eq!(
        left("SELECT COUNT(*) FROM attempts WHERE user_id = ?1 AND (log IS NOT NULL OR device IS NOT NULL OR bridges IS NOT NULL OR paid_actions != '[]')").await,
        0
    );
    for table in ["sessions", "stamps", "dojo_submissions"] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE user_id = ?1");
        let mut rows = conn.query(&sql, params![b]).await.unwrap();
        let n: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(n, 0, "{table}");
    }
    assert_eq!(
        left("SELECT COUNT(*) FROM users WHERE id = ?1 AND username IS NULL AND password_hash IS NULL AND name = '' AND deleted_at IS NOT NULL").await,
        1
    );
    // Nameless players don't get stamps back
    sync_recent(&conn).await.unwrap();
    assert_eq!(
        left("SELECT COUNT(*) FROM stamps WHERE user_id = ?1").await,
        0
    );

    // Keep only an anonymous tombstone even without ranked times: ids must not be reused.
    let c = user(&conn, "carol", Some("carol")).await;
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at) VALUES (?1, ?2, 0)",
        params![c, today],
    )
    .await
    .unwrap();
    erase(&conn, c).await.unwrap();
    let mut rows = conn
        .query("SELECT COUNT(*) FROM users WHERE id = ?1 AND deleted_at IS NOT NULL AND username IS NULL AND name = ''", params![c])
        .await
        .unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        1
    );
}

#[tokio::test]
async fn guests_nobody_can_reach_are_cleaned_up() {
    use crate::server::auth::cleanup_guests;
    let (_db, conn) = test_db().await;
    let now = now_ms();
    let gone = user(&conn, "Sleepy Crane 10", None).await;
    let expired = user(&conn, "Brave Koi 11", None).await;
    let active = user(&conn, "Quiet Owl 12", None).await;
    let member = user(&conn, "dana", Some("dana")).await;
    finish(&conn, gone, today() - 1, 50_000).await;
    for (u, expires) in [(expired, now - 1000), (active, now + MS_PER_DAY)] {
        conn.execute(
            "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, 0, ?3)",
            params![format!("t{u}"), u, expires],
        )
        .await
        .unwrap();
    }
    assert_eq!(cleanup_guests(&conn).await.unwrap(), 2);
    // Runs again without touching the nameless ones
    assert_eq!(cleanup_guests(&conn).await.unwrap(), 0);
    let mut rows = conn
        .query(
            "SELECT id, name FROM users WHERE deleted_at IS NULL ORDER BY id",
            (),
        )
        .await
        .unwrap();
    let mut kept = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        kept.push(row.get::<i64>(0).unwrap());
    }
    assert_eq!(kept, [active, member]);
    // Neither id can transfer a configured numeric grant to a later account.
    let mut rows = conn
        .query(
            "SELECT id FROM users WHERE deleted_at IS NOT NULL ORDER BY id",
            (),
        )
        .await
        .unwrap();
    let mut erased = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        erased.push(row.get::<i64>(0).unwrap());
    }
    assert_eq!(erased, [gone, expired]);
}

#[tokio::test]
async fn contact_messages_are_checked_stored_and_expire() {
    use crate::server::contact;
    let (_db, conn) = test_db().await;
    let message = "Hello, I found a bug in the editor.";
    let code = |r: Result<(), ServerFnError>| ApiError::from_message(&r.unwrap_err().to_string());
    assert_eq!(
        code(contact::send(&conn, "", "not an address", message, "").await),
        Some(ApiError::EmailInvalid)
    );
    assert_eq!(
        code(contact::send(&conn, "", "", "too short", "").await),
        Some(ApiError::MessageInvalid)
    );
    // Bots that fill in the hidden field are thanked, but nothing is kept
    contact::send(&conn, "Bot", "", message, "https://spam.example")
        .await
        .unwrap();
    assert_eq!(contact::list(&conn).await.unwrap(), []);

    contact::send(&conn, " Ann\n", "ann@example.com", message, "")
        .await
        .unwrap();
    let list = contact::list(&conn).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(
        (
            list[0].name.as_str(),
            list[0].email.as_str(),
            list[0].message.as_str()
        ),
        ("Ann", "ann@example.com", message)
    );

    // Half a year later it's gone
    conn.execute(
        "UPDATE contact_messages SET created_at = created_at - 181 * ?1",
        params![MS_PER_DAY],
    )
    .await
    .unwrap();
    contact::delete_old(&conn).await.unwrap();
    assert_eq!(contact::list(&conn).await.unwrap(), []);
}

/// What changing the daily's derivation (or sha2, or the generator) would do.
const DAILIES_CHANGE: &str = "the dailies not generated yet get other puzzles. Stored dailies \
                              stay (they are generated once and kept), but the seed must stay a \
                              secret function of the day, or upcoming dailies could be generated \
                              in advance";

#[test]
fn daily_seeds_are_unchanged() {
    // SHA-256 of "<secret>:<day>", the first 8 bytes little endian
    assert_eq!(
        [
            seed_with("hashi-daily", LAUNCH_DAY),
            seed_with("another secret", 20_745)
        ],
        [15_878_984_539_695_726_903, 12_953_186_420_028_709_165],
        "{DAILIES_CHANGE}"
    );
    assert_ne!(
        seed_with("a", 1),
        seed_with("b", 1),
        "the secret must matter"
    );
    assert_ne!(
        seed_with("a", 1),
        seed_with("a", 2),
        "every day has its own seed"
    );
}

#[test]
fn daily_puzzle_of_a_day_is_unchanged() {
    // Monday 19 October 2026, an easy one
    let day = 20_745;
    let params = params(day, seed_with("golden", day));
    assert_eq!(params.difficulty, Difficulty::Easy);
    let generated = backend::api::generate(&params);
    let solution: String = generated.solution.iter().map(u8::to_string).collect();
    let actual = format!("{}solution {solution}", generated.puzzle.to_txt());
    let expected = "\
7 7
.......
2.6..3.
.......
...2.4.
..3.1..
.......
1.2..2.
solution 20221211011";
    assert_eq!(actual, expected, "{DAILIES_CHANGE}\n\nactual:\n{actual}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checks_and_submission_share_one_penalty_snapshot() {
    let (db, conn) = test_db().await;
    let id = user(&conn, "racer", Some("racer")).await;
    let info = start(&conn, id, MONDAY, None).await.unwrap();
    let puzzle = info.puzzle.unwrap();
    let daily = get_or_create(&conn, MONDAY).await.unwrap();
    age(&conn, id, MONDAY, 60_000).await;
    let first = db::connect(&db.database).unwrap();
    let second = db::connect(&db.database).unwrap();
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let gate = barrier.clone();
    let solution = daily.solution.clone();
    let submitter = tokio::spawn(async move {
        gate.wait().await;
        submit(
            &first,
            id,
            MONDAY,
            solution.clone(),
            10,
            played(&solution),
            None,
        )
        .await
    });
    let checker = tokio::spawn(async move {
        barrier.wait().await;
        check(&second, id, MONDAY, Board::new(puzzle).empty_bridges()).await
    });
    submitter.await.unwrap().unwrap();
    let outcome = checker.await.unwrap();
    let saved = attempt(&conn, id, MONDAY).await.unwrap().unwrap();
    assert_eq!(saved.checks, i64::from(outcome.is_ok()));
    assert_eq!(
        saved.time_ms,
        Some(saved.finished_at.unwrap() - saved.started_at + saved.penalty_ms())
    );
    if let Err(e) = outcome {
        assert_eq!(
            ApiError::from_message(&e.to_string()),
            Some(ApiError::AlreadyFinished)
        );
    }
}

#[tokio::test]
async fn erased_accounts_never_donate_their_numeric_identity() {
    let (_db, conn) = test_db().await;
    let old = user(&conn, "old_admin", Some("old_admin")).await;
    crate::server::auth::erase(&conn, old).await.unwrap();
    let new = user(&conn, "replacement", Some("replacement")).await;
    assert!(new > old);
    assert!(profile(&conn, old).await.unwrap().is_none());
}

#[tokio::test]
async fn contact_history_is_bounded_and_can_be_paged_without_duplicates() {
    let (_db, conn) = test_db().await;
    for i in 0..105 {
        conn.execute("INSERT INTO contact_messages (name, email, message, created_at) VALUES ('', '', 'message', ?1)", params![i]).await.unwrap();
    }
    let first = crate::server::contact::page(&conn, None).await.unwrap();
    assert_eq!(first.len(), 100);
    let second = crate::server::contact::page(&conn, Some(first.last().unwrap().id))
        .await
        .unwrap();
    assert_eq!(second.len(), 5);
    assert!(second.iter().all(|m| m.id < first.last().unwrap().id));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_paid_intents_charge_once_across_connections() {
    use crate::api::PaidAction;
    let (db, conn) = test_db().await;
    let id = user(&conn, "paid racer", None).await;
    let info = start(&conn, id, MONDAY, None).await.unwrap();
    let empty = Board::new(info.puzzle.unwrap()).empty_bridges();
    // Exercise both purchases, each with two independent requests starting together.
    for (action, request) in [
        (PaidAction::Hint, "concurrent-hint-001"),
        (PaidAction::Check, "concurrent-check-01"),
    ] {
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let run = |conn: Connection| {
            let gate = barrier.clone();
            let bridges = empty.clone();
            tokio::spawn(async move {
                gate.wait().await;
                paid_current(&conn, id, MONDAY, bridges, action, request)
                    .await
                    .unwrap()
            })
        };
        let first = run(db::connect(&db.database).unwrap());
        let second = run(db::connect(&db.database).unwrap());
        assert_eq!(first.await.unwrap(), second.await.unwrap());
    }
    let attempt = attempt(&conn, id, MONDAY).await.unwrap().unwrap();
    assert_eq!((attempt.hints, attempt.checks), (1, 1));
}

#[tokio::test]
async fn a_lost_paid_reply_replays_but_different_intents_still_charge() {
    use crate::api::{PaidAction, PaidOutcome};
    let (_db, conn) = test_db().await;
    let id = user(&conn, "retry player", None).await;
    let info = start(&conn, id, MONDAY, None).await.unwrap();
    let empty = Board::new(info.puzzle.unwrap()).empty_bridges();
    let lost = paid_current(
        &conn,
        id,
        MONDAY,
        empty.clone(),
        PaidAction::Hint,
        "lost-hint-response-1",
    )
    .await
    .unwrap();
    assert!(matches!(lost.outcome, PaidOutcome::Hint(Some(_))));
    // A different ID explicitly buys another hint, even on identical bridges.
    let other = paid_current(
        &conn,
        id,
        MONDAY,
        empty.clone(),
        PaidAction::Hint,
        "new-hint-intent-002",
    )
    .await
    .unwrap();
    assert_eq!((other.hints, other.checks), (2, 0));
    let recovered = paid_current(
        &conn,
        id,
        MONDAY,
        empty.clone(),
        PaidAction::Hint,
        "lost-hint-response-1",
    )
    .await
    .unwrap();
    assert_eq!(recovered.outcome, lost.outcome);
    assert_eq!((recovered.hints, recovered.checks), (2, 0));
    // Retry remains valid after finishing; it cannot buy a penalty after the time was saved.
    let daily = get_or_create(&conn, MONDAY).await.unwrap();
    age(&conn, id, MONDAY, 60_000).await;
    submit(
        &conn,
        id,
        MONDAY,
        daily.solution.clone(),
        10,
        played(&daily.solution),
        None,
    )
    .await
    .unwrap();
    let again = paid_current(
        &conn,
        id,
        MONDAY,
        empty.clone(),
        PaidAction::Hint,
        "lost-hint-response-1",
    )
    .await
    .unwrap();
    assert_eq!(again, recovered);
    assert!(
        paid_current(
            &conn,
            id,
            MONDAY,
            empty,
            PaidAction::Hint,
            "new-after-finish-001"
        )
        .await
        .is_err()
    );
    let saved = attempt(&conn, id, MONDAY).await.unwrap().unwrap();
    assert_eq!(saved.hints, 2);
    assert_eq!(
        saved.time_ms,
        Some(saved.finished_at.unwrap() - saved.started_at + saved.penalty_ms())
    );
}

#[tokio::test]
async fn paid_receipts_are_bound_to_player_day_action_and_payload() {
    use crate::api::PaidAction;
    let (_db, conn) = test_db().await;
    let a = user(&conn, "first payer", None).await;
    let b = user(&conn, "second payer", None).await;
    let info = start(&conn, a, MONDAY, None).await.unwrap();
    start(&conn, b, MONDAY, None).await.unwrap();
    let empty = Board::new(info.puzzle.unwrap()).empty_bridges();
    let request = "ownership-intent-01";
    paid_current(&conn, a, MONDAY, empty.clone(), PaidAction::Check, request)
        .await
        .unwrap();
    assert!(
        paid_current(&conn, a, MONDAY, empty.clone(), PaidAction::Hint, request)
            .await
            .is_err()
    );
    let mut changed = empty.clone();
    changed[0] = 1;
    assert!(
        paid_current(&conn, a, MONDAY, changed, PaidAction::Check, request)
            .await
            .is_err()
    );
    assert!(
        paid_current(
            &conn,
            a,
            MONDAY + 1,
            empty.clone(),
            PaidAction::Check,
            request
        )
        .await
        .is_err()
    );
    let second = paid_current(&conn, b, MONDAY, empty, PaidAction::Check, request)
        .await
        .unwrap();
    assert_eq!(second.checks, 1);
    let first = attempt(&conn, a, MONDAY).await.unwrap().unwrap();
    assert_eq!((first.hints, first.checks), (0, 1));
}

#[tokio::test]
async fn paid_receipts_are_bounded_without_evicting_retry_safety() {
    use crate::api::PaidAction;
    let (_db, conn) = test_db().await;
    let id = user(&conn, "bounded payer", None).await;
    let info = start(&conn, id, MONDAY, None).await.unwrap();
    let empty = Board::new(info.puzzle.unwrap()).empty_bridges();
    for bad in [
        String::new(),
        "short".into(),
        "x".repeat(65),
        "invalid!intent-id-1".into(),
    ] {
        assert!(
            paid_current(&conn, id, MONDAY, empty.clone(), PaidAction::Check, &bad)
                .await
                .is_err()
        );
    }
    assert!(
        paid_current(
            &conn,
            id,
            MONDAY,
            vec![3],
            PaidAction::Check,
            "invalid-board-id-01"
        )
        .await
        .is_err()
    );
    assert_eq!(attempt(&conn, id, MONDAY).await.unwrap().unwrap().checks, 0);
    for n in 0..256 {
        paid_current(
            &conn,
            id,
            MONDAY,
            empty.clone(),
            PaidAction::Check,
            &format!("bounded-intent-{n:04}"),
        )
        .await
        .unwrap();
    }
    let overflow = paid_current(
        &conn,
        id,
        MONDAY,
        empty.clone(),
        PaidAction::Check,
        "overflow-intent-001",
    )
    .await
    .unwrap_err();
    assert_eq!(
        ApiError::from_message(&overflow.to_string()),
        Some(ApiError::TooManyAttempts)
    );
    let replay = paid_current(
        &conn,
        id,
        MONDAY,
        empty,
        PaidAction::Check,
        "bounded-intent-0000",
    )
    .await
    .unwrap();
    assert_eq!(replay.checks, 256);
}

#[tokio::test]
async fn paid_noop_hints_replay_without_penalties_and_failed_writes_leave_no_receipt() {
    use crate::api::{PaidAction, PaidOutcome};
    let (_db, conn) = test_db().await;
    let id = user(&conn, "no-op payer", None).await;
    start(&conn, id, MONDAY, None).await.unwrap();
    let daily = get_or_create(&conn, MONDAY).await.unwrap();
    let request = "no-op-hint-intent-01";
    let noop = paid_current(
        &conn,
        id,
        MONDAY,
        daily.solution.clone(),
        PaidAction::Hint,
        request,
    )
    .await
    .unwrap();
    assert_eq!(noop.outcome, PaidOutcome::Hint(None));
    assert_eq!(noop.hints, 0);
    assert_eq!(
        paid_current(
            &conn,
            id,
            MONDAY,
            daily.solution.clone(),
            PaidAction::Hint,
            request
        )
        .await
        .unwrap(),
        noop
    );
    conn.execute_batch("CREATE TRIGGER fail_paid BEFORE UPDATE OF paid_actions ON attempts BEGIN SELECT RAISE(ABORT, 'injected receipt write failure'); END;").await.unwrap();
    let empty = Board::new(daily.puzzle).empty_bridges();
    assert!(
        paid_current(
            &conn,
            id,
            MONDAY,
            empty.clone(),
            PaidAction::Check,
            "failed-write-id-001"
        )
        .await
        .is_err()
    );
    assert_eq!(attempt(&conn, id, MONDAY).await.unwrap().unwrap().checks, 0);
    conn.execute_batch("DROP TRIGGER fail_paid").await.unwrap();
    let retry = paid_current(
        &conn,
        id,
        MONDAY,
        empty,
        PaidAction::Check,
        "failed-write-id-001",
    )
    .await
    .unwrap();
    assert_eq!(retry.checks, 1);
}

#[tokio::test]
async fn lost_actions_never_charge_a_reused_guest_id_even_with_identical_start_times() {
    use crate::api::{PaidAction, PaidIntent};
    let (_db, conn) = test_db().await;
    for action in [PaidAction::Hint, PaidAction::Check] {
        // Keep the guest at the maximum rowid so SQLite really reuses it after transfer.
        let account = user(&conn, "receiving owner", Some("receiving_owner")).await;
        let guest = user(&conn, "original guest", None).await;
        let token = crate::server::auth::insert_session(&conn, guest)
            .await
            .unwrap();
        let info = start(&conn, guest, MONDAY, None).await.unwrap();
        let original = info.attempt.unwrap();
        let intent = PaidIntent {
            attempt_id: original.attempt_id.clone(),
            user: guest,
            day: MONDAY,
            started_at: original.started_at,
            action,
            bridges: Board::new(info.puzzle.unwrap()).empty_bridges(),
            id: "reused-guest-intent-01".into(),
        };
        let accepted = paid_action(
            &conn,
            guest,
            MONDAY,
            intent.bridges.clone(),
            action,
            &intent.id,
            &intent.attempt_id,
        )
        .await
        .unwrap();
        crate::server::auth::merge_guest(&conn, guest, account, &token)
            .await
            .unwrap();
        // A stale tab can retry even before the replacement guest starts.
        assert_eq!(
            ApiError::from_message(
                &paid_action(
                    &conn,
                    guest,
                    MONDAY,
                    intent.bridges.clone(),
                    action,
                    &intent.id,
                    &intent.attempt_id
                )
                .await
                .unwrap_err()
                .to_string()
            ),
            Some(ApiError::SessionExpired)
        );
        let replacement = user(&conn, "replacement guest", None).await;
        assert_eq!(replacement, guest, "exercise actual SQLite rowid reuse");
        let replacement_info = start(&conn, replacement, MONDAY, None)
            .await
            .unwrap()
            .attempt
            .unwrap();
        assert_ne!(replacement_info.attempt_id, intent.attempt_id);
        conn.execute(
            "UPDATE attempts SET started_at = ?1 WHERE user_id = ?2",
            params![original.started_at, replacement],
        )
        .await
        .unwrap();
        for expected in [&intent.attempt_id, &String::new()] {
            let error = paid_action(
                &conn,
                replacement,
                MONDAY,
                intent.bridges.clone(),
                action,
                &intent.id,
                expected,
            )
            .await
            .unwrap_err();
            assert_eq!(
                ApiError::from_message(&error.to_string()),
                Some(ApiError::SessionExpired)
            );
        }
        let untouched = attempt(&conn, replacement, MONDAY).await.unwrap().unwrap();
        assert_eq!((untouched.hints, untouched.checks), (0, 0));
        let mut rows = conn
            .query(
                "SELECT paid_actions FROM attempts WHERE user_id = ?1",
                params![replacement],
            )
            .await
            .unwrap();
        assert_eq!(
            rows.next()
                .await
                .unwrap()
                .unwrap()
                .get::<String>(0)
                .unwrap(),
            "[]"
        );
        drop(rows);
        assert!(
            recover_actions(
                &conn,
                replacement,
                MONDAY,
                intent.started_at,
                std::slice::from_ref(&intent)
            )
            .await
            .unwrap()
            .is_none()
        );
        let recovered = recover_actions(
            &conn,
            account,
            MONDAY,
            intent.started_at,
            std::slice::from_ref(&intent),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(recovered.intent, intent);
        assert_eq!(recovered.reply, accepted);
        assert_eq!(
            attempt(&conn, account, MONDAY)
                .await
                .unwrap()
                .unwrap()
                .attempt_id,
            original.attempt_id
        );
        // Permit another iteration with a separate account/day ownership history.
        crate::server::auth::erase(&conn, account).await.unwrap();
    }
}

#[tokio::test]
async fn transferred_paid_receipts_recover_only_the_exact_original_purchase_without_writes() {
    use crate::api::{PaidAction, PaidIntent};
    let (_db, conn) = test_db().await;
    let guest = user(&conn, "recover guest", None).await;
    let account = user(&conn, "recover owner", Some("recover_owner")).await;
    let info = start(&conn, guest, MONDAY, None).await.unwrap();
    let empty = Board::new(info.puzzle.unwrap()).empty_bridges();
    let mut intents = vec![];
    for (action, id) in [
        (PaidAction::Hint, "transferred-hint-01"),
        (PaidAction::Check, "transferred-check-1"),
    ] {
        paid_current(&conn, guest, MONDAY, empty.clone(), action, id)
            .await
            .unwrap();
        intents.push(PaidIntent {
            attempt_id: String::new(),
            user: guest,
            day: MONDAY,
            started_at: info.attempt.as_ref().unwrap().started_at,
            action,
            bridges: empty.clone(),
            id: id.into(),
        });
    }
    // The previous release's receipts had no provenance field. The actual merge must annotate
    // those too, using database ownership rather than a browser assertion.
    conn.execute("UPDATE attempts SET paid_actions = (SELECT json_group_array(json_remove(value, '$.origin_user')) FROM json_each(attempts.paid_actions)) WHERE user_id = ?1", params![guest]).await.unwrap();
    crate::server::auth::merge_guest(
        &conn,
        guest,
        account,
        &crate::server::auth::insert_session(&conn, guest)
            .await
            .unwrap(),
    )
    .await
    .unwrap();
    let started = intents[0].started_at;
    for intent in &intents {
        let recovered = recover_actions(
            &conn,
            account,
            MONDAY,
            started,
            std::slice::from_ref(intent),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(recovered.intent, *intent);
        assert_eq!((recovered.reply.hints, recovered.reply.checks), (1, 1));
        // Every field participates in recovery; timestamps alone never authorize adoption.
        for changed in [
            PaidIntent {
                attempt_id: String::new(),
                user: account,
                ..intent.clone()
            },
            PaidIntent {
                attempt_id: String::new(),
                id: "unaccepted-intent-1".into(),
                ..intent.clone()
            },
            PaidIntent {
                attempt_id: String::new(),
                action: if intent.action == PaidAction::Hint {
                    PaidAction::Check
                } else {
                    PaidAction::Hint
                },
                ..intent.clone()
            },
            PaidIntent {
                attempt_id: String::new(),
                bridges: vec![2; empty.len()],
                ..intent.clone()
            },
            PaidIntent {
                attempt_id: String::new(),
                started_at: started + 1,
                ..intent.clone()
            },
            PaidIntent {
                attempt_id: String::new(),
                day: MONDAY + 1,
                ..intent.clone()
            },
        ] {
            assert!(
                recover_actions(&conn, account, MONDAY, started, &[changed])
                    .await
                    .unwrap()
                    .is_none()
            );
        }
    }
    assert!(
        recover_actions(&conn, guest, MONDAY, started, &intents)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        recover_actions(&conn, account, MONDAY, started + 1, &intents)
            .await
            .unwrap()
            .is_none()
    );
    let untouched = attempt(&conn, account, MONDAY).await.unwrap().unwrap();
    assert_eq!((untouched.hints, untouched.checks), (1, 1));
    // Also recover after the attempt finishes; the lookup must never need an open purchase.
    reveal(&conn, account, MONDAY).await.unwrap();
    assert!(
        recover_actions(&conn, account, MONDAY, started, &intents)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        ApiError::from_message(
            &recover_actions(
                &conn,
                account,
                MONDAY,
                started,
                &vec![intents[0].clone(); 257]
            )
            .await
            .unwrap_err()
            .to_string()
        ),
        Some(ApiError::WrongPuzzle)
    );
}

#[tokio::test]
async fn conflicting_attempts_with_identical_timestamps_ids_and_boards_cannot_adopt_guest_receipts()
{
    use crate::api::{PaidAction, PaidIntent};
    let (_db, conn) = test_db().await;
    let guest = user(&conn, "conflicting guest", None).await;
    let account = user(&conn, "conflicting owner", Some("conflicting_owner")).await;
    let info = start(&conn, guest, MONDAY, None).await.unwrap();
    let empty = Board::new(info.puzzle.unwrap()).empty_bridges();
    let started = info.attempt.unwrap().started_at;
    let intent = PaidIntent {
        attempt_id: String::new(),
        user: guest,
        day: MONDAY,
        started_at: started,
        action: PaidAction::Hint,
        bridges: empty.clone(),
        id: "same-receipt-id-001".into(),
    };
    for owner in [guest, account] {
        start(&conn, owner, MONDAY, None).await.unwrap();
        paid_current(
            &conn,
            owner,
            MONDAY,
            empty.clone(),
            PaidAction::Hint,
            &intent.id,
        )
        .await
        .unwrap();
    }
    conn.execute(
        "UPDATE attempts SET started_at = ?1 WHERE day = ?2",
        params![started, MONDAY],
    )
    .await
    .unwrap();
    crate::server::auth::merge_guest(
        &conn,
        guest,
        account,
        &crate::server::auth::insert_session(&conn, guest)
            .await
            .unwrap(),
    )
    .await
    .unwrap();
    assert!(
        recover_actions(
            &conn,
            account,
            MONDAY,
            started,
            std::slice::from_ref(&intent)
        )
        .await
        .unwrap()
        .is_none()
    );
    let own = PaidIntent {
        attempt_id: String::new(),
        user: account,
        ..intent
    };
    let recovered = recover_actions(&conn, account, MONDAY, started, &[own])
        .await
        .unwrap()
        .unwrap();
    assert_eq!((recovered.reply.hints, recovered.reply.checks), (1, 0));
    let unrelated = user(&conn, "unrelated owner", Some("unrelated_owner")).await;
    assert!(
        recover_actions(&conn, unrelated, MONDAY, started, &[recovered.intent])
            .await
            .unwrap()
            .is_none()
    );
    assert!(attempt(&conn, unrelated, MONDAY).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_paid_action_racing_guest_transfer_is_either_recoverable_or_never_charged() {
    use crate::api::{PaidAction, PaidIntent};
    let (db, conn) = test_db().await;
    for action in [PaidAction::Hint, PaidAction::Check] {
        let guest = user(&conn, "racing guest", None).await;
        let account = user(&conn, "racing owner", Some(&format!("racer_{guest}"))).await;
        let info = start(&conn, guest, MONDAY, None).await.unwrap();
        let empty = Board::new(info.puzzle.unwrap()).empty_bridges();
        let intent = PaidIntent {
            attempt_id: String::new(),
            user: guest,
            day: MONDAY,
            started_at: info.attempt.unwrap().started_at,
            action,
            bridges: empty.clone(),
            id: "racing-transfer-001".into(),
        };
        let token = crate::server::auth::insert_session(&conn, guest)
            .await
            .unwrap();
        let gate = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let buying_conn = db::connect(&db.database).unwrap();
        let buying_gate = gate.clone();
        let buying = tokio::spawn(async move {
            buying_gate.wait().await;
            paid_current(
                &buying_conn,
                guest,
                MONDAY,
                empty,
                action,
                "racing-transfer-001",
            )
            .await
        });
        let merging_conn = db::connect(&db.database).unwrap();
        let merging = tokio::spawn(async move {
            gate.wait().await;
            crate::server::auth::merge_guest(&merging_conn, guest, account, &token).await
        });
        let bought = buying.await.unwrap();
        merging.await.unwrap().unwrap();
        let recovered = recover_actions(&conn, account, MONDAY, intent.started_at, &[intent])
            .await
            .unwrap();
        let current = attempt(&conn, account, MONDAY).await.unwrap().unwrap();
        match bought {
            Ok(reply) => {
                assert_eq!(recovered.unwrap().reply, reply);
                assert_eq!(current.hints + current.checks, 1);
            }
            Err(error) => {
                assert_eq!(
                    ApiError::from_message(&error.to_string()),
                    Some(ApiError::SessionExpired)
                );
                assert!(recovered.is_none());
                assert_eq!(current.hints + current.checks, 0);
            }
        }
    }
}

#[tokio::test]
async fn a_login_with_an_earlier_guest_snapshot_cannot_transfer_a_claimed_account() {
    let (_db, conn) = test_db().await;
    let claimed = user(&conn, "just registered", Some("just_registered")).await;
    let other = user(&conn, "login destination", Some("login_destination")).await;
    start(&conn, claimed, MONDAY, None).await.unwrap();
    // Login observed this ID as a guest, then registration claimed it before merge got its
    // transaction. The claimed identity and its attempt must remain owned by that account.
    crate::server::auth::merge_guest(
        &conn,
        claimed,
        other,
        &crate::server::auth::insert_session(&conn, claimed)
            .await
            .unwrap(),
    )
    .await
    .unwrap();
    assert!(attempt(&conn, claimed, MONDAY).await.unwrap().is_some());
    assert!(attempt(&conn, other, MONDAY).await.unwrap().is_none());
}

#[tokio::test]
async fn guarded_submit_replays_lost_success_without_mutating_the_finish_or_penalties() {
    let (db, conn) = test_db().await;
    let id = user(&conn, "recover", Some("recover")).await;
    let started = start(&conn, id, MONDAY, None).await.unwrap();
    let identity = started.attempt.unwrap().attempt_id;
    let daily = get_or_create(&conn, MONDAY).await.unwrap();
    age(&conn, id, MONDAY, 600_000).await;
    check(&conn, id, MONDAY, Board::new(daily.puzzle).empty_bridges())
        .await
        .unwrap();
    let first = submit_owned(
        &conn,
        id,
        MONDAY,
        daily.solution.clone(),
        20,
        played(&daily.solution),
        Some(-120),
        &identity,
    )
    .await
    .unwrap();
    let original = attempt(&conn, id, MONDAY).await.unwrap().unwrap();
    let saved = conn
        .query(
            "SELECT log, moves, run, daypart FROM attempts WHERE user_id = ?1 AND day = ?2",
            params![id, MONDAY],
        )
        .await
        .unwrap()
        .next()
        .await
        .unwrap()
        .unwrap();
    let saved_fields = (
        saved.get::<String>(0).unwrap(),
        saved.get::<i64>(1).unwrap(),
        saved.get::<String>(2).unwrap(),
        saved.get::<Option<String>>(3).unwrap(),
    );
    // Different payload metadata on replay must not overwrite the accepted log/moves/time.
    // Separate connections also exercise the serialization used for simultaneous retries.
    let mut tasks = vec![];
    for _ in 0..2 {
        let connection = db.database.connect().unwrap();
        let solution = daily.solution.clone();
        let expected = identity.clone();
        tasks.push(tokio::spawn(async move {
            submit_owned(
                &connection,
                id,
                MONDAY,
                solution,
                999,
                vec![],
                None,
                &expected,
            )
            .await
            .unwrap()
        }));
    }
    for task in tasks {
        let replay = task.await.unwrap();
        assert_eq!(
            (replay.time_ms, replay.rank, replay.flag),
            (first.time_ms, first.rank, first.flag)
        );
    }
    let confirmed = owned_info(&conn, id, MONDAY, &identity).await.unwrap();
    assert_eq!(confirmed.attempt, Some(original));
    let after = conn
        .query(
            "SELECT log, moves, run, daypart FROM attempts WHERE user_id = ?1 AND day = ?2",
            params![id, MONDAY],
        )
        .await
        .unwrap()
        .next()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            after.get::<String>(0).unwrap(),
            after.get::<i64>(1).unwrap(),
            after.get::<String>(2).unwrap(),
            after.get::<Option<String>>(3).unwrap()
        ),
        saved_fields
    );
    assert_eq!(finishers(&conn, MONDAY).await.unwrap(), 1);
    // The legacy endpoint retains its behavior and exact public request shape.
    assert!(
        submit(&conn, id, MONDAY, daily.solution.clone(), 20, vec![], None)
            .await
            .is_err()
    );
    for wrong in ["", "replacement-attempt"] {
        assert!(owned_info(&conn, id, MONDAY, wrong).await.is_err());
        assert!(
            submit_owned(
                &conn,
                id,
                MONDAY,
                daily.solution.clone(),
                20,
                vec![],
                None,
                wrong
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn guarded_submit_rejects_a_replaced_attempt_before_any_write_or_replay() {
    let (_db, conn) = test_db().await;
    let id = user(&conn, "replace", Some("replace")).await;
    let identity = start(&conn, id, MONDAY, None)
        .await
        .unwrap()
        .attempt
        .unwrap()
        .attempt_id;
    let daily = get_or_create(&conn, MONDAY).await.unwrap();
    // Same account/day/start millisecond, different stable identity (as in guest replacement).
    conn.execute(
        "UPDATE attempts SET attempt_id = 'replacement' WHERE user_id = ?1 AND day = ?2",
        params![id, MONDAY],
    )
    .await
    .unwrap();
    assert!(owned_info(&conn, id, MONDAY, &identity).await.is_err());
    assert!(
        submit_owned(
            &conn,
            id,
            MONDAY,
            daily.solution,
            20,
            vec![],
            None,
            &identity
        )
        .await
        .is_err()
    );
    let untouched = attempt(&conn, id, MONDAY).await.unwrap().unwrap();
    assert!(!untouched.finished());
    assert_eq!((untouched.hints, untouched.checks), (0, 0));
    assert!(untouched.bridges.is_none());
}
