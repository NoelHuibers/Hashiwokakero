//! Tests of the dailies kept in memory ([`Stored`]), against fresh database files.

use super::*;
use crate::model::LAUNCH_DAY;
use crate::server::db::TestConn;

/// Monday dailies are Easy, quick to generate.
const MONDAY: i64 = LAUNCH_DAY + 2;
/// Another easy daily, whose puzzle replaces [`MONDAY`]'s behind the cache's back.
const NEXT_MONDAY: i64 = MONDAY + 7;

/// Gives `day` the stored puzzle of `from`, as only a change outside the server could.
async fn replace(conn: &Connection, day: i64, from: i64) {
    get_or_create(conn, from).await.unwrap();
    conn.execute("DELETE FROM dailies WHERE day = ?1", params![day])
        .await
        .unwrap();
    conn.execute(
        "INSERT INTO dailies (day, difficulty, puzzle, solution, created_at)
         SELECT ?1, difficulty, puzzle, solution, created_at FROM dailies WHERE day = ?2",
        params![day, from],
    )
    .await
    .unwrap();
}

/// The second request of a day doesn't read the database again: a puzzle changed there isn't
/// seen, also not in a transaction. Without a request's cache the change is read.
#[tokio::test]
async fn a_stored_daily_is_read_once() {
    static STORED: Stored = Stored::new();
    let conn = TestConn::new("stored-once").await;
    let first = with_stored(&STORED, get_or_create(&conn, MONDAY))
        .await
        .unwrap();
    assert_eq!(STORED.days(), [MONDAY]);
    replace(&conn, MONDAY, NEXT_MONDAY).await;
    let changed = get_or_create(&conn, MONDAY).await.unwrap().puzzle;
    assert_ne!(changed, first.puzzle);

    let again = with_stored(&STORED, get_or_create(&conn, MONDAY))
        .await
        .unwrap();
    assert_eq!(again.puzzle, first.puzzle);
    assert_eq!(again.solution, first.solution);
    let tx = conn.transaction().await.unwrap();
    let in_tx = with_stored(&STORED, get_or_create(&tx, MONDAY))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(in_tx.puzzle, first.puzzle);
}

/// Two databases in one process (as the tests have them) keep their own dailies, and without a
/// cache of its own a database doesn't get another one's.
#[tokio::test]
async fn databases_keep_their_own_dailies() {
    static FIRST: Stored = Stored::new();
    static SECOND: Stored = Stored::new();
    let first = TestConn::new("stored-first").await;
    let second = TestConn::new("stored-second").await;
    replace(&second, MONDAY, NEXT_MONDAY).await;
    let theirs = load(&second, MONDAY).await.unwrap().unwrap().puzzle;

    let mine = with_stored(&FIRST, get_or_create(&first, MONDAY))
        .await
        .unwrap()
        .puzzle;
    assert_ne!(mine, theirs);
    assert_eq!(get_or_create(&second, MONDAY).await.unwrap().puzzle, theirs);
    assert_eq!(
        with_stored(&SECOND, get_or_create(&second, MONDAY))
            .await
            .unwrap()
            .puzzle,
        theirs
    );
    assert_eq!(
        with_stored(&FIRST, get_or_create(&first, MONDAY))
            .await
            .unwrap()
            .puzzle,
        mine
    );
}

/// A daily generated in a transaction that rolls back isn't kept: the next request finds what
/// the database stores then. A transaction's read isn't kept either, it may be its own insert.
#[tokio::test]
async fn a_rolled_back_generation_is_not_kept() {
    static STORED: Stored = Stored::new();
    let conn = TestConn::new("stored-rollback").await;
    let tx = conn.transaction().await.unwrap();
    let generated = with_stored(&STORED, get_or_create(&tx, MONDAY))
        .await
        .unwrap();
    assert!(load(&tx, MONDAY).await.unwrap().is_some());
    tx.rollback().await.unwrap();
    assert_eq!(STORED.days(), [0; 0]);
    assert!(load(&conn, MONDAY).await.unwrap().is_none());

    replace(&conn, MONDAY, NEXT_MONDAY).await;
    let stored = load(&conn, MONDAY).await.unwrap().unwrap().puzzle;
    assert_ne!(stored, generated.puzzle);
    let tx = conn.transaction().await.unwrap();
    let in_tx = with_stored(&STORED, get_or_create(&tx, MONDAY))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(in_tx.puzzle, stored);
    assert_eq!(STORED.days(), [0; 0]);
    let read = with_stored(&STORED, get_or_create(&conn, MONDAY))
        .await
        .unwrap();
    assert_eq!(read.puzzle, stored);
    assert_eq!(STORED.days(), [MONDAY]);
}

/// Only the latest days stay, whatever order they come in: an older one goes at once.
#[tokio::test]
async fn only_the_latest_days_are_kept() {
    let conn = TestConn::new("stored-days").await;
    let daily = get_or_create(&conn, MONDAY).await.unwrap();
    let stored = Stored::new();
    for day in [12, 10, 14, 11, 13] {
        stored.keep(day, &daily);
    }
    assert_eq!(stored.days(), [12, 13, 14]);
    stored.keep(3, &daily);
    assert_eq!(stored.days(), [12, 13, 14]);
    assert!(stored.get(11).is_none());
    assert_eq!(stored.get(14).unwrap().puzzle, daily.puzzle);
}
