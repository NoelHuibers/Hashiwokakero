//! Races (勝負 shōbu): members of a group race each other live on the same fresh puzzle.
//!
//! A member opens a lobby ([`open`]), the others [`join`]; at most one race is open per group.
//! The host [`start`]s it with at least `MIN_RACERS`: the server generates a puzzle
//! ([`PuzzleSource`]) and stores it with its solution, and the race starts `COUNTDOWN_MS` later,
//! for everyone at the same moment of the server's clock. The puzzle is only sent once that moment
//! came, and only to the players. They play it like a group puzzle (`kadai`): hints and checks
//! cost the daily's prices for the difficulty, the finish is verified with the move log
//! (`model::verify_log`), and a flagged result gets no place.
//!
//! Everything lives in the database, nothing in the server's memory: the site runs on more than
//! one machine, and any of them answers. The pages ask for the state every second or two
//! ([`poll`]), which also notes that the player is still there (`last_seen`) and takes their
//! progress, the share of the solution's bridges on their board, for everyone's progress bars.
//! What changes by time alone is noted lazily, by whoever asks next ([`settle`]): a lobby nobody
//! started for `LOBBY_MS` is deleted, and a race is over once nobody races any more (everyone
//! solved, gave up or wasn't seen for `AWAY_MS`) or `RACE_MS` after its start. `auth`'s cleanup
//! does the same for the races nobody asks about ([`delete_expired`]).
//!
//! Everything goes through `groups::require_member`, like the group's other pages. Finished races
//! stay as the group's history and go with the group (trigger `group_races_gone`, db.rs).
//!
//! Races and shared puzzles (`shares`, `/p/<code>`): a member can open a race on a shared puzzle
//! from its page ([`open_shared`], [`PuzzleSource::Shared`]). Its racers came through that link:
//! their verified results go on the puzzle's board through it and count for its sharer, like a
//! solve on the page ([`finish`]). And once a race is over, any member can [`share`] its puzzle
//! as a link of their own: the first link puts the racers' results on the puzzle's board as the
//! times to beat, once per race (`published_at`), without counting for the sharer (`counts = 0`):
//! they solved it before there was a link.

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use common::{Board, Difficulty, GenerateParams, Puzzle};
use leptos::prelude::ServerFnError;
use libsql::{Connection, TransactionBehavior, params, params::IntoParams};

use super::kadai::{add_help, check_bridges, from_json, json, since};
use super::{Group, require_member};
use crate::model::{
    AWAY_MS, ApiError, BLOCKED_NAME, Flag, GroupRaces, HINT_PENALTY_MS, HISTORY_SHOWN, Help,
    HelpPenalties, LOBBY_MS, Lane, LaneOutcome, MIN_RACERS, Mark, MarkKind, MoveEvent,
    PROGRESS_CAP, PROGRESS_MIN_GAP_MS, RACE_MS, RaceEntrant, RaceOutcome, RacePhase, RacePlayer,
    RaceResult, RaceShare, RaceSummary, RaceView, ReplayData, ReplayGroup, SharedBy, clean_log,
    day_of, group_day, pick_hint, race_over, race_places, shared_difficulty, shown_progress,
    verify_log, wrong_edges,
};
use crate::server::db::db_err;
use crate::server::{run_blocking, shares};

/// A race as stored, without its puzzle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Race {
    pub id: i64,
    pub group_id: i64,
    pub host_id: Option<i64>,
    pub difficulty: Difficulty,
    pub created_at: i64,
    pub starts_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub rematch_id: Option<i64>,
    /// A race on a shared puzzle: the puzzle (`shared_puzzles.id`) and the link it was opened
    /// from.
    pub share_puzzle_id: Option<i64>,
    pub share_code: Option<String>,
    /// When the racers' results went on the board of its puzzle, shared after the race.
    pub published_at: Option<i64>,
}

impl Race {
    pub fn phase(&self, now: i64) -> RacePhase {
        RacePhase::at(now, self.starts_at, self.ended_at)
    }

    /// A lobby nobody started in time.
    fn expired(&self, now: i64) -> bool {
        self.starts_at.is_none() && now >= self.created_at + LOBBY_MS
    }
}

const RACE_COLUMNS: &str = "id, group_id, host_id, difficulty, created_at, starts_at, ended_at,
    rematch_id, share_puzzle_id, share_code, published_at";

fn row_to_race(row: &libsql::Row, at: i32) -> Result<Race, ServerFnError> {
    Ok(Race {
        id: row.get(at).map_err(db_err)?,
        group_id: row.get(at + 1).map_err(db_err)?,
        host_id: row.get(at + 2).map_err(db_err)?,
        difficulty: Difficulty::from_id(&row.get::<String>(at + 3).map_err(db_err)?)
            .unwrap_or_default(),
        created_at: row.get(at + 4).map_err(db_err)?,
        starts_at: row.get(at + 5).map_err(db_err)?,
        ended_at: row.get(at + 6).map_err(db_err)?,
        rematch_id: row.get(at + 7).map_err(db_err)?,
        share_puzzle_id: row.get(at + 8).map_err(db_err)?,
        share_code: row.get(at + 9).map_err(db_err)?,
        published_at: row.get(at + 10).map_err(db_err)?,
    })
}

async fn races_where(
    conn: &Connection,
    filter: &str,
    values: impl IntoParams,
) -> Result<Vec<Race>, ServerFnError> {
    let mut rows = conn
        .query(
            &format!("SELECT {RACE_COLUMNS} FROM group_races WHERE {filter}"),
            values,
        )
        .await
        .map_err(db_err)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        out.push(row_to_race(&row, 0)?);
    }
    Ok(out)
}

pub async fn load(conn: &Connection, id: i64) -> Result<Option<Race>, ServerFnError> {
    Ok(races_where(conn, "id = ?1", params![id]).await?.pop())
}

/// The race and its group, if `user` is a member of the group. A race that isn't there (any more),
/// or a lobby that expired, is [`ApiError::RaceGone`]; one of a group the player isn't in is
/// [`ApiError::GroupNotFound`], like the group itself.
async fn require(
    conn: &Connection,
    id: i64,
    user: i64,
    now: i64,
) -> Result<(Group, Race), ServerFnError> {
    let race = load(conn, id)
        .await?
        .ok_or_else(|| ApiError::RaceGone.fail())?;
    let group = require_member(conn, race.group_id, user).await?;
    if race.expired(now) {
        return Err(ApiError::RaceGone.into());
    }
    Ok((group, race))
}

