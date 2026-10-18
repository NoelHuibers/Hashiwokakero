//! Tests of the daily puzzle against a fresh database file.

use common::Board;

use super::*;
use crate::model::{CHECK_PENALTY_MS, Flag, HINT_PENALTY_MS, LAUNCH_DAY, MoveEvent};
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
            played(&daily.solution)
        )
        .await
        .is_err()
    );
    start(&conn, a, MONDAY, None).await.unwrap();
    start(&conn, b, MONDAY, None).await.unwrap();
    // Wrong boards are rejected
    assert!(
        submit(&conn, a, MONDAY, board.empty_bridges(), 1, vec![])
            .await
            .is_err()
    );
    assert!(submit(&conn, a, MONDAY, vec![1], 1, vec![]).await.is_err());

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
            played(&daily.solution)
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
    let r = submit(&conn, bot, MONDAY, daily.solution.clone(), 1, vec![])
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
    // Another puzzle isn't locked, and the daily unlocks once you gave up or finished
    assert!(
        !locked(
            &conn,
            None,
            &get_or_create(&conn, MONDAY).await.unwrap().puzzle
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
        submit(&conn, id, MONDAY, solution, 5, vec![])
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
async fn stats_and_all_time() {
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

    let all = all_time(&conn).await.unwrap();
    assert_eq!(all[0].name, "alice");
    assert_eq!((all[0].solved, all[0].best_streak), (3, 3));
    assert_eq!((all[1].solved, all[1].best_streak), (1, 1));
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
