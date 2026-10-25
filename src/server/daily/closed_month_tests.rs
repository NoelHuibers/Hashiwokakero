//! The stored standings of months that are over against counting them from every ranked time
//! ([`count_points`], what every read did before), after everything that changes old results.

use libsql::{Connection, params};

use super::{champion, closed_month, count_points, month_points};
use crate::golden::puzzle;
use crate::model::{Flag, MS_PER_DAY, MoveEvent, month_days, month_of};
use crate::server::daily::{set_flag, submit, today};
use crate::server::db::TestConn;
use crate::server::{admin, auth};

/// 1970-01-01 to 1970-05-10: four whole months that are over and ten days of May.
const DAYS: i64 = 130;

/// xorshift64*, enough for test data that is the same on every run.
struct Random(u64);

impl Random {
    fn below(&mut self, n: u64) -> i64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        i64::try_from(self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) % n).unwrap()
    }
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

/// Players with registered and guest accounts, and a history of their dailies with gaps, running
/// and revealed attempts, reviews, times tied with each other (also down to the millisecond of
/// the finish) and legacy finishes without a time.
async fn history(conn: &Connection, seed: u64, players: i64) -> Vec<i64> {
    let mut random = Random(seed);
    let mut users = Vec::new();
    for i in 0..players {
        let username = format!("player{i}");
        let guest = i % 3 == 0;
        users.push(user(conn, &username, (!guest).then_some(username.as_str())).await);
    }
    let mut values = Vec::new();
    for day in 0..DAYS {
        for &user in &users {
            if random.below(10) < 3 {
                continue;
            }
            let started = day * MS_PER_DAY + random.below(3) * 1000;
            let time = 60_000 + random.below(30) * 1000;
            let (finished, time) = match random.below(40) {
                0..=2 => ("NULL".to_string(), "NULL".to_string()),
                3 => ((started + time).to_string(), "NULL".to_string()),
                _ => ((started + time).to_string(), time.to_string()),
            };
            let revealed = i64::from(random.below(25) == 0);
            let flag = if random.below(20) == 0 {
                "'review'"
            } else {
                "NULL"
            };
            values.push(format!(
                "({user}, {day}, {started}, {finished}, {revealed}, {time}, {flag})"
            ));
        }
    }
    for chunk in values.chunks(500) {
        conn.execute(
            &format!(
                "INSERT INTO attempts (user_id, day, started_at, finished_at, revealed, time_ms, flag) VALUES {}",
                chunk.join(",")
            ),
            (),
        )
        .await
        .unwrap();
    }
    users
}

fn months() -> Vec<i64> {
    (month_of(0)..=month_of(DAYS - 1)).collect()
}

async fn stored(conn: &Connection) -> Vec<i64> {
    let mut rows = conn
        .query("SELECT month FROM closed_months ORDER BY month", ())
        .await
        .unwrap();
    let mut months = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        months.push(row.get(0).unwrap());
    }
    months
}

/// Every month's stored standings, the whole list and its winner, are what counting says.
async fn equivalent(conn: &Connection, label: &str) {
    for month in months() {
        let counted = count_points(conn, month).await.unwrap();
        assert_eq!(
            month_points(conn, month).await.unwrap(),
            counted,
            "{label}: month {month}"
        );
        assert_eq!(
            champion(conn, month).await.unwrap(),
            counted.first().map(|p| p.0),
            "{label}: champion of month {month}"
        );
        assert_eq!(
            closed_month(conn, month, Some(3)).await.unwrap(),
            counted.iter().take(3).copied().collect::<Vec<_>>(),
            "{label}: podium of month {month}"
        );
    }
    // Read once, kept
    assert_eq!(stored(conn).await, months(), "{label}");
}

/// Marks the stored standings (negative points), to tell the months a change dropped from the
/// others: the stamps of the player are synced right after most changes, which stores the
/// months again.
async fn mark(conn: &Connection) {
    conn.execute(
        "UPDATE month_standings SET points = -1 - points WHERE points >= 0",
        (),
    )
    .await
    .unwrap();
}

/// Exactly the months of `days` lost their marked standings, the others kept them (unmarked
/// again).
async fn dropped(conn: &Connection, days: &[i64]) {
    let mut rows = conn
        .query(
            "SELECT DISTINCT month FROM month_standings WHERE points < 0 ORDER BY month",
            (),
        )
        .await
        .unwrap();
    let mut kept = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        kept.push(row.get::<i64>(0).unwrap());
    }
    drop(rows);
    let gone: Vec<i64> = months().into_iter().filter(|m| !kept.contains(m)).collect();
    let mut expected: Vec<i64> = days.iter().map(|&d| month_of(d)).collect();
    expected.sort_unstable();
    expected.dedup();
    assert_eq!(gone, expected, "dropped months");
    conn.execute(
        "UPDATE month_standings SET points = -1 - points WHERE points < 0",
        (),
    )
    .await
    .unwrap();
}