/// A player of a race as stored, with what the group knows of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub race_id: i64,
    pub user_id: i64,
    pub joined_at: i64,
    pub last_seen: i64,
    pub progress: i64,
    pub finished_at: Option<i64>,
    pub revealed: bool,
    pub time_ms: Option<i64>,
    pub hints: i64,
    pub checks: i64,
    pub flag: Option<Flag>,
    pub helps: Vec<Help>,
    pub name: String,
    pub guest: bool,
    pub colour: u8,
    /// A race on a shared puzzle: they have a result on it from before the race (only [`read`]
    /// knows).
    pub solved_before: bool,
}

impl Player {
    fn done(&self) -> bool {
        self.finished_at.is_some() || self.revealed
    }

    fn result(&self) -> RaceResult {
        RaceResult {
            user_id: self.user_id,
            finished_at: self.finished_at,
            revealed: self.revealed,
            time_ms: self.time_ms,
            flag: self.flag,
        }
    }
}

/// The players' columns (`p`) and their names and colours in the race's group (`u`, `m`).
const PLAYER_COLUMNS: &str = "p.race_id, p.user_id, p.joined_at, p.last_seen, p.progress,
    p.finished_at, p.revealed, p.time_ms, p.hints, p.checks, p.flag, p.helps,
    u.name, u.username, m.colour";

/// The players of the race `r` who are members of its group (and not banned): the ones who left
/// drop off it, like off the group's other boards.
const PLAYERS_OF: &str = "group_race_players p
    JOIN group_races r ON r.id = p.race_id
    JOIN group_members m ON m.group_id = r.group_id AND m.user_id = p.user_id
    JOIN users u ON u.id = p.user_id AND u.banned_at IS NULL";

fn row_to_player(row: &libsql::Row, at: i32) -> Result<Player, ServerFnError> {
    Ok(Player {
        race_id: row.get(at).map_err(db_err)?,
        user_id: row.get(at + 1).map_err(db_err)?,
        joined_at: row.get(at + 2).map_err(db_err)?,
        last_seen: row.get(at + 3).map_err(db_err)?,
        progress: row.get(at + 4).map_err(db_err)?,
        finished_at: row.get(at + 5).map_err(db_err)?,
        revealed: row.get::<i64>(at + 6).map_err(db_err)? != 0,
        time_ms: row.get(at + 7).map_err(db_err)?,
        hints: row.get(at + 8).map_err(db_err)?,
        checks: row.get(at + 9).map_err(db_err)?,
        flag: row
            .get::<Option<String>>(at + 10)
            .map_err(db_err)?
            .and_then(|f| Flag::from_id(&f)),
        // Helps that don't parse are no help to anyone, the result stays
        helps: serde_json::from_str(&row.get::<String>(at + 11).map_err(db_err)?)
            .unwrap_or_default(),
        name: row.get(at + 12).map_err(db_err)?,
        guest: row
            .get::<Option<String>>(at + 13)
            .map_err(db_err)?
            .is_none(),
        colour: u8::try_from(row.get::<i64>(at + 14).map_err(db_err)?).unwrap_or(0),
        solved_before: false,
    })
}

/// The players of some races, the first to join first.
async fn players(conn: &Connection, races: &[i64]) -> Result<Vec<Player>, ServerFnError> {
    if races.is_empty() {
        return Ok(Vec::new());
    }
    // Ids are integers from the database
    let ids: Vec<String> = races.iter().map(ToString::to_string).collect();
    let mut rows = conn
        .query(
            &format!(
                "SELECT {PLAYER_COLUMNS} FROM {PLAYERS_OF}
                 WHERE p.race_id IN ({}) ORDER BY p.joined_at, p.user_id",
                ids.join(",")
            ),
            (),
        )
        .await
        .map_err(db_err)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        out.push(row_to_player(&row, 0)?);
    }
    Ok(out)
}

/// What happened to a race by `now` that nobody noted yet: a lobby that expired is deleted
/// (`None`), a race that is over gets its end. Writes only when something happened.
async fn settle(
    conn: &Connection,
    race: &Race,
    players: &[Player],
    now: i64,
) -> Result<Option<Race>, ServerFnError> {
    if race.expired(now) {
        for sql in [
            "DELETE FROM group_race_players WHERE race_id = ?1",
            "DELETE FROM group_races WHERE id = ?1",
        ] {
            conn.execute(sql, params![race.id]).await.map_err(db_err)?;
        }
        return Ok(None);
    }
    let (Some(start), None) = (race.starts_at, race.ended_at) else {
        return Ok(Some(race.clone()));
    };
    let seen: Vec<(bool, i64)> = players
        .iter()
        .filter(|p| p.race_id == race.id)
        .map(|p| (p.done(), p.last_seen))
        .collect();
    let Some(end) = race_over(now, start, &seen) else {
        return Ok(Some(race.clone()));
    };
    conn.execute(
        "UPDATE group_races SET ended_at = ?2 WHERE id = ?1 AND ended_at IS NULL",
        params![race.id, end],
    )
    .await
    .map_err(db_err)?;
    Ok(Some(Race {
        ended_at: Some(end),
        ..race.clone()
    }))
}

/// [`settle`] for the open races of a group (`?1`) at `?2`, or of all groups without the group:
/// statements for [`open`] and [`delete_expired`].
fn settle_statements(group: bool) -> [String; 3] {
    let of_group = if group { "group_id = ?1 AND " } else { "" };
    let now = if group { "?2" } else { "?1" };
    [
        format!(
            "DELETE FROM group_race_players WHERE race_id IN (SELECT id FROM group_races
             WHERE {of_group}starts_at IS NULL AND created_at <= {now} - {LOBBY_MS})"
        ),
        format!(
            "DELETE FROM group_races WHERE {of_group}starts_at IS NULL AND created_at <= {now} - {LOBBY_MS}"
        ),
        format!(
            "UPDATE group_races SET ended_at = MIN({now}, starts_at + {RACE_MS})
             WHERE {of_group}ended_at IS NULL AND starts_at <= {now}
               AND (starts_at + {RACE_MS} <= {now} OR NOT EXISTS (
                   SELECT 1 FROM group_race_players p WHERE p.race_id = group_races.id
                     AND p.finished_at IS NULL AND p.revealed = 0 AND p.last_seen > {now} - {AWAY_MS}))"
        ),
    ]
}

/// Deletes the lobbies nobody started in time and ends the races that are over, of every group:
/// what nobody looked at since. For `auth::start_cleanup`.
pub async fn delete_expired(conn: &Connection, now: i64) -> Result<(), ServerFnError> {
    for sql in settle_statements(false) {
        conn.execute(&sql, params![now]).await.map_err(db_err)?;
    }
    Ok(())
}

