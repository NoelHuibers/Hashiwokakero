//! Group puzzles (課題 kadai): a member's own puzzle, posted to their group for a week.
//!
//! Posting proves the puzzle unique with the SAT solver here, whatever the workshop said
//! ([`post`]). The others then play it like a daily: the clock runs on the server from the start,
//! hints and checks cost the daily's penalties, and the move log is checked the same way
//! (`model::verify_log`), so a flagged result stays off the group's board. The puzzle is only
//! sent once the clock runs. The author can't play it, they see everyone's times and where the
//! help went instead ([`PuzzleStats`]). A week after posting it closes: everyone sees the final
//! board and the solution, and the author gets the applause.
//!
//! Everything goes through `groups::require_member`, a puzzle of a group the player isn't in is
//! like one that doesn't exist. Members who left (or are banned) drop off its board, like on the
//! daily's.

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use common::{Board, Puzzle, SolveOutcome, SolverConfig};
use leptos::prelude::ServerFnError;
use libsql::{Connection, TransactionBehavior, params, params::IntoParams};

use super::{Group, members, require_member};
use crate::model::{
    ARCHIVE_SHOWN, ApiError, AttemptInfo, FinalBoard, Flag, GroupMember, GroupPuzzles, Help, Kudos,
    MAX_OPEN, MarkKind, MoveEvent, MyPuzzle, Pip, PuzzleAuthor, PuzzlePage, PuzzleStats,
    PuzzleSummary, TodayEntry, TodayState, canonical, check_size, clean_title, closes_at, day_of,
    hardest, median, pick_hint, spoiler_safe, verify_log, wrong_edges,
};
use crate::model::{Lane, LaneOutcome, Mark, ReplayData, ReplayGroup, clean_log, group_day};
use crate::server::db::db_err;
use crate::server::{count, names, run_blocking};

/// How long the solver may take to prove a posted puzzle unique.
const SOLVER_MS: u32 = 10_000;

/// A group puzzle as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    pub id: i64,
    pub group_id: i64,
    /// `None`: the author deleted their account.
    pub author_id: Option<i64>,
    pub title: Option<String>,
    pub puzzle: Puzzle,
    pub solution: Vec<u8>,
    pub created_at: i64,
    pub closes_at: i64,
    pub kudos_seen: bool,
}

impl Stored {
    pub fn open(&self, now: i64) -> bool {
        now < self.closes_at
    }

    fn author(&self, user: i64) -> bool {
        self.author_id == Some(user)
    }
}

const COLUMNS: &str =
    "id, group_id, author_id, title, puzzle, solution, created_at, closes_at, kudos_seen";

fn json<T: serde::Serialize>(value: &T) -> Result<String, ServerFnError> {
    serde_json::to_string(value).map_err(ServerFnError::new)
}

fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, ServerFnError> {
    serde_json::from_str(s).map_err(ServerFnError::new)
}

fn row_to_stored(row: &libsql::Row) -> Result<Stored, ServerFnError> {
    let puzzle: Puzzle = from_json(&row.get::<String>(4).map_err(db_err)?)?;
    Ok(Stored {
        id: row.get(0).map_err(db_err)?,
        group_id: row.get(1).map_err(db_err)?,
        author_id: row.get(2).map_err(db_err)?,
        title: row.get(3).map_err(db_err)?,
        // Stored JSON skips Puzzle::new, keep the islands in reading order
        puzzle: Puzzle::new(puzzle.width, puzzle.height, puzzle.islands),
        solution: from_json(&row.get::<String>(5).map_err(db_err)?)?,
        created_at: row.get(6).map_err(db_err)?,
        closes_at: row.get(7).map_err(db_err)?,
        kudos_seen: row.get::<i64>(8).map_err(db_err)? != 0,
    })
}

async fn stored_where(
    conn: &Connection,
    filter: &str,
    values: impl IntoParams,
) -> Result<Vec<Stored>, ServerFnError> {
    let mut rows = conn
        .query(
            &format!("SELECT {COLUMNS} FROM group_puzzles WHERE {filter}"),
            values,
        )
        .await
        .map_err(db_err)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        out.push(row_to_stored(&row)?);
    }
    Ok(out)
}

