//! Tests of races against a fresh database file.

use libsql::{Connection, params};

use super::*;
use crate::model::{COUNTDOWN_MS, MoveEvent};
use crate::server::auth::{cleanup_guests, erase as erase_user};
use crate::server::db::TestConn;
use crate::server::groups::{create, join as join_group, leave as leave_group, merge_guest};

async fn db() -> TestConn {
    TestConn::new("race").await
}

/// A registered player (`username`), or a guest (`None`).
async fn user(conn: &Connection, name: &str, username: Option<&str>) -> i64 {
    conn.execute(
        "INSERT INTO users (username, name, created_at) VALUES (?1, ?2, 0)",
        params![username, name],
    )
    .await
    .unwrap();
    conn.last_insert_rowid()
}

async fn player(conn: &Connection, name: &str) -> i64 {
    user(conn, name, Some(name)).await
}

async fn number(conn: &Connection, sql: &str) -> i64 {
    let mut rows = conn.query(sql, ()).await.unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

#[track_caller]
fn error<T: std::fmt::Debug>(result: Result<T, ServerFnError>) -> ApiError {
    let e = result.expect_err("should have failed");
    ApiError::from_message(&e.to_string()).unwrap_or_else(|| panic!("{e}"))
}

/// A group of `owner` that `others` joined.
async fn group_of(conn: &Connection, owner: i64, others: &[i64]) -> i64 {
    let id = create(conn, owner, "Lunch").await.unwrap();
    let mut rows = conn
        .query("SELECT invite FROM groups WHERE id = ?1", params![id])
        .await
        .unwrap();
    let code: String = rows.next().await.unwrap().unwrap().get(0).unwrap();
    for &other in others {
        join_group(conn, &code, other).await.unwrap();
    }
    id
}

/// A 2 and a 3 joined by a double bridge, the 3 by a single one to a 1 below it: one solution,
/// `[2, 1]`.
fn small() -> RacePuzzle {
    RacePuzzle {
        puzzle: Puzzle::from_grid(&[vec![2, 0, 3], vec![0, 0, 0], vec![0, 0, 1]]),
        solution: SOLUTION.to_vec(),
    }
}

const SOLUTION: [u8; 2] = [2, 1];

/// The clicks of a player solving [`small`] in a few seconds.
fn log() -> Vec<MoveEvent> {
    let tap = |t, edge, count| MoveEvent {
        t,
        edge,
        count,
        tap: true,
    };
    vec![tap(1_000, 0, 1), tap(2_000, 0, 2), tap(3_500, 1, 1)]
}

const T0: i64 = 1_800_000_000_000;

/// A race of `host` in `group` with `others` joined, started at `T0` with [`small`]: it runs
/// from `T0 + COUNTDOWN_MS`.
async fn started(conn: &Connection, group: i64, host: i64, others: &[i64]) -> (i64, i64) {
    let id = open(conn, group, host, Difficulty::Easy, T0 - 60_000)
        .await
        .unwrap();
    for &other in others {
        join(conn, id, other, T0 - 30_000).await.unwrap();
    }
    let view = begin(conn, id, host, &small(), T0).await.unwrap();
    (id, view.starts_at.unwrap())
}

/// The view of a race, which has to be there.
async fn seen(conn: &Connection, id: i64, viewer: i64, now: i64) -> RaceView {
    poll(conn, id, viewer, None, true, now)
        .await
        .unwrap()
        .expect("the race")
}

fn outcome(view: &RaceView, user: i64) -> RaceOutcome {
    view.players
        .iter()
        .find(|p| p.user_id == user)
        .unwrap()
        .outcome
}

#[tokio::test]
async fn a_race_goes_from_the_lobby_to_the_results() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;

    // aiko opens it, bram joins; chen watches
    let id = open(&conn, group, aiko, Difficulty::Easy, T0 - 60_000)
        .await
        .unwrap();
    let lobby = seen(&conn, id, chen, T0 - 50_000).await;
    assert_eq!(lobby.phase(), RacePhase::Lobby);
    assert_eq!(lobby.host, Some(aiko));
    assert_eq!(lobby.penalties, HelpPenalties::of(Difficulty::Easy));
    assert_eq!(lobby.players.len(), 1);
    assert!(!lobby.joined() && !lobby.can_start());
    // Alone, aiko can't start yet; only she can start it
    assert_eq!(
        error(begin(&conn, id, aiko, &small(), T0).await),
        ApiError::RaceTooFew
    );
    let joined = join(&conn, id, bram, T0 - 30_000).await.unwrap();
    assert_eq!(
        joined
            .players
            .iter()
            .map(|p| (p.user_id, p.name.as_str()))
            .collect::<Vec<_>>(),
        [(aiko, "aiko"), (bram, "bram")]
    );
    // Joining twice does nothing
    join(&conn, id, bram, T0 - 20_000).await.unwrap();
    assert_eq!(
        error(begin(&conn, id, bram, &small(), T0).await),
        ApiError::RaceHostOnly
    );

    // The start: the countdown, no puzzle before the start
    let counting = begin(&conn, id, aiko, &small(), T0).await.unwrap();
    let start = T0 + COUNTDOWN_MS;
    assert_eq!(counting.starts_at, Some(start));
    assert_eq!(counting.phase(), RacePhase::Countdown);
    assert_eq!(counting.puzzle, None);
    let before = seen(&conn, id, bram, start - 1).await;
    assert_eq!(
        (before.phase(), before.puzzle),
        (RacePhase::Countdown, None)
    );
    // Nobody joins or starts it again
    assert_eq!(
        error(join(&conn, id, chen, start - 1).await),
        ApiError::RaceStarted
    );
    assert_eq!(
        error(begin(&conn, id, aiko, &small(), start).await),
        ApiError::RaceStarted
    );
    // Nothing to do before the start
    assert_eq!(
        error(hint(&conn, id, bram, vec![0, 0], start - 1).await),
        ApiError::RaceNotRunning
    );

    // The puzzle at the start, for the players who ask for it; not for the watcher
    let running = seen(&conn, id, bram, start).await;
    assert_eq!(running.phase(), RacePhase::Running);
    assert_eq!(running.puzzle, Some(small().puzzle));
    assert_eq!(running.solution, None);
    let not_asked = poll(&conn, id, bram, None, false, start).await.unwrap();
    assert_eq!(not_asked.unwrap().puzzle, None);
    let watching = seen(&conn, id, chen, start).await;
    assert_eq!((watching.joined(), watching.puzzle), (false, None));
    // Watchers can't play
    assert_eq!(
        error(hint(&conn, id, chen, vec![0, 0], start + 1_000).await),
        ApiError::RaceNotJoined
    );

    // bram takes a hint and a check, aiko solves it first, bram after her
    assert_eq!(
        hint(&conn, id, bram, vec![0, 0], start + 2_000)
            .await
            .unwrap(),
        Some((0, 2))
    );
    assert_eq!(
        check(&conn, id, bram, vec![2, 0], start + 3_000)
            .await
            .unwrap(),
        Vec::<usize>::new()
    );
    let after_aiko = submit(&conn, id, aiko, SOLUTION.to_vec(), 3, log(), start + 20_000)
        .await
        .unwrap();
    assert_eq!(after_aiko.phase(), RacePhase::Running, "bram still races");
    assert_eq!(
        outcome(&after_aiko, aiko),
        RaceOutcome::Solved {
            time_ms: 20_000,
            place: Some(1)
        }
    );
    assert_eq!(after_aiko.players[0].progress, 100);
    // Done: no more help, no second finish
    assert_eq!(
        error(hint(&conn, id, aiko, vec![2, 1], start + 21_000).await),
        ApiError::RaceDone
    );
    let done = submit(&conn, id, bram, SOLUTION.to_vec(), 3, log(), start + 15_000)
        .await
        .unwrap();
    // Everyone done: over, with the results and the solution
    assert_eq!(done.phase(), RacePhase::Ended);
    assert_eq!(done.ended_at, Some(start + 15_000));
    let penalties = HelpPenalties::of(Difficulty::Easy);
    assert_eq!(
        outcome(&done, bram),
        RaceOutcome::Solved {
            time_ms: 15_000 + penalties.hint_ms + penalties.check_ms,
            place: Some(2)
        }
    );
    let (hints, checks) = (done.players[1].hints, done.players[1].checks);
    assert_eq!((hints, checks), (1, 1));
    let results = seen(&conn, id, chen, start + 30_000).await;
    assert_eq!(results.solution, Some(SOLUTION.to_vec()));
    // Everyone sees the board once it's over
    assert_eq!(results.puzzle, Some(small().puzzle));
    assert_eq!(
        error(hint(&conn, id, bram, vec![0, 0], start + 30_000).await),
        ApiError::RaceNotRunning
    );

    // The replay: a lane each, with the moves and the help
    let replay = replay(&conn, id, chen, start + 30_000).await.unwrap();
    assert!(!replay.locked);
    assert_eq!((replay.race, replay.group_puzzle), (Some(id), None));
    assert_eq!(replay.puzzle, Some(small().puzzle));
    let lanes: Vec<_> = replay
        .lanes
        .iter()
        .map(|l| (l.user_id, l.log.len(), l.marks.len(), l.official_rank))
        .collect();
    assert_eq!(lanes, [(aiko, 3, 0, Some(1)), (bram, 3, 2, Some(2))]);
    assert_eq!(replay.lanes[1].penalties, penalties);

    // On the group page: nothing open, the race in the history with its winner
    let races = list(&conn, group, chen, start + 30_000).await.unwrap();
    assert_eq!(races.open, None);
    assert_eq!(races.recent.len(), 1);
    let winner = races.recent[0].winner().unwrap();
    assert_eq!((winner.user_id, winner.time_ms), (aiko, Some(20_000)));
    assert!(!races.recent[0].joined);
}

