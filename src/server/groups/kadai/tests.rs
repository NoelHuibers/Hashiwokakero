//! Tests of group puzzles against a fresh database file.

use libsql::{Connection, params};

use super::*;
use crate::model::{MS_PER_DAY, MoveEvent, OPEN_DAYS, TodayState};
use crate::server::auth::{cleanup_guests, erase as erase_user};
use crate::server::db::TestConn;
use crate::server::groups::{create, join, leave, merge_guest};
use crate::server::now_ms;

async fn db() -> TestConn {
    TestConn::new("kadai").await
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

async fn invite_code_of(conn: &Connection, group: i64) -> Option<String> {
    let mut rows = conn
        .query("SELECT invite FROM groups WHERE id = ?1", params![group])
        .await
        .unwrap();
    rows.next().await.unwrap().map(|r| r.get(0).unwrap())
}

#[track_caller]
fn error<T: std::fmt::Debug>(result: Result<T, ServerFnError>) -> ApiError {
    let e = result.expect_err("should have failed");
    ApiError::from_message(&e.to_string()).unwrap_or_else(|| panic!("{e}"))
}

/// A group of `owner` that `others` joined, in that order.
async fn group_of(conn: &Connection, owner: i64, others: &[i64]) -> i64 {
    let id = create(conn, owner, "Puzzle Club").await.unwrap();
    let code = invite_code_of(conn, id).await.unwrap();
    for (i, &other) in others.iter().enumerate() {
        join(conn, &code, other).await.unwrap();
        conn.execute(
            "UPDATE group_members SET joined_at = joined_at + ?3 WHERE group_id = ?1 AND user_id = ?2",
            params![id, other, i64::try_from(i + 1).unwrap() * 1000],
        )
        .await
        .unwrap();
    }
    id
}

/// A 2 and a 3 joined by a double bridge, the 3 by a single one to a 1 below it: one
/// solution, `[2, 1]` (edge 0 the double, edge 1 the single).
fn small() -> Puzzle {
    Puzzle::from_grid(&[vec![2, 0, 3], vec![0, 0, 0], vec![0, 0, 1]])
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

/// Posted by `author` at `now`.
async fn posted(conn: &Connection, group: i64, author: i64, now: i64) -> i64 {
    post(conn, group, author, small(), "  Tiny   one ", now)
        .await
        .unwrap()
}

/// `player` solves it, starting at `at` and taking `ms`.
async fn solve(conn: &Connection, id: i64, player: i64, at: i64, ms: i64) -> PuzzlePage {
    start(conn, id, player, at).await.unwrap();
    submit(conn, id, player, SOLUTION.to_vec(), 3, log(), at + ms)
        .await
        .unwrap()
}

#[tokio::test]
async fn only_unique_puzzles_of_a_sensible_size_can_be_posted() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let stranger = player(&conn, "stranger").await;
    let group = group_of(&conn, aiko, &[]).await;
    let now = now_ms();

    // Four 3s in a square: a single and a double bridge on each side, two ways round
    let square = Puzzle::from_grid(&[vec![3, 0, 3], vec![0, 0, 0], vec![3, 0, 3]]);
    assert_eq!(
        error(post(&conn, group, aiko, square, "", now).await),
        ApiError::PuzzleNotUnique
    );
    // A 1 next to a 2 can never be solved
    let odd = Puzzle::from_grid(&[vec![1, 0, 2], vec![0, 0, 0]]);
    assert_eq!(
        error(post(&conn, group, aiko, odd, "", now).await),
        ApiError::PuzzleUnsolvable
    );
    let mut wide = Puzzle::empty(crate::model::MAX_SIDE + 1, 3);
    wide.set(common::Pos::new(0, 0), 1);
    wide.set(common::Pos::new(2, 0), 1);
    assert_eq!(
        error(post(&conn, group, aiko, wide, "", now).await),
        ApiError::PuzzleTooBig
    );
    let lonely = Puzzle::from_grid(&[vec![0, 0, 1], vec![0, 0, 0]]);
    assert_eq!(
        error(post(&conn, group, aiko, lonely, "", now).await),
        ApiError::WrongPuzzle
    );
    assert_eq!(
        error(post(&conn, group, aiko, small(), &"x".repeat(41), now).await),
        ApiError::PuzzleTitleInvalid
    );
    // Not in the group: no such group
    assert_eq!(
        error(post(&conn, group, stranger, small(), "", now).await),
        ApiError::GroupNotFound
    );
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM group_puzzles").await, 0);

    let id = posted(&conn, group, aiko, now).await;
    let stored = load(&conn, id).await.unwrap().unwrap();
    assert_eq!(stored.title.as_deref(), Some("Tiny one"));
    assert_eq!(stored.solution, SOLUTION);
    assert_eq!(stored.closes_at, now + OPEN_DAYS * MS_PER_DAY);
    assert_eq!(stored.author_id, Some(aiko));
}

#[tokio::test]
async fn todays_daily_cant_be_posted() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let group = group_of(&conn, aiko, &[]).await;
    // Today's daily is the small puzzle, turned around
    let turned = Puzzle::from_grid(&[vec![1, 0, 0], vec![0, 0, 0], vec![3, 0, 2]]);
    conn.execute(
        "INSERT INTO dailies (day, difficulty, puzzle, solution, created_at) VALUES (?1, 'easy', ?2, '[1, 2]', 0)",
        params![crate::server::daily::today(), serde_json::to_string(&turned).unwrap()],
    )
    .await
    .unwrap();
    assert_eq!(
        error(post(&conn, group, aiko, small(), "", now_ms()).await),
        ApiError::DailyLocked
    );
}

