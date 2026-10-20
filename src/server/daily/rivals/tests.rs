//! Tests of rivals against a fresh database file.

use super::*;
use crate::model::{Outcome, RECENT_DUELS};
use crate::server::{admin, auth, db};

async fn test_db(name: &str) -> db::TestConn {
    db::TestConn::new(&format!("rivals-{name}")).await
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

/// A finished, ranked daily of `user` on `day` in `time` ms.
async fn solved(conn: &Connection, user: i64, day: i64, time: i64) {
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![user, day, day * 86_400_000, day * 86_400_000 + time, time],
    )
    .await
    .unwrap();
}

async fn change(conn: &Connection, sql: &str, user: i64, day: i64) {
    conn.execute(sql, params![user, day]).await.unwrap();
}

fn error<T: std::fmt::Debug>(result: Result<T, ServerFnError>) -> ApiError {
    let e = result.expect_err("should have failed");
    ApiError::from_message(&e.to_string()).unwrap_or_else(|| panic!("not an ApiError: {e}"))
}

#[tokio::test]
async fn marking_and_unmarking_a_rival() {
    let conn = test_db("toggle").await;
    let me = user(&conn, "noel", Some("noel")).await;
    let keiko = user(&conn, "keiko", Some("keiko")).await;
    let lena = user(&conn, "Brave Koi 40", None).await;
    assert_eq!(list(&conn, me).await.unwrap(), Vec::<i64>::new());

    assert_eq!(set(&conn, me, keiko, true).await.unwrap(), [keiko]);
    // Guests can be rivals too, and marking twice changes nothing
    assert_eq!(set(&conn, me, lena, true).await.unwrap(), [keiko, lena]);
    assert_eq!(set(&conn, me, keiko, true).await.unwrap(), [keiko, lena]);
    // One-sided: keiko has no rivals
    assert_eq!(list(&conn, keiko).await.unwrap(), Vec::<i64>::new());

    assert_eq!(set(&conn, me, keiko, false).await.unwrap(), [lena]);
    assert_eq!(set(&conn, me, keiko, false).await.unwrap(), [lena]);
    assert_eq!(list(&conn, me).await.unwrap(), [lena]);
}

#[tokio::test]
async fn nobody_is_their_own_rival() {
    let conn = test_db("self").await;
    let me = user(&conn, "noel", Some("noel")).await;
    assert_eq!(error(set(&conn, me, me, true).await), ApiError::RivalSelf);
    assert_eq!(list(&conn, me).await.unwrap(), Vec::<i64>::new());
    // Unmarking oneself is harmless
    assert_eq!(set(&conn, me, me, false).await.unwrap(), Vec::<i64>::new());
}

#[tokio::test]
async fn only_players_who_still_play_can_be_marked() {
    let conn = test_db("missing").await;
    let me = user(&conn, "noel", Some("noel")).await;
    assert_eq!(
        error(set(&conn, me, 999, true).await),
        ApiError::NoSuchPlayer
    );
    let gone = user(&conn, "gone", Some("gone")).await;
    auth::erase(&conn, gone).await.unwrap();
    assert_eq!(
        error(set(&conn, me, gone, true).await),
        ApiError::NoSuchPlayer
    );
    let banned = user(&conn, "cheater", Some("cheater")).await;
    admin::ban(&conn, banned, "").await.unwrap();
    assert_eq!(
        error(set(&conn, me, banned, true).await),
        ApiError::NoSuchPlayer
    );
}