/// The race open in a group (in the lobby or running), after settling the group's races.
async fn open_race(conn: &Connection, group: i64, now: i64) -> Result<Option<Race>, ServerFnError> {
    for sql in settle_statements(true) {
        conn.execute(&sql, params![group, now])
            .await
            .map_err(db_err)?;
    }
    Ok(
        races_where(conn, "group_id = ?1 AND ended_at IS NULL", params![group])
            .await?
            .pop(),
    )
}

/// A new lobby of `user` in a group, with them in it, for a puzzle of `source`.
async fn insert_lobby(
    conn: &Connection,
    group: i64,
    user: i64,
    source: &PuzzleSource,
    now: i64,
) -> Result<i64, ServerFnError> {
    if open_race(conn, group, now).await?.is_some() {
        return Err(ApiError::RaceOpen.into());
    }
    let (difficulty, puzzle_id, code) = match source {
        PuzzleSource::Preset(difficulty) => (*difficulty, None, None),
        PuzzleSource::Shared {
            puzzle_id,
            code,
            difficulty,
        } => (*difficulty, Some(*puzzle_id), Some(code.clone())),
    };
    conn.execute(
        "INSERT INTO group_races (group_id, host_id, difficulty, created_at, share_puzzle_id, share_code)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![group, user, difficulty.id(), now, puzzle_id, code],
    )
    .await
    .map_err(db_err)?;
    let id = conn.last_insert_rowid();
    add_player(conn, id, user, now).await?;
    Ok(id)
}

async fn add_player(
    conn: &Connection,
    race: i64,
    user: i64,
    now: i64,
) -> Result<(), ServerFnError> {
    conn.execute(
        "INSERT OR IGNORE INTO group_race_players (race_id, user_id, joined_at, last_seen) VALUES (?1, ?2, ?3, ?3)",
        params![race, user, now],
    )
    .await
    .map_err(db_err)?;
    Ok(())
}

/// Opens a lobby in a group with the puzzle size and difficulty of a preset of Play, with
/// `user` as its host and first player. Returns its id. Only one race at a time per group.
pub async fn open(
    conn: &Connection,
    group: i64,
    user: i64,
    difficulty: Difficulty,
    now: i64,
) -> Result<i64, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    require_member(&tx, group, user).await?;
    let id = insert_lobby(&tx, group, user, &PuzzleSource::Preset(difficulty), now).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(id)
}

/// Opens a lobby in a group on the shared puzzle of the link `code` (`/p/<code>`).
///
/// With `user` as its host and first player; returns its id. Like [`open`] otherwise: one race at
/// a time per group, members only. The help costs the prices of the preset closest in size
/// (`model::shared_difficulty`).
pub async fn open_shared(
    conn: &Connection,
    group: i64,
    user: i64,
    code: &str,
    now: i64,
) -> Result<i64, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    require_member(&tx, group, user).await?;
    let (puzzle_id, puzzle, _) = shares::linked_puzzle(&tx, code)
        .await?
        .ok_or_else(|| ApiError::WrongPuzzle.fail())?;
    let source = PuzzleSource::Shared {
        puzzle_id,
        code: code.to_string(),
        difficulty: shared_difficulty(puzzle.width, puzzle.height),
    };
    let id = insert_lobby(&tx, group, user, &source, now).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(id)
}

/// The race's page as `viewer` sees it, `None` for a race that isn't there or a lobby that expired.
/// `puzzle`: the page asks for the puzzle, it gets it once the race started if it plays (and
/// once it's over in any case).
///
/// One query: the race, its group, the viewer's membership, its rematch and its players with
/// their names (a player the viewer blocked as `BLOCKED_NAME`), a row per player. On a shared
/// puzzle also its sharer and size, and who solved it before the race.
async fn read(
    conn: &Connection,
    id: i64,
    viewer: i64,
    puzzle: bool,
    now: i64,
) -> Result<Option<(Race, Vec<Player>, RaceView)>, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT r.id, r.group_id, r.host_id, r.difficulty, r.created_at, r.starts_at,
                     r.ended_at, r.rematch_id, r.share_puzzle_id, r.share_code, r.published_at,
                     g.name,
                     x.id, x.starts_at IS NULL AND x.created_at > ?3 - {LOBBY_MS},
                     CASE WHEN ?4 AND r.starts_at <= ?3 THEN r.puzzle END,
                     CASE WHEN r.ended_at IS NOT NULL THEN r.solution END,
                     EXISTS (SELECT 1 FROM blocks b WHERE b.user_id = ?2 AND b.blocked_id = q.user_id),
                     q.race_id, q.user_id, q.joined_at, q.last_seen, q.progress, q.finished_at,
                     q.revealed, q.time_ms, q.hints, q.checks, q.flag, q.helps, q.name, q.username,
                     q.colour,
                     EXISTS (SELECT 1 FROM shared_results sr WHERE sr.puzzle_id = r.share_puzzle_id
                         AND sr.user_id = q.user_id AND sr.finished_at < COALESCE(r.starts_at, ?3 + 1)),
                     {}
                 FROM group_races r
                 JOIN groups g ON g.id = r.group_id
                 JOIN group_members me ON me.group_id = r.group_id AND me.user_id = ?2
                 LEFT JOIN group_races x ON x.id = r.rematch_id
                 LEFT JOIN (SELECT {PLAYER_COLUMNS} FROM {PLAYERS_OF} WHERE p.race_id = ?1) q
                     ON q.race_id = r.id
                 {SHARE_JOINS}
                 WHERE r.id = ?1
                 ORDER BY q.joined_at, q.user_id",
                share_columns("?2")
            ),
            params![id, viewer, now, i64::from(puzzle)],
        )
        .await
        .map_err(db_err)?;
    let mut found = None;
    let mut players = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        if found.is_none() {
            let race = row_to_race(&row, 0)?;
            let rematch: Option<i64> = row.get(12).map_err(db_err)?;
            let rematch_open = row.get::<Option<i64>>(13).map_err(db_err)? == Some(1);
            let puzzle = row
                .get::<Option<String>>(14)
                .map_err(db_err)?
                .map(|p| from_json::<Puzzle>(&p))
                .transpose()?
                // Stored JSON skips Puzzle::new, keep the islands in reading order
                .map(|p| Puzzle::new(p.width, p.height, p.islands));
            let solution = row
                .get::<Option<String>>(15)
                .map_err(db_err)?
                .map(|s| from_json::<Vec<u8>>(&s))
                .transpose()?;
            let name: String = row.get(11).map_err(db_err)?;
            let shared = row_to_share(&row, 33)?;
            found = Some((race, name, rematch, rematch_open, puzzle, solution, shared));
        }
        if row.get::<Option<i64>>(17).map_err(db_err)?.is_some() {
            let mut player = row_to_player(&row, 17)?;
            if row.get::<i64>(16).map_err(db_err)? != 0 {
                BLOCKED_NAME.clone_into(&mut player.name);
            }
            player.solved_before = row.get::<i64>(32).map_err(db_err)? != 0;
            players.push(player);
        }
    }
    let Some((race, group_name, rematch, rematch_open, puzzle, solution, shared)) = found else {
        return Ok(None);
    };
    let ended = race.phase(now) == RacePhase::Ended;
    let joined = players.iter().any(|p| p.user_id == viewer);
    let view = RaceView {
        id: race.id,
        group_id: race.group_id,
        group_name,
        me: viewer,
        host: race.host_id,
        difficulty: race.difficulty,
        penalties: HelpPenalties::of(race.difficulty),
        created_at: race.created_at,
        starts_at: race.starts_at,
        ended_at: race.ended_at,
        players: view_players(&players, ended, now),
        // Not for the watchers before the end: nobody solves it for a friend
        puzzle: puzzle.filter(|_| joined || ended),
        solution: solution.filter(|_| ended),
        rematch: rematch.filter(|_| ended),
        rematch_open: rematch_open && ended,
        server_now: now,
        shared,
    };
    Ok(Some((race, players, view)))
}

