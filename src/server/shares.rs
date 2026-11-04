//! Shared puzzles (`/p/<code>`): a puzzle a player solved on Play, or built in the workshop, as
//! a short link for anyone.
//!
//! The puzzle is stored once ([`puzzle_hash`]), with the solution the SAT solver proved to be
//! the only one here, whatever the page said ([`share`]). Each player who shares it gets a link
//! of their own (`puzzle_shares`, a random [`SHARE_CODE_LEN`] code): it says who shared it, and
//! whoever solves it through that link counts for them. A player's result on a puzzle is their
//! first solve (`shared_results`, one per puzzle and player): solving it again, through any link,
//! changes nothing. The sharer's own solve on Play is their result, through their own link.
//!
//! Times are Play's clock, sent by the page, like the clock of Play itself: the server checks the
//! board and that the moves and the time are within reason (`model::check_result`), the board
//! ranks by the time with 30 s per hint ([`share_score`]). No free text anywhere, names follow
//! blocks (`blocks::hidden`), banned players and deleted accounts aren't on the boards.
//!
//! For the sharing stamps: [`players_reached`] counts the other players who solved a puzzle
//! through a player's links, from the index on `shared_results (share_id, user_id)`.

#[cfg(test)]
mod tests;

use common::{Board, Puzzle};
use leptos::prelude::ServerFnError;
use libsql::{Connection, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use super::db::db_err;
use super::{hex, run_blocking};
use crate::model::{
    ApiError, MAX_SHARES_PER_DAY, MS_PER_DAY, SHARE_ALPHABET, SHARE_BOARD_SHOWN, SHARE_CODE_LEN,
    SHARE_MIN_ISLANDS, SharedBy, SharedEntry, SharedPuzzlePage, SharedSolve, Stamp, canonical,
    check_result, is_share_code, share_score,
};

/// The SHA-256 of a puzzle's layout: its size and its islands in reading order. The same puzzle
/// (from the same seed, or built again) is stored once.
pub fn puzzle_hash(puzzle: &Puzzle) -> String {
    let puzzle = Puzzle::new(puzzle.width, puzzle.height, puzzle.islands.clone());
    let mut text = format!("{}x{}:", puzzle.width, puzzle.height);
    for island in &puzzle.islands {
        text.push_str(&format!(
            "{},{},{};",
            island.pos.x, island.pos.y, island.required
        ));
    }
    hex(&Sha256::digest(text.as_bytes()))
}

/// A new share code, [`SHARE_CODE_LEN`] characters of [`SHARE_ALPHABET`], every one as likely.
pub fn new_code() -> String {
    // The largest multiple of the alphabet's 31 characters a byte holds: the bytes above it
    // would make the first ones likelier
    const FAIR: u8 = 248;
    let mut code = String::with_capacity(SHARE_CODE_LEN);
    while code.len() < SHARE_CODE_LEN {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("no system randomness");
        for b in bytes.into_iter().filter(|&b| b < FAIR) {
            if code.len() < SHARE_CODE_LEN {
                code.push(char::from(
                    SHARE_ALPHABET[usize::from(b) % SHARE_ALPHABET.len()],
                ));
            }
        }
    }
    code
}

fn json<T: serde::Serialize>(value: &T) -> Result<String, ServerFnError> {
    serde_json::to_string(value).map_err(ServerFnError::new)
}

fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, ServerFnError> {
    serde_json::from_str(s).map_err(ServerFnError::new)
}

/// A puzzle as stored, in reading order: stored JSON skips `Puzzle::new`.
fn stored_puzzle(json: &str) -> Result<Puzzle, ServerFnError> {
    let puzzle: Puzzle = from_json(json)?;
    Ok(Puzzle::new(puzzle.width, puzzle.height, puzzle.islands))
}

/// Whether `puzzle` (however it is turned) is a group puzzle that is still open, in any group: a
/// link would let its players study it before their clock runs.
async fn open_group_puzzle(
    conn: &Connection,
    puzzle: &Puzzle,
    now: i64,
) -> Result<bool, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT puzzle FROM group_puzzles WHERE closes_at > ?1",
            params![now],
        )
        .await
        .map_err(db_err)?;
    let wanted = canonical(puzzle);
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let open: Puzzle = from_json(&row.get::<String>(0).map_err(db_err)?)?;
        if canonical(&open) == wanted {
            return Ok(true);
        }
    }
    Ok(false)
}