pub async fn load(conn: &Connection, id: i64) -> Result<Option<Stored>, ServerFnError> {
    Ok(stored_where(conn, "id = ?1", params![id]).await?.pop())
}

/// The puzzle and its group, if `user` is a member of the group. A puzzle that isn't there (any
/// more) is [`ApiError::PuzzleClosed`], one of a group the player isn't in is
/// [`ApiError::GroupNotFound`], like the group itself.
async fn require(conn: &Connection, id: i64, user: i64) -> Result<(Group, Stored), ServerFnError> {
    let stored = load(conn, id)
        .await?
        .ok_or_else(|| ApiError::PuzzleClosed.fail())?;
    let group = require_member(conn, stored.group_id, user).await?;
    Ok((group, stored))
}

/// Puzzles of `author` in a group still open at `now`.
async fn open_count(
    conn: &Connection,
    group: i64,
    author: i64,
    now: i64,
) -> Result<usize, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM group_puzzles WHERE group_id = ?1 AND author_id = ?2 AND closes_at > ?3",
            params![group, author, now],
        )
        .await
        .map_err(db_err)?;
    let n: i64 = match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err)?,
        None => 0,
    };
    Ok(usize::try_from(n).unwrap_or(usize::MAX))
}

/// The solution of a puzzle if it is the only one, or why it can't be posted.
pub fn unique_solution(puzzle: &Puzzle) -> Result<Vec<u8>, ApiError> {
    let config = SolverConfig {
        timeout_ms: SOLVER_MS,
        ..SolverConfig::default()
    };
    let report = backend::api::solve_unique(puzzle, &config).map_err(|_| ApiError::WrongPuzzle)?;
    match report.outcome {
        SolveOutcome::Solved {
            bridges,
            unique: Some(true),
        } => Ok(bridges),
        // Another solution, or no proof within the time: the same for the players
        SolveOutcome::Solved { .. } | SolveOutcome::Timeout => Err(ApiError::PuzzleNotUnique),
        SolveOutcome::Unsatisfiable => Err(ApiError::PuzzleUnsolvable),
    }
}

/// Posts a puzzle to a group, returns its id.
///
/// The server checks it all again: the size, the title and, with the SAT solver, that it has
/// exactly one solution. Today's daily can't be posted: its hints would help with it.
pub async fn post(
    conn: &Connection,
    group: i64,
    user: i64,
    puzzle: Puzzle,
    title: &str,
    now: i64,
) -> Result<i64, ServerFnError> {
    let title = clean_title(title)?;
    if title.as_deref().and_then(names::blocked).is_some() {
        return Err(ApiError::GroupNameNotAllowed.into());
    }
    let puzzle = Puzzle::new(puzzle.width, puzzle.height, puzzle.islands);
    backend::api::validate(&puzzle).map_err(|_| ApiError::WrongPuzzle.fail())?;
    check_size(&puzzle)?;
    require_member(conn, group, user).await?;
    // Before the solver's work, and again below with the lock
    if open_count(conn, group, user, now).await? >= MAX_OPEN {
        return Err(ApiError::TooManyPuzzles.into());
    }
    if crate::server::daily::locked(conn, None, &puzzle).await? {
        return Err(ApiError::DailyLocked.into());
    }
    let solving = puzzle.clone();
    let solution = run_blocking(move || Ok(unique_solution(&solving))).await??;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    require_member(&tx, group, user).await?;
    if open_count(&tx, group, user, now).await? >= MAX_OPEN {
        return Err(ApiError::TooManyPuzzles.into());
    }
    tx.execute(
        "INSERT INTO group_puzzles (group_id, author_id, title, puzzle, solution, created_at, closes_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            group,
            user,
            title,
            json(&puzzle)?,
            json(&solution)?,
            now,
            closes_at(now)
        ],
    )
    .await
    .map_err(db_err)?;
    let id = tx.last_insert_rowid();
    tx.commit().await.map_err(db_err)?;
    Ok(id)
}

/// What the attempts table says about a player of a puzzle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub puzzle_id: i64,
    pub user_id: i64,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub revealed: bool,
    pub flag: Option<Flag>,
    pub time_ms: Option<i64>,
    pub hints: i64,
    pub checks: i64,
    pub bridges: Option<Vec<u8>>,
    pub helps: Vec<Help>,
}