#[tokio::test]
async fn the_generator_makes_the_puzzle_at_the_start() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    let id = open(&conn, group, aiko, Difficulty::Easy, T0)
        .await
        .unwrap();
    join(&conn, id, bram, T0).await.unwrap();
    let view = start(&conn, id, aiko, || T0 + 1_000).await.unwrap();
    assert_eq!(view.starts_at, Some(T0 + 1_000 + COUNTDOWN_MS));
    let puzzle = seen(&conn, id, bram, T0 + 1_000 + COUNTDOWN_MS)
        .await
        .puzzle
        .unwrap();
    assert_eq!((puzzle.width, puzzle.height), Difficulty::Easy.size());
    let solution: String = {
        let mut rows = conn
            .query(
                "SELECT solution FROM group_races WHERE id = ?1",
                params![id],
            )
            .await
            .unwrap();
        rows.next().await.unwrap().unwrap().get(0).unwrap()
    };
    let solution: Vec<u8> = serde_json::from_str(&solution).unwrap();
    assert!(Board::new(puzzle).check(&solution).solved);
}

#[tokio::test]
async fn outsiders_see_nothing_and_join_nothing() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let stranger = player(&conn, "stranger").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    assert_eq!(
        error(open(&conn, group, stranger, Difficulty::Easy, T0).await),
        ApiError::GroupNotFound
    );
    let id = open(&conn, group, aiko, Difficulty::Easy, T0)
        .await
        .unwrap();
    assert_eq!(
        poll(&conn, id, stranger, None, true, T0).await.unwrap(),
        None
    );
    assert_eq!(
        error(join(&conn, id, stranger, T0).await),
        ApiError::GroupNotFound
    );
    assert_eq!(
        error(list(&conn, group, stranger, T0).await),
        ApiError::GroupNotFound
    );
    join(&conn, id, bram, T0).await.unwrap();
    begin(&conn, id, aiko, &small(), T0).await.unwrap();
    let start = T0 + COUNTDOWN_MS;
    assert_eq!(
        poll(&conn, id, stranger, None, true, start).await.unwrap(),
        None
    );
    assert_eq!(
        error(hint(&conn, id, stranger, vec![0, 0], start).await),
        ApiError::GroupNotFound
    );
    assert_eq!(
        error(replay(&conn, id, stranger, start).await),
        ApiError::GroupNotFound
    );
    // A member who leaves the group drops off the race, and sees it no more
    leave_group(&conn, group, bram).await.unwrap();
    assert_eq!(
        poll(&conn, id, bram, None, true, start).await.unwrap(),
        None
    );
    let view = seen(&conn, id, aiko, start).await;
    assert_eq!(view.players.len(), 1);
    // No such race
    assert_eq!(
        poll(&conn, id + 100, aiko, None, true, start)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        error(join(&conn, id + 100, aiko, start).await),
        ApiError::RaceGone
    );
}

