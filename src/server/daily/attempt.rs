//! A player's attempt at a daily: starting the clock, hints, checks, giving up and submitting.

use common::{Board, Puzzle};
use leptos::prelude::ServerFnError;
use libsql::{Connection, TransactionBehavior, params};

use super::stamps::sync;
use super::{from_json, get_or_create, json, load, today};
use crate::model::{
    ApiError, AttemptInfo, DailyInfo, Daypart, Flag, MoveEvent, SubmitResult, canonical,
    daily_difficulty, pick_hint, server_times, summarize, verify_log, wrong_edges,
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
            "SELECT started_at, finished_at, hints, checks, revealed, time_ms, bridges, flag, attempt_id
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
        attempt_id: row.get(8).map_err(db_err)?,
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
    let stamps = match user {
        Some(user) => sync(conn, user, true).await?.1,
        None => Vec::new(),
    };
    Ok(DailyInfo {
        day,
        difficulty: daily_difficulty(day),
        puzzle,
        attempt,
        server_now: now_ms(),
        finishers: finishers(conn, day).await?,
        stamps,
    })
}

/// Validate and read under one lock: a guest transfer or replacement cannot interleave.
pub async fn owned_info(
    conn: &Connection,
    user: i64,
    day: i64,
    expected: &str,
) -> Result<DailyInfo, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let current = attempt(&tx, user, day)
        .await?
        .ok_or_else(|| ApiError::SessionExpired.fail())?;
    if expected.is_empty() || current.attempt_id != expected {
        return Err(ApiError::SessionExpired.into());
    }
    let info = info(&tx, Some(user), day).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(info)
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
    // Stored hashed with the secret and the day, so devices can't be followed from one day to the
    // next, not even by someone who knows a device's id
    let device = device
        .filter(|d| (8..=64).contains(&d.len()))
        .map(|d| crate::server::keyed_hash("device", day, d));
    conn.execute(
        "INSERT OR IGNORE INTO attempts (user_id, day, started_at, device, flag)
         SELECT ?1, ?2, ?3, ?4, CASE WHEN ?4 IS NOT NULL AND EXISTS (
             SELECT 1 FROM attempts WHERE day = ?2 AND device = ?4 AND user_id != ?1
         ) THEN ?5 ELSE NULL END",
        params![user, day, now_ms(), device, Flag::SecondAccount.id()],
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
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    running(&tx, user, day).await?;
    let daily = get_or_create(&tx, day).await?;
    let board = Board::new(daily.puzzle);
    check_bridges(&board, &bridges)?;
    let hint = pick_hint(&board, &bridges, &daily.solution);
    if hint.is_some() {
        tx.execute(
            "UPDATE attempts SET hints = hints + 1 WHERE user_id = ?1 AND day = ?2",
            params![user, day],
        )
        .await
        .map_err(db_err)?;
    }
    tx.commit().await.map_err(db_err)?;
    Ok(hint)
}

/// Bridges that aren't part of the solution, costs a check penalty.
pub async fn check(
    conn: &Connection,
    user: i64,
    day: i64,
    bridges: Vec<u8>,
) -> Result<Vec<usize>, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    running(&tx, user, day).await?;
    let daily = get_or_create(&tx, day).await?;
    check_bridges(&Board::new(daily.puzzle), &bridges)?;
    tx.execute(
        "UPDATE attempts SET checks = checks + 1 WHERE user_id = ?1 AND day = ?2",
        params![user, day],
    )
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(wrong_edges(&bridges, &daily.solution))
}

/// A maximum of 256 distinct purchases per attempt. Receipts live with the attempt and are
/// never evicted, even after finishing: an arbitrarily late retry cannot charge again. This
/// bounds retained receipt count without an expiry window that could turn retries into charges.
/// Account erasure clears them along with bridges/logs and revokes every session.
const MAX_PAID_ACTIONS: usize = 256;

#[derive(serde::Serialize, serde::Deserialize)]
struct PaidReceipt {
    // Older receipts get their provenance from the actual owning guest inside merge_guest.
    #[serde(default)]
    origin_user: Option<i64>,
    id: String,
    action: crate::api::PaidAction,
    bridges: Vec<u8>,
    outcome: crate::api::PaidOutcome,
    /// When it was bought (server time), for the markers of the replay theatre (server/replay).
    /// Receipts from before it have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    at: Option<i64>,
}