impl Row {
    fn solved(&self) -> bool {
        self.finished_at.is_some() && !self.revealed
    }

    fn attempt(&self) -> AttemptInfo {
        AttemptInfo {
            attempt_id: String::new(),
            started_at: self.started_at,
            finished_at: self.finished_at,
            hints: self.hints,
            checks: self.checks,
            revealed: self.revealed,
            time_ms: self.time_ms,
            bridges: self.bridges.clone(),
            flag: self.flag,
        }
    }
}

const ROW_COLUMNS: &str = "puzzle_id, user_id, started_at, finished_at, revealed, flag, time_ms, hints, checks, bridges, helps";

fn row_to_attempt(row: &libsql::Row) -> Result<Row, ServerFnError> {
    let bridges: Option<String> = row.get(9).map_err(db_err)?;
    Ok(Row {
        puzzle_id: row.get(0).map_err(db_err)?,
        user_id: row.get(1).map_err(db_err)?,
        started_at: row.get(2).map_err(db_err)?,
        finished_at: row.get(3).map_err(db_err)?,
        revealed: row.get::<i64>(4).map_err(db_err)? != 0,
        flag: row
            .get::<Option<String>>(5)
            .map_err(db_err)?
            .and_then(|f| Flag::from_id(&f)),
        time_ms: row.get(6).map_err(db_err)?,
        hints: row.get(7).map_err(db_err)?,
        checks: row.get(8).map_err(db_err)?,
        bridges: bridges.map(|b| from_json(&b)).transpose()?,
        // Helps that don't parse are no help to the author's view, the result stays
        helps: serde_json::from_str(&row.get::<String>(10).map_err(db_err)?).unwrap_or_default(),
    })
}

/// The attempts at some puzzles.
async fn attempts(conn: &Connection, puzzles: &[i64]) -> Result<Vec<Row>, ServerFnError> {
    if puzzles.is_empty() {
        return Ok(Vec::new());
    }
    // Ids are integers from the database
    let ids: Vec<String> = puzzles.iter().map(ToString::to_string).collect();
    let mut rows = conn
        .query(
            &format!(
                "SELECT {ROW_COLUMNS} FROM group_puzzle_attempts WHERE puzzle_id IN ({})",
                ids.join(",")
            ),
            (),
        )
        .await
        .map_err(db_err)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        out.push(row_to_attempt(&row)?);
    }
    Ok(out)
}

async fn attempt(conn: &Connection, puzzle: i64, user: i64) -> Result<Option<Row>, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT {ROW_COLUMNS} FROM group_puzzle_attempts WHERE puzzle_id = ?1 AND user_id = ?2"
            ),
            params![puzzle, user],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => Ok(Some(row_to_attempt(&row)?)),
        None => Ok(None),
    }
}

/// The board of a puzzle before [`spoiler_safe`]: a line per member but the author, ranked like
/// the daily's (solved, not given up, not flagged; by time, then who was first).
pub fn board_entries(
    members: &[GroupMember],
    author: Option<i64>,
    rows: &[&Row],
) -> Vec<TodayEntry> {
    let by_user: HashMap<i64, &Row> = rows.iter().map(|r| (r.user_id, *r)).collect();
    let mut ranked: Vec<(i64, i64, i64)> = rows
        .iter()
        .filter(|r| r.solved() && r.flag.is_none() && Some(r.user_id) != author)
        .filter(|r| members.iter().any(|m| m.user_id == r.user_id))
        .filter_map(|r| Some((r.time_ms?, r.finished_at?, r.user_id)))
        .collect();
    ranked.sort_unstable();
    let places: HashMap<i64, i64> = ranked
        .iter()
        .enumerate()
        .map(|(i, &(_, _, user))| (user, count(i + 1)))
        .collect();
    members
        .iter()
        .filter(|m| Some(m.user_id) != author)
        .map(|m| TodayEntry {
            user_id: m.user_id,
            name: m.name.clone(),
            guest: m.guest,
            colour: m.colour,
            stamp: m.stamp,
            state: match by_user.get(&m.user_id) {
                None => TodayState::Waiting,
                Some(r) if r.finished_at.is_none() && !r.revealed => TodayState::Waiting,
                Some(r) => match (places.get(&m.user_id), r.time_ms) {
                    (Some(&rank), Some(time_ms)) => TodayState::Ranked {
                        rank,
                        time_ms,
                        hints: r.hints,
                        checks: r.checks,
                    },
                    _ => TodayState::Unranked,
                },
            },
        })
        .collect()
}