#[tokio::test]
async fn finishes_are_verified_and_flagged_ones_get_no_place() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    let (id, start) = started(&conn, group, aiko, &[bram, chen]).await;
    let at = start + 30_000;
    // Not solved, not of this puzzle, too many bridges
    assert_eq!(
        error(submit(&conn, id, aiko, vec![2, 0], 2, log(), at).await),
        ApiError::NotSolved
    );
    assert_eq!(
        error(submit(&conn, id, aiko, vec![2, 1, 0], 3, log(), at).await),
        ApiError::WrongPuzzle
    );
    assert_eq!(
        error(submit(&conn, id, aiko, vec![3, 1], 3, log(), at).await),
        ApiError::WrongPuzzle
    );
    // bram's three bridges 200 ms after the start: too fast for a person, no place
    let flagged = submit(&conn, id, bram, SOLUTION.to_vec(), 3, log(), start + 200)
        .await
        .unwrap();
    assert_eq!(
        outcome(&flagged, bram),
        RaceOutcome::Solved {
            time_ms: 200,
            place: None
        }
    );
    assert_eq!(flagged.players[1].flag, Some(Flag::TooFast));
    // A log that doesn't lead to the board
    let wrong_log = vec![MoveEvent {
        t: 1_000,
        edge: 0,
        count: 1,
        tap: true,
    }];
    let logless = submit(&conn, id, chen, SOLUTION.to_vec(), 1, wrong_log, at)
        .await
        .unwrap();
    assert_eq!(logless.players[2].flag, Some(Flag::Log));
    // aiko is slower than both, but first
    let done = submit(&conn, id, aiko, SOLUTION.to_vec(), 3, log(), at + 10_000)
        .await
        .unwrap();
    assert_eq!(
        outcome(&done, aiko),
        RaceOutcome::Solved {
            time_ms: 40_000,
            place: Some(1)
        }
    );
    assert_eq!(done.phase(), RacePhase::Ended);
    let summary = &list(&conn, group, aiko, at + 20_000).await.unwrap().recent[0];
    assert_eq!(summary.winner().map(|w| w.user_id), Some(aiko));
    assert_eq!(
        summary
            .entrants
            .iter()
            .filter(|e| e.place.is_some())
            .count(),
        1
    );
}

