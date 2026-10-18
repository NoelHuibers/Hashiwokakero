//! The daily puzzle: one per UTC day, the same for everyone, timed by the server.

use std::collections::HashMap;

use common::{Board, Difficulty, GenerateParams, Puzzle};
use leptos::prelude::*;
use libsql::{Connection, params};
use sha2::{Digest, Sha256};

use super::db::db_err;
use super::{count, now_ms, run_blocking};
use crate::model::{
    AllTimeEntry, ApiError, AttemptInfo, DailyInfo, DayResult, Flag, Leaderboard, LeaderboardEntry,
    MoveEvent, SubmitResult, UserStats, canonical, daily_difficulty, day_of, pick_hint, streaks,
    verify_log, wrong_edges,
};

/// Ranked entries shown on a daily leaderboard.
const LEADERBOARD_SIZE: i64 = 50;
/// Days shown in the player's recent results.
const RECENT_DAYS: i64 = 35;

pub fn today() -> i64 {
    day_of(now_ms())
}

/// Seeds depend on a server secret, so upcoming puzzles can't be generated in advance with the
/// open source generator.
fn seed(day: i64) -> u64 {
    let secret = std::env::var("HASHI_DAILY_SECRET").unwrap_or_else(|_| "hashi-daily".into());
    let digest = Sha256::digest(format!("{secret}:{day}").as_bytes());
    u64::from_le_bytes(digest[..8].try_into().expect("8 bytes"))
}

#[derive(Debug)]
pub struct Daily {
    pub difficulty: Difficulty,
    pub puzzle: Puzzle,
    pub solution: Vec<u8>,
}

fn parse_difficulty(id: &str) -> Difficulty {
    Difficulty::from_id(id).unwrap_or_default()
}

fn json<T: serde::Serialize>(value: &T) -> Result<String, ServerFnError> {
    serde_json::to_string(value).map_err(ServerFnError::new)
}

fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, ServerFnError> {
    serde_json::from_str(s).map_err(ServerFnError::new)
}

async fn load(conn: &Connection, day: i64) -> Result<Option<Daily>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT difficulty, puzzle, solution FROM dailies WHERE day = ?1",
            params![day],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    let difficulty: String = row.get(0).map_err(db_err)?;
    let puzzle: String = row.get(1).map_err(db_err)?;
    let solution: String = row.get(2).map_err(db_err)?;
    let puzzle: Puzzle = from_json(&puzzle)?;
    Ok(Some(Daily {
        difficulty: parse_difficulty(&difficulty),
        // Stored JSON skips Puzzle::new, keep the islands in reading order
        puzzle: Puzzle::new(puzzle.width, puzzle.height, puzzle.islands),
        solution: from_json(&solution)?,
    }))
}

/// The puzzle of a day, generated and stored on first use.
pub async fn get_or_create(conn: &Connection, day: i64) -> Result<Daily, ServerFnError> {
    if let Some(daily) = load(conn, day).await? {
        return Ok(daily);
    }
    let difficulty = daily_difficulty(day);
    let (width, height) = difficulty.size();
    let params = GenerateParams {
        width,
        height,
        difficulty,
        seed: seed(day),
    };
    let generated = run_blocking(move || Ok(backend::api::generate(&params))).await?;
    // Another server may have been faster, then its puzzle wins
    conn.execute(
        "INSERT OR IGNORE INTO dailies (day, difficulty, puzzle, solution, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            day,
            difficulty.id(),
            json(&generated.puzzle)?,
            json(&generated.solution)?,
            now_ms()
        ],
    )
    .await
    .map_err(db_err)?;
    load(conn, day)
        .await?
        .ok_or_else(|| ApiError::Internal.fail())
}

/// A puzzle of an earlier day, for practice.
pub async fn archived(conn: &Connection, day: i64) -> Result<Option<Puzzle>, ServerFnError> {
    if day >= today() {
        return Err(ApiError::ArchiveOnlyPast.into());
    }
    Ok(load(conn, day).await?.map(|d| d.puzzle))
}

pub async fn attempt(
    conn: &Connection,
    user: i64,
    day: i64,
) -> Result<Option<AttemptInfo>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT started_at, finished_at, hints, checks, revealed, time_ms, bridges, flag
             FROM attempts WHERE user_id = ?1 AND day = ?2",
            params![user, day],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    let bridges: Option<String> = row.get(6).map_err(db_err)?;
    Ok(Some(AttemptInfo {
        started_at: row.get(0).map_err(db_err)?,
        finished_at: row.get(1).map_err(db_err)?,
        hints: row.get(2).map_err(db_err)?,
        checks: row.get(3).map_err(db_err)?,
        revealed: row.get::<i64>(4).map_err(db_err)? != 0,
        time_ms: row.get(5).map_err(db_err)?,
        bridges: bridges.map(|b| from_json(&b)).transpose()?,
        flag: row
            .get::<Option<String>>(7)
            .map_err(db_err)?
            .and_then(|f| Flag::from_id(&f)),
    }))
}

