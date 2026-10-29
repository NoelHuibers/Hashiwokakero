//! Tests of reports against a fresh database file.

use super::*;
use crate::server::auth::{erase, insert_session, merge_guest};
use crate::server::db::TestConn;

async fn db(name: &str) -> TestConn {
    TestConn::new(&format!("reports-{name}")).await
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

async fn open_ids(conn: &Connection) -> Vec<i64> {
    open(conn)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect()
}

#[track_caller]
fn error<T: std::fmt::Debug>(result: Result<T, ServerFnError>) -> ApiError {
    let e = result.expect_err("should have failed");
    ApiError::from_message(&e.to_string()).unwrap_or_else(|| panic!("{e}"))
}

#[tokio::test]
async fn reporting_again_updates_the_open_report() {
    let conn = db("upsert").await;
    let me = user(&conn, "Sleepy Tanuki 27", None).await;
    let troll = player(&conn, "troll").await;
    let first = store(&conn, me, troll, ReportReason::Cheating, "", 1000)
        .await
        .unwrap();
    assert!(first.new);
    assert_eq!(
        (first.name.as_str(), first.reporter_name.as_str()),
        ("troll", "Sleepy Tanuki 27")
    );
    let again = store(&conn, me, troll, ReportReason::Name, "rude", 2000)
        .await
        .unwrap();
    assert_eq!(again.id, first.id);
    assert!(!again.new, "an update mails nothing");

    let open = open(&conn).await.unwrap();
    assert_eq!(open.len(), 1);
    let r = &open[0];
    assert_eq!(
        (
            r.player,
            r.reported_name.as_str(),
            r.reporter,
            r.reason,
            r.note.as_str()
        ),
        (troll, "troll", me, ReportReason::Name, "rude")
    );
    assert_eq!((r.created_at, r.updated_at), (1000, 2000));
    assert_eq!(open_count(&conn).await.unwrap(), 1);

    // Once it's resolved, a new report is a new one
    resolve(&conn, first.id, ReportOutcome::Dismissed)
        .await
        .unwrap();
    assert_eq!(open_count(&conn).await.unwrap(), 0);
    let later = store(&conn, me, troll, ReportReason::Name, "", 3000)
        .await
        .unwrap();
    assert!(later.new);
    assert_ne!(later.id, first.id);
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM reports").await, 2);
}

/// The note as the reporter wrote it, without the space around it, control characters (but
/// line breaks) and what is too long.
#[tokio::test]
async fn notes_are_cleaned_and_cut() {
    let conn = db("note").await;
    let me = player(&conn, "noel").await;
    let troll = player(&conn, "troll").await;
    let note = format!("  Line one\nline\u{7} two{}  ", "x".repeat(600));
    report(&conn, me, troll, ReportReason::Other, &note)
        .await
        .unwrap();
    let stored = &open(&conn).await.unwrap()[0].note;
    assert!(stored.starts_with("Line one\nline two"), "{stored}");
    assert_eq!(stored.chars().count(), MAX_REPORT_NOTE);
}

#[tokio::test]
async fn nobody_reports_themselves_or_nobody() {
    let conn = db("missing").await;
    let me = player(&conn, "noel").await;
    assert_eq!(
        error(report(&conn, me, me, ReportReason::Name, "").await),
        ApiError::NoSuchPlayer
    );
    assert_eq!(
        error(report(&conn, me, 999, ReportReason::Name, "").await),
        ApiError::NoSuchPlayer
    );
    let gone = player(&conn, "gone").await;
    erase(&conn, gone).await.unwrap();
    assert_eq!(
        error(report(&conn, me, gone, ReportReason::Name, "").await),
        ApiError::NoSuchPlayer
    );
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM reports").await, 0);
}

#[tokio::test]
async fn reports_per_player_and_day_are_limited() {
    let conn = db("limit").await;
    let me = player(&conn, "noel").await;
    let mut others = Vec::new();
    for i in 0..=PER_REPORTER {
        others.push(player(&conn, &format!("p{i}")).await);
    }
    let day = 100 * MS_PER_DAY;
    for (i, &other) in others.iter().take(others.len() - 1).enumerate() {
        store(
            &conn,
            me,
            other,
            ReportReason::Name,
            "",
            day + i64::try_from(i).unwrap(),
        )
        .await
        .unwrap();
    }
    let last = others[others.len() - 1];
    assert_eq!(
        error(store(&conn, me, last, ReportReason::Name, "", day + 100).await),
        ApiError::TooManyReports
    );
    // Somebody else still can, and so can I a day later
    let keiko = player(&conn, "keiko").await;
    store(&conn, keiko, last, ReportReason::Name, "", day + 100)
        .await
        .unwrap();
    store(
        &conn,
        me,
        last,
        ReportReason::Name,
        "",
        day + MS_PER_DAY + 10,
    )
    .await
    .unwrap();

    // And all players together: a day later, many others reported troll
    let later = day + 3 * MS_PER_DAY;
    conn.execute(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1)
         INSERT INTO reports (reporter_id, player_id, reason, name, created_at, updated_at)
         SELECT 1000 + i, ?2, 'name', 'troll', ?3, ?3 FROM n",
        params![PER_DAY, last, later],
    )
    .await
    .unwrap();
    assert_eq!(
        error(store(&conn, keiko, me, ReportReason::Name, "", later + 1).await),
        ApiError::TooManyReports
    );
}