#[tokio::test]
async fn a_member_has_a_few_open_puzzles_at_most() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    let other = group_of(&conn, aiko, &[]).await;
    let now = now_ms();
    for i in 0..MAX_OPEN {
        posted(&conn, group, aiko, now + count(i)).await;
    }
    assert_eq!(
        list(&conn, group, aiko, now + 10).await.unwrap().can_post,
        0
    );
    assert_eq!(
        error(post(&conn, group, aiko, small(), "", now + 10).await),
        ApiError::TooManyPuzzles
    );
    // Others still can, and so can aiko in another group
    assert_eq!(
        list(&conn, group, bram, now + 10).await.unwrap().can_post,
        MAX_OPEN
    );
    posted(&conn, group, bram, now + 10).await;
    posted(&conn, other, aiko, now + 10).await;
    // Once the first closes, there's room again
    let later = closes_at(now);
    assert_eq!(list(&conn, group, aiko, later).await.unwrap().can_post, 1);
    posted(&conn, group, aiko, later).await;
}

#[tokio::test]
async fn the_author_watches_and_the_others_play() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    let now = now_ms();
    let id = posted(&conn, group, aiko, now).await;

    // The author can't play, sees the puzzle with its solution and everyone's progress
    assert_eq!(
        error(start(&conn, id, aiko, now + 1).await),
        ApiError::PuzzleOwn
    );
    let mine = page(&conn, id, aiko, now + 1).await.unwrap();
    assert_eq!(mine.summary.mine, MyPuzzle::Author);
    assert_eq!(mine.puzzle, Some(small()));
    assert_eq!(mine.solution.as_deref(), Some(&SOLUTION[..]));
    assert!(mine.board_open);
    let seen: Vec<_> = mine.board.iter().map(|e| (e.user_id, e.state)).collect();
    assert_eq!(
        seen,
        [(bram, TodayState::Waiting), (chen, TodayState::Waiting)]
    );
    assert_eq!(mine.stats.as_ref().map(|s| s.players), Some(2));

    // Before the start: no puzzle, nothing to play with
    let fresh = page(&conn, id, bram, now + 1).await.unwrap();
    assert_eq!(fresh.summary.mine, MyPuzzle::Fresh);
    assert_eq!(
        (fresh.puzzle, fresh.solution, fresh.stats),
        (None, None, None)
    );
    assert_eq!(
        error(hint(&conn, id, bram, vec![0, 0], now + 1).await),
        ApiError::NotStarted
    );
    assert_eq!(
        error(submit(&conn, id, bram, SOLUTION.to_vec(), 3, log(), now + 1).await),
        ApiError::NotStarted
    );

    // The clock starts once, the puzzle comes with it
    let started = start(&conn, id, bram, now + 1_000).await.unwrap();
    assert_eq!(started.puzzle, Some(small()));
    assert_eq!(started.solution, None);
    assert_eq!(started.summary.mine, MyPuzzle::Playing);
    let again = start(&conn, id, bram, now + 9_000).await.unwrap();
    assert_eq!(again.attempt.map(|a| a.started_at), Some(now + 1_000));

    // A hint (30 s), a check finding a wrong double bridge (10 s), then solved in 60 s
    assert_eq!(
        hint(&conn, id, bram, vec![0, 0], now + 5_000)
            .await
            .unwrap(),
        Some((0, 2))
    );
    assert_eq!(
        check(&conn, id, bram, vec![2, 2], now + 6_000)
            .await
            .unwrap(),
        vec![1]
    );
    assert_eq!(
        error(hint(&conn, id, bram, vec![0], now + 7_000).await),
        ApiError::WrongPuzzle
    );
    assert_eq!(
        error(submit(&conn, id, bram, vec![1, 1], 3, log(), now + 61_000).await),
        ApiError::NotSolved
    );
    let done = submit(&conn, id, bram, SOLUTION.to_vec(), 3, log(), now + 61_000)
        .await
        .unwrap();
    assert_eq!(
        done.summary.mine,
        MyPuzzle::Solved {
            time_ms: 60_000 + 30_000 + 10_000,
            rank: Some(1)
        }
    );
    assert!(done.board_open);
    assert_eq!(
        error(submit(&conn, id, bram, SOLUTION.to_vec(), 3, log(), now + 62_000).await),
        ApiError::AlreadyFinished
    );
    assert_eq!(
        error(hint(&conn, id, bram, vec![0, 0], now + 62_000).await),
        ApiError::AlreadyFinished
    );

    // chen hasn't played: the board says who is done, not how
    let waiting = page(&conn, id, chen, now + 62_000).await.unwrap();
    assert!(!waiting.board_open);
    let seen: Vec<_> = waiting.board.iter().map(|e| (e.user_id, e.state)).collect();
    assert_eq!(
        seen,
        [(bram, TodayState::Done), (chen, TodayState::Waiting)]
    );
    // A quicker solve without help goes first
    let quick = solve(&conn, id, chen, now + 70_000, 40_000).await;
    let seen: Vec<_> = quick.board.iter().map(|e| (e.user_id, e.state)).collect();
    assert_eq!(
        seen,
        [
            (
                chen,
                TodayState::Ranked {
                    rank: 1,
                    time_ms: 40_000,
                    hints: 0,
                    checks: 0
                }
            ),
            (
                bram,
                TodayState::Ranked {
                    rank: 2,
                    time_ms: 100_000,
                    hints: 1,
                    checks: 1
                }
            ),
        ]
    );
    assert_eq!(quick.my_rank(), Some(1));

    // The author's view: where the help went, the times
    let stats = page(&conn, id, aiko, now + 200_000)
        .await
        .unwrap()
        .stats
        .unwrap();
    assert_eq!(
        (stats.players, stats.solved, stats.gave_up, stats.playing),
        (2, 2, 0, 0)
    );
    assert_eq!(
        (stats.fastest_ms, stats.median_ms),
        (Some(40_000), Some(40_000))
    );
    assert_eq!((stats.hints, stats.checks), (1, 1));
    assert_eq!(
        stats.hardest,
        Some(crate::model::Hardest {
            edge: 0,
            hints: 1,
            wrong: 0
        })
    );
}

