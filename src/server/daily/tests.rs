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
    path: std::path::PathBuf,
    /// Kept open for as long as the test runs.
    _database: libsql::Database,
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn test_db() -> (TestDb, Connection) {
    let name = format!(
        "hashi-test-{}-{}.db",
        std::process::id(),
        now_ms() ^ rand_suffix()
    );
    let path = std::env::temp_dir().join(name);
    let database = db::open_local(path.to_str().unwrap()).await.unwrap();
    let conn = database.connect().unwrap();
    db::migrate(&conn).await.unwrap();
    // Running the migrations again is a no-op
    db::migrate(&conn).await.unwrap();
    (
        TestDb {
            path,
            _database: database,
        },
        conn,
    )
}

fn rand_suffix() -> i64 {
    static COUNTER: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) << 40
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
    // Bob's profile synced his stamps, alice's aren't stored yet
    assert_eq!(p.stats.players, 1);

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
        "UPDATE attempts SET log = '[]', device = 'd', bridges = '[]' WHERE user_id = ?1",
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
        left("SELECT COUNT(*) FROM attempts WHERE user_id = ?1 AND (log IS NOT NULL OR device IS NOT NULL OR bridges IS NOT NULL)").await,
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

    // Without a ranked time nothing is left at all
    let c = user(&conn, "carol", Some("carol")).await;
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at) VALUES (?1, ?2, 0)",
        params![c, today],
    )
    .await
    .unwrap();
    erase(&conn, c).await.unwrap();
    let mut rows = conn
        .query("SELECT COUNT(*) FROM users WHERE id = ?1", params![c])
        .await
        .unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        0
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
    // The guest with a ranked time stays as a nameless player, the other one is gone
    let mut rows = conn
        .query("SELECT id FROM users WHERE deleted_at IS NOT NULL", ())
        .await
        .unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        gone
    );
    assert!(rows.next().await.unwrap().is_none());
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