/// What [`share`] knows before it writes.
struct Known {
    /// Links the player made in the last day.
    today: i64,
    /// The solution, if the puzzle is stored already.
    solution: Option<Vec<u8>>,
    /// The player's link to it, if they have one.
    code: Option<String>,
}

async fn known(
    conn: &Connection,
    hash: &str,
    user: i64,
    now: i64,
) -> Result<Known, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT (SELECT COUNT(*) FROM puzzle_shares WHERE user_id = ?2 AND created_at > ?3),
                    p.solution,
                    (SELECT code FROM puzzle_shares s WHERE s.puzzle_id = p.id AND s.user_id = ?2
                     ORDER BY s.id LIMIT 1)
             FROM (SELECT 1) LEFT JOIN shared_puzzles p ON p.hash = ?1",
            params![hash, user, now - MS_PER_DAY],
        )
        .await
        .map_err(db_err)?;
    let row = rows
        .next()
        .await
        .map_err(db_err)?
        .ok_or_else(|| ApiError::Database.fail())?;
    let solution: Option<String> = row.get(1).map_err(db_err)?;
    Ok(Known {
        today: row.get(0).map_err(db_err)?,
        solution: solution.map(|s| from_json(&s)).transpose()?,
        code: row.get(2).map_err(db_err)?,
    })
}

/// A result the server stands behind: checked against the solution ([`check_result`] for a
/// solve from Play), or the server's own (a race it timed). Play's clock, in ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verified {
    pub time_ms: i64,
    pub moves: i64,
    pub hints: i64,
}

impl Verified {
    /// The numbers of a solve [`check_result`] accepted.
    pub fn of(solve: &SharedSolve) -> Self {
        Self {
            time_ms: solve.time_ms,
            moves: i64::from(solve.moves),
            hints: i64::from(solve.hints),
        }
    }
}

/// Stores the result of `user` on the puzzle of the link `code`, through that link, unless they
/// have one: only the first counts. Whether it was stored.
///
/// For results the server checked itself (see [`Verified`]): after a race, every racer's time
/// goes on the board of the puzzle a racer shared with [`create`].
pub async fn add_result(
    conn: &Connection,
    code: &str,
    user: i64,
    result: Verified,
    now: i64,
) -> Result<bool, ServerFnError> {
    let changed = conn
        .execute(
            "INSERT OR IGNORE INTO shared_results
                 (puzzle_id, user_id, share_id, time_ms, moves, hints, score_ms, finished_at)
             SELECT puzzle_id, ?2, id, ?3, ?4, ?5, ?6, ?7 FROM puzzle_shares WHERE code = ?1",
            params![
                code,
                user,
                result.time_ms,
                result.moves,
                result.hints,
                share_score(result.time_ms, result.hints),
                now
            ],
        )
        .await
        .map_err(db_err)?;
    Ok(changed > 0)
}

/// Shares a puzzle solved on Play as `user`, returns the code of their link to it (the one they
/// have, if they shared it before).
///
/// `solve`: how they solved it, checked against the solution ([`check_result`]) and their result
/// unless they have one. `None`: shared from the workshop, without a result. See [`create`] for
/// the rest.
pub async fn share(
    conn: &Connection,
    user: i64,
    puzzle: Puzzle,
    solve: Option<SharedSolve>,
    now: i64,
) -> Result<String, ServerFnError> {
    let check = |board: &Board, solution: &[u8]| match &solve {
        Some(solve) => check_result(board, solution, solve).map_err(ServerFnError::from),
        None => Ok(()),
    };
    let (code, _) = create_checked(conn, user, puzzle, now, check).await?;
    if let Some(solve) = &solve {
        add_result(conn, &code, user, Verified::of(solve), now).await?;
    }
    Ok(code)
}