/// The columns of the shared puzzle of a race (`s`, `sp`) and who shared it (`su`) for
/// [`row_to_share`], with whether the `viewer` (a parameter, `?2`) blocked them.
fn share_columns(viewer: &str) -> String {
    format!(
        "s.code, su.id, su.name, su.username IS NULL,
         EXISTS (SELECT 1 FROM blocks bs WHERE bs.user_id = {viewer} AND bs.blocked_id = su.id),
         json_extract(sp.puzzle, '$.width'), json_extract(sp.puzzle, '$.height')"
    )
}

/// The joins of [`share_columns`] to a race `r`: its link, the puzzle and the sharer, if they
/// are still there (not deleted, nor banned).
const SHARE_JOINS: &str = "LEFT JOIN puzzle_shares s ON s.code = r.share_code
    LEFT JOIN shared_puzzles sp ON sp.id = s.puzzle_id
    LEFT JOIN users su ON su.id = s.user_id AND su.deleted_at IS NULL AND su.banned_at IS NULL";

/// The shared puzzle of a race, from [`share_columns`] at `at`: `None` for a race of the generator.
fn row_to_share(row: &libsql::Row, at: i32) -> Result<Option<RaceShare>, ServerFnError> {
    let Some(code) = row.get::<Option<String>>(at).map_err(db_err)? else {
        return Ok(None);
    };
    let size = |i| -> Result<u8, ServerFnError> {
        Ok(u8::try_from(row.get::<Option<i64>>(i).map_err(db_err)?.unwrap_or(0)).unwrap_or(0))
    };
    let sharer = match row.get::<Option<i64>>(at + 1).map_err(db_err)? {
        Some(user_id) => {
            let blocked = row.get::<Option<i64>>(at + 4).map_err(db_err)? == Some(1);
            Some(SharedBy {
                user_id,
                name: if blocked {
                    BLOCKED_NAME.to_string()
                } else {
                    row.get(at + 2).map_err(db_err)?
                },
                guest: row.get::<Option<i64>>(at + 3).map_err(db_err)? == Some(1),
                stamp: None,
                result: None,
            })
        }
        None => None,
    };
    Ok(Some(RaceShare {
        code,
        sharer,
        width: size(at + 5)?,
        height: size(at + 6)?,
    }))
}

/// The players as the race's page shows them, with their places once they solved it.
fn view_players(players: &[Player], ended: bool, now: i64) -> Vec<RacePlayer> {
    let results: Vec<RaceResult> = players.iter().map(Player::result).collect();
    let places = race_places(&results);
    players
        .iter()
        .map(|p| {
            let solved = p.finished_at.is_some() && !p.revealed;
            let outcome = match (solved, p.time_ms) {
                (true, Some(time_ms)) => RaceOutcome::Solved {
                    time_ms,
                    place: places.get(&p.user_id).copied(),
                },
                _ if p.revealed => RaceOutcome::GaveUp,
                _ if ended => RaceOutcome::Out,
                _ => RaceOutcome::Racing,
            };
            RacePlayer {
                user_id: p.user_id,
                name: p.name.clone(),
                guest: p.guest,
                colour: p.colour,
                joined_at: p.joined_at,
                progress: shown_progress(p.progress, solved),
                away: !p.done() && now - p.last_seen >= AWAY_MS,
                hints: p.hints,
                checks: p.checks,
                flag: p.flag,
                outcome,
                solved_before: p.solved_before,
            }
        })
        .collect()
}

/// [`read`], with what happened by `now` noted ([`settle`]).
async fn current(
    conn: &Connection,
    id: i64,
    viewer: i64,
    puzzle: bool,
    now: i64,
) -> Result<Option<(Race, RaceView)>, ServerFnError> {
    let Some((race, players, view)) = read(conn, id, viewer, puzzle, now).await? else {
        return Ok(None);
    };
    match settle(conn, &race, &players, now).await? {
        None => Ok(None),
        Some(settled) if settled == race => Ok(Some((race, view))),
        // Just over: again, with the results
        Some(_) => Ok(read(conn, id, viewer, puzzle, now)
            .await?
            .map(|(race, _, view)| (race, view))),
    }
}

/// The view of a race the viewer is known to be in the group of, an error if it went.
async fn view_of(
    conn: &Connection,
    id: i64,
    viewer: i64,
    now: i64,
) -> Result<RaceView, ServerFnError> {
    current(conn, id, viewer, false, now)
        .await?
        .map(|(_, view)| view)
        .ok_or_else(|| ApiError::RaceGone.fail())
}

