//! Tests of shared puzzles against a fresh database file.

use libsql::{Connection, params};

use super::*;
use crate::server::auth::{cleanup_guests, erase as erase_user};
use crate::server::db::TestConn;
use crate::server::now_ms;

async fn db() -> TestConn {
    TestConn::new("shares").await
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

/// A 2 and a 3 joined by a double bridge, the 3 by a single one to a 1 below it: one
/// solution, `[2, 1]`.
fn small() -> Puzzle {
    Puzzle::from_grid(&[vec![2, 0, 3], vec![0, 0, 0], vec![0, 0, 1]])
}

const SOLUTION: [u8; 2] = [2, 1];

/// [`small`] solved in `ms` with `hints`.
fn solved(ms: i64, hints: u32) -> SharedSolve {
    SharedSolve {
        bridges: SOLUTION.to_vec(),
        time_ms: ms,
        moves: 2,
        hints,
    }
}

async fn page_of(conn: &Connection, code: &str, viewer: Option<i64>) -> SharedPuzzlePage {
    page(conn, code, viewer).await.unwrap().unwrap()
}

#[tokio::test]
async fn a_puzzle_is_stored_once_with_a_link_per_sharer() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let now = now_ms();
    let code = share(&conn, aiko, small(), Some(solved(61_000, 0)), now)
        .await
        .unwrap();
    assert!(is_share_code(&code), "{code}");
    // Again: the same link, the first result stays
    let again = share(&conn, aiko, small(), Some(solved(30_000, 0)), now + 1)
        .await
        .unwrap();
    assert_eq!(again, code);
    // From the workshop, the islands in another order: still the same puzzle
    let mut shuffled = small();
    shuffled.islands.reverse();
    let bram_code = share(&conn, bram, shuffled, None, now + 2).await.unwrap();
    assert_ne!(bram_code, code);
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM shared_puzzles").await, 1);
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM puzzle_shares").await, 2);
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM shared_results").await,
        1
    );

    let page = page_of(&conn, &code, None).await;
    assert_eq!(page.puzzle, small());
    let sharer = page.sharer.unwrap();
    assert_eq!((sharer.user_id, sharer.name.as_str()), (aiko, "aiko"));
    let result = sharer.result.unwrap();
    assert_eq!((result.time_ms, result.score_ms, result.rank), (61_000, 61_000, 1));
    assert_eq!(page.solvers, 1);
    assert_eq!(page.mine, None);
    // bram's link: shared without playing, aiko's time is on its board too
    let page = page_of(&conn, &bram_code, Some(bram)).await;
    assert_eq!(page.sharer.unwrap().result, None);
    assert_eq!(page.board.len(), 1);
    assert_eq!(page.board[0].user_id, aiko);

    // Not a code, or nobody's
    assert_eq!(super::page(&conn, "../admin", None).await.unwrap(), None);
    assert_eq!(super::page(&conn, "aaaaaaaa", None).await.unwrap(), None);
    assert_eq!(puzzle_hash(&small()), puzzle_hash(&{
        let mut p = small();
        p.islands.reverse();
        p
    }));
}

#[tokio::test]
async fn only_puzzles_with_one_solution_can_be_shared() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let now = now_ms();
    let square = Puzzle::from_grid(&[vec![3, 0, 3], vec![0, 0, 0], vec![3, 0, 3]]);
    assert_eq!(
        error(share(&conn, aiko, square, None, now).await),
        ApiError::PuzzleNotUnique
    );
    let odd = Puzzle::from_grid(&[vec![1, 0, 2], vec![0, 0, 0]]);
    assert_eq!(
        error(share(&conn, aiko, odd, None, now).await),
        ApiError::PuzzleUnsolvable
    );
    let lonely = Puzzle::from_grid(&[vec![0, 0, 1], vec![0, 0, 0]]);
    assert_eq!(
        error(share(&conn, aiko, lonely, None, now).await),
        ApiError::WrongPuzzle
    );
    // A board that isn't solved, a solve that is too quick to be one
    assert_eq!(
        error(share(&conn, aiko, small(), Some(SharedSolve { bridges: vec![2, 0], ..solved(9_000, 0) }), now).await),
        ApiError::NotSolved
    );
    assert_eq!(
        error(share(&conn, aiko, small(), Some(SharedSolve { moves: 1, ..solved(9_000, 0) }), now).await),
        ApiError::WrongPuzzle
    );
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM shared_puzzles").await, 0);
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM puzzle_shares").await, 0);
}