#[tokio::test]
async fn a_doubtful_log_or_giving_up_isnt_ranked() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    let now = now_ms();
    let id = posted(&conn, group, aiko, now).await;

    start(&conn, id, bram, now).await.unwrap();
    // No move log: solved, not on the board
    let done = submit(&conn, id, bram, SOLUTION.to_vec(), 3, vec![], now + 30_000)
        .await
        .unwrap();
    assert_eq!(
        done.summary.mine,
        MyPuzzle::Solved {
            time_ms: 30_000,
            rank: None
        }
    );
    assert_eq!(done.attempt.and_then(|a| a.flag), Some(Flag::Log));

    start(&conn, id, chen, now).await.unwrap();
    let given_up = reveal(&conn, id, chen, now + 5_000).await.unwrap();
    assert_eq!(given_up.summary.mine, MyPuzzle::GaveUp);
    assert_eq!(
        given_up.attempt.and_then(|a| a.bridges),
        Some(SOLUTION.to_vec())
    );
    assert!(given_up.board_open);
    let states: Vec<_> = given_up.board.iter().map(|e| e.state).collect();
    assert_eq!(states, [TodayState::Unranked, TodayState::Unranked]);
    // Solved counts the solves, not the give ups
    let summary = given_up.summary;
    assert_eq!((summary.solved(), summary.players()), (1, 2));
}

