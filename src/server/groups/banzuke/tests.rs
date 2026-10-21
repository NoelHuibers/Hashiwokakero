//! The banzuke against a fresh database file.

use libsql::{Connection, params};

use super::*;
use crate::model::{ApiError, AwardKind, Feat, MS_PER_DAY, Movement, Stamp, Title};
use crate::server::auth::erase;
use crate::server::daily::{sync_stamps, today as real_today};
use crate::server::db::TestConn;
use crate::server::groups::{create, join, leave};

/// 2026-10-21, a Wednesday.
const TODAY: i64 = 20_747;
const MONDAY: i64 = 20_745;

async fn db() -> TestConn {
    TestConn::new("banzuke").await
}

async fn player(conn: &Connection, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO users (username, name, created_at) VALUES (?1, ?1, 0)",
        params![name],
    )
    .await
    .unwrap();
    conn.last_insert_rowid()
}

/// A daily of `day` solved in `time_ms`, without hints or checks if `clean`.
async fn solved(conn: &Connection, user: i64, day: i64, time_ms: i64, clean: bool) {
    let at = day * MS_PER_DAY + 8 * 3_600_000;
    let help = i64::from(!clean);
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, hints, checks, time_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
        params![user, day, at - time_ms, at, help, time_ms],
    )
    .await
    .unwrap();
}

/// A group of `owner` and `others`, made on the day `since`.
async fn group_of(conn: &Connection, owner: i64, others: &[i64], since: i64) -> i64 {
    let id = create(conn, owner, "Sumo Club").await.unwrap();
    let mut rows = conn
        .query("SELECT invite FROM groups WHERE id = ?1", params![id])
        .await
        .unwrap();
    let code: String = rows.next().await.unwrap().unwrap().get(0).unwrap();
    drop(rows);
    for &other in others {
        join(conn, &code, other).await.unwrap();
    }
    conn.execute(
        "UPDATE groups SET created_at = ?2 WHERE id = ?1",
        params![id, since * MS_PER_DAY],
    )
    .await
    .unwrap();
    id
}

fn lines(b: &Banzuke) -> Vec<(i64, i64, Option<Title>, Option<Movement>)> {
    b.rows
        .iter()
        .map(|r| (r.user_id, r.points, r.title, r.movement))
        .collect()
}

#[track_caller]
fn error<T: std::fmt::Debug>(result: Result<T, ServerFnError>) -> ApiError {
    let e = result.expect_err("should have failed");
    ApiError::from_message(&e.to_string()).unwrap_or_else(|| panic!("{e}"))
}