/// What a race's page asks every second or two: the race as `viewer` sees it, `None` if it isn't
/// there (any more) or the viewer isn't in its group.
///
/// Notes that the viewer is still there, if they joined, and takes their `progress` (percent of
/// the solution's bridges on their board, for the others' bars) at most every
/// `PROGRESS_MIN_GAP_MS`, while they race. Untrusted, it's only for show: the finish is checked.
/// One write and one query, a second write only when the race just ended or expired.
pub async fn poll(
    conn: &Connection,
    id: i64,
    viewer: i64,
    progress: Option<u8>,
    puzzle: bool,
    now: i64,
) -> Result<Option<RaceView>, ServerFnError> {
    let progress = progress.map(|p| i64::from(p.min(PROGRESS_CAP)));
    let taken = format!(
        "?4 IS NOT NULL AND progress_at <= ?3 - {PROGRESS_MIN_GAP_MS}
         AND finished_at IS NULL AND revealed = 0"
    );
    conn.execute(
        &format!(
            "UPDATE group_race_players SET last_seen = ?3,
                 progress = CASE WHEN {taken} THEN ?4 ELSE progress END,
                 progress_at = CASE WHEN {taken} THEN ?3 ELSE progress_at END
             WHERE race_id = ?1 AND user_id = ?2 AND EXISTS (
                 SELECT 1 FROM group_races r JOIN group_members m ON m.group_id = r.group_id
                 WHERE r.id = ?1 AND m.user_id = ?2 AND r.ended_at IS NULL)"
        ),
        params![id, viewer, now, progress],
    )
    .await
    .map_err(db_err)?;
    Ok(current(conn, id, viewer, puzzle, now)
        .await?
        .map(|(_, view)| view))
}

/// Joins a lobby, and returns the race. Joining twice does nothing; after the start nobody joins.
pub async fn join(
    conn: &Connection,
    id: i64,
    user: i64,
    now: i64,
) -> Result<RaceView, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let (_, race) = require(&tx, id, user, now).await?;
    if race.starts_at.is_some() {
        return Err(ApiError::RaceStarted.into());
    }
    add_player(&tx, id, user, now).await?;
    let view = view_of(&tx, id, user, now).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(view)
}

/// Leaves a lobby. The host calls it off: the lobby goes, with everyone in it.
pub async fn leave(conn: &Connection, id: i64, user: i64, now: i64) -> Result<(), ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let (_, race) = require(&tx, id, user, now).await?;
    if race.starts_at.is_some() {
        return Err(ApiError::RaceStarted.into());
    }
    if race.host_id == Some(user) {
        for sql in [
            "DELETE FROM group_race_players WHERE race_id = ?1",
            "DELETE FROM group_races WHERE id = ?1",
        ] {
            tx.execute(sql, params![id]).await.map_err(db_err)?;
        }
    } else {
        tx.execute(
            "DELETE FROM group_race_players WHERE race_id = ?1 AND user_id = ?2",
            params![id, user],
        )
        .await
        .map_err(db_err)?;
    }
    tx.commit().await.map_err(db_err)
}

/// Whether `user` can start the race now: its host, in the lobby, with enough players.
async fn startable(conn: &Connection, id: i64, user: i64, now: i64) -> Result<Race, ServerFnError> {
    let (_, race) = require(conn, id, user, now).await?;
    if race.starts_at.is_some() {
        return Err(ApiError::RaceStarted.into());
    }
    if race.host_id != Some(user) {
        return Err(ApiError::RaceHostOnly.into());
    }
    if players(conn, &[id]).await?.len() < MIN_RACERS {
        return Err(ApiError::RaceTooFew.into());
    }
    Ok(race)
}

/// A race's puzzle with its solution, as it is stored at the start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RacePuzzle {
    pub puzzle: Puzzle,
    pub solution: Vec<u8>,
}

/// Where a race's puzzle comes from at its start.
///
/// A fresh puzzle of the generator, of a difficulty's preset size; or a shared puzzle
/// (`/p/<code>`), stored with the race (`share_puzzle_id`, `share_code`). Either way the puzzle
/// is stored with the race at its start and sent at `starts_at`: a shared puzzle is public, but
/// the race goes the same way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PuzzleSource {
    Preset(Difficulty),
    /// The puzzle `puzzle_id` of the link `code` the race was opened from. `difficulty`: whose
    /// prices the help costs.
    Shared {
        puzzle_id: i64,
        code: String,
        difficulty: Difficulty,
    },
}

impl PuzzleSource {
    /// The source of a race as stored.
    pub fn of(race: &Race) -> Self {
        match (race.share_puzzle_id, &race.share_code) {
            (Some(puzzle_id), Some(code)) => Self::Shared {
                puzzle_id,
                code: code.clone(),
                difficulty: race.difficulty,
            },
            _ => Self::Preset(race.difficulty),
        }
    }

    /// The puzzle for a start, proven unique: a shared puzzle's is (`shares::create`).
    pub async fn puzzle(self, conn: &Connection) -> Result<RacePuzzle, ServerFnError> {
        match self {
            Self::Preset(difficulty) => generate(difficulty).await,
            Self::Shared { code, .. } => {
                let (_, puzzle, solution) = shares::linked_puzzle(conn, &code)
                    .await?
                    .ok_or_else(|| ApiError::WrongPuzzle.fail())?;
                Ok(RacePuzzle { puzzle, solution })
            }
        }
    }
}

/// A fresh puzzle of the generator, proven unique, of a difficulty's preset size.
async fn generate(difficulty: Difficulty) -> Result<RacePuzzle, ServerFnError> {
    let (width, height) = difficulty.size();
    for _ in 0..3 {
        let mut seed = [0u8; 8];
        getrandom::fill(&mut seed).map_err(|_| ApiError::Internal.fail())?;
        let params = GenerateParams {
            width,
            height,
            difficulty,
            seed: u64::from_le_bytes(seed),
        };
        let generated = run_blocking(move || Ok(backend::api::generate(&params))).await?;
        if generated.unique {
            return Ok(RacePuzzle {
                puzzle: generated.puzzle,
                solution: generated.solution,
            });
        }
    }
    Err(ApiError::Internal.into())
}

/// The host starts the race: its puzzle ([`PuzzleSource`]), and the race begins `COUNTDOWN_MS`
/// after it's ready, on the server's `clock`. Returns the race, without the puzzle yet.
pub async fn start(
    conn: &Connection,
    id: i64,
    user: i64,
    clock: impl Fn() -> i64,
) -> Result<RaceView, ServerFnError> {
    // Before the generator's work, and again below with the lock
    let race = startable(conn, id, user, clock()).await?;
    let puzzle = PuzzleSource::of(&race).puzzle(conn).await?;
    begin(conn, id, user, &puzzle, clock()).await
}

/// [`start`] with its puzzle, at `now`.
pub async fn begin(
    conn: &Connection,
    id: i64,
    user: i64,
    puzzle: &RacePuzzle,
    now: i64,
) -> Result<RaceView, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    startable(&tx, id, user, now).await?;
    tx.execute(
        "UPDATE group_races SET starts_at = ?2, puzzle = ?3, solution = ?4 WHERE id = ?1 AND starts_at IS NULL",
        params![
            id,
            now + crate::model::COUNTDOWN_MS,
            json(&puzzle.puzzle)?,
            json(&puzzle.solution)?
        ],
    )
    .await
    .map_err(db_err)?;
    tx.execute(
        "UPDATE group_race_players SET last_seen = ?3 WHERE race_id = ?1 AND user_id = ?2",
        params![id, user, now],
    )
    .await
    .map_err(db_err)?;
    let view = view_of(&tx, id, user, now).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(view)
}

