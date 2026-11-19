//! Streak saves on the server: caught-up days in the statistics, rivals, hall of fame and stamps,
//! and catch-ups going with the account. See `docs/streak-saves.md`.

use libsql::{Connection, params};

use super::streak::SavesFrom;
use super::*;
use crate::model::{AllTimeEntry, DayResult, Stamp, UserStats};
use crate::server::auth::{cleanup_guests, erase, insert_session, merge_guest};
use crate::server::db::TestConn;

async fn db(name: &str) -> TestConn {
    TestConn::new(&format!("streak-{name}")).await
}

async fn user(conn: &Connection, name: &str, registered: bool) -> i64 {
    conn.execute(
        "INSERT INTO users (username, name, created_at) VALUES (?1, ?2, 0)",
        params![registered.then_some(name), name],
    )
    .await
    .unwrap();
    conn.last_insert_rowid()
}

async fn ranked(conn: &Connection, user: i64, days: impl IntoIterator<Item = i64>) {
    for day in days {
        conn.execute(
            "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms) VALUES (?1, ?2, 0, 60000, 60000)",
            params![user, day],
        )
        .await
        .unwrap();
    }
}

/// A catch-up of `day`, finished in 45 s unless `finished` is false.
async fn catch_up(conn: &Connection, user: i64, day: i64, finished: bool) {
    conn.execute(
        "INSERT INTO catch_ups (user_id, day, started_at, finished_at, time_ms) VALUES (?1, ?2, 0, ?3, ?4)",
        params![
            user,
            day,
            finished.then_some(45_000_i64),
            finished.then_some(45_000_i64)
        ],
    )
    .await
    .unwrap();
}

async fn number(conn: &Connection, sql: &str) -> i64 {
    let mut rows = conn.query(sql, ()).await.unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

fn day_of_recent(stats: &UserStats, day: i64) -> DayResult {
    stats.recent.iter().find(|d| d.day == day).unwrap().clone()
}

fn progress(stats: &UserStats, stamp: Stamp) -> i64 {
    stats
        .stamps
        .iter()
        .find(|p| p.stamp == stamp)
        .unwrap()
        .progress
}

fn streaks_of<'a>(entries: &'a [AllTimeEntry], name: &str) -> (i64, i64) {
    let e: &'a AllTimeEntry = entries.iter().find(|e| e.name == name).unwrap();
    (e.best_streak, e.current_streak)
}

/// aiko played 7 days (a save), caught up the day before yesterday and played yesterday. ben
/// did the same but hasn't finished his catch-up: his gap is still open.
async fn two_players(conn: &Connection) -> (i64, i64) {
    let today = today();
    let aiko = user(conn, "aiko", true).await;
    let ben = user(conn, "ben", true).await;
    for (player, finished) in [(aiko, true), (ben, false)] {
        ranked(conn, player, (today - 9..=today - 3).chain([today - 1])).await;
        catch_up(conn, player, today - 2, finished).await;
    }
    rivals::set(conn, aiko, ben, true).await.unwrap();
    (aiko, ben)
}

#[tokio::test]
async fn a_finished_catch_up_closes_the_gap_everywhere() {
    let conn = db("closes").await;
    let today = today();
    let _saves = SavesFrom::set(Some(today - 30));
    let (aiko, ben) = two_players(&conn).await;

    let s = stats(&conn, aiko).await.unwrap();
    assert_eq!(
        (s.solved, s.current_streak, s.best_streak, s.saves),
        (8, 9, 9, 0)
    );
    assert_eq!(s.missed, Vec::<i64>::new());
    // The caught-up day: the player's time, no rank
    let caught = day_of_recent(&s, today - 2);
    assert_eq!(
        (caught.time_ms, caught.rank, caught.caught_up),
        (Some(45_000), None, true)
    );
    assert!(!day_of_recent(&s, today - 1).caught_up);
    // Everyone else sees the day caught up, not the time
    let public = profile(&conn, aiko).await.unwrap().unwrap().stats;
    let caught = day_of_recent(&public, today - 2);
    assert_eq!((caught.time_ms, caught.caught_up), (None, true));
    assert_eq!(public.current_streak, 9);
    // The stamps: the Streak ones count the caught-up day, the solved ones don't
    assert_eq!(progress(&s, Stamp::Streak30), 9);
    assert_eq!(progress(&s, Stamp::Solved10), 8);
    let (book, _) = sync_stamps(&conn, aiko, false).await.unwrap();
    assert!(
        book.iter()
            .any(|p| p.stamp == Stamp::Streak7 && p.earned_at.is_some())
    );

    // ben's catch-up isn't finished: it doesn't count, his gap is still open
    let s = stats(&conn, ben).await.unwrap();
    assert_eq!(
        (s.solved, s.current_streak, s.best_streak, s.saves),
        (8, 8, 7, 1)
    );
    assert_eq!(s.missed, [today - 2]);
    let open = day_of_recent(&s, today - 2);
    assert_eq!((open.time_ms, open.caught_up), (None, false));
    assert_eq!(progress(&s, Stamp::Streak30), 7);

    // The hall of fame's best streaks are exact; its current one breaks at an open gap
    let fame = hall_of_fame(&conn).await.unwrap();
    assert_eq!(streaks_of(&fame.streaks, "aiko"), (9, 9));
    assert_eq!(streaks_of(&fame.streaks, "ben"), (7, 1));
    assert_eq!(fame.streaks[0].name, "aiko");
    // Rivals: as the profiles say
    let board = rivals_board(&conn, aiko).await.unwrap();
    let rows: Vec<AllTimeEntry> = board.rows.into_iter().map(|r| r.entry).collect();
    assert_eq!(streaks_of(&rows, "aiko"), (9, 9));
    assert_eq!(streaks_of(&rows, "ben"), (7, 8));
}