#[tokio::test]
async fn at_most_twenty_rivals() {
    let conn = test_db("limit").await;
    let me = user(&conn, "noel", Some("noel")).await;
    let mut others = Vec::new();
    for i in 0..=MAX_RIVALS {
        others.push(user(&conn, &format!("player {i}"), None).await);
    }
    for &other in &others[..MAX_RIVALS] {
        set(&conn, me, other, true).await.unwrap();
    }
    assert_eq!(list(&conn, me).await.unwrap(), others[..MAX_RIVALS]);
    let last = others[MAX_RIVALS];
    assert_eq!(
        error(set(&conn, me, last, true).await),
        ApiError::TooManyRivals
    );
    // Marking one of them again at the limit is fine
    set(&conn, me, others[0], true).await.unwrap();

    // A banned rival drops out of the list and doesn't count, lifting the ban brings them back
    admin::ban(&conn, others[0], "").await.unwrap();
    assert!(!list(&conn, me).await.unwrap().contains(&others[0]));
    set(&conn, me, last, true).await.unwrap();
    admin::unban(&conn, others[0]).await.unwrap();
    assert_eq!(list(&conn, me).await.unwrap().len(), MAX_RIVALS + 1);
    // Over the limit now: no new ones until there is room again
    let another = user(&conn, "another", None).await;
    assert_eq!(
        error(set(&conn, me, another, true).await),
        ApiError::TooManyRivals
    );
    set(&conn, me, others[1], false).await.unwrap();
    set(&conn, me, others[2], false).await.unwrap();
    set(&conn, me, another, true).await.unwrap();
}

#[tokio::test]
async fn the_head_to_head_counts_days_both_have_a_ranked_time() {
    let conn = test_db("record").await;
    let me = user(&conn, "noel", Some("noel")).await;
    let keiko = user(&conn, "keiko", Some("keiko")).await;
    let other = user(&conn, "other", Some("other")).await;
    // Days 1–3: a win, a loss, a draw
    for (day, mine, theirs) in [
        (1, 60_000, 72_000),
        (2, 90_000, 50_000),
        (3, 70_000, 70_000),
    ] {
        solved(&conn, me, day, mine).await;
        solved(&conn, keiko, day, theirs).await;
    }
    // Days without both ranked times don't count: only one of us, given up, flagged, unfinished
    solved(&conn, me, 4, 10_000).await;
    solved(&conn, keiko, 5, 10_000).await;
    for (day, sql) in [
        (
            6,
            "UPDATE attempts SET revealed = 1 WHERE user_id = ?1 AND day = ?2",
        ),
        (
            7,
            "UPDATE attempts SET flag = 'pace' WHERE user_id = ?1 AND day = ?2",
        ),
        (
            8,
            "UPDATE attempts SET finished_at = NULL WHERE user_id = ?1 AND day = ?2",
        ),
    ] {
        solved(&conn, me, day, 10_000).await;
        solved(&conn, keiko, day, 90_000).await;
        change(&conn, sql, keiko, day).await;
    }
    // Someone else's times don't mix in
    solved(&conn, other, 1, 1_000).await;

    // Not a rival: no record
    assert_eq!(record(&conn, me, keiko).await.unwrap(), None);
    set(&conn, me, keiko, true).await.unwrap();
    let r = record(&conn, me, keiko).await.unwrap().unwrap();
    assert_eq!((r.wins, r.losses, r.draws), (1, 1, 1));
    assert_eq!(
        r.recent.iter().map(Duel::outcome).collect::<Vec<_>>(),
        [Outcome::Win, Outcome::Loss, Outcome::Draw]
    );
    assert_eq!(
        r.recent[0],
        Duel {
            day: 1,
            mine: 60_000,
            theirs: 72_000
        }
    );
    // Seen from the other side (once marked), it's the mirror image
    set(&conn, keiko, me, true).await.unwrap();
    let theirs = record(&conn, keiko, me).await.unwrap().unwrap();
    assert_eq!((theirs.wins, theirs.losses, theirs.draws), (1, 1, 1));
    assert_eq!(theirs.recent[1].outcome(), Outcome::Win);

    // Against the other one only day 1 counts, lost
    set(&conn, me, other, true).await.unwrap();
    let r = record(&conn, me, other).await.unwrap().unwrap();
    assert_eq!((r.wins, r.losses, r.draws, r.recent.len()), (0, 1, 0, 1));
    // A rival without a shared day has an empty record
    let new = user(&conn, "new", None).await;
    set(&conn, me, new, true).await.unwrap();
    assert_eq!(
        record(&conn, me, new).await.unwrap(),
        Some(HeadToHead::default())
    );
}