/// A link of `user` to `puzzle`: the code of the one they have, or of a new one.
///
/// The server checks the puzzle: valid, at least two islands, not today's daily nor an open group
/// puzzle, exactly one solution (the SAT solver proves it the first time the puzzle is shared, it
/// is stored with that solution), and at most [`MAX_SHARES_PER_DAY`] new links of a player a
/// day. Results go on its board with [`add_result`].
///
/// The building block of sharing: the page's [`share`], and whatever else shares a puzzle with
/// times the server knows (a race).
pub async fn create(
    conn: &Connection,
    user: i64,
    puzzle: Puzzle,
    now: i64,
) -> Result<String, ServerFnError> {
    Ok(create_checked(conn, user, puzzle, now, |_, _| Ok(()))
        .await?
        .0)
}

/// [`create`], with `check` run on the board and its solution before anything is written. Also
/// returns the solution.
async fn create_checked(
    conn: &Connection,
    user: i64,
    puzzle: Puzzle,
    now: i64,
    check: impl Fn(&Board, &[u8]) -> Result<(), ServerFnError>,
) -> Result<(String, Vec<u8>), ServerFnError> {
    let puzzle = Puzzle::new(puzzle.width, puzzle.height, puzzle.islands);
    backend::api::validate(&puzzle).map_err(|_| ApiError::WrongPuzzle.fail())?;
    if puzzle.islands.len() < SHARE_MIN_ISLANDS {
        return Err(ApiError::WrongPuzzle.into());
    }
    if super::daily::locked(conn, None, &puzzle).await? {
        return Err(ApiError::DailyLocked.into());
    }
    if open_group_puzzle(conn, &puzzle, now).await? {
        return Err(ApiError::PuzzleLocked.into());
    }
    let hash = puzzle_hash(&puzzle);
    let known = known(conn, &hash, user, now).await?;
    let board = Board::new(puzzle.clone());
    // Shared before: its solution is proven
    if let (Some(code), Some(solution)) = (known.code, &known.solution) {
        check(&board, solution)?;
        return Ok((code, solution.clone()));
    }
    if known.today >= MAX_SHARES_PER_DAY {
        return Err(ApiError::TooManyAttempts.into());
    }
    let solution = match known.solution {
        Some(solution) => solution,
        None => {
            let solving = puzzle.clone();
            run_blocking(move || Ok(super::groups::kadai::unique_solution(&solving))).await??
        }
    };
    check(&board, &solution)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    tx.execute(
        "INSERT OR IGNORE INTO shared_puzzles (hash, puzzle, solution, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![hash.clone(), json(&puzzle)?, json(&solution)?, now],
    )
    .await
    .map_err(db_err)?;
    // Made meanwhile by another request of theirs?
    let code = match known_code(&tx, &hash, user).await? {
        Some(code) => code,
        None => insert_share(&tx, &hash, user, now).await?,
    };
    tx.commit().await.map_err(db_err)?;
    Ok((code, solution))
}

/// The code of the link of `user` to the puzzle with `hash`, if they have one.
async fn known_code(
    conn: &Connection,
    hash: &str,
    user: i64,
) -> Result<Option<String>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT s.code FROM puzzle_shares s JOIN shared_puzzles p ON p.id = s.puzzle_id
             WHERE p.hash = ?1 AND s.user_id = ?2 ORDER BY s.id LIMIT 1",
            params![hash, user],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => Ok(Some(row.get(0).map_err(db_err)?)),
        None => Ok(None),
    }
}

/// A new link of `user` to the stored puzzle with `hash`, with a code no other link has.
async fn insert_share(
    conn: &Connection,
    hash: &str,
    user: i64,
    now: i64,
) -> Result<String, ServerFnError> {
    // Two codes alike are rare (40 bits), three in a row never happen
    for _ in 0..3 {
        let code = new_code();
        let added = conn
            .execute(
                "INSERT OR IGNORE INTO puzzle_shares (code, puzzle_id, user_id, created_at)
                 SELECT ?1, id, ?2, ?3 FROM shared_puzzles WHERE hash = ?4",
                params![code.clone(), user, now, hash],
            )
            .await
            .map_err(db_err)?;
        if added > 0 {
            return Ok(code);
        }
    }
    Err(ApiError::Internal.into())
}

/// A link as stored, with its puzzle and who shared it.
struct Link {
    puzzle_id: i64,
    puzzle: Puzzle,
    solution: Vec<u8>,
    created_at: i64,
    /// `None`: deleted, or banned.
    sharer: Option<SharedBy>,
}