/// Where `viewer` stands with a puzzle, from its board (before [`spoiler_safe`]).
pub fn my_puzzle(
    viewer: i64,
    author: Option<i64>,
    row: Option<&Row>,
    board: &[TodayEntry],
) -> MyPuzzle {
    if author == Some(viewer) {
        return MyPuzzle::Author;
    }
    match row {
        None => MyPuzzle::Fresh,
        Some(r) if r.revealed => MyPuzzle::GaveUp,
        Some(r) => match (r.finished_at, r.time_ms) {
            (Some(_), Some(time_ms)) => MyPuzzle::Solved {
                time_ms,
                rank: board
                    .iter()
                    .find(|e| e.user_id == viewer)
                    .and_then(|e| match e.state {
                        TodayState::Ranked { rank, .. } => Some(rank),
                        _ => None,
                    }),
            },
            _ => MyPuzzle::Playing,
        },
    }
}

/// How the members (but the author) did with a puzzle.
pub fn stats(members: &[GroupMember], author: Option<i64>, rows: &[&Row]) -> PuzzleStats {
    let playing: Vec<&&Row> = rows
        .iter()
        .filter(|r| Some(r.user_id) != author && members.iter().any(|m| m.user_id == r.user_id))
        .collect();
    let times: Vec<i64> = playing
        .iter()
        .filter(|r| r.solved() && r.flag.is_none())
        .filter_map(|r| r.time_ms)
        .collect();
    let helps: Vec<Help> = playing.iter().flat_map(|r| r.helps.clone()).collect();
    PuzzleStats {
        players: count(members.iter().filter(|m| Some(m.user_id) != author).count()),
        solved: count(playing.iter().filter(|r| r.solved()).count()),
        gave_up: count(playing.iter().filter(|r| r.revealed).count()),
        playing: count(
            playing
                .iter()
                .filter(|r| r.finished_at.is_none() && !r.revealed)
                .count(),
        ),
        fastest_ms: times.iter().copied().min(),
        median_ms: median(&times),
        hints: playing.iter().map(|r| r.hints).sum(),
        checks: playing.iter().map(|r| r.checks).sum(),
        hardest: hardest(&helps),
    }
}

/// A puzzle in the list, as `viewer` sees it.
fn summary(
    stored: &Stored,
    members: &[GroupMember],
    rows: &[&Row],
    viewer: i64,
    now: i64,
) -> PuzzleSummary {
    let open = stored.open(now);
    let board = board_entries(members, stored.author_id, rows);
    let mine = my_puzzle(
        viewer,
        stored.author_id,
        rows.iter().find(|r| r.user_id == viewer).copied(),
        &board,
    );
    let pips = board
        .iter()
        .map(|e| Pip {
            colour: e.colour,
            solved: rows.iter().any(|r| r.user_id == e.user_id && r.solved()),
        })
        .collect();
    let winner = (!open)
        .then(|| {
            board.iter().find_map(|e| match e.state {
                TodayState::Ranked {
                    rank: 1, time_ms, ..
                } => Some((e.name.clone(), time_ms)),
                _ => None,
            })
        })
        .flatten();
    PuzzleSummary {
        id: stored.id,
        group_id: stored.group_id,
        title: stored.title.clone(),
        author: stored.author_id.and_then(|a| {
            members
                .iter()
                .find(|m| m.user_id == a)
                .map(|m| PuzzleAuthor {
                    user_id: m.user_id,
                    name: m.name.clone(),
                    guest: m.guest,
                    colour: m.colour,
                })
        }),
        width: stored.puzzle.width,
        height: stored.puzzle.height,
        islands: u32::try_from(stored.puzzle.islands.len()).unwrap_or(u32::MAX),
        created_at: stored.created_at,
        closes_at: stored.closes_at,
        open,
        pips,
        mine,
        board: (!open).then(|| FinalBoard {
            puzzle: stored.puzzle.clone(),
            solution: stored.solution.clone(),
        }),
        winner,
    }
}