/// A running race's puzzle and solution, and the player's line, for what they do during it.
struct Running {
    race: Race,
    start: i64,
    board: Board,
    solution: Vec<u8>,
    player: Player,
}

async fn running(
    conn: &Connection,
    id: i64,
    user: i64,
    now: i64,
) -> Result<Running, ServerFnError> {
    let (_, race) = require(conn, id, user, now).await?;
    let Some(start) = race
        .starts_at
        .filter(|_| race.phase(now) == RacePhase::Running)
    else {
        return Err(ApiError::RaceNotRunning.into());
    };
    let player = players(conn, &[id])
        .await?
        .into_iter()
        .find(|p| p.user_id == user)
        .ok_or_else(|| ApiError::RaceNotJoined.fail())?;
    if player.done() {
        return Err(ApiError::RaceDone.into());
    }
    let mut rows = conn
        .query(
            "SELECT puzzle, solution FROM group_races WHERE id = ?1",
            params![id],
        )
        .await
        .map_err(db_err)?;
    let row = rows
        .next()
        .await
        .map_err(db_err)?
        .ok_or_else(|| ApiError::RaceGone.fail())?;
    let puzzle: Puzzle = from_json(&row.get::<String>(0).map_err(db_err)?)?;
    let solution: Vec<u8> = from_json(&row.get::<String>(1).map_err(db_err)?)?;
    Ok(Running {
        race,
        start,
        board: Board::new(Puzzle::new(puzzle.width, puzzle.height, puzzle.islands)),
        solution,
        player,
    })
}

/// Notes the help of a player and that they're there.
async fn record_help(
    conn: &Connection,
    run: &Running,
    kind: MarkKind,
    edges: Vec<u16>,
    now: i64,
) -> Result<(), ServerFnError> {
    let help = Help {
        t: since(run.start, now),
        kind,
        edges,
    };
    add_help(
        conn,
        ("group_race_players", "race_id"),
        (run.race.id, run.player.user_id),
        &run.player.helps,
        help,
    )
    .await?;
    seen(conn, run, now).await
}

async fn seen(conn: &Connection, run: &Running, now: i64) -> Result<(), ServerFnError> {
    conn.execute(
        "UPDATE group_race_players SET last_seen = ?3 WHERE race_id = ?1 AND user_id = ?2",
        params![run.race.id, run.player.user_id, now],
    )
    .await
    .map_err(db_err)?;
    Ok(())
}

/// Fixes one bridge, at the daily's price of a hint for the difficulty.
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
    let run = running(&tx, id, user, now).await?;
    check_bridges(&run.board, &bridges)?;
    let hint = pick_hint(&run.board, &bridges, &run.solution);
    if let Some((edge, _)) = hint {
        let edges = vec![u16::try_from(edge).unwrap_or(u16::MAX)];
        record_help(&tx, &run, MarkKind::Hint, edges, now).await?;
    }
    tx.commit().await.map_err(db_err)?;
    Ok(hint)
}

/// The bridges that aren't part of the solution, at the price of a check.
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
    let run = running(&tx, id, user, now).await?;
    check_bridges(&run.board, &bridges)?;
    let wrong = wrong_edges(&bridges, &run.solution);
    let edges = wrong
        .iter()
        .map(|&e| u16::try_from(e).unwrap_or(u16::MAX))
        .collect();
    record_help(&tx, &run, MarkKind::Check, edges, now).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(wrong)
}

/// Gives up: out of the race, without a place. The solution comes with the results, once it's
/// over for everyone.
pub async fn give_up(
    conn: &Connection,
    id: i64,
    user: i64,
    now: i64,
) -> Result<RaceView, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    running(&tx, id, user, now).await?;
    tx.execute(
        "UPDATE group_race_players SET revealed = 1, last_seen = ?3 WHERE race_id = ?1 AND user_id = ?2",
        params![id, user, now],
    )
    .await
    .map_err(db_err)?;
    let view = view_of(&tx, id, user, now).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(view)
}

/// Verifies a solved board and records the time, like a group puzzle's: from the race's start on
/// the server's clock, with the penalties; a move log that doesn't hold up gets no place.
pub async fn submit(
    conn: &Connection,
    id: i64,
    user: i64,
    bridges: Vec<u8>,
    moves: u32,
    log: Vec<MoveEvent>,
    now: i64,
) -> Result<RaceView, ServerFnError> {
    Ok(finish(conn, id, user, bridges, moves, log, now).await?.view)
}

/// A finish recorded by [`finish`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finished {
    pub view: RaceView,
    /// A race on a shared puzzle: the sharer whose link the result went through, if it counted
    /// (the player's first result on the puzzle). Their stamps need a sync (`shares::reached`).
    pub reached: Option<i64>,
}

/// [`submit`], and on a shared puzzle the result on its board.
///
/// Through the link the race was opened from, unless it was flagged: the server timed it (Play's
/// clock: from the start, the hints on top as on the board, not the race's penalties). Only a
/// player's first result on a puzzle counts, as on its page.
pub async fn finish(
    conn: &Connection,
    id: i64,
    user: i64,
    bridges: Vec<u8>,
    moves: u32,
    log: Vec<MoveEvent>,
    now: i64,
) -> Result<Finished, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let run = running(&tx, id, user, now).await?;
    check_bridges(&run.board, &bridges)?;
    if !run.board.check(&bridges).solved {
        return Err(ApiError::NotSolved.into());
    }
    let elapsed = now - run.start;
    let penalties = HelpPenalties::of(run.race.difficulty);
    let time_ms = elapsed + penalties.total(run.player.hints, run.player.checks);
    let flag = verify_log(
        &run.board,
        &run.solution,
        &bridges,
        &log,
        elapsed,
        run.player.hints,
    )
    .err();
    tx.execute(
        "UPDATE group_race_players SET finished_at = ?3, time_ms = ?4, moves = ?5, bridges = ?6,
             log = ?7, flag = ?8, last_seen = ?3, progress = 100
         WHERE race_id = ?1 AND user_id = ?2 AND finished_at IS NULL AND revealed = 0",
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
    let counted = match (&run.race.share_code, flag) {
        (Some(code), None) => {
            let result = shares::Verified {
                time_ms: elapsed,
                moves: i64::from(moves),
                hints: run.player.hints,
            };
            shares::add_result(&tx, code, user, result, true, now).await?
        }
        _ => false,
    };
    let view = view_of(&tx, id, user, now).await?;
    tx.commit().await.map_err(db_err)?;
    let reached = view
        .shared
        .as_ref()
        .and_then(|s| s.sharer.as_ref())
        .map(|s| s.user_id)
        .filter(|_| counted);
    Ok(Finished { view, reached })
}