async fn link(conn: &Connection, code: &str) -> Result<Option<Link>, ServerFnError> {
    if !is_share_code(code) {
        return Ok(None);
    }
    let mut rows = conn
        .query(
            "SELECT s.puzzle_id, s.created_at, p.puzzle, p.solution, s.user_id, u.name,
                    u.username IS NULL, st.stamp
             FROM puzzle_shares s JOIN shared_puzzles p ON p.id = s.puzzle_id
             LEFT JOIN users u ON u.id = s.user_id AND u.deleted_at IS NULL AND u.banned_at IS NULL
             LEFT JOIN stamps st ON st.user_id = u.id AND st.stamp = u.stamp
             WHERE s.code = ?1",
            params![code],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    let name: Option<String> = row.get(5).map_err(db_err)?;
    let sharer_id: Option<i64> = row.get(4).map_err(db_err)?;
    let stamp: Option<String> = row.get(7).map_err(db_err)?;
    Ok(Some(Link {
        puzzle_id: row.get(0).map_err(db_err)?,
        created_at: row.get(1).map_err(db_err)?,
        puzzle: stored_puzzle(&row.get::<String>(2).map_err(db_err)?)?,
        solution: from_json(&row.get::<String>(3).map_err(db_err)?)?,
        sharer: sharer_id.zip(name).map(|(user_id, name)| SharedBy {
            user_id,
            name,
            guest: row.get::<i64>(6).unwrap_or(0) != 0,
            stamp: stamp.as_deref().and_then(Stamp::from_id),
            result: None,
        }),
    }))
}

/// The board of a puzzle: the best [`SHARE_BOARD_SHOWN`] places, and the results of `viewer` and
/// the sharer wherever they are. With everyone who solved it.
async fn board(
    conn: &Connection,
    puzzle: i64,
    viewer: Option<i64>,
    sharer: Option<i64>,
) -> Result<(Vec<SharedEntry>, i64), ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT user_id, name, guest, stamp, time_ms, moves, hints, score_ms, place, total FROM (
                 SELECT r.user_id, u.name, u.username IS NULL AS guest, st.stamp, r.time_ms,
                        r.moves, r.hints, r.score_ms, r.finished_at,
                        RANK() OVER (ORDER BY r.score_ms) AS place, COUNT(*) OVER () AS total
                 FROM shared_results r
                 JOIN users u ON u.id = r.user_id AND u.deleted_at IS NULL AND u.banned_at IS NULL
                 LEFT JOIN stamps st ON st.user_id = u.id AND st.stamp = u.stamp
                 WHERE r.puzzle_id = ?1
             )
             WHERE place <= ?2 OR user_id = ?3 OR user_id = ?4
             ORDER BY place, finished_at, user_id",
            params![
                puzzle,
                SHARE_BOARD_SHOWN,
                viewer.unwrap_or(-1),
                sharer.unwrap_or(-1)
            ],
        )
        .await
        .map_err(db_err)?;
    let mut entries = Vec::new();
    let mut total = 0;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let stamp: Option<String> = row.get(3).map_err(db_err)?;
        entries.push(SharedEntry {
            user_id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            guest: row.get::<i64>(2).map_err(db_err)? != 0,
            stamp: stamp.as_deref().and_then(Stamp::from_id),
            time_ms: row.get(4).map_err(db_err)?,
            moves: row.get(5).map_err(db_err)?,
            hints: row.get(6).map_err(db_err)?,
            score_ms: row.get(7).map_err(db_err)?,
            rank: row.get(8).map_err(db_err)?,
        });
        total = row.get(9).map_err(db_err)?;
    }
    Ok((entries, total))
}

/// The page of the link `code` as `viewer` sees it, `None` for a code that isn't one.
pub async fn page(
    conn: &Connection,
    code: &str,
    viewer: Option<i64>,
) -> Result<Option<SharedPuzzlePage>, ServerFnError> {
    let Some(link) = link(conn, code).await? else {
        return Ok(None);
    };
    Ok(Some(page_of(conn, code, link, viewer).await?))
}