#[tokio::test]
async fn giving_up_is_out_without_the_solution_until_the_end() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    let (id, start) = started(&conn, group, aiko, &[bram]).await;
    let gave_up = give_up(&conn, id, bram, start + 5_000).await.unwrap();
    assert_eq!(outcome(&gave_up, bram), RaceOutcome::GaveUp);
    // aiko still races: no solution for bram yet
    assert_eq!(
        (gave_up.phase(), gave_up.solution),
        (RacePhase::Running, None)
    );
    assert_eq!(
        error(give_up(&conn, id, bram, start + 6_000).await),
        ApiError::RaceDone
    );
    let ended = give_up(&conn, id, aiko, start + 7_000).await.unwrap();
    assert_eq!(ended.phase(), RacePhase::Ended);
    assert_eq!(ended.solution, Some(SOLUTION.to_vec()));
    // Nobody won
    let races = list(&conn, group, aiko, start + 8_000).await.unwrap();
    assert_eq!(races.recent[0].winner(), None);
    let replay = replay(&conn, id, aiko, start + 8_000).await.unwrap();
    assert!(
        replay
            .lanes
            .iter()
            .all(|l| l.outcome == LaneOutcome::GaveUp)
    );
}

#[tokio::test]
async fn progress_is_capped_and_taken_once_a_second() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    let (id, start) = started(&conn, group, aiko, &[bram]).await;
    let progress = |view: &RaceView| view.players[1].progress;
    let view = poll(&conn, id, bram, Some(40), false, start + 1_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(progress(&view), 40);
    // Too soon after the last one: not taken
    let view = poll(&conn, id, bram, Some(60), false, start + 1_500)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(progress(&view), 40);
    // Never full before the finish, whatever is reported
    let view = poll(&conn, id, bram, Some(100), false, start + 2_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(progress(&view), PROGRESS_CAP);
    // A watcher's progress goes nowhere
    poll(&conn, id, chen, Some(5), false, start + 3_000)
        .await
        .unwrap();
    assert_eq!(
        number(
            &conn,
            "SELECT COUNT(*) FROM group_race_players WHERE progress = 5"
        )
        .await,
        0
    );
}

#[tokio::test]
async fn players_away_drop_out_and_may_come_back_while_it_runs() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    let (id, start) = started(&conn, group, aiko, &[bram, chen]).await;
    // Everyone is there at the start
    for (user, at) in [(aiko, start), (bram, start), (chen, start)] {
        poll(&conn, id, user, Some(0), true, at).await.unwrap();
    }
    // chen closes the tab; aiko and bram go on
    let later = start + AWAY_MS + 1_000;
    poll(&conn, id, bram, Some(10), false, later - 500)
        .await
        .unwrap();
    let view = seen(&conn, id, aiko, later).await;
    let away: Vec<_> = view.players.iter().map(|p| (p.user_id, p.away)).collect();
    assert_eq!(away, [(aiko, false), (bram, false), (chen, true)]);
    assert_eq!(view.phase(), RacePhase::Running);
    // chen comes back and plays on
    let back = seen(&conn, id, chen, later + 1_000).await;
    assert_eq!(back.phase(), RacePhase::Running);
    assert!(!back.players[2].away);
    hint(&conn, id, chen, vec![0, 0], later + 2_000)
        .await
        .unwrap();
    // chen leaves again for good; aiko and bram finish: over, chen out
    submit(&conn, id, aiko, SOLUTION.to_vec(), 3, log(), later + 3_000)
        .await
        .unwrap();
    let still = submit(&conn, id, bram, SOLUTION.to_vec(), 3, log(), later + 4_000)
        .await
        .unwrap();
    assert_eq!(still.phase(), RacePhase::Running, "chen was seen just now");
    let over = seen(&conn, id, aiko, later + 2_000 + AWAY_MS).await;
    assert_eq!(over.phase(), RacePhase::Ended);
    assert_eq!(over.ended_at, Some(later + 2_000 + AWAY_MS));
    assert_eq!(outcome(&over, chen), RaceOutcome::Out);
    // Too late to come back
    assert_eq!(
        error(hint(&conn, id, chen, vec![0, 0], later + 3_000 + AWAY_MS).await),
        ApiError::RaceNotRunning
    );
}