/// The week by points, the movement since the week before, last week's yokozuna crowned, and
/// today left out until the viewer finished it.
#[tokio::test]
async fn the_banzuke_ranks_the_week_and_keeps_today_secret() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let id = group_of(&conn, aiko, &[bram, chen], MONDAY - 20).await;
    // Last week: aiko before bram
    solved(&conn, aiko, MONDAY - 7, 60_000, true).await;
    solved(&conn, bram, MONDAY - 7, 70_000, true).await;
    // This week: three on Monday, aiko alone on Tuesday, chen today
    solved(&conn, bram, MONDAY, 50_000, false).await;
    solved(&conn, aiko, MONDAY, 60_000, true).await;
    solved(&conn, chen, MONDAY, 70_000, true).await;
    solved(&conn, aiko, MONDAY + 1, 40_000, true).await;
    solved(&conn, chen, TODAY, 30_000, true).await;

    // aiko hasn't played today: chen's daily of today doesn't count yet
    let seen = banzuke(&conn, id, aiko, None, TODAY).await.unwrap();
    assert_eq!(
        (seen.monday, seen.first, seen.today),
        (MONDAY, MONDAY - 21, TODAY)
    );
    assert!(seen.today_hidden && !seen.over());
    assert_eq!(
        lines(&seen),
        [
            (aiko, 5, Some(Title::Yokozuna), Some(Movement::Stayed)),
            (bram, 3, Some(Title::Ozeki), Some(Movement::Stayed)),
            (chen, 1, Some(Title::Sekiwake), Some(Movement::New)),
        ]
    );
    assert_eq!(seen.rows[2].days[2], None);
    assert_eq!(seen.champions, [aiko]);
    // aiko played the most days
    let fastest = seen.award(AwardKind::Fastest).unwrap();
    assert_eq!(
        (fastest.winners.as_slice(), fastest.feat),
        (
            [aiko].as_slice(),
            Feat::Fastest {
                solves: 2,
                total_ms: 100_000
            }
        )
    );

    // chen finished today: it counts for them
    let theirs = banzuke(&conn, id, chen, None, TODAY).await.unwrap();
    assert!(!theirs.today_hidden);
    assert_eq!(
        lines(&theirs),
        [
            (aiko, 5, Some(Title::Yokozuna), Some(Movement::Stayed)),
            (chen, 4, Some(Title::Ozeki), Some(Movement::New)),
            (bram, 3, Some(Title::Sekiwake), Some(Movement::Down(1))),
        ]
    );
    assert_eq!(theirs.rows[1].days[2], Some(1));

    // Last week: final, chen sitting out, the week before had nobody
    let last = banzuke(&conn, id, aiko, Some(MONDAY - 3), TODAY)
        .await
        .unwrap();
    assert_eq!(last.monday, MONDAY - 7);
    assert!(last.over() && !last.today_hidden);
    assert_eq!(
        lines(&last),
        [
            (aiko, 3, Some(Title::Yokozuna), Some(Movement::New)),
            (bram, 2, Some(Title::Ozeki), Some(Movement::New)),
            (chen, 0, None, None),
        ]
    );
    assert_eq!(last.champions, [aiko]);
    // Not before the group's first week, not after this one
    let first = banzuke(&conn, id, aiko, Some(MONDAY - 400), TODAY)
        .await
        .unwrap();
    assert_eq!(first.monday, MONDAY - 21);
    assert!(first.rows.iter().all(|r| r.place.is_none()));
    assert_eq!(
        banzuke(&conn, id, aiko, Some(MONDAY + 30), TODAY)
            .await
            .unwrap()
            .monday,
        MONDAY
    );

    // Nobody else sees it
    let stranger = player(&conn, "stranger").await;
    assert_eq!(
        error(banzuke(&conn, id, stranger, None, TODAY).await),
        ApiError::GroupNotFound
    );
}

/// Given up and flagged dailies aren't ranked, so they score nothing, like on the boards.
#[tokio::test]
async fn only_ranked_dailies_count() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let id = group_of(&conn, aiko, &[bram, chen], MONDAY).await;
    solved(&conn, aiko, MONDAY, 90_000, true).await;
    solved(&conn, bram, MONDAY, 10_000, true).await;
    solved(&conn, chen, MONDAY, 20_000, true).await;
    conn.execute(
        "UPDATE attempts SET flag = 'too_fast' WHERE user_id = ?1",
        params![bram],
    )
    .await
    .unwrap();
    conn.execute(
        "UPDATE attempts SET revealed = 1 WHERE user_id = ?1",
        params![chen],
    )
    .await
    .unwrap();
    let seen = banzuke(&conn, id, aiko, None, TODAY).await.unwrap();
    assert_eq!(
        lines(&seen),
        [
            (aiko, 3, Some(Title::Yokozuna), Some(Movement::New)),
            (bram, 0, None, None),
            (chen, 0, None, None),
        ]
    );
    // The group's first week: no crown from before it
    assert_eq!(seen.champions, Vec::<i64>::new());
}

/// The paces compare with the median of everyone who played that day, not just the group.
#[tokio::test]
async fn medians_of_all_players() {
    let conn = db().await;
    let mut players = Vec::new();
    for (i, time) in [10_000, 30_000, 20_000, 50_000].into_iter().enumerate() {
        let p = player(&conn, &format!("p{i}")).await;
        solved(&conn, p, MONDAY, time, true).await;
        players.push(p);
    }
    solved(&conn, players[0], MONDAY + 1, 7_000, true).await;
    let found = medians(&conn, MONDAY - 1, MONDAY + 2).await.unwrap();
    assert_eq!(
        found,
        HashMap::from([(MONDAY, 20_000), (MONDAY + 1, 7_000)])
    );
}

