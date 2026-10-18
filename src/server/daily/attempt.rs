//! A player's attempt at a daily: starting the clock, hints, checks, giving up and submitting.

use common::{Board, Puzzle};
use leptos::prelude::ServerFnError;
use libsql::{Connection, params};
use sha2::{Digest, Sha256};

use super::{from_json, get_or_create, json, load, today};
use crate::model::{
    ApiError, AttemptInfo, DailyInfo, Flag, MoveEvent, SubmitResult, canonical, daily_difficulty,
    pick_hint, verify_log, wrong_edges,
};
use crate::server::db::db_err;
use crate::server::now_ms;

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
        .map(|d| crate::server::hex(&Sha256::digest(format!("device:{day}:{d}").as_bytes())));
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
pub(super) async fn rank(
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