async fn days_of(conn: &Connection, user: i64, sql_filter: &str) -> Vec<i64> {
    let mut rows = conn
        .query(
            &format!("SELECT day FROM attempts WHERE user_id = ?1 AND ({sql_filter})"),
            params![user],
        )
        .await
        .unwrap();
    let mut days = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        days.push(row.get(0).unwrap());
    }
    days
}

#[tokio::test]
async fn stored_standings_match_counting_through_every_change() {
    let conn = TestConn::new("closed-months").await;
    let users = history(&conn, 0x5eed_1970, 24).await;
    // Nothing stored before the first read
    assert_eq!(stored(&conn).await, Vec::<i64>::new());
    equivalent(&conn, "first read").await;
    // A month with nobody in it is stored too, without standings
    let empty = month_of(DAYS) + 1;
    assert_eq!(month_points(&conn, empty).await.unwrap(), []);
    assert_eq!(champion(&conn, empty).await.unwrap(), None);
    assert!(stored(&conn).await.contains(&empty));
    conn.execute("DELETE FROM closed_months WHERE month = ?1", params![empty])
        .await
        .unwrap();

    // The winner of January, and a day of theirs in it
    let january = month_of(0);
    let winner = champion(&conn, january).await.unwrap().unwrap();
    let day = days_of(&conn, winner, "day < 31 AND finished_at IS NOT NULL AND flag IS NULL AND revealed = 0 AND time_ms IS NOT NULL")
        .await[0];

    // A review (/admin/flag) takes the result off: January only is worked out again
    mark(&conn).await;
    assert!(set_flag(&conn, winner, day, Some(Flag::Review)).await.unwrap());
    dropped(&conn, &[day]).await;
    equivalent(&conn, "flagged").await;
    // and puts it back
    mark(&conn).await;
    assert!(set_flag(&conn, winner, day, None).await.unwrap());
    dropped(&conn, &[day]).await;
    equivalent(&conn, "unflagged").await;

    // A ban takes all of a player's results off, lifting it puts them back
    let banned = users[4];
    let theirs = days_of(&conn, banned, "flag IS NULL").await;
    mark(&conn).await;
    admin::ban(&conn, banned, "closed months").await.unwrap();
    dropped(&conn, &theirs).await;
    equivalent(&conn, "banned").await;
    mark(&conn).await;
    admin::unban(&conn, banned).await.unwrap();
    dropped(&conn, &theirs).await;
    equivalent(&conn, "unbanned").await;

    // A new name changes no standings (they keep the id), nothing is worked out again
    mark(&conn).await;
    admin::rename_player(&conn, winner).await.unwrap();
    dropped(&conn, &[]).await;
    equivalent(&conn, "renamed").await;
    // Neither do the move log, the run and penalties counted into the time already
    mark(&conn).await;
    conn.execute(
        "UPDATE attempts SET log = '[]', run = NULL, daypart = 'night', hints = hints + 1, checks = 1,
            paid_actions = '[]', bridges = '[]', device = 'd', moves = 3",
        (),
    )
    .await
    .unwrap();
    dropped(&conn, &[]).await;

    // A deleted account: the ranked times stay (without a name), the others go
    let deleted = users[1];
    let unranked = days_of(
        &conn,
        deleted,
        "finished_at IS NULL OR revealed != 0 OR flag IS NOT NULL",
    )
    .await;
    assert!(!unranked.is_empty());
    mark(&conn).await;
    auth::erase(&conn, deleted).await.unwrap();
    dropped(&conn, &unranked).await;
    equivalent(&conn, "deleted").await;

    // A guest signs in to an account: their dailies become the account's (where the account
    // didn't play the day), the rest goes
    let guest = users[3];
    let account = users[2];
    let guest_days = days_of(&conn, guest, "1").await;
    let token = auth::insert_session(&conn, guest).await.unwrap();
    mark(&conn).await;
    auth::merge_guest(&conn, guest, account, &token)
        .await
        .unwrap();
    assert!(days_of(&conn, guest, "1").await.is_empty());
    dropped(&conn, &guest_days).await;
    equivalent(&conn, "guest moved").await;

    // A finish long after the month was over (the client sends the day it started), a reveal
    let late = days_of(&conn, users[5], "finished_at IS NULL").await[0];
    mark(&conn).await;
    conn.execute(
        "UPDATE attempts SET finished_at = started_at + 1000, time_ms = 1000
         WHERE user_id = ?1 AND day = ?2",
        params![users[5], late],
    )
    .await
    .unwrap();
    dropped(&conn, &[late]).await;
    equivalent(&conn, "late finish").await;
    let shown = days_of(&conn, users[6], "flag IS NULL AND revealed = 0").await[0];
    mark(&conn).await;
    conn.execute(
        "UPDATE attempts SET revealed = 1 WHERE user_id = ?1 AND day = ?2",
        params![users[6], shown],
    )
    .await
    .unwrap();
    dropped(&conn, &[shown]).await;
    equivalent(&conn, "revealed").await;

    // Changes no server makes, for the triggers: a time, a result moving to another month,
    // a new one, one that goes
    let ranked = days_of(&conn, users[7], "finished_at IS NOT NULL AND flag IS NULL").await;
    mark(&conn).await;
    conn.execute(
        "UPDATE attempts SET time_ms = 1 WHERE user_id = ?1 AND day = ?2",
        params![users[7], ranked[0]],
    )
    .await
    .unwrap();
    dropped(&conn, &[ranked[0]]).await;
    equivalent(&conn, "time").await;
    let (first_of_april, _) = month_days(month_of(95));
    let moved = *ranked.iter().find(|&&d| d < first_of_april - 40).unwrap();
    let played = days_of(&conn, users[7], "1").await;
    let free = (first_of_april..DAYS)
        .find(|d| !played.contains(d))
        .unwrap();
    mark(&conn).await;
    conn.execute(
        "UPDATE attempts SET day = ?3 WHERE user_id = ?1 AND day = ?2",
        params![users[7], moved, free],
    )
    .await
    .unwrap();
    dropped(&conn, &[moved, free]).await;
    equivalent(&conn, "moved").await;
    let newcomer = user(&conn, "newcomer", Some("newcomer")).await;
    mark(&conn).await;
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms) VALUES (?1, 40, 0, ?2, 1)",
        params![newcomer, 40 * MS_PER_DAY + 1],
    )
    .await
    .unwrap();
    dropped(&conn, &[40]).await;
    equivalent(&conn, "new").await;
    mark(&conn).await;
    conn.execute(
        "DELETE FROM attempts WHERE user_id = ?1",
        params![newcomer],
    )
    .await
    .unwrap();
    dropped(&conn, &[40]).await;
    equivalent(&conn, "removed").await;

    // Guests nobody can sign in to any more are deleted, like accounts
    let removed = auth::cleanup_guests(&conn).await.unwrap();
    assert!(removed > 0);
    equivalent(&conn, "guests cleaned up").await;

    // Planning with statistics doesn't change what is counted or stored
    conn.execute_batch("ANALYZE").await.unwrap();
    conn.execute("DELETE FROM closed_months", ()).await.unwrap();
    assert_eq!(
        conn.query("SELECT COUNT(*) FROM month_standings", ())
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .unwrap()
            .get::<i64>(0)
            .unwrap(),
        0
    );
    equivalent(&conn, "analyzed").await;
}

