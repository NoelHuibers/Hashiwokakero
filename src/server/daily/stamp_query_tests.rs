//! Compare all 13 columns against the independent original window-query oracle.

use libsql::{Connection, Value, params};

use super::{RECORD_QUERY, record};
use crate::model::{Daypart, Record, Solve, Stamp, month_days, month_of};
use crate::server::{admin, db::TestConn};

#[path = "../../../tests/support/stamp_query.rs"]
mod fixture;
use fixture::{ORIGINAL, seed, values};

async fn equivalent(conn: &Connection, users: impl IntoIterator<Item = i64>) {
    for user in users {
        assert_eq!(
            values(conn, RECORD_QUERY, user).await,
            values(conn, ORIGINAL, user).await,
            "user {user}"
        );
    }
}

#[tokio::test]
async fn stamp_query_preserves_exact_places_and_neighbors() {
    let conn = TestConn::new("stamp-query-edges").await;
    // Insert IDs out of order: rowid and user_id must not be confused as a new tie-break.
    // Day 10: tied times with earlier/later finishes, and completely identical keys.
    // The fastest solve isn't the first finish of the day. Ineligible rows must not count.
    conn.execute_batch(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms, hints, checks, revealed, flag) VALUES
         (50,10,100,900,100,1,2,0,NULL), (20,10,100,700,100,0,1,0,NULL),
         (10,10,100,700,100,2,0,0,NULL), (40,10,100,500,300,0,0,0,NULL),
         (30,10,100,800,200,0,0,0,NULL), (60,10,100,400,50,0,0,1,NULL),
         (70,10,100,300,40,0,0,0,'review'), (80,10,100,200,30,0,0,0,'banned'),
         (90,10,100,NULL,NULL,0,0,0,NULL),
         (10,11,100,600,500,0,0,0,NULL),
         (30,12,100,800,100,0,0,0,NULL), (10,12,100,800,100,0,0,0,NULL),
         (20,12,100,800,100,0,0,0,NULL),
         (10,13,100,700,200,0,0,0,NULL), (20,13,100,700,100,0,0,0,NULL),
         (10,14,100,600,100,0,0,0,NULL), (20,14,100,700,200,0,0,0,NULL),
         (10,15,100,600,NULL,0,0,0,NULL), (20,15,100,700,100,0,0,0,NULL),
         (30,15,100,800,NULL,0,0,0,NULL), (40,15,100,900,200,0,0,0,NULL),
         (20,16,100,700,100,0,0,1,NULL), (10,17,100,NULL,NULL,0,0,0,NULL);",
    )
    .await
    .unwrap();
    equivalent(&conn, [10, 20, 30, 40, 50, 60, 70, 80, 90, 999]).await;
    let oracle = values(&conn, ORIGINAL, 10).await;
    assert_eq!(oracle.len(), 6);
    // A solo finisher has no neighbors; identical keys retain distinct ROW_NUMBER places.
    assert_eq!(&oracle[1][9..11], &[Value::Null, Value::Null]);
    let tied_places = [10, 20, 30];
    let mut places = Vec::new();
    for user in tied_places {
        let rows = values(&conn, RECORD_QUERY, user).await;
        places.push(rows.iter().find(|r| r[0] == Value::Integer(12)).unwrap()[3].clone());
    }
    places.sort_by_key(|v| match v {
        Value::Integer(i) => *i,
        _ => panic!("place"),
    });
    assert_eq!(
        places,
        vec![Value::Integer(1), Value::Integer(2), Value::Integer(3)]
    );
    // Query planning changes must not turn an accidental tie order into a new rank policy.
    conn.execute_batch("ANALYZE").await.unwrap();
    equivalent(&conn, [10, 20, 30, 40, 50, 999]).await;
}