#[tokio::test]
async fn a_race_ends_half_an_hour_after_its_start() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    let (id, start) = started(&conn, group, aiko, &[bram]).await;
    // Both play on, seen every minute
    for minute in 0..30 {
        for user in [aiko, bram] {
            poll(&conn, id, user, Some(1), false, start + minute * 60_000)
                .await
                .unwrap();
        }
    }
    let running = seen(&conn, id, aiko, start + RACE_MS - 1).await;
    assert_eq!(running.phase(), RacePhase::Running);
    assert_eq!(
        error(
            submit(
                &conn,
                id,
                aiko,
                SOLUTION.to_vec(),
                3,
                log(),
                start + RACE_MS
            )
            .await
        ),
        ApiError::RaceNotRunning
    );
    let over = seen(&conn, id, aiko, start + RACE_MS + 5_000).await;
    assert_eq!(over.ended_at, Some(start + RACE_MS));
    assert!(over.players.iter().all(|p| p.outcome == RaceOutcome::Out));
}

#[tokio::test]
async fn one_race_at_a_time_per_group_and_lobbies_expire() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    let other = group_of(&conn, bram, &[]).await;
    let id = open(&conn, group, aiko, Difficulty::Hard, T0)
        .await
        .unwrap();
    assert_eq!(
        error(open(&conn, group, bram, Difficulty::Easy, T0 + 1).await),
        ApiError::RaceOpen
    );
    // Another group is another matter
    open(&conn, other, bram, Difficulty::Easy, T0)
        .await
        .unwrap();
    let listed = list(&conn, group, bram, T0 + 1).await.unwrap();
    let open_one = listed.open.unwrap();
    assert_eq!(
        (open_one.id, open_one.difficulty, open_one.joined),
        (id, Difficulty::Hard, false)
    );
    join(&conn, id, bram, T0 + 2).await.unwrap();

    // Nobody started it in 15 minutes: gone, a new one can be opened
    let expired = T0 + LOBBY_MS;
    assert_eq!(
        seen(&conn, id, aiko, expired - 1).await.phase(),
        RacePhase::Lobby
    );
    assert_eq!(
        poll(&conn, id, aiko, None, true, expired).await.unwrap(),
        None
    );
    assert_eq!(
        number(
            &conn,
            &format!("SELECT COUNT(*) FROM group_race_players WHERE race_id = {id}")
        )
        .await,
        0
    );
    assert_eq!(
        error(join(&conn, id, bram, expired).await),
        ApiError::RaceGone
    );
    let next = open(&conn, group, bram, Difficulty::Easy, expired + 1)
        .await
        .unwrap();
    assert_ne!(next, id);
    // The host calls the new one off
    leave(&conn, next, bram, expired + 2).await.unwrap();

    // The cleanup: the lobby in the other group nobody looked at goes, and a race nobody looked at
    // since everyone went is over
    let other_lobby = races_where(&conn, "group_id = ?1", params![other])
        .await
        .unwrap()[0]
        .id;
    let raced = open(&conn, group, aiko, Difficulty::Easy, expired + 10)
        .await
        .unwrap();
    join(&conn, raced, bram, expired + 10).await.unwrap();
    let start = begin(&conn, raced, aiko, &small(), expired + 20)
        .await
        .unwrap()
        .starts_at
        .unwrap();
    delete_expired(&conn, start + AWAY_MS + 1).await.unwrap();
    assert_eq!(load(&conn, other_lobby).await.unwrap(), None);
    assert_eq!(
        load(&conn, raced).await.unwrap().unwrap().ended_at,
        Some(start + AWAY_MS + 1)
    );
    // One whose time ran out ends at its deadline
    let late = open(&conn, group, bram, Difficulty::Easy, start)
        .await
        .unwrap();
    join(&conn, late, aiko, start).await.unwrap();
    let late_start = begin(&conn, late, bram, &small(), start)
        .await
        .unwrap()
        .starts_at
        .unwrap();
    delete_expired(&conn, late_start + RACE_MS + 5_000)
        .await
        .unwrap();
    assert_eq!(
        load(&conn, late).await.unwrap().unwrap().ended_at,
        Some(late_start + RACE_MS)
    );
}