/// A new lobby with the settings of a race that is over, for everyone still on its results: the
/// first to ask opens it (as its host), the others join it. Returns its id.
pub async fn rematch(
    conn: &Connection,
    id: i64,
    user: i64,
    now: i64,
) -> Result<i64, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let (group, race) = require(&tx, id, user, now).await?;
    if race.phase(now) != RacePhase::Ended {
        return Err(ApiError::RaceNotRunning.into());
    }
    let next = match race.rematch_id {
        Some(next) => load(&tx, next).await?,
        None => None,
    };
    let next_id = match next {
        Some(next) if next.starts_at.is_none() && !next.expired(now) => {
            add_player(&tx, next.id, user, now).await?;
            next.id
        }
        Some(next) if next.phase(now) != RacePhase::Ended && !next.expired(now) => {
            return Err(ApiError::RaceStarted.into());
        }
        // The same settings: the same difficulty, or the same shared puzzle again
        _ => {
            let next = insert_lobby(&tx, group.id, user, &PuzzleSource::of(&race), now).await?;
            tx.execute(
                "UPDATE group_races SET rematch_id = ?2 WHERE id = ?1",
                params![id, next],
            )
            .await
            .map_err(db_err)?;
            next
        }
    };
    tx.commit().await.map_err(db_err)?;
    Ok(next_id)
}

/// Shares the puzzle of a race that is over as a link of `user` (a player or a member who
/// watched): the code of their link, made with `shares::create` (its checks and limits) or the one
/// they have.
///
/// The first link to a race's puzzle puts the racers' results on its board ([`publish`]), as the
/// times to beat. A race on a shared puzzle has them there already, through its link.
pub async fn share(
    conn: &Connection,
    id: i64,
    user: i64,
    now: i64,
) -> Result<String, ServerFnError> {
    let (_, race) = require(conn, id, user, now).await?;
    if race.phase(now) != RacePhase::Ended {
        return Err(ApiError::RaceNotRunning.into());
    }
    let mut rows = conn
        .query("SELECT puzzle FROM group_races WHERE id = ?1", params![id])
        .await
        .map_err(db_err)?;
    let puzzle: Puzzle = match rows.next().await.map_err(db_err)? {
        Some(row) => from_json(&row.get::<String>(0).map_err(db_err)?)?,
        None => return Err(ApiError::RaceGone.into()),
    };
    drop(rows);
    let code = shares::create(conn, user, puzzle, now).await?;
    if race.share_code.is_none() && race.published_at.is_none() {
        publish(conn, id, &code, now).await?;
    }
    Ok(code)
}

/// Puts the results of the race `id` on the board of the puzzle of the link `code`, once per race
/// (`published_at`): every placed racer's (solved, not flagged, not given up), with the server's
/// time (Play's clock, as on the board: from the start, with 30 s per hint) and their moves.
///
/// They don't count for the sharer's stamps (`counts = 0`, see `shares::PLAYERS_REACHED`): they
/// solved it before there was a link. Only a player's first result on a puzzle stays, and not of
/// a racer who blocked the sharer (it would put them on their board), nor of banned players or
/// players who left the group. Returns how many went on the board.
async fn publish(conn: &Connection, id: i64, code: &str, now: i64) -> Result<u64, ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let claimed = tx
        .execute(
            "UPDATE group_races SET published_at = ?2 WHERE id = ?1 AND published_at IS NULL",
            params![id, now],
        )
        .await
        .map_err(db_err)?;
    let published = if claimed > 0 {
        tx.execute(
                &format!(
                    "INSERT OR IGNORE INTO shared_results
                         (puzzle_id, user_id, share_id, time_ms, moves, hints, score_ms, finished_at, counts)
                     SELECT s.puzzle_id, p.user_id, s.id, p.finished_at - r.starts_at,
                            COALESCE(p.moves, 0), p.hints,
                            p.finished_at - r.starts_at + p.hints * {HINT_PENALTY_MS}, p.finished_at, 0
                     FROM {PLAYERS_OF}
                     JOIN puzzle_shares s ON s.code = ?2
                     WHERE p.race_id = ?1 AND p.finished_at IS NOT NULL AND p.revealed = 0
                       AND p.flag IS NULL AND u.deleted_at IS NULL
                       AND NOT EXISTS (SELECT 1 FROM blocks b
                                       WHERE b.user_id = p.user_id AND b.blocked_id = s.user_id)"
                ),
            params![id, code],
        )
        .await
        .map_err(db_err)?
    } else {
        0
    };
    tx.commit().await.map_err(db_err)?;
    Ok(published)
}

/// A race on the group page, as `viewer` sees it.
fn summary(
    race: &Race,
    players: &[Player],
    shares: &HashMap<i64, RaceShare>,
    viewer: i64,
) -> RaceSummary {
    let mine: Vec<&Player> = players.iter().filter(|p| p.race_id == race.id).collect();
    let results: Vec<RaceResult> = mine.iter().map(|p| p.result()).collect();
    let places = race_places(&results);
    RaceSummary {
        id: race.id,
        difficulty: race.difficulty,
        host: race.host_id,
        created_at: race.created_at,
        starts_at: race.starts_at,
        ended_at: race.ended_at,
        entrants: mine
            .iter()
            .map(|p| RaceEntrant {
                user_id: p.user_id,
                name: p.name.clone(),
                colour: p.colour,
                place: places.get(&p.user_id).copied(),
                time_ms: p.time_ms.filter(|_| !p.revealed),
            })
            .collect(),
        joined: mine.iter().any(|p| p.user_id == viewer),
        shared: shares.get(&race.id).cloned(),
    }
}

/// The shared puzzles of the races `ids` that are on one, by race, as `viewer` sees their sharers.
async fn race_shares(
    conn: &Connection,
    ids: &[i64],
    viewer: i64,
) -> Result<HashMap<i64, RaceShare>, ServerFnError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    // Ids are integers from the database
    let ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
    let mut rows = conn
        .query(
            &format!(
                "SELECT r.id, {} FROM group_races r {SHARE_JOINS}
                 WHERE r.id IN ({}) AND r.share_code IS NOT NULL",
                share_columns("?1"),
                ids.join(",")
            ),
            params![viewer],
        )
        .await
        .map_err(db_err)?;
    let mut out = HashMap::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        if let Some(share) = row_to_share(&row, 1)? {
            out.insert(row.get::<i64>(0).map_err(db_err)?, share);
        }
    }
    Ok(out)
}