#[tokio::test]
async fn todays_daily_and_open_group_puzzles_cant_be_shared() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let now = now_ms();
    // An open group puzzle, the small one turned around
    let turned = Puzzle::from_grid(&[vec![1, 0, 0], vec![0, 0, 0], vec![3, 0, 2]]);
    conn.execute(
        "INSERT INTO group_puzzles (group_id, author_id, puzzle, solution, created_at, closes_at)
         VALUES (1, 1, ?1, '[1, 2]', 0, ?2)",
        params![serde_json::to_string(&turned).unwrap(), now + 1000],
    )
    .await
    .unwrap();
    assert_eq!(
        error(share(&conn, aiko, small(), None, now).await),
        ApiError::PuzzleLocked
    );
    // Once it closed it can
    share(&conn, aiko, small(), None, now + 1000).await.unwrap();

    let other = Puzzle::from_grid(&[vec![1, 0, 1], vec![0, 0, 0]]);
    conn.execute(
        "INSERT INTO dailies (day, difficulty, puzzle, solution, created_at) VALUES (?1, 'easy', ?2, '[1]', 0)",
        params![crate::server::daily::today(), serde_json::to_string(&other).unwrap()],
    )
    .await
    .unwrap();
    assert_eq!(
        error(share(&conn, aiko, other, None, now).await),
        ApiError::DailyLocked
    );
}

#[tokio::test]
async fn a_player_makes_a_limited_number_of_links_a_day() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let now = now_ms();
    let code = share(&conn, aiko, small(), None, now).await.unwrap();
    for i in 1..MAX_SHARES_PER_DAY {
        conn.execute(
            "INSERT INTO puzzle_shares (code, puzzle_id, user_id, created_at) VALUES (?1, 1, ?2, ?3)",
            params![format!("cap{i:05}"), aiko, now],
        )
        .await
        .unwrap();
    }
    let other = Puzzle::from_grid(&[vec![1, 0, 1], vec![0, 0, 0]]);
    assert_eq!(
        error(share(&conn, aiko, other.clone(), None, now).await),
        ApiError::TooManyAttempts
    );
    // The links made are still theirs, and the next day there is room again
    assert_eq!(share(&conn, aiko, small(), None, now).await.unwrap(), code);
    share(&conn, aiko, other, None, now + MS_PER_DAY + 1)
        .await
        .unwrap();
}

#[tokio::test]
async fn only_the_first_solve_counts() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let now = now_ms();
    let code = share(&conn, aiko, small(), Some(solved(61_000, 0)), now)
        .await
        .unwrap();
    // bram is quicker with a hint: 40 s + 30 s
    let (page, counted) = submit(&conn, &code, bram, &solved(40_000, 1), now + 10)
        .await
        .unwrap();
    assert!(counted);
    let mine = page.mine.unwrap();
    assert_eq!((mine.time_ms, mine.hints, mine.score_ms, mine.rank), (40_000, 1, 70_000, 2));
    // Again, faster: nothing changes
    let (page, counted) = submit(&conn, &code, bram, &solved(5_000, 0), now + 20)
        .await
        .unwrap();
    assert!(!counted);
    assert_eq!(page.mine.unwrap().score_ms, 70_000);
    // chen ties with aiko: a shared place
    submit(&conn, &code, chen, &solved(61_000, 0), now + 30)
        .await
        .unwrap();
    let page = page_of(&conn, &code, Some(chen)).await;
    let places: Vec<(i64, i64)> = page.board.iter().map(|e| (e.user_id, e.rank)).collect();
    assert_eq!(places, [(aiko, 1), (chen, 1), (bram, 3)]);
    assert_eq!(page.solvers, 3);
    // A board that isn't solved doesn't count, nor does one through no link
    assert_eq!(
        error(submit(&conn, &code, chen, &SharedSolve { bridges: vec![1, 1], ..solved(1, 0) }, now).await),
        ApiError::NotSolved
    );
    assert_eq!(
        error(submit(&conn, "aaaaaaaa", chen, &solved(1, 0), now).await),
        ApiError::WrongPuzzle
    );
}