/// The group page's puzzles: the open ones, the latest closed ones and the viewer's applause.
pub async fn list(
    conn: &Connection,
    group: i64,
    viewer: i64,
    now: i64,
) -> Result<GroupPuzzles, ServerFnError> {
    let group = require_member(conn, group, viewer).await?;
    let open = stored_where(
        conn,
        "group_id = ?1 AND closes_at > ?2 ORDER BY closes_at, id",
        params![group.id, now],
    )
    .await?;
    let closed = stored_where(
        conn,
        &format!("group_id = ?1 AND closes_at <= ?2 ORDER BY closes_at DESC, id DESC LIMIT {ARCHIVE_SHOWN}"),
        params![group.id, now],
    )
    .await?;
    let applause = stored_where(
        conn,
        "group_id = ?1 AND closes_at <= ?2 AND author_id = ?3 AND kudos_seen = 0 ORDER BY closes_at, id",
        params![group.id, now, viewer],
    )
    .await?;
    let members = members(conn, &group, day_of(now)).await?;
    let ids: Vec<i64> = open
        .iter()
        .chain(&closed)
        .chain(&applause)
        .map(|p| p.id)
        .collect();
    let rows = attempts(conn, &ids).await?;
    let of = |id: i64| {
        rows.iter()
            .filter(|r| r.puzzle_id == id)
            .collect::<Vec<_>>()
    };
    let mine = open.iter().filter(|p| p.author(viewer)).count();
    Ok(GroupPuzzles {
        open: open
            .iter()
            .map(|p| summary(p, &members, &of(p.id), viewer, now))
            .collect(),
        closed: closed
            .iter()
            .map(|p| summary(p, &members, &of(p.id), viewer, now))
            .collect(),
        kudos: applause
            .iter()
            .map(|p| Kudos {
                puzzle: summary(p, &members, &of(p.id), viewer, now),
                stats: stats(&members, p.author_id, &of(p.id)),
            })
            .collect(),
        can_post: MAX_OPEN.saturating_sub(mine),
        server_now: now,
    })
}

/// The page of a puzzle, as `viewer` may see it.
pub async fn page(
    conn: &Connection,
    id: i64,
    viewer: i64,
    now: i64,
) -> Result<PuzzlePage, ServerFnError> {
    let (group, stored) = require(conn, id, viewer).await?;
    page_of(conn, &group, &stored, viewer, now).await
}

async fn page_of(
    conn: &Connection,
    group: &Group,
    stored: &Stored,
    viewer: i64,
    now: i64,
) -> Result<PuzzlePage, ServerFnError> {
    let members = members(conn, group, day_of(now)).await?;
    let rows = attempts(conn, &[stored.id]).await?;
    let rows: Vec<&Row> = rows.iter().collect();
    let summary = summary(stored, &members, &rows, viewer, now);
    let mine = rows.iter().find(|r| r.user_id == viewer).copied();
    let open = stored.open(now);
    let author = stored.author(viewer);
    // The author and everyone after the close know it all; before, the times wait until the
    // viewer is done with it, and the puzzle until their clock runs
    let board_open = summary.mine.done() || !open;
    let entries = board_entries(&members, stored.author_id, &rows);
    Ok(PuzzlePage {
        group_id: group.id,
        group_name: group.name.clone(),
        me: viewer,
        puzzle: (mine.is_some() || author || !open).then(|| stored.puzzle.clone()),
        solution: (author || !open).then(|| stored.solution.clone()),
        attempt: mine.map(Row::attempt),
        board: spoiler_safe(entries, board_open),
        board_open,
        stats: (author || !open).then(|| stats(&members, stored.author_id, &rows)),
        summary,
        server_now: now,
    })
}

/// Starts the clock (once) and returns the page, with the puzzle.
pub async fn start(
    conn: &Connection,
    id: i64,
    user: i64,
    now: i64,
) -> Result<PuzzlePage, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let (group, stored) = require(&tx, id, user).await?;
    if stored.author(user) {
        return Err(ApiError::PuzzleOwn.into());
    }
    if !stored.open(now) {
        return Err(ApiError::PuzzleClosed.into());
    }
    tx.execute(
        "INSERT OR IGNORE INTO group_puzzle_attempts (puzzle_id, user_id, started_at) VALUES (?1, ?2, ?3)",
        params![id, user, now],
    )
    .await
    .map_err(db_err)?;
    let page = page_of(&tx, &group, &stored, user, now).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(page)
}