#[tokio::test]
async fn other_histories_match_counting() {
    for seed in [1, 2, 3] {
        let conn = TestConn::new("closed-months-seeds").await;
        let players = 5 + 7 * i64::try_from(seed).unwrap();
        history(&conn, seed, players).await;
        equivalent(&conn, &format!("seed {seed}")).await;
    }
}

/// Exact ties: the same points and the same first finish, down to the millisecond. The lower id
/// is ahead, stored or counted, also when it was inserted later.
#[tokio::test]
async fn exact_ties_are_stored_in_the_order_counted() {
    let conn = TestConn::new("closed-months-ties").await;
    conn.execute_batch(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms) VALUES
            (9, 3, 0, 1000, 1000), (4, 4, 0, 1000, 1000), (7, 5, 0, 1000, 1000)",
    )
    .await
    .unwrap();
    let month = month_of(3);
    let counted = count_points(&conn, month).await.unwrap();
    assert_eq!(
        counted.iter().map(|p| p.0).collect::<Vec<_>>(),
        [4, 7, 9]
    );
    assert_eq!(month_points(&conn, month).await.unwrap(), counted);
    assert_eq!(month_points(&conn, month).await.unwrap(), counted);
    assert_eq!(champion(&conn, month).await.unwrap(), Some(4));
}