#[tokio::test]
async fn the_record_keeps_the_last_week_of_duels() {
    let conn = test_db("recent").await;
    let me = user(&conn, "noel", Some("noel")).await;
    let keiko = user(&conn, "keiko", Some("keiko")).await;
    set(&conn, me, keiko, true).await.unwrap();
    for day in 1..=10 {
        solved(&conn, me, day, 60_000).await;
        solved(&conn, keiko, day, 60_000 + day * 1000).await;
    }
    let r = record(&conn, me, keiko).await.unwrap().unwrap();
    assert_eq!((r.wins, r.losses), (10, 0));
    let days: Vec<i64> = r.recent.iter().map(|d| d.day).collect();
    assert_eq!(days.len(), RECENT_DUELS);
    assert_eq!(days, (4..=10).collect::<Vec<_>>());
}

/// Nothing about the rivals' times today before the player finished the daily themselves.
#[tokio::test]
async fn todays_duels_wait_for_my_own_finish() {
    let conn = test_db("spoiler").await;
    let me = user(&conn, "noel", Some("noel")).await;
    let keiko = user(&conn, "keiko", Some("keiko")).await;
    let tobias = user(&conn, "tobias", Some("tobias")).await;
    let lena = user(&conn, "lena", Some("lena")).await;
    let flagged = user(&conn, "flagged", Some("flagged")).await;
    for rival in [keiko, tobias, lena, flagged] {
        set(&conn, me, rival, true).await.unwrap();
    }
    let day = 100;
    solved(&conn, keiko, day, 72_000).await;
    solved(&conn, tobias, day, 20_000).await;
    solved(&conn, flagged, day, 10_000).await;
    change(
        &conn,
        "UPDATE attempts SET flag = 'pace' WHERE user_id = ?1 AND day = ?2",
        flagged,
        day,
    )
    .await;
    // An earlier shared day for the record
    solved(&conn, me, day - 1, 50_000).await;
    solved(&conn, keiko, day - 1, 40_000).await;

    // Not started, then running: nothing
    assert_eq!(duels_of_day(&conn, me, day).await.unwrap(), None);
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at) VALUES (?1, ?2, 0)",
        params![me, day],
    )
    .await
    .unwrap();
    assert_eq!(duels_of_day(&conn, me, day).await.unwrap(), None);

    // Finished: every rival with their ranked time, if they have one
    conn.execute(
        "UPDATE attempts SET finished_at = 60000, time_ms = 60000 WHERE user_id = ?1 AND day = ?2",
        params![me, day],
    )
    .await
    .unwrap();
    let today = duels_of_day(&conn, me, day).await.unwrap().unwrap();
    assert_eq!(today.mine, Some(60_000));
    let times: Vec<(i64, Option<i64>)> = today
        .rivals
        .iter()
        .map(|r| (r.user_id, r.time_ms))
        .collect();
    assert_eq!(
        times,
        [
            (keiko, Some(72_000)),
            (tobias, Some(20_000)),
            (lena, None),
            (flagged, None)
        ]
    );
    let k = &today.rivals[0];
    assert_eq!((k.name.as_str(), k.guest), ("keiko", false));
    // Today's win and yesterday's loss
    assert_eq!((k.record.wins, k.record.losses), (1, 1));

    // Given up: finished, but no time of mine to compare
    change(
        &conn,
        "UPDATE attempts SET revealed = 1 WHERE user_id = ?1 AND day = ?2",
        me,
        day,
    )
    .await;
    let today = duels_of_day(&conn, me, day).await.unwrap().unwrap();
    assert_eq!(today.mine, None);
    assert_eq!(today.rivals[0].time_ms, Some(72_000));
    assert_eq!(
        (today.rivals[0].record.wins, today.rivals[0].record.losses),
        (0, 1)
    );

    // Without rivals there's still the answer, just empty
    let alone = user(&conn, "alone", Some("alone")).await;
    solved(&conn, alone, day, 30_000).await;
    let today = duels_of_day(&conn, alone, day).await.unwrap().unwrap();
    assert_eq!((today.mine, today.rivals.len()), (Some(30_000), 0));
}