/// The puzzle and the player's running attempt at it, for the actions during the game. Only
/// while it's open: once closed, the final board is out.
async fn running(
    conn: &Connection,
    id: i64,
    user: i64,
    now: i64,
) -> Result<(Stored, Row), ServerFnError> {
    let (_, stored) = require(conn, id, user).await?;
    if !stored.open(now) {
        return Err(ApiError::PuzzleClosed.into());
    }
    match attempt(conn, id, user).await? {
        Some(r) if r.finished_at.is_some() || r.revealed => Err(ApiError::AlreadyFinished.into()),
        Some(r) => Ok((stored, r)),
        None => Err(ApiError::NotStarted.into()),
    }
}

fn check_bridges(board: &Board, bridges: &[u8]) -> Result<(), ServerFnError> {
    if bridges.len() != board.edges.len() || bridges.iter().any(|&b| b > 2) {
        return Err(ApiError::WrongPuzzle.into());
    }
    Ok(())
}

/// Ms since the start, for the time of a help.
fn since(started_at: i64, now: i64) -> u32 {
    u32::try_from((now - started_at).max(0)).unwrap_or(u32::MAX)
}

/// Stores a help with the attempt and counts it.
async fn record_help(conn: &Connection, row: &Row, help: Help) -> Result<(), ServerFnError> {
    let column = match help.kind {
        MarkKind::Hint => "hints",
        MarkKind::Check => "checks",
    };
    let mut helps = row.helps.clone();
    helps.push(help);
    conn.execute(
        &format!(
            "UPDATE group_puzzle_attempts SET {column} = {column} + 1, helps = ?3
             WHERE puzzle_id = ?1 AND user_id = ?2"
        ),
        params![row.puzzle_id, row.user_id, json(&helps)?],
    )
    .await
    .map_err(db_err)?;
    Ok(())
}

/// Fixes one bridge, costs a hint penalty like the daily's.
pub async fn hint(
    conn: &Connection,
    id: i64,
    user: i64,
    bridges: Vec<u8>,
    now: i64,
) -> Result<Option<(usize, u8)>, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let (stored, row) = running(&tx, id, user, now).await?;
    let board = Board::new(stored.puzzle);
    check_bridges(&board, &bridges)?;
    let hint = pick_hint(&board, &bridges, &stored.solution);
    if let Some((edge, _)) = hint {
        let help = Help {
            t: since(row.started_at, now),
            kind: MarkKind::Hint,
            edges: vec![u16::try_from(edge).unwrap_or(u16::MAX)],
        };
        record_help(&tx, &row, help).await?;
    }
    tx.commit().await.map_err(db_err)?;
    Ok(hint)
}

/// Bridges that aren't part of the solution, costs a check penalty.
pub async fn check(
    conn: &Connection,
    id: i64,
    user: i64,
    bridges: Vec<u8>,
    now: i64,
) -> Result<Vec<usize>, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let (stored, row) = running(&tx, id, user, now).await?;
    check_bridges(&Board::new(stored.puzzle), &bridges)?;
    let wrong = wrong_edges(&bridges, &stored.solution);
    let help = Help {
        t: since(row.started_at, now),
        kind: MarkKind::Check,
        edges: wrong
            .iter()
            .map(|&e| u16::try_from(e).unwrap_or(u16::MAX))
            .collect(),
    };
    record_help(&tx, &row, help).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(wrong)
}

/// Gives up: the solution, the attempt isn't ranked.
pub async fn reveal(
    conn: &Connection,
    id: i64,
    user: i64,
    now: i64,
) -> Result<PuzzlePage, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let (stored, _) = running(&tx, id, user, now).await?;
    tx.execute(
        "UPDATE group_puzzle_attempts SET revealed = 1, bridges = ?3 WHERE puzzle_id = ?1 AND user_id = ?2",
        params![id, user, json(&stored.solution)?],
    )
    .await
    .map_err(db_err)?;
    let page = page(&tx, id, user, now).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(page)
}