pub async fn finishers(conn: &Connection, day: i64) -> Result<i64, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM attempts WHERE day = ?1 AND finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL",
            params![day],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err),
        None => Ok(0),
    }
}

/// Today's daily for the current user. The puzzle is only included once they started.
pub async fn info(
    conn: &Connection,
    user: Option<i64>,
    day: i64,
) -> Result<DailyInfo, ServerFnError> {
    let attempt = match user {
        Some(user) => attempt(conn, user, day).await?,
        None => None,
    };
    let puzzle = match &attempt {
        Some(_) => Some(get_or_create(conn, day).await?.puzzle),
        None => None,
    };
    Ok(DailyInfo {
        day,
        difficulty: daily_difficulty(day),
        puzzle,
        attempt,
        server_now: now_ms(),
        finishers: finishers(conn, day).await?,
    })
}

/// Starts the clock (once) and returns the puzzle.
///
/// `device` is a random id the browser keeps: if the same device already started this daily
/// with another account, the player has seen the puzzle, and this attempt isn't ranked.
pub async fn start(
    conn: &Connection,
    user: i64,
    day: i64,
    device: Option<&str>,
) -> Result<DailyInfo, ServerFnError> {
    get_or_create(conn, day).await?;
    // Stored hashed per day, so devices can't be followed from one day to the next
    let device = device
        .filter(|d| (8..=64).contains(&d.len()))
        .map(|d| super::hex(&Sha256::digest(format!("device:{day}:{d}").as_bytes())));
    let seen = if let Some(device) = &device {
        let mut rows = conn
            .query(
                "SELECT 1 FROM attempts WHERE day = ?1 AND device = ?2 AND user_id != ?3 LIMIT 1",
                params![day, device.clone(), user],
            )
            .await
            .map_err(db_err)?;
        rows.next().await.map_err(db_err)?.is_some()
    } else {
        false
    };
    conn.execute(
        "INSERT OR IGNORE INTO attempts (user_id, day, started_at, device, flag) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            user,
            day,
            now_ms(),
            device,
            seen.then(|| Flag::SecondAccount.id())
        ],
    )
    .await
    .map_err(db_err)?;
    info(conn, Some(user), day).await
}

/// The running attempt of a user, or an error if there is none.
async fn running(conn: &Connection, user: i64, day: i64) -> Result<AttemptInfo, ServerFnError> {
    match attempt(conn, user, day).await? {
        Some(a) if a.finished() => Err(ApiError::AlreadyFinished.into()),
        Some(a) => Ok(a),
        None => Err(ApiError::NotStarted.into()),
    }
}

fn check_bridges(board: &Board, bridges: &[u8]) -> Result<(), ServerFnError> {
    if bridges.len() != board.edges.len() || bridges.iter().any(|&b| b > 2) {
        return Err(ApiError::WrongPuzzle.into());
    }
    Ok(())
}

/// Fixes one bridge, costs a hint penalty.
pub async fn hint(
    conn: &Connection,
    user: i64,
    day: i64,
    bridges: Vec<u8>,
) -> Result<Option<(usize, u8)>, ServerFnError> {
    running(conn, user, day).await?;
    let daily = get_or_create(conn, day).await?;
    let board = Board::new(daily.puzzle);
    check_bridges(&board, &bridges)?;
    let hint = pick_hint(&board, &bridges, &daily.solution);
    if hint.is_some() {
        conn.execute(
            "UPDATE attempts SET hints = hints + 1 WHERE user_id = ?1 AND day = ?2",
            params![user, day],
        )
        .await
        .map_err(db_err)?;
    }
    Ok(hint)
}

/// Bridges that aren't part of the solution, costs a check penalty.
pub async fn check(
    conn: &Connection,
    user: i64,
    day: i64,
    bridges: Vec<u8>,
) -> Result<Vec<usize>, ServerFnError> {
    running(conn, user, day).await?;
    let daily = get_or_create(conn, day).await?;
    check_bridges(&Board::new(daily.puzzle), &bridges)?;
    conn.execute(
        "UPDATE attempts SET checks = checks + 1 WHERE user_id = ?1 AND day = ?2",
        params![user, day],
    )
    .await
    .map_err(db_err)?;
    Ok(wrong_edges(&bridges, &daily.solution))
}