/// Reads an owned receipt and its counters together, without creating an attempt or charge.
///
/// The current owner, exact original user, attempt, ID, action and board must all agree;
/// a coincidentally equal `started_at` on a conflicting attempt proves nothing.
pub async fn recover_actions(
    conn: &Connection,
    user: i64,
    day: i64,
    started_at: i64,
    intents: &[crate::api::PaidIntent],
) -> Result<Option<crate::api::PaidRecovery>, ServerFnError> {
    use crate::api::{PaidRecovery, PaidReply};
    if intents.len() > MAX_PAID_ACTIONS
        || intents.iter().any(|i| {
            !(16..=64).contains(&i.id.len())
                || i.bridges.len() > 2 * usize::from(common::MAX_SIZE).pow(2)
        })
    {
        return Err(ApiError::WrongPuzzle.into());
    }
    let mut rows = conn.query(
        "SELECT paid_actions, hints, checks, attempt_id FROM attempts WHERE user_id = ?1 AND day = ?2 AND started_at = ?3",
        params![user, day, started_at],
    ).await.map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    let receipts: Vec<PaidReceipt> = from_json(&row.get::<String>(0).map_err(db_err)?)?;
    for intent in intents {
        if intent.day != day
            || intent.started_at != started_at
            || (!intent.attempt_id.is_empty()
                && intent.attempt_id != row.get::<String>(3).map_err(db_err)?)
        {
            continue;
        }
        if let Some(receipt) = receipts.iter().find(|r| {
            r.origin_user == Some(intent.user)
                && r.id == intent.id
                && r.action == intent.action
                && r.bridges == intent.bridges
        }) {
            return Ok(Some(PaidRecovery {
                intent: intent.clone(),
                reply: PaidReply {
                    outcome: receipt.outcome.clone(),
                    hints: row.get(1).map_err(db_err)?,
                    checks: row.get(2).map_err(db_err)?,
                },
            }));
        }
    }
    Ok(None)
}

/// The receipt and the penalty are committed together under the same cross-connection lock as
/// submission. Identity comes exclusively from the session, never from the request body.
pub async fn paid_action(
    conn: &Connection,
    user: i64,
    day: i64,
    bridges: Vec<u8>,
    action: crate::api::PaidAction,
    request_id: &str,
    expected_attempt: &str,
) -> Result<crate::api::PaidReply, ServerFnError> {
    use crate::api::{PaidAction, PaidOutcome, PaidReply};

    if !(16..=64).contains(&request_id.len())
        || !request_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ApiError::WrongPuzzle.into());
    }
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let current = attempt(&tx, user, day)
        .await?
        .ok_or_else(|| ApiError::SessionExpired.fail())?;
    // Check before reading receipts or charging, while holding the write lock. A deleted
    // guest's numeric ID (and even its start millisecond) can belong to a replacement guest.
    // This error must retain the unresolved intent: its receipt may have moved to an account.
    if expected_attempt.is_empty() || current.attempt_id != expected_attempt {
        return Err(ApiError::SessionExpired.into());
    }
    let saved: String = tx
        .query(
            "SELECT paid_actions FROM attempts WHERE user_id = ?1 AND day = ?2",
            params![user, day],
        )
        .await
        .map_err(db_err)?
        .next()
        .await
        .map_err(db_err)?
        .ok_or_else(|| ApiError::NotStarted.fail())?
        .get(0)
        .map_err(db_err)?;
    let mut receipts: Vec<PaidReceipt> = from_json(&saved)?;
    // Replay before checking 'finished', so a lost reply can also be recovered after submission.
    if let Some(receipt) = receipts.iter().find(|r| r.id == request_id) {
        if receipt.action != action || receipt.bridges != bridges {
            return Err(ApiError::WrongPuzzle.into());
        }
        let reply = PaidReply {
            outcome: receipt.outcome.clone(),
            hints: current.hints,
            checks: current.checks,
        };
        tx.commit().await.map_err(db_err)?;
        return Ok(reply);
    }
    if current.finished() {
        return Err(ApiError::AlreadyFinished.into());
    }
    if receipts.len() >= MAX_PAID_ACTIONS {
        return Err(ApiError::TooManyAttempts.into());
    }
    let daily = get_or_create(&tx, day).await?;
    let board = Board::new(daily.puzzle);
    check_bridges(&board, &bridges)?;
    let outcome = match action {
        PaidAction::Hint => PaidOutcome::Hint(pick_hint(&board, &bridges, &daily.solution)),
        PaidAction::Check => PaidOutcome::Check(wrong_edges(&bridges, &daily.solution)),
    };
    let hints = current.hints + i64::from(matches!(&outcome, PaidOutcome::Hint(Some(_))));
    let checks = current.checks + i64::from(action == PaidAction::Check);
    receipts.push(PaidReceipt {
        origin_user: Some(user),
        id: request_id.to_owned(),
        action,
        bridges,
        outcome: outcome.clone(),
        at: Some(now_ms()),
    });
    tx.execute(
        "UPDATE attempts SET hints = ?3, checks = ?4, paid_actions = ?5 WHERE user_id = ?1 AND day = ?2",
        params![user, day, hints, checks, json(&receipts)?],
    ).await.map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(PaidReply {
        outcome,
        hints,
        checks,
    })
}