#[tokio::test]
async fn after_a_week_it_closes_with_the_final_board_and_applause() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    let now = now_ms();
    let id = posted(&conn, group, aiko, now).await;
    solve(&conn, id, bram, now + 1_000, 50_000).await;
    // chen started, and didn't finish in time
    start(&conn, id, chen, now + 2_000).await.unwrap();

    let open = list(&conn, group, chen, now + 3_000).await.unwrap();
    assert_eq!(open.open.len(), 1);
    assert_eq!(open.closed.len(), 0);
    let row = &open.open[0];
    assert_eq!(row.title.as_deref(), Some("Tiny one"));
    assert_eq!(row.author.as_ref().map(|a| a.user_id), Some(aiko));
    assert_eq!((row.width, row.height, row.islands), (3, 3, 3));
    assert_eq!((row.solved(), row.players()), (1, 2));
    assert_eq!(row.mine, MyPuzzle::Playing);
    assert_eq!(row.board, None);
    assert_eq!(row.winner, None);
    assert_eq!(open.kudos.len(), 0);

    let closed = closes_at(now);
    assert_eq!(
        error(submit(&conn, id, chen, SOLUTION.to_vec(), 3, log(), closed).await),
        ApiError::PuzzleClosed
    );
    assert_eq!(
        error(start(&conn, id, chen, closed).await),
        ApiError::PuzzleClosed
    );
    assert_eq!(
        error(withdraw(&conn, id, aiko, closed).await),
        ApiError::PuzzleClosed
    );
    // Everyone sees the solution and the times now
    let last = page(&conn, id, chen, closed).await.unwrap();
    assert_eq!(last.solution.as_deref(), Some(&SOLUTION[..]));
    assert!(last.board_open && !last.summary.open);
    assert!(last.stats.is_some());
    let archive = list(&conn, group, chen, closed).await.unwrap();
    assert_eq!(archive.open.len(), 0);
    let row = &archive.closed[0];
    assert_eq!(row.winner, Some(("bram".into(), 50_000)));
    assert_eq!(
        row.board.as_ref().map(|b| b.solution.clone()),
        Some(SOLUTION.to_vec())
    );
    assert_eq!(archive.kudos.len(), 0, "only for the author");

    // The author's applause, until they've seen it
    let applause = list(&conn, group, aiko, closed).await.unwrap().kudos;
    assert_eq!(applause.len(), 1);
    assert_eq!(applause[0].puzzle.id, id);
    assert_eq!(
        (applause[0].stats.solved, applause[0].stats.playing),
        (1, 1)
    );
    // Nobody else can make it go
    seen_kudos(&conn, id, bram, closed).await.unwrap();
    assert_eq!(
        list(&conn, group, aiko, closed).await.unwrap().kudos.len(),
        1
    );
    seen_kudos(&conn, id, aiko, closed).await.unwrap();
    assert_eq!(
        list(&conn, group, aiko, closed).await.unwrap().kudos.len(),
        0
    );
}

#[tokio::test]
async fn only_the_author_withdraws_an_open_puzzle() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    let now = now_ms();
    let id = posted(&conn, group, aiko, now).await;
    solve(&conn, id, bram, now, 20_000).await;
    assert_eq!(
        error(withdraw(&conn, id, bram, now + 1).await),
        ApiError::PuzzleAuthorOnly
    );
    withdraw(&conn, id, aiko, now + 1).await.unwrap();
    assert_eq!(load(&conn, id).await.unwrap(), None);
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM group_puzzle_attempts").await,
        0
    );
    assert_eq!(
        error(page(&conn, id, bram, now + 2).await),
        ApiError::PuzzleClosed
    );
}

#[tokio::test]
async fn outsiders_and_former_members_see_nothing() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let stranger = player(&conn, "stranger").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    let now = now_ms();
    let id = posted(&conn, group, aiko, now).await;
    for result in [
        page(&conn, id, stranger, now).await.map(|_| ()),
        start(&conn, id, stranger, now).await.map(|_| ()),
        list(&conn, group, stranger, now).await.map(|_| ()),
    ] {
        assert_eq!(error(result), ApiError::GroupNotFound);
    }
    // The author left: their puzzle stays, without their name; their solves drop off
    solve(&conn, id, bram, now, 20_000).await;
    leave(&conn, group, aiko).await.unwrap();
    let after = page(&conn, id, bram, now + 1).await.unwrap();
    assert_eq!(after.summary.author, None);
    assert_eq!(after.board.len(), 1);
    leave(&conn, group, bram).await.unwrap();
    // The last one left: the group went, and its puzzles with it
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM group_puzzles").await, 0);
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM group_puzzle_attempts").await,
        0
    );
}