/// The races of a group page: the open one and the last ones that are over.
pub async fn list(
    conn: &Connection,
    group: i64,
    viewer: i64,
    now: i64,
) -> Result<GroupRaces, ServerFnError> {
    let group = require_member(conn, group, viewer).await?;
    let races = races_where(
        conn,
        &format!(
            "group_id = ?1 AND (ended_at IS NULL OR id IN (
                 SELECT id FROM group_races WHERE group_id = ?1 AND ended_at IS NOT NULL
                 ORDER BY starts_at DESC, id DESC LIMIT {}))
             ORDER BY starts_at DESC, id DESC",
            // The open race may be one that ended just now
            HISTORY_SHOWN + 1
        ),
        params![group.id],
    )
    .await?;
    let ids: Vec<i64> = races.iter().map(|r| r.id).collect();
    let players = players(conn, &ids).await?;
    let on_shared: Vec<i64> = races
        .iter()
        .filter(|r| r.share_code.is_some())
        .map(|r| r.id)
        .collect();
    let shares = race_shares(conn, &on_shared, viewer).await?;
    let mut open = None;
    let mut recent = Vec::new();
    for race in races {
        let race = if race.ended_at.is_none() {
            match settle(conn, &race, &players, now).await? {
                Some(race) => race,
                None => continue,
            }
        } else {
            race
        };
        if race.ended_at.is_none() {
            open = Some(summary(&race, &players, &shares, viewer));
        } else {
            recent.push(summary(&race, &players, &shares, viewer));
        }
    }
    recent.sort_by_key(|r| std::cmp::Reverse((r.starts_at, r.id)));
    recent.truncate(HISTORY_SHOWN);
    Ok(GroupRaces {
        open,
        recent,
        server_now: now,
    })
}

/// The replay of a race for the replay theatre: a lane per player, from their move logs. Only
/// once it's over, locked before.
pub async fn replay(
    conn: &Connection,
    id: i64,
    viewer: i64,
    now: i64,
) -> Result<ReplayData, ServerFnError> {
    let (group, _) = require(conn, id, viewer, now).await?;
    let view = view_of(conn, id, viewer, now).await?;
    let start = view.starts_at.unwrap_or(view.created_at);
    let mut data = ReplayData {
        day: day_of(start),
        today: day_of(now),
        group: Some(ReplayGroup {
            id: group.id,
            name: group.name.clone(),
            first_day: group_day(group.created_at),
        }),
        me: viewer,
        locked: view.phase() != RacePhase::Ended,
        puzzle: None,
        lanes: Vec::new(),
        group_puzzle: None,
        race: Some(id),
    };
    if data.locked {
        return Ok(data);
    }
    let mut rows = conn
        .query("SELECT puzzle FROM group_races WHERE id = ?1", params![id])
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(data);
    };
    let puzzle: Puzzle = from_json(&row.get::<String>(0).map_err(db_err)?)?;
    let puzzle = Puzzle::new(puzzle.width, puzzle.height, puzzle.islands);
    drop(rows);
    let edges = Board::new(puzzle.clone()).edges.len();
    let mut logs: HashMap<i64, Vec<MoveEvent>> = HashMap::new();
    let mut found = conn
        .query(
            "SELECT user_id, log FROM group_race_players WHERE race_id = ?1 AND log IS NOT NULL",
            params![id],
        )
        .await
        .map_err(db_err)?;
    while let Some(row) = found.next().await.map_err(db_err)? {
        let log: Vec<MoveEvent> =
            serde_json::from_str(&row.get::<String>(1).map_err(db_err)?).unwrap_or_default();
        logs.insert(row.get::<i64>(0).map_err(db_err)?, log);
    }
    let stored = players(conn, &[id]).await?;
    data.lanes = view
        .players
        .iter()
        .map(|p| {
            let helps = stored
                .iter()
                .find(|s| s.user_id == p.user_id)
                .map(|s| s.helps.as_slice())
                .unwrap_or_default();
            let mut lane = Lane {
                penalties: view.penalties,
                user_id: p.user_id,
                name: p.name.clone(),
                guest: p.guest,
                colour: p.colour,
                outcome: LaneOutcome::Missing,
                log: Vec::new(),
                marks: Vec::new(),
                official_rank: None,
            };
            match p.outcome {
                RaceOutcome::GaveUp => lane.outcome = LaneOutcome::GaveUp,
                RaceOutcome::Solved { time_ms, place } => {
                    lane.official_rank = place.and_then(|p| usize::try_from(p).ok());
                    lane.outcome = LaneOutcome::Solved {
                        time_ms,
                        hints: p.hints,
                        checks: p.checks,
                        ranked: place.is_some(),
                    };
                    lane.log =
                        clean_log(logs.get(&p.user_id).map_or(&[][..], Vec::as_slice), edges);
                    lane.marks = helps
                        .iter()
                        .map(|h| Mark {
                            t: h.t,
                            kind: h.kind,
                        })
                        .collect();
                }
                RaceOutcome::Racing | RaceOutcome::Out => {}
            }
            lane
        })
        .collect();
    data.puzzle = Some(puzzle);
    Ok(data)
}

/// What deleting a user does to races, in `auth::erase`'s transaction (`?1` the user).
///
/// The lobbies
/// they host go with everyone in them, races they hosted stay without a host, and their own lines
/// go, also from races that are over, like their attempts at group puzzles. The races of groups that
/// went with them go by the trigger `group_races_gone` (db.rs).
pub const ERASE: [&str; 4] = [
    "DELETE FROM group_race_players WHERE race_id IN
        (SELECT id FROM group_races WHERE host_id = ?1 AND starts_at IS NULL)",
    "DELETE FROM group_races WHERE host_id = ?1 AND starts_at IS NULL",
    "UPDATE group_races SET host_id = NULL WHERE host_id = ?1",
    "DELETE FROM group_race_players WHERE user_id = ?1",
];

/// What signing in does to the races of the guest who played in this browser before.
///
/// Statements for `auth::merge_guest`, `?1` the account, `?2` the guest: the account takes their
/// lines and the races they host. Where both are in the same race, the account's line stays.
pub const MERGE_GUEST: [&str; 3] = [
    "DELETE FROM group_race_players WHERE user_id = ?2
       AND race_id IN (SELECT race_id FROM group_race_players WHERE user_id = ?1)",
    "UPDATE group_race_players SET user_id = ?1 WHERE user_id = ?2",
    "UPDATE group_races SET host_id = ?1 WHERE host_id = ?2",
];