#[tokio::test]
async fn players_leave_the_lobby_and_the_host_calls_it_off() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    let id = open(&conn, group, aiko, Difficulty::Easy, T0)
        .await
        .unwrap();
    join(&conn, id, bram, T0).await.unwrap();
    leave(&conn, id, bram, T0 + 1).await.unwrap();
    assert_eq!(seen(&conn, id, bram, T0 + 2).await.players.len(), 1);
    leave(&conn, id, aiko, T0 + 3).await.unwrap();
    assert_eq!(
        poll(&conn, id, bram, None, true, T0 + 4).await.unwrap(),
        None
    );
    // After the start nobody leaves, they give up
    let (id, start) = started(&conn, group, aiko, &[bram]).await;
    assert_eq!(
        error(leave(&conn, id, bram, start).await),
        ApiError::RaceStarted
    );
}

#[tokio::test]
async fn the_rematch_takes_everyone_on_the_results_to_one_new_lobby() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    let (id, start) = started(&conn, group, aiko, &[bram]).await;
    assert_eq!(
        error(rematch(&conn, id, bram, start).await),
        ApiError::RaceNotRunning,
        "not before it's over"
    );
    give_up(&conn, id, aiko, start + 1_000).await.unwrap();
    give_up(&conn, id, bram, start + 1_000).await.unwrap();
    let over = start + 2_000;
    assert_eq!(seen(&conn, id, chen, over).await.rematch, None);
    // bram asks first: his lobby, with the same settings
    let next = rematch(&conn, id, bram, over).await.unwrap();
    let view = seen(&conn, id, aiko, over + 1).await;
    assert_eq!((view.rematch, view.rematch_open), (Some(next), true));
    let lobby = seen(&conn, next, aiko, over + 1).await;
    assert_eq!(
        (lobby.host, lobby.difficulty, lobby.players.len()),
        (Some(bram), Difficulty::Easy, 1)
    );
    // aiko and chen (who only watched) join it from the results
    assert_eq!(rematch(&conn, id, aiko, over + 2).await.unwrap(), next);
    assert_eq!(rematch(&conn, id, chen, over + 3).await.unwrap(), next);
    assert_eq!(seen(&conn, next, aiko, over + 4).await.players.len(), 3);
    // Once it started, the old results say so, and nobody joins it from there
    begin(&conn, next, bram, &small(), over + 5).await.unwrap();
    let view = seen(&conn, id, aiko, over + 6).await;
    assert_eq!((view.rematch, view.rematch_open), (Some(next), false));
    assert_eq!(
        error(rematch(&conn, id, aiko, over + 6).await),
        ApiError::RaceStarted
    );
}