#[tokio::test]
async fn deleting_an_account_takes_its_open_puzzles_and_attempts() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    let now = now_ms();
    // A week ago: closed now, solved by bram. And an open one, chen is on it.
    let old = posted(&conn, group, aiko, now - 8 * MS_PER_DAY).await;
    solve(&conn, old, bram, now - 8 * MS_PER_DAY, 20_000).await;
    let open = posted(&conn, group, aiko, now).await;
    start(&conn, open, chen, now).await.unwrap();
    // bram's own puzzle, which aiko solved
    let brams = posted(&conn, group, bram, now).await;
    solve(&conn, brams, aiko, now, 30_000).await;

    erase_user(&conn, aiko).await.unwrap();
    // The open one went with chen's attempt, the closed one stays without a name
    assert_eq!(load(&conn, open).await.unwrap(), None);
    let kept = load(&conn, old).await.unwrap().unwrap();
    assert_eq!(kept.author_id, None);
    assert_eq!(
        number(
            &conn,
            &format!("SELECT COUNT(*) FROM group_puzzle_attempts WHERE user_id = {aiko}")
        )
        .await,
        0
    );
    assert_eq!(
        number(
            &conn,
            &format!("SELECT COUNT(*) FROM group_puzzle_attempts WHERE puzzle_id = {open}")
        )
        .await,
        0
    );
    let archive = page(&conn, old, bram, now).await.unwrap();
    assert_eq!(archive.summary.author, None);
    assert_eq!(
        archive.summary.mine,
        MyPuzzle::Solved {
            time_ms: 20_000,
            rank: Some(1)
        }
    );
    let theirs = page(&conn, brams, bram, now).await.unwrap();
    assert_eq!(theirs.stats.map(|s| s.solved), Some(0));
}

/// Turso refuses a statement given more parameters than it uses, which local SQLite lets through:
/// deleting an account failed on the site with "Number of arguments mismatch: expected 1, got 2".
/// Each statement of the deletion and of a guest's sign-in takes exactly what it's given.
#[tokio::test]
async fn each_statement_takes_the_parameters_it_is_given() {
    let conn = db().await;
    for (statements, given) in [
        (&ERASE_OPEN[..], 2),
        (&ERASE[..], 1),
        (&MERGE_GUEST[..], 2),
        (&crate::server::groups::ERASE[..], 1),
        (&crate::server::groups::hanko::ERASE[..], 1),
        (&crate::server::groups::hanko::MERGE_GUEST[..], 2),
        (&crate::server::blocks::ERASE[..], 1),
        (&crate::server::blocks::MERGE_GUEST[..], 2),
        (&crate::server::reports::ERASE[..], 1),
        (&crate::server::reports::MERGE_GUEST[..], 2),
        (&crate::server::contact::ERASE[..], 1),
    ] {
        for sql in statements {
            let statement = conn.prepare(sql).await.unwrap();
            assert_eq!(statement.parameter_count(), given, "{sql}");
        }
    }
}

#[tokio::test]
async fn abandoned_guests_go_with_their_puzzles() {
    let conn = db().await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, bram, &[guest]).await;
    let now = now_ms();
    let id = posted(&conn, group, guest, now).await;
    solve(&conn, id, bram, now, 20_000).await;
    // Created long ago, no session
    assert_eq!(cleanup_guests(&conn).await.unwrap(), 1);
    assert_eq!(load(&conn, id).await.unwrap(), None);
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM group_puzzle_attempts").await,
        0
    );
}