/// Verifies a finished board and records the time, like a daily's: from the start on the
/// server's clock, with the penalties, and a move log that doesn't hold up takes it off the board.
pub async fn submit(
    conn: &Connection,
    id: i64,
    user: i64,
    bridges: Vec<u8>,
    moves: u32,
    log: Vec<MoveEvent>,
    now: i64,
) -> Result<PuzzlePage, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let (stored, row) = running(&tx, id, user, now).await?;
    let board = Board::new(stored.puzzle.clone());
    check_bridges(&board, &bridges)?;
    if !board.check(&bridges).solved {
        return Err(ApiError::NotSolved.into());
    }
    let elapsed = now - row.started_at;
    let time_ms = elapsed + row.attempt().penalty_ms();
    let flag = verify_log(&board, &stored.solution, &bridges, &log, elapsed, row.hints).err();
    tx.execute(
        "UPDATE group_puzzle_attempts SET finished_at = ?3, time_ms = ?4, moves = ?5, bridges = ?6, log = ?7, flag = ?8
         WHERE puzzle_id = ?1 AND user_id = ?2 AND finished_at IS NULL AND revealed = 0",
        params![
            id,
            user,
            now,
            time_ms,
            i64::from(moves),
            json(&bridges)?,
            json(&log)?,
            flag.map(Flag::id)
        ],
    )
    .await
    .map_err(db_err)?;
    let page = page(&tx, id, user, now).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(page)
}

/// The author takes back a puzzle that is still open, with everyone's attempts at it.
pub async fn withdraw(
    conn: &Connection,
    id: i64,
    user: i64,
    now: i64,
) -> Result<(), ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let (_, stored) = require(&tx, id, user).await?;
    if !stored.author(user) {
        return Err(ApiError::PuzzleAuthorOnly.into());
    }
    if !stored.open(now) {
        return Err(ApiError::PuzzleClosed.into());
    }
    for sql in [
        "DELETE FROM group_puzzle_attempts WHERE puzzle_id = ?1",
        "DELETE FROM group_puzzles WHERE id = ?1",
    ] {
        tx.execute(sql, params![id]).await.map_err(db_err)?;
    }
    tx.commit().await.map_err(db_err)
}

/// The author has seen the applause of a closed puzzle.
pub async fn seen_kudos(
    conn: &Connection,
    id: i64,
    user: i64,
    now: i64,
) -> Result<(), ServerFnError> {
    let (_, stored) = require(conn, id, user).await?;
    if stored.author(user) && !stored.open(now) {
        conn.execute(
            "UPDATE group_puzzles SET kudos_seen = 1 WHERE id = ?1",
            params![id],
        )
        .await
        .map_err(db_err)?;
    }
    Ok(())
}