/// Gives up: the solution, the attempt won't be ranked.
pub async fn reveal(conn: &Connection, user: i64, day: i64) -> Result<Vec<u8>, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    running(&tx, user, day).await?;
    let daily = get_or_create(&tx, day).await?;
    tx.execute(
        "UPDATE attempts SET revealed = 1, bridges = ?3 WHERE user_id = ?1 AND day = ?2",
        params![user, day, json(&daily.solution)?],
    )
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(daily.solution)
}

/// Rank among eligible finishers under the policy of this day.
pub(super) async fn rank(
    conn: &Connection,
    day: i64,
    time_ms: i64,
    finished_at: i64,
) -> Result<i64, ServerFnError> {
    let (time_ms, finished_at) = crate::model::ranking_key(day, time_ms, finished_at);
    let mut rows = conn
        .query(
            "SELECT COUNT(*) + 1 FROM attempts
             WHERE day = ?1 AND finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL
               AND (rank_ms < ?2 OR (rank_ms = ?2 AND rank_finish < ?3))",
            params![day, time_ms, finished_at],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err),
        None => Ok(1),
    }
}

/// Verifies a finished board and records the time. `utc_offset` is the player's time zone
/// (minutes behind UTC, as the browser says), only the part of the day is kept, for stamps.
pub async fn submit(
    conn: &Connection,
    user: i64,
    day: i64,
    bridges: Vec<u8>,
    moves: u32,
    log: Vec<MoveEvent>,
    utc_offset: Option<i32>,
) -> Result<SubmitResult, ServerFnError> {
    submit_inner(
        conn,
        user,
        day,
        bridges,
        moves,
        log,
        utc_offset,
        None,
        now_ms(),
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "Legacy submission fields plus the stable attempt assertion"
)]
pub async fn submit_owned(
    conn: &Connection,
    user: i64,
    day: i64,
    bridges: Vec<u8>,
    moves: u32,
    log: Vec<MoveEvent>,
    utc_offset: Option<i32>,
    expected: &str,
) -> Result<SubmitResult, ServerFnError> {
    submit_inner(
        conn,
        user,
        day,
        bridges,
        moves,
        log,
        utc_offset,
        Some(expected),
        now_ms(),
    )
    .await
}