#[tokio::test]
async fn signing_in_brings_the_guests_puzzles_and_solves_along() {
    let conn = db().await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, bram, &[guest, aiko]).await;
    let now = now_ms();
    let guests = posted(&conn, group, guest, now).await;
    let brams = posted(&conn, group, bram, now).await;
    solve(&conn, brams, guest, now, 20_000).await;
    // aiko had started the guest's puzzle on another device: she can't play her own now
    start(&conn, guests, aiko, now).await.unwrap();
    merge_guest(&conn, aiko, guest).await.unwrap();
    for sql in MERGE_GUEST {
        conn.execute(sql, params![aiko, guest]).await.unwrap();
    }
    assert_eq!(
        load(&conn, guests).await.unwrap().unwrap().author_id,
        Some(aiko)
    );
    let mine = page(&conn, guests, aiko, now + 1).await.unwrap();
    assert_eq!(mine.summary.mine, MyPuzzle::Author);
    assert_eq!(mine.attempt, None);
    let solved = page(&conn, brams, aiko, now + 1).await.unwrap();
    assert_eq!(
        solved.summary.mine,
        MyPuzzle::Solved {
            time_ms: 20_000,
            rank: Some(1)
        }
    );
    assert_eq!(
        number(
            &conn,
            &format!("SELECT COUNT(*) FROM group_puzzle_attempts WHERE user_id = {guest}")
        )
        .await,
        0
    );
}

/// The solvers don't help with an open group puzzle before you're done with it, however it's
/// turned, nor with today's daily.
#[tokio::test]
async fn the_solvers_wait_until_youre_done() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let stranger = player(&conn, "stranger").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    let now = now_ms();
    let id = posted(&conn, group, aiko, now).await;
    let turned = Puzzle::from_grid(&[vec![1, 0, 0], vec![0, 0, 0], vec![3, 0, 2]]);
    assert!(locked(&conn, Some(bram), &turned, now).await.unwrap());
    assert!(locked(&conn, Some(bram), &small(), now).await.unwrap());
    // The author, an outsider, nobody, another puzzle
    assert!(!locked(&conn, Some(aiko), &small(), now).await.unwrap());
    assert!(!locked(&conn, Some(stranger), &small(), now).await.unwrap());
    assert!(!locked(&conn, None, &small(), now).await.unwrap());
    let other = Puzzle::from_grid(&[vec![1, 0, 1]]);
    assert!(!locked(&conn, Some(bram), &other, now).await.unwrap());
    // Started isn't done
    start(&conn, id, bram, now).await.unwrap();
    assert!(locked(&conn, Some(bram), &small(), now).await.unwrap());
    submit(&conn, id, bram, SOLUTION.to_vec(), 3, log(), now + 20_000)
        .await
        .unwrap();
    assert!(!locked(&conn, Some(bram), &small(), now).await.unwrap());
    // Closed: the solution is out anyway
    let chen = player(&conn, "chen").await;
    let code = invite_code_of(&conn, group).await.unwrap();
    join(&conn, &code, chen).await.unwrap();
    assert!(locked(&conn, Some(chen), &small(), now).await.unwrap());
    assert!(
        !locked(&conn, Some(chen), &small(), closes_at(now))
            .await
            .unwrap()
    );
}

/// The replay theatre plays the members' solves of a puzzle once the viewer is done with it.
#[tokio::test]
async fn the_replay_opens_once_youre_done() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    let now = now_ms();
    let id = posted(&conn, group, aiko, now).await;
    start(&conn, id, bram, now).await.unwrap();
    hint(&conn, id, bram, vec![0, 0], now + 2_000)
        .await
        .unwrap();
    submit(&conn, id, bram, SOLUTION.to_vec(), 3, log(), now + 20_000)
        .await
        .unwrap();
    // chen hasn't played: nothing to see yet
    let locked = replay(&conn, id, chen, now + 30_000).await.unwrap();
    assert!(locked.locked);
    assert_eq!((locked.puzzle, locked.lanes.len()), (None, 0));
    assert_eq!(locked.group_puzzle, Some(id));
    // The author watches: a lane per player, bram's with his moves and his hint
    let open = replay(&conn, id, aiko, now + 30_000).await.unwrap();
    assert!(!open.locked);
    assert_eq!(open.puzzle, Some(small()));
    let lanes: Vec<_> = open
        .lanes
        .iter()
        .map(|l| (l.user_id, l.log.len(), l.marks.len()))
        .collect();
    assert_eq!(lanes, [(bram, 3, 1), (chen, 0, 0)]);
    assert_eq!(open.lanes[1].outcome, LaneOutcome::Missing);
    assert_eq!(
        error(replay(&conn, id, player(&conn, "stranger").await, now).await),
        ApiError::GroupNotFound
    );
}