#[tokio::test]
async fn resolving_a_report() {
    let conn = db("resolve").await;
    let (a, b, c) = (
        player(&conn, "a").await,
        player(&conn, "b").await,
        player(&conn, "c").await,
    );
    let troll = player(&conn, "troll").await;
    let other = player(&conn, "other").await;
    let id = |sent: Sent| sent.id;
    let name_a = id(store(&conn, a, troll, ReportReason::Name, "", 1)
        .await
        .unwrap());
    let name_b = id(store(&conn, b, troll, ReportReason::Name, "", 2)
        .await
        .unwrap());
    let cheat = id(store(&conn, c, troll, ReportReason::Cheating, "", 3)
        .await
        .unwrap());
    let elsewhere = id(store(&conn, a, other, ReportReason::Name, "", 4)
        .await
        .unwrap());

    // A new name settles every report of the name, not the others
    assert_eq!(
        resolve(&conn, name_a, ReportOutcome::Renamed)
            .await
            .unwrap(),
        2
    );
    assert_eq!(open_ids(&conn).await, [cheat, elsewhere]);
    // A ban all of the player's
    let name_c = id(store(&conn, c, other, ReportReason::Name, "", 5)
        .await
        .unwrap());
    assert_eq!(
        resolve(&conn, cheat, ReportOutcome::Banned).await.unwrap(),
        1
    );
    assert_eq!(open_ids(&conn).await, [elsewhere, name_c]);
    // Dismissed: only that one, and once
    assert_eq!(
        resolve(&conn, elsewhere, ReportOutcome::Dismissed)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        resolve(&conn, elsewhere, ReportOutcome::Dismissed)
            .await
            .unwrap(),
        0
    );
    assert_eq!(open_ids(&conn).await, [name_c]);
    assert_eq!(
        number(
            &conn,
            &format!("SELECT COUNT(*) FROM reports WHERE id = {name_b} AND resolution = 'renamed'")
        )
        .await,
        1
    );

    // Resolved ones go after half a year, open ones stay
    conn.execute(
        "UPDATE reports SET resolved_at = 1 WHERE resolved_at IS NOT NULL",
        (),
    )
    .await
    .unwrap();
    delete_old(&conn).await.unwrap();
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM reports").await, 1);
}

#[tokio::test]
async fn deleting_an_account_takes_its_reports_along_both_ways() {
    let conn = db("erase").await;
    let me = player(&conn, "noel").await;
    let troll = player(&conn, "troll").await;
    let keiko = player(&conn, "keiko").await;
    report(&conn, me, troll, ReportReason::Name, "")
        .await
        .unwrap();
    report(&conn, troll, keiko, ReportReason::Name, "")
        .await
        .unwrap();
    report(&conn, keiko, me, ReportReason::Cheating, "")
        .await
        .unwrap();
    erase(&conn, troll).await.unwrap();
    let left: Vec<_> = open(&conn)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.reporter, r.player))
        .collect();
    assert_eq!(left, [(keiko, me)]);
}

/// Signing in after playing as a guest: the account takes over the guest's reports.
#[tokio::test]
async fn signing_in_brings_the_guests_reports_along() {
    let conn = db("merge").await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let me = player(&conn, "noel").await;
    let troll = player(&conn, "troll").await;
    let keiko = player(&conn, "keiko").await;
    // The guest reported troll (as did the account) and the account; keiko the guest
    store(&conn, guest, troll, ReportReason::Cheating, "guest", 1)
        .await
        .unwrap();
    store(&conn, me, troll, ReportReason::Name, "account", 2)
        .await
        .unwrap();
    store(&conn, guest, me, ReportReason::Name, "", 3)
        .await
        .unwrap();
    store(&conn, keiko, guest, ReportReason::Cheating, "", 4)
        .await
        .unwrap();

    let token = insert_session(&conn, guest).await.unwrap();
    merge_guest(&conn, guest, me, &token).await.unwrap();

    let left: Vec<_> = open(&conn)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.reporter, r.player, r.note))
        .collect();
    assert_eq!(
        left,
        [(me, troll, "account".into()), (keiko, me, String::new())]
    );
}