/// A result that comes in after the month is over: started on its last day, finished after
/// midnight. And a server whose clock is ahead and stores the running month too early.
#[tokio::test]
async fn late_finishes_and_early_clocks_drop_stored_months() {
    let conn = TestConn::new("closed-months-late").await;
    let current = month_of(today());
    let (first, _) = month_days(current);
    let last = first - 1;
    let (earlier, _) = month_days(current - 1);
    let alice = user(&conn, "alice", Some("alice")).await;
    let bob = user(&conn, "bob", Some("bob")).await;
    conn.execute(
        "INSERT INTO dailies (day, difficulty, puzzle, solution, created_at) VALUES (?1, 'easy', ?2, '[2,1]', 0)",
        params![last, serde_json::to_string(&puzzle()).unwrap()],
    )
    .await
    .unwrap();
    // Bob alone on the first and the last day of last month, alice alone on another day: bob
    // won with 40 points
    for (user, day, time) in [(bob, earlier, 90_000), (bob, last, 90_000), (alice, earlier + 1, 90_000)] {
        conn.execute(
            "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms) VALUES (?1, ?2, ?3, ?3 + ?4, ?4)",
            params![user, day, day * MS_PER_DAY, time],
        )
        .await
        .unwrap();
    }
    // Alice started the last day too, and is still at it
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at) VALUES (?1, ?2, ?3)",
        params![alice, last, crate::server::now_ms() - 60_000],
    )
    .await
    .unwrap();
    assert_eq!(champion(&conn, current - 1).await.unwrap(), Some(bob));
    assert_eq!(stored(&conn).await, [current - 1]);
    // The running month is counted on every read, never stored
    assert_eq!(champion(&conn, current).await.unwrap(), None);
    month_points(&conn, current).await.unwrap();
    assert_eq!(stored(&conn).await, [current - 1]);

    // She finishes now, faster than bob: 20 + 20 against 20 + 10
    let log: Vec<MoveEvent> = [(0, 1), (0, 2), (1, 1)]
        .into_iter()
        .zip(1..)
        .map(|((edge, count), i)| MoveEvent {
            t: i * 5000,
            edge,
            count,
            tap: true,
        })
        .collect();
    let result = submit(&conn, alice, last, vec![2, 1], 3, log, None)
        .await
        .unwrap();
    assert_eq!((result.flag, result.rank), (None, 1));
    // Her stamps were synced right away, with the month worked out again
    assert_eq!(champion(&conn, current - 1).await.unwrap(), Some(alice));
    assert_eq!(
        month_points(&conn, current - 1).await.unwrap(),
        count_points(&conn, current - 1).await.unwrap()
    );

    // A server whose clock is ahead stores the running month: the next result drops it again
    assert_eq!(closed_month(&conn, current, None).await.unwrap(), []);
    assert!(stored(&conn).await.contains(&current));
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms) VALUES (?1, ?2, ?3, ?3 + 1000, 1000)",
        params![bob, today(), today() * MS_PER_DAY],
    )
    .await
    .unwrap();
    assert!(!stored(&conn).await.contains(&current));
}

/// A change committed while the standings are worked out (on another server, say) is either in
/// them, or drops them: they're counted under the write lock.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_while_storing_is_never_lost() {
    let conn = TestConn::new("closed-months-race").await;
    history(&conn, 7, 8).await;
    let month = month_of(0);
    let before = count_points(&conn, month).await.unwrap();
    // Both wait for the other's writes, like the server's connections
    let timeout = std::time::Duration::from_secs(5);
    conn.busy_timeout(timeout).unwrap();
    let other = conn.independent_connection().await;
    other.busy_timeout(timeout).unwrap();
    // The other server reviews January's winner and hasn't committed yet
    other
        .execute("BEGIN IMMEDIATE", ())
        .await
        .unwrap();
    other
        .execute(
            "UPDATE attempts SET flag = 'review' WHERE user_id = ?1 AND day < 31",
            params![before[0].0],
        )
        .await
        .unwrap();
    let commit = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        other.execute("COMMIT", ()).await.unwrap();
    });
    let stored = month_points(&conn, month).await.unwrap();
    commit.await.unwrap();
    let after = count_points(&conn, month).await.unwrap();
    assert_ne!(before, after);
    assert_eq!(stored, after);
    assert_eq!(month_points(&conn, month).await.unwrap(), after);
}