/// Members who left or deleted their account are off the banzuke, earlier weeks too; banned
/// players are left out like everywhere.
#[tokio::test]
async fn who_left_is_off_the_banzuke() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let dana = player(&conn, "dana").await;
    let id = group_of(&conn, aiko, &[bram, chen, dana], MONDAY - 7).await;
    for (user, time) in [
        (aiko, 60_000),
        (bram, 50_000),
        (chen, 40_000),
        (dana, 30_000),
    ] {
        solved(&conn, user, MONDAY - 7, time, true).await;
    }
    erase(&conn, bram).await.unwrap();
    leave(&conn, id, chen).await.unwrap();
    conn.execute(
        "UPDATE users SET banned_at = 1 WHERE id = ?1",
        params![dana],
    )
    .await
    .unwrap();
    let last = banzuke(&conn, id, aiko, Some(MONDAY - 7), TODAY)
        .await
        .unwrap();
    assert_eq!(
        lines(&last),
        [(aiko, 1, Some(Title::Yokozuna), Some(Movement::New))]
    );
    // One player is no tournament: no crown
    assert_eq!(last.champions, Vec::<i64>::new());
}

/// A yokozuna or an award of a finished week with three players is an honour for the stamps;
/// with two, or in a week that's still running, it isn't.
#[tokio::test]
async fn honours_of_finished_weeks() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let small = player(&conn, "small").await;
    let id = group_of(&conn, aiko, &[bram, chen], MONDAY - 35).await;
    let pair = group_of(&conn, small, &[aiko], MONDAY - 35).await;
    assert!(pair > id);
    // Last week aiko won with three playing
    solved(&conn, aiko, MONDAY - 7, 50_000, false).await;
    solved(&conn, bram, MONDAY - 7, 60_000, true).await;
    solved(&conn, chen, MONDAY - 7, 70_000, false).await;
    // The week before only two played: aiko's win there doesn't count
    solved(&conn, aiko, MONDAY - 14, 50_000, true).await;
    solved(&conn, bram, MONDAY - 14, 60_000, true).await;
    // Too long ago to be looked at
    for user in [aiko, bram, chen] {
        solved(&conn, user, MONDAY - 7 * (HONOUR_WEEKS + 1), 50_000, true).await;
    }
    // This week isn't over
    for (user, time) in [(aiko, 90_000), (bram, 10_000), (chen, 20_000)] {
        solved(&conn, user, MONDAY, time, true).await;
    }
    // aiko: yokozuna and fastest of last week
    assert_eq!(honours(&conn, aiko, TODAY).await.unwrap(), (1, 1));
    // bram: cleanest of last week, the only one without help
    assert_eq!(honours(&conn, bram, TODAY).await.unwrap(), (0, 1));
    // chen won nothing
    assert_eq!(honours(&conn, chen, TODAY).await.unwrap(), (0, 0));
    // Someone in no group of three
    assert_eq!(honours(&conn, small, TODAY).await.unwrap(), (0, 0));
}

/// The stamps come with the stamp sync, and stay when the player leaves the group.
#[tokio::test]
async fn the_yokozuna_stamp_stays() {
    let conn = db().await;
    let today = real_today();
    let monday = monday_of(today);
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let id = group_of(&conn, aiko, &[bram, chen], monday - 14).await;
    for (user, time) in [(aiko, 50_000), (bram, 60_000), (chen, 70_000)] {
        solved(&conn, user, monday - 7, time, true).await;
    }
    let (book, _) = sync_stamps(&conn, aiko, false).await.unwrap();
    let earned = |stamp: Stamp| {
        book.iter()
            .any(|p| p.stamp == stamp && p.earned_at.is_some())
    };
    assert!(earned(Stamp::Yokozuna) && earned(Stamp::Sansho));
    let (book, _) = sync_stamps(&conn, chen, false).await.unwrap();
    assert!(
        !book
            .iter()
            .any(|p| p.stamp == Stamp::Yokozuna && p.earned_at.is_some())
    );

    leave(&conn, id, aiko).await.unwrap();
    let (book, new) = sync_stamps(&conn, aiko, true).await.unwrap();
    assert!(
        book.iter()
            .any(|p| p.stamp == Stamp::Yokozuna && p.earned_at.is_some())
    );
    assert!(new.contains(&Stamp::Yokozuna));
}