#[tokio::test]
async fn stamp_query_matches_oracle_across_history_and_moderation() {
    let conn = TestConn::new("stamp-query-history").await;
    seed(&conn, 32, 73).await;
    // Missing days, unfinished/revealed/flagged attempts, equal times, and exact ties.
    conn.execute_batch(
        "DELETE FROM attempts WHERE (user_id * 7 + day * 3) % 13 = 0;
         UPDATE attempts SET finished_at = NULL, time_ms = NULL WHERE (user_id + day) % 17 = 0;
         UPDATE attempts SET revealed = 1 WHERE (user_id + day) % 19 = 0;
         UPDATE attempts SET flag = 'review' WHERE (user_id + day) % 23 = 0;
         UPDATE attempts SET time_ms = 60000 WHERE day % 3 = 0;
         UPDATE attempts SET finished_at = day * 86400000 + 60000
            WHERE day % 5 = 0 AND finished_at IS NOT NULL;",
    )
    .await
    .unwrap();
    equivalent(&conn, 1..=33).await;
    admin::ban(&conn, 7, "stamp query regression")
        .await
        .unwrap();
    equivalent(&conn, 1..=33).await;
    admin::unban(&conn, 7).await.unwrap();
    // Moderation changes and guest identity reassignment must be visible without a cache.
    conn.execute_batch(
        "UPDATE attempts SET flag = NULL WHERE flag = 'review';
         DELETE FROM attempts WHERE user_id = 32;
         UPDATE attempts SET user_id = 32 WHERE user_id = 31;",
    )
    .await
    .unwrap();
    equivalent(&conn, 1..=33).await;
    conn.execute_batch("ANALYZE").await.unwrap();
    equivalent(&conn, 1..=33).await;
}

#[tokio::test]
async fn stamp_query_keeps_progress_at_day_and_month_boundaries() {
    let conn = TestConn::new("stamp-query-progress").await;
    seed(&conn, 12, 65).await;
    let (first, last) = month_days(month_of(super::super::today()) - 1);
    conn.execute(
        "UPDATE attempts SET day = day + ?1, started_at = started_at + ?1 * 86400000,
            finished_at = finished_at + ?1 * 86400000",
        params![first - 3],
    )
    .await
    .unwrap();
    conn.execute_batch(
        "UPDATE attempts SET run = '{\"bridges\":20,\"islands\":12,\"steady\":true,\"dithers\":0,\"cleared\":false,\"burst\":2}', daypart = 'night';
         UPDATE attempts SET time_ms = 60000 + user_id * 100;
         DELETE FROM attempts WHERE user_id = 1 AND day % 11 = 0;",
    )
    .await
    .unwrap();
    equivalent(&conn, 1..=12).await;
    let mut oracle_solves = Vec::new();
    let mut rows = conn.query(ORIGINAL, params![1]).await.unwrap();
    while let Some(row) = rows.next().await.unwrap() {
        let above: Option<i64> = row.get(9).unwrap();
        let below: Option<i64> = row.get(10).unwrap();
        oracle_solves.push(Solve {
            day: row.get(0).unwrap(),
            hints: row.get(1).unwrap(),
            checks: row.get(2).unwrap(),
            place: row.get(3).unwrap(),
            finishers: row.get(4).unwrap(),
            first_finish: row.get::<i64>(5).unwrap() != 0,
            time_ms: row.get(6).unwrap(),
            started_at: row.get(7).unwrap(),
            finished_at: row.get(8).unwrap(),
            gap_ms: above.into_iter().chain(below).min(),
            run: serde_json::from_str(&row.get::<String>(11).unwrap()).unwrap(),
            daypart: Some(Daypart::Night),
        });
    }
    let actual = record(&conn, 1, vec![Stamp::First]).await.unwrap();
    let expected = Record {
        solves: oracle_solves,
        ..actual.clone()
    };
    assert_eq!(actual, expected);
    for today in [first - 1, first, last, last + 1, last + 2] {
        assert_eq!(
            actual.progress(today),
            expected.progress(today),
            "day {today}"
        );
    }
}