#[tokio::test]
async fn the_board_compares_me_with_my_rivals() {
    let conn = test_db("board").await;
    let me = user(&conn, "noel", Some("noel")).await;
    let keiko = user(&conn, "keiko", Some("keiko")).await;
    let lena = user(&conn, "Brave Koi 40", None).await;
    // Alone, without a result: just me
    let board_now = board(&conn, me).await.unwrap();
    assert_eq!(board_now.rows.len(), 1);
    let row = &board_now.rows[0];
    assert!(row.me && row.record.is_none());
    assert_eq!(
        (row.entry.name.as_str(), row.entry.solved, row.entry.rank),
        ("noel", 0, 1)
    );

    set(&conn, me, keiko, true).await.unwrap();
    set(&conn, me, lena, true).await.unwrap();
    // keiko solved three days (a streak), me two of them, lena none
    for day in [1, 2, 3] {
        solved(&conn, keiko, day, 50_000).await;
    }
    for day in [2, 3] {
        solved(&conn, me, day, 40_000).await;
    }
    let rows = board(&conn, me).await.unwrap().rows;
    let order: Vec<(i64, i64, bool)> = rows
        .iter()
        .map(|r| (r.entry.user_id, r.entry.rank, r.me))
        .collect();
    assert_eq!(order, [(keiko, 1, false), (me, 2, true), (lena, 3, false)]);
    assert_eq!((rows[0].entry.solved, rows[0].entry.best_streak), (3, 3));
    // Two medals: keiko got gold on day 1 alone, silver behind me on days 2 and 3
    assert_eq!((rows[0].entry.gold, rows[0].entry.silver), (1, 2));
    assert_eq!((rows[1].entry.gold, rows[1].entry.silver), (2, 0));
    let k = rows[0].record.as_ref().unwrap();
    assert_eq!((k.wins, k.losses), (2, 0));
    assert_eq!(rows[2].record, Some(HeadToHead::default()));
    assert!(rows[2].entry.guest);
    assert_eq!(rows[2].entry.name, "Brave Koi 40");
}

/// Rows of the rivals table `user` is in, on either side.
async fn rows(conn: &Connection, user: i64) -> i64 {
    let mut r = conn
        .query(
            "SELECT COUNT(*) FROM rivals WHERE user_id = ?1 OR rival_id = ?1",
            params![user],
        )
        .await
        .unwrap();
    r.next().await.unwrap().unwrap().get(0).unwrap()
}

/// Deleting an account takes its rivals and its place on everyone else's list; so does the
/// cleanup of abandoned guests.
#[tokio::test]
async fn deleted_players_leave_every_list() {
    let conn = test_db("delete").await;
    let me = user(&conn, "noel", Some("noel")).await;
    let keiko = user(&conn, "keiko", Some("keiko")).await;
    let guest = user(&conn, "Sleepy Crane 12", None).await;
    set(&conn, me, keiko, true).await.unwrap();
    set(&conn, me, guest, true).await.unwrap();
    set(&conn, keiko, me, true).await.unwrap();
    set(&conn, guest, me, true).await.unwrap();
    // A ranked time keeps the deleted user's row, without a name
    solved(&conn, keiko, 1, 60_000).await;

    auth::erase(&conn, keiko).await.unwrap();
    assert_eq!(list(&conn, me).await.unwrap(), [guest]);
    assert_eq!(rows(&conn, keiko).await, 0);

    // The guest without a session, created long ago: cleaned up
    assert_eq!(auth::cleanup_guests(&conn).await.unwrap(), 1);
    assert_eq!(rows(&conn, guest).await, 0);
    assert_eq!(list(&conn, me).await.unwrap(), Vec::<i64>::new());
    assert_eq!(rows(&conn, me).await, 0);
}