/// While saves are off, catch-ups change nothing.
#[tokio::test]
async fn without_saves_nothing_changes() {
    let conn = db("off").await;
    let today = today();
    let _saves = SavesFrom::set(None);
    let (aiko, ben) = two_players(&conn).await;
    for player in [aiko, ben] {
        let s = stats(&conn, player).await.unwrap();
        assert_eq!(
            (s.solved, s.current_streak, s.best_streak, s.saves),
            (8, 1, 7, 0)
        );
        assert_eq!(s.missed, Vec::<i64>::new());
        assert_eq!(progress(&s, Stamp::Streak30), 7);
    }
    let s = stats(&conn, aiko).await.unwrap();
    let day = day_of_recent(&s, today - 2);
    assert_eq!((day.time_ms, day.caught_up), (None, false));
    let fame = hall_of_fame(&conn).await.unwrap();
    assert_eq!(streaks_of(&fame.streaks, "aiko"), (7, 1));
    let board = rivals_board(&conn, aiko).await.unwrap();
    let rows: Vec<AllTimeEntry> = board.rows.into_iter().map(|r| r.entry).collect();
    assert_eq!(streaks_of(&rows, "aiko"), (7, 1));
    assert_eq!(streaks_of(&rows, "ben"), (7, 1));
}

/// Caught-up days only count from the day saves started.
#[tokio::test]
async fn catch_ups_before_saves_started_dont_count() {
    let conn = db("before").await;
    let today = today();
    let _saves = SavesFrom::set(Some(today - 1));
    let (aiko, _) = two_players(&conn).await;
    let s = stats(&conn, aiko).await.unwrap();
    assert_eq!((s.current_streak, s.best_streak, s.saves), (1, 7, 0));
    let fame = hall_of_fame(&conn).await.unwrap();
    assert_eq!(streaks_of(&fame.streaks, "aiko"), (7, 1));
}

#[tokio::test]
async fn catch_ups_go_with_the_account() {
    let conn = db("erase").await;
    let today = today();
    let aiko = user(&conn, "aiko", true).await;
    ranked(&conn, aiko, [today - 3]).await;
    catch_up(&conn, aiko, today - 2, true).await;
    catch_up(&conn, aiko, today - 1, false).await;
    erase(&conn, aiko).await.unwrap();
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM catch_ups").await, 0);
    // The ranked time stays, without a name
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM attempts").await, 1);

    // An abandoned guest's too
    let guest = user(&conn, "Sleepy Koi 12", false).await;
    catch_up(&conn, guest, today - 2, true).await;
    assert_eq!(cleanup_guests(&conn).await.unwrap(), 1);
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM catch_ups").await, 0);
}

/// Signing in moves the guest's catch-ups to the account; a day is never both ranked and caught
/// up, and of two catch-ups of a day the account's stays.
#[tokio::test]
async fn signing_in_brings_the_guests_catch_ups_along() {
    let conn = db("merge").await;
    let today = today();
    let account = user(&conn, "noel", true).await;
    let guest = user(&conn, "Brave Heron 42", false).await;
    // Only the guest's
    catch_up(&conn, guest, today - 10, true).await;
    // Both caught it up: the account's stays
    catch_up(&conn, account, today - 9, false).await;
    catch_up(&conn, guest, today - 9, true).await;
    // The account solved it ranked: the guest's catch-up goes
    ranked(&conn, account, [today - 8]).await;
    catch_up(&conn, guest, today - 8, true).await;
    // The guest solved it ranked, which moves over: the account's catch-up goes
    ranked(&conn, guest, [today - 7]).await;
    catch_up(&conn, account, today - 7, true).await;
    // The account has an unfinished attempt, the guest's ranked one stays behind: the
    // account's catch-up stays
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at) VALUES (?1, ?2, 0)",
        params![account, today - 6],
    )
    .await
    .unwrap();
    ranked(&conn, guest, [today - 6]).await;
    catch_up(&conn, account, today - 6, true).await;

    let token = insert_session(&conn, guest).await.unwrap();
    merge_guest(&conn, guest, account, &token).await.unwrap();
    let mut rows = conn
        .query(
            "SELECT user_id, day - ?1, finished_at IS NOT NULL FROM catch_ups ORDER BY day",
            params![today],
        )
        .await
        .unwrap();
    let mut left = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        left.push((
            row.get::<i64>(0).unwrap(),
            row.get::<i64>(1).unwrap(),
            row.get::<i64>(2).unwrap() != 0,
        ));
    }
    assert_eq!(
        left,
        [
            (account, -10, true),
            (account, -9, false),
            (account, -6, true)
        ]
    );
}