#[tokio::test]
async fn deleting_an_account_takes_its_lines_and_lobbies() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    // A finished race of bram's: aiko won, bram second
    let (old, start) = started(&conn, group, bram, &[aiko]).await;
    submit(
        &conn,
        old,
        aiko,
        SOLUTION.to_vec(),
        3,
        log(),
        start + 10_000,
    )
    .await
    .unwrap();
    submit(
        &conn,
        old,
        bram,
        SOLUTION.to_vec(),
        3,
        log(),
        start + 20_000,
    )
    .await
    .unwrap();
    // And a lobby of bram's that chen joined
    let lobby = open(&conn, group, bram, Difficulty::Easy, start + 30_000)
        .await
        .unwrap();
    join(&conn, lobby, chen, start + 30_000).await.unwrap();
    erase_user(&conn, bram).await.unwrap();
    assert_eq!(load(&conn, lobby).await.unwrap(), None);
    let race = load(&conn, old).await.unwrap().unwrap();
    assert_eq!(race.host_id, None);
    assert_eq!(
        number(
            &conn,
            &format!("SELECT COUNT(*) FROM group_race_players WHERE user_id = {bram}")
        )
        .await,
        0
    );
    // aiko keeps her win
    let races = list(&conn, group, aiko, start + 40_000).await.unwrap();
    assert_eq!(races.recent[0].entrants.len(), 1);
    assert_eq!(races.recent[0].winner().map(|w| w.user_id), Some(aiko));
    assert_eq!(races.open, None);
}