#[tokio::test]
async fn the_board_shows_the_best_places_the_viewer_and_the_sharer() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let now = now_ms();
    // aiko took her time
    let code = share(&conn, aiko, small(), Some(solved(3_600_000, 0)), now)
        .await
        .unwrap();
    for i in 0..SHARE_BOARD_SHOWN + 5 {
        let other = player(&conn, &format!("p{i}")).await;
        submit(&conn, &code, other, &solved(10_000 + i, 0), now)
            .await
            .unwrap();
    }
    let slow = player(&conn, "slow").await;
    submit(&conn, &code, slow, &solved(7_200_000, 0), now)
        .await
        .unwrap();
    let page = page_of(&conn, &code, Some(slow)).await;
    assert_eq!(page.board.len(), usize::try_from(SHARE_BOARD_SHOWN).unwrap());
    assert_eq!(page.solvers, SHARE_BOARD_SHOWN + 7);
    assert_eq!(page.mine.unwrap().rank, SHARE_BOARD_SHOWN + 7);
    assert_eq!(page.sharer.unwrap().result.unwrap().rank, SHARE_BOARD_SHOWN + 6);
}

#[tokio::test]
async fn other_players_reached_count_for_the_sharer() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let now = now_ms();
    let code = share(&conn, aiko, small(), Some(solved(61_000, 0)), now)
        .await
        .unwrap();
    // Her own solve isn't anyone reached
    assert_eq!(players_reached(&conn, aiko).await.unwrap(), 0);
    submit(&conn, &code, bram, &solved(40_000, 0), now).await.unwrap();
    submit(&conn, &code, bram, &solved(40_000, 0), now).await.unwrap();
    assert_eq!(players_reached(&conn, aiko).await.unwrap(), 1);
    // chen shares it too, bram's solve stays aiko's
    let chen_code = share(&conn, chen, small(), None, now).await.unwrap();
    submit(&conn, &chen_code, bram, &solved(1_000, 0), now).await.unwrap();
    assert_eq!(players_reached(&conn, chen).await.unwrap(), 0);
    // aiko solving chen's link is chen's
    let other = Puzzle::from_grid(&[vec![1, 0, 1], vec![0, 0, 0]]);
    let other_code = share(&conn, chen, other, None, now).await.unwrap();
    let one = SharedSolve {
        bridges: vec![1],
        time_ms: 2_000,
        moves: 1,
        hints: 0,
    };
    submit(&conn, &other_code, aiko, &one, now).await.unwrap();
    submit(&conn, &other_code, bram, &one, now).await.unwrap();
    assert_eq!(players_reached(&conn, chen).await.unwrap(), 2);
    // Banned players don't count
    conn.execute("UPDATE users SET banned_at = 1 WHERE id = ?1", params![bram])
        .await
        .unwrap();
    assert_eq!(players_reached(&conn, aiko).await.unwrap(), 0);
    assert_eq!(players_reached(&conn, chen).await.unwrap(), 1);
    assert_eq!(page_of(&conn, &code, None).await.solvers, 1);
}