/// Gives up: the solution, the attempt won't be ranked.
pub async fn reveal(conn: &Connection, user: i64, day: i64) -> Result<Vec<u8>, ServerFnError> {
    running(conn, user, day).await?;
    let daily = get_or_create(conn, day).await?;
    conn.execute(
        "UPDATE attempts SET revealed = 1, bridges = ?3 WHERE user_id = ?1 AND day = ?2",
        params![user, day, json(&daily.solution)?],
    )
    .await
    .map_err(db_err)?;
    Ok(daily.solution)
}

/// Rank among the ranked finishers: faster times first, earlier finishes break ties.
async fn rank(
    conn: &Connection,
    day: i64,
    time_ms: i64,
    finished_at: i64,
) -> Result<i64, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT COUNT(*) + 1 FROM attempts
             WHERE day = ?1 AND finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL
               AND (time_ms < ?2 OR (time_ms = ?2 AND finished_at < ?3))",
            params![day, time_ms, finished_at],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err),
        None => Ok(1),
    }
}

/// Verifies a finished board and records the time.
pub async fn submit(
    conn: &Connection,
    user: i64,
    day: i64,
    bridges: Vec<u8>,
    moves: u32,
    log: Vec<MoveEvent>,
) -> Result<SubmitResult, ServerFnError> {
    let attempt = running(conn, user, day).await?;
    let daily = get_or_create(conn, day).await?;
    let board = Board::new(daily.puzzle);
    check_bridges(&board, &bridges)?;
    if !board.check(&bridges).solved {
        return Err(ApiError::NotSolved.into());
    }
    let finished_at = now_ms();
    let elapsed = finished_at - attempt.started_at;
    let time_ms = elapsed + attempt.penalty_ms();
    // A flag from the start (second account) stays, otherwise the move log decides
    let flag = attempt.flag.or_else(|| {
        verify_log(
            &board,
            &daily.solution,
            &bridges,
            &log,
            elapsed,
            attempt.hints,
        )
        .err()
    });
    conn.execute(
        "UPDATE attempts SET finished_at = ?3, time_ms = ?4, moves = ?5, bridges = ?6, log = ?7, flag = ?8
         WHERE user_id = ?1 AND day = ?2 AND finished_at IS NULL AND revealed = 0",
        params![
            user,
            day,
            finished_at,
            time_ms,
            i64::from(moves),
            json(&bridges)?,
            json(&log)?,
            flag.map(Flag::id)
        ],
    )
    .await
    .map_err(db_err)?;
    Ok(SubmitResult {
        time_ms,
        rank: if flag.is_some() {
            0
        } else {
            rank(conn, day, time_ms, finished_at).await?
        },
        finishers: finishers(conn, day).await?,
        flag,
    })
}

/// Whether `puzzle` is today's daily (however it is turned) and `user` hasn't finished it yet:
/// our solvers don't help with it then.
pub async fn locked(
    conn: &Connection,
    user: Option<i64>,
    puzzle: &Puzzle,
) -> Result<bool, ServerFnError> {
    let day = today();
    let Some(daily) = load(conn, day).await? else {
        return Ok(false);
    };
    if canonical(puzzle) != canonical(&daily.puzzle) {
        return Ok(false);
    }
    let done = match user {
        Some(user) => attempt(conn, user, day)
            .await?
            .is_some_and(|a| a.finished() || a.revealed),
        None => false,
    };
    Ok(!done)
}

/// Takes a result off the board after a review (`flag` `None` puts it back).
pub async fn set_flag(
    conn: &Connection,
    user: i64,
    day: i64,
    flag: Option<Flag>,
) -> Result<bool, ServerFnError> {
    let changed = conn
        .execute(
            "UPDATE attempts SET flag = ?3 WHERE user_id = ?1 AND day = ?2",
            params![user, day, flag.map(Flag::id)],
        )
        .await
        .map_err(db_err)?;
    Ok(changed > 0)
}

fn entry(row: &libsql::Row, rank: i64) -> Result<LeaderboardEntry, libsql::Error> {
    Ok(LeaderboardEntry {
        rank,
        user_id: row.get(0)?,
        name: row.get(1)?,
        guest: row.get::<Option<String>>(2)?.is_none(),
        time_ms: row.get(3)?,
        hints: row.get(4)?,
        checks: row.get(5)?,
    })
}

const ENTRY_COLUMNS: &str =
    "a.user_id, u.name, u.username, a.time_ms, a.hints, a.checks, a.finished_at";

