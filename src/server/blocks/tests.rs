//! Tests of blocks against a fresh database file.

use super::*;
use crate::model::{LeaderboardEntry, MS_PER_DAY};
use crate::server::auth::{erase, insert_session, merge_guest};
use crate::server::daily;
use crate::server::db::TestConn;
use crate::server::groups::{self, kadai};

async fn db(name: &str) -> TestConn {
    TestConn::new(&format!("blocks-{name}")).await
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

fn ids(list: &[BlockedPlayer]) -> Vec<i64> {
    list.iter().map(|b| b.id).collect()
}

/// A finished, ranked daily of `user` on `day` in `time` ms.
async fn solved(conn: &Connection, user: i64, day: i64, time: i64) {
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![user, day, day * MS_PER_DAY, day * MS_PER_DAY + time, time],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn blocking_and_unblocking() {
    let conn = db("toggle").await;
    let me = player(&conn, "noel").await;
    let troll = player(&conn, "troll").await;
    let guest = user(&conn, "Brave Koi 40", None).await;
    assert_eq!(list(&conn, me).await.unwrap(), []);

    let blocked = set(&conn, me, troll, true).await.unwrap();
    assert_eq!(
        (blocked[0].id, blocked[0].name.as_str(), blocked[0].guest),
        (troll, "troll", false)
    );
    // Guests can be blocked too, twice changes nothing, the latest first
    conn.execute("UPDATE blocks SET created_at = 1", ())
        .await
        .unwrap();
    assert_eq!(
        ids(&set(&conn, me, guest, true).await.unwrap()),
        [guest, troll]
    );
    assert_eq!(
        ids(&set(&conn, me, guest, true).await.unwrap()),
        [guest, troll]
    );
    assert!(is_blocked(&conn, me, troll).await.unwrap());
    // One-sided: troll blocked nobody, and nobody blocked me
    assert_eq!(list(&conn, troll).await.unwrap(), []);
    assert!(!is_blocked(&conn, troll, me).await.unwrap());

    assert_eq!(ids(&set(&conn, me, troll, false).await.unwrap()), [guest]);
    assert_eq!(ids(&set(&conn, me, troll, false).await.unwrap()), [guest]);
    assert!(!is_blocked(&conn, me, troll).await.unwrap());
}

#[tokio::test]
async fn only_other_players_who_are_there_can_be_blocked() {
    let conn = db("missing").await;
    let me = player(&conn, "noel").await;
    assert_eq!(
        error(set(&conn, me, me, true).await),
        ApiError::NoSuchPlayer
    );
    assert_eq!(
        error(set(&conn, me, 999, true).await),
        ApiError::NoSuchPlayer
    );
    let gone = player(&conn, "gone").await;
    erase(&conn, gone).await.unwrap();
    assert_eq!(
        error(set(&conn, me, gone, true).await),
        ApiError::NoSuchPlayer
    );
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM blocks").await, 0);
    // Unblocking always works
    assert_eq!(set(&conn, me, 999, false).await.unwrap(), []);
}

#[tokio::test]
async fn blocking_unmarks_a_rival_who_cant_be_marked_again() {
    let conn = db("rival").await;
    let me = player(&conn, "noel").await;
    let troll = player(&conn, "troll").await;
    let keiko = player(&conn, "keiko").await;
    daily::set_rival(&conn, me, troll, true).await.unwrap();
    daily::set_rival(&conn, me, keiko, true).await.unwrap();
    // troll marked me too, which stays: only my list changes
    daily::set_rival(&conn, troll, me, true).await.unwrap();

    set(&conn, me, troll, true).await.unwrap();
    assert_eq!(daily::rivals(&conn, me).await.unwrap(), [keiko]);
    assert_eq!(daily::rivals(&conn, troll).await.unwrap(), [me]);
    assert_eq!(
        error(daily::set_rival(&conn, me, troll, true).await),
        ApiError::NoSuchPlayer
    );

    // Unblocked, they can be a rival again
    set(&conn, me, troll, false).await.unwrap();
    daily::set_rival(&conn, me, troll, true).await.unwrap();
    assert_eq!(daily::rivals(&conn, me).await.unwrap(), [keiko, troll]);
}

fn names(entries: &[LeaderboardEntry]) -> Vec<(i64, &str)> {
    entries.iter().map(|e| (e.rank, e.name.as_str())).collect()
}

/// The blocker sees the sign in place of the name, in the same place; everyone else sees the
/// name.
#[tokio::test]
async fn names_are_hidden_from_the_blocker_only() {
    let conn = db("hidden").await;
    let me = player(&conn, "noel").await;
    let troll = player(&conn, "troll").await;
    let keiko = player(&conn, "keiko").await;
    let day = 20_400;
    solved(&conn, troll, day, 60_000).await;
    solved(&conn, keiko, day, 70_000).await;
    solved(&conn, me, day, 80_000).await;
    set(&conn, me, troll, true).await.unwrap();

    let board = |viewer: Option<i64>| {
        let conn = &conn;
        async move {
            let board = daily::leaderboard(conn, day, viewer).await.unwrap();
            hidden(conn, viewer, board).await.unwrap()
        }
    };
    let mine = board(Some(me)).await;
    assert_eq!(
        names(&mine.entries),
        [(1, BLOCKED_NAME), (2, "keiko"), (3, "noel")]
    );
    // Still their row, for the rivals' marks and the places
    assert_eq!(mine.entries[0].user_id, troll);
    for viewer in [Some(keiko), Some(troll), None] {
        let theirs = board(viewer).await;
        assert_eq!(
            names(&theirs.entries),
            [(1, "troll"), (2, "keiko"), (3, "noel")],
            "{viewer:?}"
        );
    }
    // Nobody's own row is hidden from them
    let theirs = board(Some(troll)).await;
    assert_eq!(theirs.me.unwrap().name, "troll");

    // The all time boards alike
    let fame = hidden(&conn, Some(me), daily::hall_of_fame(&conn).await.unwrap())
        .await
        .unwrap();
    let solved: Vec<_> = fame
        .solved
        .iter()
        .map(|e| (e.user_id, e.name.as_str()))
        .collect();
    assert!(solved.contains(&(troll, BLOCKED_NAME)), "{solved:?}");
    assert!(solved.contains(&(keiko, "keiko")), "{solved:?}");
    let fame = hidden(
        &conn,
        Some(keiko),
        daily::hall_of_fame(&conn).await.unwrap(),
    )
    .await
    .unwrap();
    assert!(fame.solved.iter().all(|e| e.name != BLOCKED_NAME));

    // Unblocked, the name is back
    set(&conn, me, troll, false).await.unwrap();
    assert_eq!(board(Some(me)).await.entries[0].name, "troll");
}

/// In a group: the member list, today's board and a puzzle of theirs, which loses its title.
#[tokio::test]
async fn groups_hide_blocked_members_and_their_puzzle_titles() {
    let conn = db("groups").await;
    let me = player(&conn, "noel").await;
    let troll = player(&conn, "troll").await;
    let keiko = player(&conn, "keiko").await;
    let group = groups::create(&conn, me, "Team").await.unwrap();
    let code: String = {
        let mut rows = conn
            .query("SELECT invite FROM groups WHERE id = ?1", params![group])
            .await
            .unwrap();
        rows.next().await.unwrap().unwrap().get(0).unwrap()
    };
    groups::join(&conn, &code, troll).await.unwrap();
    groups::join(&conn, &code, keiko).await.unwrap();
    let now = now_ms();
    for (author, title) in [(troll, "Rude title"), (keiko, "Spiral")] {
        conn.execute(
            r#"INSERT INTO group_puzzles (group_id, author_id, title, puzzle, solution, created_at, closes_at)
               VALUES (?1, ?2, ?3, '{"width":3,"height":1,"islands":[{"pos":{"x":0,"y":0},"required":1},{"pos":{"x":2,"y":0},"required":1}]}', '[1]', ?4, ?5)"#,
            params![group, author, title, now, now + 7 * MS_PER_DAY],
        )
        .await
        .unwrap();
    }
    set(&conn, me, troll, true).await.unwrap();
    let today = daily::today();

    let page = groups::page(&conn, group, me, today).await.unwrap();
    let page = hidden(&conn, Some(me), page).await.unwrap();
    let members: Vec<_> = page
        .members
        .iter()
        .map(|m| (m.user_id, m.name.as_str()))
        .collect();
    assert_eq!(
        members,
        [(me, "noel"), (troll, BLOCKED_NAME), (keiko, "keiko")]
    );
    assert!(
        page.today
            .entries
            .iter()
            .any(|e| e.user_id == troll && e.name == BLOCKED_NAME)
    );

    let puzzles = kadai::list(&conn, group, me, now).await.unwrap();
    let puzzles = hidden(&conn, Some(me), puzzles).await.unwrap();
    let seen: Vec<_> = puzzles
        .open
        .iter()
        .map(|p| {
            (
                p.title.as_deref(),
                p.author.as_ref().map(|a| a.name.as_str()),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [(None, Some(BLOCKED_NAME)), (Some("Spiral"), Some("keiko"))]
    );
    // keiko sees it all
    let puzzles = kadai::list(&conn, group, keiko, now).await.unwrap();
    let puzzles = hidden(&conn, Some(keiko), puzzles).await.unwrap();
    assert_eq!(puzzles.open[0].title.as_deref(), Some("Rude title"));
    let page = groups::page(&conn, group, keiko, today).await.unwrap();
    let page = hidden(&conn, Some(keiko), page).await.unwrap();
    assert!(page.members.iter().all(|m| m.name != BLOCKED_NAME));
}

#[tokio::test]
async fn deleting_an_account_takes_its_blocks_along_both_ways() {
    let conn = db("erase").await;
    let me = player(&conn, "noel").await;
    let troll = player(&conn, "troll").await;
    let keiko = player(&conn, "keiko").await;
    set(&conn, me, troll, true).await.unwrap();
    set(&conn, troll, keiko, true).await.unwrap();
    set(&conn, keiko, me, true).await.unwrap();
    erase(&conn, troll).await.unwrap();
    assert_eq!(list(&conn, me).await.unwrap(), []);
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM blocks").await,
        1,
        "only keiko's block of noel"
    );
    assert_eq!(ids(&list(&conn, keiko).await.unwrap()), [me]);
}

/// Signing in after playing as a guest: the account blocks whom the guest blocked, and who
/// blocked the guest blocks the account.
#[tokio::test]
async fn signing_in_brings_the_guests_blocks_along() {
    let conn = db("merge").await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let me = player(&conn, "noel").await;
    let troll = player(&conn, "troll").await;
    let keiko = player(&conn, "keiko").await;
    let bram = player(&conn, "bram").await;
    // The guest blocked troll and the account, the account had troll as a rival and blocked
    // keiko; bram blocked the guest and had the account as a rival
    set(&conn, guest, troll, true).await.unwrap();
    set(&conn, guest, me, true).await.unwrap();
    daily::set_rival(&conn, me, troll, true).await.unwrap();
    set(&conn, me, keiko, true).await.unwrap();
    daily::set_rival(&conn, bram, me, true).await.unwrap();
    set(&conn, bram, guest, true).await.unwrap();

    let token = insert_session(&conn, guest).await.unwrap();
    merge_guest(&conn, guest, me, &token).await.unwrap();

    let mut mine = ids(&list(&conn, me).await.unwrap());
    mine.sort_unstable();
    assert_eq!(mine, [troll, keiko]);
    assert_eq!(ids(&list(&conn, bram).await.unwrap()), [me]);
    // Nobody keeps a rival they blocked
    assert_eq!(daily::rivals(&conn, me).await.unwrap(), Vec::<i64>::new());
    assert_eq!(daily::rivals(&conn, bram).await.unwrap(), Vec::<i64>::new());
    assert_eq!(
        number(
            &conn,
            &format!("SELECT COUNT(*) FROM blocks WHERE user_id = {guest} OR blocked_id = {guest}")
        )
        .await,
        0
    );
}