/// Whether `puzzle` (however it is turned) is an open puzzle of one of `user`'s groups that they
/// haven't finished and didn't make: our solvers don't help with it then.
pub async fn locked(
    conn: &Connection,
    user: Option<i64>,
    puzzle: &Puzzle,
    now: i64,
) -> Result<bool, ServerFnError> {
    let Some(user) = user else {
        return Ok(false);
    };
    let mut rows = conn
        .query(
            "SELECT p.puzzle FROM group_puzzles p
             JOIN group_members m ON m.group_id = p.group_id AND m.user_id = ?1
             WHERE p.closes_at > ?2 AND (p.author_id IS NULL OR p.author_id != ?1)
               AND NOT EXISTS (SELECT 1 FROM group_puzzle_attempts a WHERE a.puzzle_id = p.id
                   AND a.user_id = ?1 AND (a.finished_at IS NOT NULL OR a.revealed != 0))",
            params![user, now],
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

/// What deleting a user does to group puzzles, in `auth::erase`'s transaction after
/// `groups::ERASE`.
///
/// Their open puzzles go with everyone's attempts at them (nobody can be ranked on them by an
/// author who's gone), their closed ones stay in the groups' archives without a name, and their
/// own attempts go. The puzzles of groups that went with them go by the trigger
/// `group_puzzles_gone` (db.rs).
pub async fn erase(conn: &Connection, user: i64, now: i64) -> Result<(), ServerFnError> {
    for sql in [
        "DELETE FROM group_puzzle_attempts WHERE puzzle_id IN
            (SELECT id FROM group_puzzles WHERE author_id = ?1 AND closes_at > ?2)",
        "DELETE FROM group_puzzles WHERE author_id = ?1 AND closes_at > ?2",
        "UPDATE group_puzzles SET author_id = NULL WHERE author_id = ?1",
        "DELETE FROM group_puzzle_attempts WHERE user_id = ?1",
    ] {
        conn.execute(sql, params![user, now])
            .await
            .map_err(db_err)?;
    }
    Ok(())
}

/// What signing in does to the group puzzles of the guest who played in this browser before.
///
/// Statements for `auth::merge_guest`, `?1` the account, `?2` the guest: the account takes their
/// attempts and their puzzles. Nobody ends up with an attempt at their own puzzle, and where both
/// played the same one, the account's attempt stays.
pub const MERGE_GUEST: [&str; 5] = [
    "DELETE FROM group_puzzle_attempts WHERE user_id = ?2
       AND puzzle_id IN (SELECT id FROM group_puzzles WHERE author_id = ?1)",
    "DELETE FROM group_puzzle_attempts WHERE user_id = ?1
       AND puzzle_id IN (SELECT id FROM group_puzzles WHERE author_id = ?2)",
    "UPDATE OR IGNORE group_puzzle_attempts SET user_id = ?1 WHERE user_id = ?2",
    "DELETE FROM group_puzzle_attempts WHERE user_id = ?2",
    "UPDATE group_puzzles SET author_id = ?1 WHERE author_id = ?2",
];

/// The replay of a group puzzle for the replay theatre (`pages::replay`).
///
/// A lane per member but the author, from their move logs. Like the times, only once the viewer is done with it, they
/// made it, or it's closed ([`ReplayData::locked`] before).
pub async fn replay(
    conn: &Connection,
    id: i64,
    viewer: i64,
    now: i64,
) -> Result<ReplayData, ServerFnError> {
    let (group, stored) = require(conn, id, viewer).await?;
    let page = page_of(conn, &group, &stored, viewer, now).await?;
    let mut data = ReplayData {
        day: day_of(stored.created_at),
        today: day_of(now),
        group: Some(ReplayGroup {
            id: group.id,
            name: group.name.clone(),
            first_day: group_day(group.created_at),
        }),
        me: viewer,
        locked: !page.board_open,
        puzzle: None,
        lanes: Vec::new(),
        group_puzzle: Some(stored.id),
    };
    if data.locked {
        return Ok(data);
    }
    let edges = Board::new(stored.puzzle.clone()).edges.len();
    let rows = attempts(conn, &[stored.id]).await?;
    let mut logs = HashMap::new();
    let mut found = conn
        .query(
            "SELECT user_id, log FROM group_puzzle_attempts WHERE puzzle_id = ?1 AND log IS NOT NULL",
            params![stored.id],
        )
        .await
        .map_err(db_err)?;
    while let Some(row) = found.next().await.map_err(db_err)? {
        let log: Vec<MoveEvent> =
            serde_json::from_str(&row.get::<String>(1).map_err(db_err)?).unwrap_or_default();
        logs.insert(row.get::<i64>(0).map_err(db_err)?, log);
    }
    data.lanes = members(conn, &group, day_of(now))
        .await?
        .into_iter()
        .filter(|m| Some(m.user_id) != stored.author_id)
        .map(|m| {
            let row = rows.iter().find(|r| r.user_id == m.user_id);
            let mut lane = Lane {
                user_id: m.user_id,
                name: m.name,
                guest: m.guest,
                colour: m.colour,
                outcome: LaneOutcome::Missing,
                log: Vec::new(),
                marks: Vec::new(),
            };
            match row {
                Some(r) if r.revealed => lane.outcome = LaneOutcome::GaveUp,
                Some(r) if r.solved() => {
                    lane.outcome = LaneOutcome::Solved {
                        time_ms: r.time_ms.unwrap_or(0),
                        hints: r.hints,
                        checks: r.checks,
                        ranked: r.flag.is_none(),
                    };
                    lane.log =
                        clean_log(logs.get(&r.user_id).map_or(&[][..], Vec::as_slice), edges);
                    lane.marks = r
                        .helps
                        .iter()
                        .map(|h| Mark {
                            t: h.t,
                            kind: h.kind,
                        })
                        .collect();
                }
                _ => {}
            }
            lane
        })
        .collect();
    data.puzzle = Some(stored.puzzle);
    Ok(data)
}