async fn page_of(
    conn: &Connection,
    code: &str,
    link: Link,
    viewer: Option<i64>,
) -> Result<SharedPuzzlePage, ServerFnError> {
    let shown_sharer = link.sharer.as_ref().map(|s| s.user_id);
    let (entries, solvers) = board(conn, link.puzzle_id, viewer, shown_sharer).await?;
    let find = |user: Option<i64>| {
        user.and_then(|user| entries.iter().find(|e| e.user_id == user).cloned())
    };
    let mine = find(viewer);
    let sharer = link.sharer.map(|mut sharer| {
        sharer.result = find(Some(sharer.user_id));
        sharer
    });
    let in_top = |e: &SharedEntry| e.rank <= SHARE_BOARD_SHOWN;
    Ok(SharedPuzzlePage {
        code: code.to_string(),
        puzzle: link.puzzle,
        sharer,
        board: entries.iter().filter(|e| in_top(e)).cloned().collect(),
        mine,
        solvers,
        created_at: link.created_at,
    })
}

/// A solve of `user` through the link `code`: the page again, and whether it counted (only the
/// first solve of a puzzle does).
pub async fn submit(
    conn: &Connection,
    code: &str,
    user: i64,
    solve: &SharedSolve,
    now: i64,
) -> Result<(SharedPuzzlePage, bool), ServerFnError> {
    let link = link(conn, code)
        .await?
        .ok_or_else(|| ApiError::WrongPuzzle.fail())?;
    check_result(&Board::new(link.puzzle.clone()), &link.solution, solve)?;
    let counted = add_result(conn, code, user, Verified::of(solve), now).await?;
    let page = page_of(conn, code, link, Some(user)).await?;
    Ok((page, counted))
}

/// The other players who solved a puzzle through one of `user`'s links: how many there are
/// (each counted once, however many of the links they solved). For the sharing stamps: "someone
/// else solved a puzzle you shared" is `players_reached(..) >= 1`. Banned players don't count,
/// deleted ones have no results any more.
pub async fn players_reached(conn: &Connection, user: i64) -> Result<i64, ServerFnError> {
    let mut rows = conn
        .query(PLAYERS_REACHED, params![user])
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err),
        None => Ok(0),
    }
}

/// [`players_reached`]'s query, `?1` the sharer. Walks the sharer's links
/// (`puzzle_shares_user`) and their results (`shared_results_share`).
pub const PLAYERS_REACHED: &str = "SELECT COUNT(DISTINCT r.user_id) FROM puzzle_shares s
     JOIN shared_results r ON r.share_id = s.id AND r.user_id != s.user_id
     JOIN users u ON u.id = r.user_id AND u.banned_at IS NULL
     WHERE s.user_id = ?1";

/// What deleting a user does to shared puzzles, for `auth::erase` (`?1` the user): their results
/// go, their links stay without their name (people may have sent them on), so does the puzzle.
pub const ERASE: [&str; 2] = [
    "DELETE FROM shared_results WHERE user_id = ?1",
    "UPDATE puzzle_shares SET user_id = NULL WHERE user_id = ?1",
];

/// What signing in does to the shared puzzles of the guest who played in this browser before,
/// for `auth::merge_guest` (`?1` the account, `?2` the guest): the account takes their links and
/// their results. Where both solved the same puzzle, the earlier result stays, the first counts.
pub const MERGE_GUEST: [&str; 4] = [
    "DELETE FROM shared_results WHERE user_id = ?1 AND EXISTS (
        SELECT 1 FROM shared_results g WHERE g.user_id = ?2
          AND g.puzzle_id = shared_results.puzzle_id AND g.finished_at < shared_results.finished_at)",
    "UPDATE OR IGNORE shared_results SET user_id = ?1 WHERE user_id = ?2",
    "DELETE FROM shared_results WHERE user_id = ?2",
    "UPDATE puzzle_shares SET user_id = ?1 WHERE user_id = ?2",
];

/// What the preview of a link (`server::og`, the page's meta tags) shows: the puzzle, who shared
/// it, and their score, if they're still there.
pub async fn preview(
    conn: &Connection,
    code: &str,
) -> Result<Option<SharedPuzzlePage>, ServerFnError> {
    page(conn, code, None).await
}