#[tokio::test]
async fn results_the_server_checked_go_on_the_board() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let now = now_ms();
    // After a race: the puzzle shared, the racers' times added
    let code = create(&conn, aiko, small(), now).await.unwrap();
    let time = |time_ms| Verified {
        time_ms,
        moves: 2,
        hints: 0,
    };
    assert!(add_result(&conn, &code, aiko, time(50_000), now).await.unwrap());
    assert!(add_result(&conn, &code, bram, time(55_000), now).await.unwrap());
    assert!(!add_result(&conn, &code, bram, time(1_000), now).await.unwrap());
    let page = page_of(&conn, &code, None).await;
    assert_eq!(page.sharer.unwrap().result.unwrap().time_ms, 50_000);
    assert_eq!(page.board.len(), 2);
    assert_eq!(players_reached(&conn, aiko).await.unwrap(), 1);
}

#[tokio::test]
async fn deleting_an_account_takes_the_results_and_the_name_off_the_links() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let now = now_ms();
    let code = share(&conn, aiko, small(), Some(solved(61_000, 0)), now)
        .await
        .unwrap();
    submit(&conn, &code, bram, &solved(40_000, 0), now).await.unwrap();
    erase_user(&conn, aiko).await.unwrap();
    // The link works on, without her and her time
    let page = page_of(&conn, &code, Some(bram)).await;
    assert_eq!(page.sharer, None);
    assert_eq!(page.solvers, 1);
    assert_eq!(page.board[0].user_id, bram);
    assert_eq!(
        number(&conn, &format!("SELECT COUNT(*) FROM shared_results WHERE user_id = {aiko}")).await,
        0
    );
    assert_eq!(
        number(&conn, &format!("SELECT COUNT(*) FROM puzzle_shares WHERE user_id = {aiko}")).await,
        0
    );
    erase_user(&conn, bram).await.unwrap();
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM shared_results").await, 0);
    assert_eq!(page_of(&conn, &code, None).await.solvers, 0);
}

#[tokio::test]
async fn abandoned_guests_take_their_links_and_results_along() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let now = now_ms();
    let code = share(&conn, guest, small(), Some(solved(61_000, 0)), now)
        .await
        .unwrap();
    submit(&conn, &code, aiko, &solved(70_000, 0), now).await.unwrap();
    // No session: abandoned
    assert_eq!(cleanup_guests(&conn).await.unwrap(), 1);
    let page = page_of(&conn, &code, None).await;
    assert_eq!(page.sharer, None);
    assert_eq!(page.board.len(), 1);
    assert_eq!(page.board[0].user_id, aiko);
}

#[tokio::test]
async fn signing_in_moves_the_guests_links_and_keeps_the_first_result() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let chen = player(&conn, "chen").await;
    let now = now_ms();
    let code = share(&conn, chen, small(), Some(solved(61_000, 0)), now)
        .await
        .unwrap();
    // The guest solved it first, the account later and faster
    submit(&conn, &code, guest, &solved(90_000, 0), now).await.unwrap();
    submit(&conn, &code, aiko, &solved(30_000, 0), now + 10).await.unwrap();
    let other = Puzzle::from_grid(&[vec![1, 0, 1], vec![0, 0, 0]]);
    let guest_code = share(&conn, guest, other, None, now).await.unwrap();
    for sql in MERGE_GUEST {
        conn.execute(sql, params![aiko, guest]).await.unwrap();
    }
    let page = page_of(&conn, &code, Some(aiko)).await;
    assert_eq!(page.mine.unwrap().time_ms, 90_000);
    assert_eq!(page.solvers, 2);
    assert_eq!(
        number(&conn, &format!("SELECT COUNT(*) FROM shared_results WHERE user_id = {guest}")).await,
        0
    );
    assert_eq!(page_of(&conn, &guest_code, None).await.sharer.unwrap().user_id, aiko);
}

#[test]
fn codes_are_random_and_plain() {
    let codes: std::collections::HashSet<String> = (0..2000).map(|_| new_code()).collect();
    assert_eq!(codes.len(), 2000);
    assert!(codes.iter().all(|c| is_share_code(c)));
    // Every character turns up
    let used: std::collections::HashSet<char> = codes.iter().flat_map(|c| c.chars()).collect();
    assert_eq!(used.len(), SHARE_ALPHABET.len());
}