#[tokio::test]
async fn signing_in_brings_the_guests_races_along() {
    let conn = db().await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, bram, &[guest, aiko]).await;
    // The guest won a race against bram, and hosts the next lobby, which aiko joined on another
    // device
    let (old, start) = started(&conn, group, guest, &[bram]).await;
    submit(
        &conn,
        old,
        guest,
        SOLUTION.to_vec(),
        3,
        log(),
        start + 10_000,
    )
    .await
    .unwrap();
    give_up(&conn, old, bram, start + 11_000).await.unwrap();
    let lobby = open(&conn, group, guest, Difficulty::Easy, start + 20_000)
        .await
        .unwrap();
    join(&conn, lobby, aiko, start + 20_000).await.unwrap();
    merge_guest(&conn, aiko, guest).await.unwrap();
    for sql in MERGE_GUEST {
        conn.execute(sql, params![aiko, guest]).await.unwrap();
    }
    let view = seen(&conn, lobby, aiko, start + 21_000).await;
    assert_eq!(view.host, Some(aiko));
    assert_eq!(view.players.len(), 1, "one line for both");
    let races = list(&conn, group, aiko, start + 21_000).await.unwrap();
    assert_eq!(races.recent[0].winner().map(|w| w.user_id), Some(aiko));
    assert_eq!(
        number(
            &conn,
            &format!("SELECT COUNT(*) FROM group_race_players WHERE user_id = {guest}")
        )
        .await,
        0
    );
}

#[tokio::test]
async fn abandoned_guests_go_with_their_lobbies() {
    let conn = db().await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, bram, &[guest]).await;
    let lobby = open(&conn, group, guest, Difficulty::Easy, T0)
        .await
        .unwrap();
    join(&conn, lobby, bram, T0).await.unwrap();
    // Created long ago, no session
    assert_eq!(cleanup_guests(&conn).await.unwrap(), 1);
    assert_eq!(load(&conn, lobby).await.unwrap(), None);
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM group_race_players").await,
        0
    );
}

#[tokio::test]
async fn a_group_that_goes_takes_its_races_along() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    let (id, start) = started(&conn, group, aiko, &[bram]).await;
    give_up(&conn, id, aiko, start).await.unwrap();
    give_up(&conn, id, bram, start).await.unwrap();
    open(&conn, group, aiko, Difficulty::Easy, start + 1)
        .await
        .unwrap();
    leave_group(&conn, group, bram).await.unwrap();
    leave_group(&conn, group, aiko).await.unwrap();
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM groups").await, 0);
    for table in ["group_races", "group_race_players"] {
        assert_eq!(
            number(&conn, &format!("SELECT COUNT(*) FROM {table}")).await,
            0,
            "{table}"
        );
    }
}

#[tokio::test]
async fn blocked_players_are_nameless_in_races() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    crate::server::blocks::set(&conn, aiko, bram, true)
        .await
        .unwrap();
    let id = open(&conn, group, aiko, Difficulty::Easy, T0)
        .await
        .unwrap();
    join(&conn, id, bram, T0).await.unwrap();
    let names = |v: &RaceView| v.players.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
    assert_eq!(
        names(&seen(&conn, id, aiko, T0).await),
        ["aiko", BLOCKED_NAME]
    );
    assert_eq!(names(&seen(&conn, id, bram, T0).await), ["aiko", "bram"]);
}