pub async fn leaderboard(
    conn: &Connection,
    day: i64,
    me: Option<i64>,
) -> Result<Leaderboard, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT {ENTRY_COLUMNS} FROM attempts a JOIN users u ON u.id = a.user_id
                 WHERE a.day = ?1 AND a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL
                 ORDER BY a.time_ms, a.finished_at LIMIT ?2"
            ),
            params![day, LEADERBOARD_SIZE],
        )
        .await
        .map_err(db_err)?;
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        entries.push(entry(&row, count(entries.len() + 1)).map_err(db_err)?);
    }

    let mut mine = entries.iter().find(|e| Some(e.user_id) == me).cloned();
    if let (None, Some(me)) = (&mine, me) {
        let mut rows = conn
            .query(
                &format!(
                    "SELECT {ENTRY_COLUMNS} FROM attempts a JOIN users u ON u.id = a.user_id
                     WHERE a.day = ?1 AND a.user_id = ?2 AND a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL"
                ),
                params![day, me],
            )
            .await
            .map_err(db_err)?;
        if let Some(row) = rows.next().await.map_err(db_err)? {
            let time: i64 = row.get(3).map_err(db_err)?;
            let finished: i64 = row.get(6).map_err(db_err)?;
            let r = rank(conn, day, time, finished).await?;
            mine = Some(entry(&row, r).map_err(db_err)?);
        }
    }

    Ok(Leaderboard {
        day,
        difficulty: daily_difficulty(day),
        entries,
        me: mine,
        finishers: finishers(conn, day).await?,
    })
}

async fn solved_days(conn: &Connection, user: i64) -> Result<Vec<i64>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT day FROM attempts WHERE user_id = ?1 AND finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL ORDER BY day",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut days = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        days.push(row.get(0).map_err(db_err)?);
    }
    Ok(days)
}

/// Players with the most solved dailies.
pub async fn all_time(conn: &Connection) -> Result<Vec<AllTimeEntry>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT a.user_id, u.name, u.username, COUNT(*) AS solved FROM attempts a
             JOIN users u ON u.id = a.user_id
             WHERE a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL
             GROUP BY a.user_id ORDER BY solved DESC, MIN(a.finished_at) LIMIT 20",
            (),
        )
        .await
        .map_err(db_err)?;
    let mut list = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        list.push(AllTimeEntry {
            rank: count(list.len() + 1),
            user_id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            guest: row.get::<Option<String>>(2).map_err(db_err)?.is_none(),
            solved: row.get(3).map_err(db_err)?,
            best_streak: 0,
        });
    }
    if list.is_empty() {
        return Ok(list);
    }
    // Solved days of all listed players in one query (ids are integers from the database)
    let ids: Vec<String> = list.iter().map(|e| e.user_id.to_string()).collect();
    let mut rows = conn
        .query(
            &format!(
                "SELECT user_id, day FROM attempts
                 WHERE user_id IN ({}) AND finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL
                 ORDER BY user_id, day",
                ids.join(",")
            ),
            (),
        )
        .await
        .map_err(db_err)?;
    let mut days: HashMap<i64, Vec<i64>> = HashMap::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let user: i64 = row.get(0).map_err(db_err)?;
        days.entry(user)
            .or_default()
            .push(row.get(1).map_err(db_err)?);
    }
    let today = today();
    for entry in &mut list {
        entry.best_streak = streaks(days.get(&entry.user_id).map_or(&[][..], |d| d), today).1;
    }
    Ok(list)
}

pub async fn stats(conn: &Connection, user: i64) -> Result<UserStats, ServerFnError> {
    let today = today();
    let days = solved_days(conn, user).await?;
    let (current_streak, best_streak) = streaks(&days, today);

    // (day, time, rank) of every ranked finish, ranks in the same query to save round trips
    let mut rows = conn
        .query(
            "SELECT a.day, a.time_ms, (
                SELECT COUNT(*) + 1 FROM attempts b
                WHERE b.day = a.day AND b.finished_at IS NOT NULL AND b.revealed = 0 AND b.flag IS NULL
                  AND (b.time_ms < a.time_ms OR (b.time_ms = a.time_ms AND b.finished_at < a.finished_at))
             ) FROM attempts a
             WHERE a.user_id = ?1 AND a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut finished = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let day: i64 = row.get(0).map_err(db_err)?;
        let time: i64 = row.get(1).map_err(db_err)?;
        let rank: i64 = row.get(2).map_err(db_err)?;
        finished.push((day, time, rank));
    }
    let best_time_ms = finished.iter().map(|f| f.1).min();
    let average_time_ms = (!finished.is_empty())
        .then(|| finished.iter().map(|f| f.1).sum::<i64>() / count(finished.len()));

    let recent = (today - RECENT_DAYS + 1..=today)
        .map(|day| {
            let found = finished.iter().find(|f| f.0 == day);
            DayResult {
                day,
                time_ms: found.map(|f| f.1),
                rank: found.map(|f| f.2),
            }
        })
        .collect();

    Ok(UserStats {
        solved: count(days.len()),
        current_streak,
        best_streak,
        best_time_ms,
        average_time_ms,
        recent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CHECK_PENALTY_MS, HINT_PENALTY_MS, LAUNCH_DAY};
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
}