/// The receipt is captured by the decoded server function, never taken from request data.
#[expect(
    clippy::too_many_arguments,
    reason = "Submission payload and server-only receipt"
)]
pub async fn submit_received(
    conn: &Connection,
    user: i64,
    day: i64,
    bridges: Vec<u8>,
    moves: u32,
    log: Vec<MoveEvent>,
    utc_offset: Option<i32>,
    expected: Option<&str>,
    received_at: i64,
) -> Result<SubmitResult, ServerFnError> {
    submit_inner(
        conn,
        user,
        day,
        bridges,
        moves,
        log,
        utc_offset,
        expected,
        received_at,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "Shared legacy and guarded submission transaction"
)]
async fn submit_inner(
    conn: &Connection,
    user: i64,
    day: i64,
    bridges: Vec<u8>,
    moves: u32,
    log: Vec<MoveEvent>,
    utc_offset: Option<i32>,
    expected: Option<&str>,
    received_at: i64,
) -> Result<SubmitResult, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let attempt = attempt(&tx, user, day)
        .await?
        .ok_or_else(|| ApiError::NotStarted.fail())?;
    if expected.is_some_and(|id| id.is_empty() || id != attempt.attempt_id) {
        return Err(ApiError::SessionExpired.into());
    }
    if attempt.finished() {
        if expected.is_some() && !attempt.revealed && attempt.bridges.as_ref() == Some(&bridges) {
            let time_ms = attempt.time_ms.ok_or_else(|| ApiError::Internal.fail())?;
            let finished_at = attempt
                .finished_at
                .ok_or_else(|| ApiError::Internal.fail())?;
            tx.commit().await.map_err(db_err)?;
            return submission_result(conn, user, day, time_ms, finished_at, attempt.flag).await;
        }
        return Err(ApiError::AlreadyFinished.into());
    }
    let daily = get_or_create(&tx, day).await?;
    let board = Board::new(daily.puzzle);
    check_bridges(&board, &bridges)?;
    if !board.check(&bridges).solved {
        return Err(ApiError::NotSolved.into());
    }
    // Only the server can supply this timestamp, after the entire JSON body was decoded.
    // Tomorrow switches to receipt timing; existing days retain their measurement contract.
    let finished_at = if day >= crate::model::RECEIPT_TIMING_DAY {
        received_at
    } else {
        now_ms()
    };
    let elapsed = finished_at
        .checked_sub(attempt.started_at)
        .filter(|&ms| ms >= 0)
        .ok_or_else(|| ApiError::NotStarted.fail())?;
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
    // The stamps read the clicks on the server's clock too, the stored log stays as sent
    let run = summarize(&board.puzzle, &bridges, &server_times(&log, elapsed));
    let daypart = utc_offset.and_then(|offset| Daypart::at(finished_at, offset));
    let changed = tx.execute(
        "UPDATE attempts SET finished_at = ?3, time_ms = ?4, moves = ?5, bridges = ?6, log = ?7, flag = ?8,
            run = ?9, daypart = ?10
         WHERE user_id = ?1 AND day = ?2 AND finished_at IS NULL AND revealed = 0",
        params![
            user,
            day,
            finished_at,
            time_ms,
            i64::from(moves),
            json(&bridges)?,
            json(&log)?,
            flag.map(Flag::id),
            json(&run)?,
            daypart.map(Daypart::id)
        ],
    )
    .await
    .map_err(db_err)?;
    if changed != 1 {
        return Err(ApiError::AlreadyFinished.into());
    }
    tx.commit().await.map_err(db_err)?;
    super::stats::invalidate();
    submission_result(conn, user, day, time_ms, finished_at, flag).await
}

async fn submission_result(
    conn: &Connection,
    user: i64,
    day: i64,
    time_ms: i64,
    finished_at: i64,
    flag: Option<Flag>,
) -> Result<SubmitResult, ServerFnError> {
    Ok(SubmitResult {
        time_ms,
        rank: if flag.is_some() {
            0
        } else {
            rank(conn, day, time_ms, finished_at).await?
        },
        finishers: finishers(conn, day).await?,
        flag,
        stamps: sync(conn, user, true).await?.1,
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

/// Takes a result off the board after a review (`flag` `None` puts it back). `false` if there
/// is no such result, or it is a banned player's: only lifting the ban puts those back.
pub async fn set_flag(
    conn: &Connection,
    user: i64,
    day: i64,
    flag: Option<Flag>,
) -> Result<bool, ServerFnError> {
    // Bans set their own flag, and one this build doesn't know isn't one to set
    if matches!(flag, Some(Flag::Banned | Flag::Unknown)) {
        return Ok(false);
    }
    let changed = conn
        .execute(
            "UPDATE attempts SET flag = ?3 WHERE user_id = ?1 AND day = ?2
               AND (flag IS NULL OR flag != 'banned')",
            params![user, day, flag.map(Flag::id)],
        )
        .await
        .map_err(db_err)?;
    super::stats::invalidate();
    // A stamp the result earned goes with it
    sync(conn, user, false).await?;
    Ok(changed > 0)
}
