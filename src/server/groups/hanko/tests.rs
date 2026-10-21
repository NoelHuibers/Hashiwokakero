//! Tests of hanko seals against a fresh database file.

use libsql::{Connection, params};

use super::*;
use crate::model::{HankoKind::*, MS_PER_DAY};
use crate::server::auth::{cleanup_guests, erase, insert_session, merge_guest};
use crate::server::db::TestConn;
use crate::server::groups::{create, join, leave, remove, today_board};

/// 2026-10-21, a Wednesday.
const TODAY: i64 = 20_747;

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

/// The daily of `day` solved in a minute.
async fn solved(conn: &Connection, user: i64, day: i64) {
    let at = day * MS_PER_DAY + 3_600_000;
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, hints, checks, time_ms)
         VALUES (?1, ?2, ?3, ?4, 0, 0, 60000)",
        params![user, day, at - 60_000, at],
    )
    .await
    .unwrap();
}

/// The daily of `day` started, not finished.
async fn started(conn: &Connection, user: i64, day: i64) {
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at) VALUES (?1, ?2, ?3)",
        params![user, day, day * MS_PER_DAY],
    )
    .await
    .unwrap();
}

/// The daily of `day` given up.
async fn gave_up(conn: &Connection, user: i64, day: i64) {
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, revealed) VALUES (?1, ?2, ?3, 1)",
        params![user, day, day * MS_PER_DAY],
    )
    .await
    .unwrap();
}

/// A group of `owner` that `others` joined.
async fn group_of(conn: &Connection, owner: i64, others: &[i64]) -> i64 {
    let id = create(conn, owner, "Team Hashi").await.unwrap();
    let mut rows = conn
        .query("SELECT invite FROM groups WHERE id = ?1", params![id])
        .await
        .unwrap();
    let code: String = rows.next().await.unwrap().unwrap().get(0).unwrap();
    drop(rows);
    for &other in others {
        join(conn, &code, other).await.unwrap();
    }
    id
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

fn mark(target: i64, giver: i64, kind: HankoKind) -> HankoMark {
    HankoMark {
        target,
        giver,
        kind,
    }
}

/// `giver` presses `kind` on `target`'s result of today.
async fn stamp(
    conn: &Connection,
    group: i64,
    giver: i64,
    target: i64,
    kind: HankoKind,
) -> Result<Vec<HankoMark>, ServerFnError> {
    press(conn, group, giver, target, TODAY, kind, true, TODAY).await
}

/// The seals on today's board of a group as `viewer` sees it.
async fn board(conn: &Connection, group: i64, viewer: i64) -> Vec<HankoMark> {
    let group = require_member(conn, group, viewer).await.unwrap();
    let members = members(conn, &group, TODAY).await.unwrap();
    today_board(conn, &group, &members, viewer, TODAY)
        .await
        .unwrap()
        .hanko
}

#[tokio::test]
async fn members_stamp_each_others_finished_results() {
    let conn = TestConn::new("hanko-press").await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let group = group_of(&conn, aiko, &[bram, guest]).await;
    solved(&conn, aiko, TODAY).await;
    solved(&conn, bram, TODAY).await;
    gave_up(&conn, guest, TODAY).await;

    assert_eq!(
        stamp(&conn, group, aiko, bram, Wow).await.unwrap(),
        [mark(bram, aiko, Wow)]
    );
    // A guest presses too, and a given up result takes seals (粘 for sticking with it)
    stamp(&conn, group, guest, bram, Fast).await.unwrap();
    let all = stamp(&conn, group, aiko, guest, Grit).await.unwrap();
    assert_eq!(
        all,
        [
            mark(bram, aiko, Wow),
            mark(bram, guest, Fast),
            mark(guest, aiko, Grit)
        ]
    );
    // Everyone who finished sees the same seals on the board
    assert_eq!(board(&conn, group, bram).await, all);
    assert_eq!(board(&conn, group, guest).await, all);
}

#[tokio::test]
async fn seals_follow_the_rules() {
    let conn = TestConn::new("hanko-rules").await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let dana = player(&conn, "dana").await;
    let outsider = player(&conn, "eve").await;
    let group = group_of(&conn, aiko, &[bram, chen, dana]).await;
    // `outsider` is in another group with aiko
    group_of(&conn, outsider, &[aiko]).await;
    solved(&conn, aiko, TODAY).await;
    solved(&conn, bram, TODAY).await;
    started(&conn, chen, TODAY).await;
    solved(&conn, outsider, TODAY).await;

    // Never on one's own result
    assert_eq!(
        error(stamp(&conn, group, aiko, aiko, Wow).await),
        ApiError::HankoNotAllowed
    );
    // Not on a result that isn't finished, or not even started
    assert_eq!(
        error(stamp(&conn, group, aiko, chen, Wow).await),
        ApiError::HankoNotAllowed
    );
    assert_eq!(
        error(stamp(&conn, group, aiko, dana, Wow).await),
        ApiError::HankoNotAllowed
    );
    // Spoiler rule: not before finishing the daily oneself
    assert_eq!(
        error(stamp(&conn, group, chen, bram, Wow).await),
        ApiError::HankoNotAllowed
    );
    assert_eq!(
        error(stamp(&conn, group, dana, bram, Wow).await),
        ApiError::HankoNotAllowed
    );
    // Not on someone outside the group, and not in a group one isn't in
    assert_eq!(
        error(stamp(&conn, group, aiko, outsider, Wow).await),
        ApiError::HankoNotAllowed
    );
    assert_eq!(
        error(stamp(&conn, group, outsider, aiko, Wow).await),
        ApiError::GroupNotFound
    );
    // Yesterday's result still takes seals (a board opened before midnight), older ones and
    // tomorrow's don't
    solved(&conn, aiko, TODAY - 1).await;
    solved(&conn, bram, TODAY - 1).await;
    solved(&conn, aiko, TODAY - 2).await;
    solved(&conn, bram, TODAY - 2).await;
    press(&conn, group, aiko, bram, TODAY - 1, Close, true, TODAY)
        .await
        .unwrap();
    for day in [TODAY - 2, TODAY + 1] {
        assert_eq!(
            error(press(&conn, group, aiko, bram, day, Close, true, TODAY).await),
            ApiError::HankoNotAllowed,
            "{day}"
        );
    }
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM group_hanko").await,
        1,
        "nothing but the allowed seal"
    );
}

#[tokio::test]
async fn pressing_again_takes_it_back() {
    let conn = TestConn::new("hanko-toggle").await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    solved(&conn, aiko, TODAY).await;
    solved(&conn, bram, TODAY).await;

    stamp(&conn, group, aiko, bram, Funny).await.unwrap();
    // Each kind once per member and result
    assert_eq!(
        stamp(&conn, group, aiko, bram, Funny).await.unwrap(),
        [mark(bram, aiko, Funny)]
    );
    assert_eq!(
        stamp(&conn, group, aiko, bram, Cheers).await.unwrap().len(),
        2
    );
    assert_eq!(
        press(&conn, group, aiko, bram, TODAY, Funny, false, TODAY)
            .await
            .unwrap(),
        [mark(bram, aiko, Cheers)]
    );
    // Taking back one that isn't there changes nothing
    assert_eq!(
        press(&conn, group, aiko, bram, TODAY, Funny, false, TODAY)
            .await
            .unwrap(),
        [mark(bram, aiko, Cheers)]
    );
}

/// The seals are as much a spoiler as the times: none on the board before finishing.
#[tokio::test]
async fn the_board_shows_seals_only_once_open() {
    let conn = TestConn::new("hanko-spoiler").await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    solved(&conn, aiko, TODAY).await;
    solved(&conn, bram, TODAY).await;
    stamp(&conn, group, aiko, bram, Fast).await.unwrap();

    assert_eq!(board(&conn, group, chen).await, []);
    solved(&conn, chen, TODAY).await;
    assert_eq!(board(&conn, group, chen).await, [mark(bram, aiko, Fast)]);
}

/// A seal pressed in one group stays in it, even between the same two players.
#[tokio::test]
async fn seals_belong_to_their_group() {
    let conn = TestConn::new("hanko-groups").await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let work = group_of(&conn, aiko, &[bram]).await;
    let friends = group_of(&conn, bram, &[aiko]).await;
    solved(&conn, aiko, TODAY).await;
    solved(&conn, bram, TODAY).await;
    stamp(&conn, work, aiko, bram, Wow).await.unwrap();

    assert_eq!(board(&conn, work, bram).await, [mark(bram, aiko, Wow)]);
    assert_eq!(board(&conn, friends, bram).await, []);
    let got = received(&conn, bram, Some(friends), TODAY).await.unwrap();
    assert_eq!(got, []);
    let got = received(&conn, bram, None, TODAY).await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(
        (got[0].group_id, got[0].giver, got[0].giver_name.as_str()),
        (work, aiko, "aiko")
    );
}

#[tokio::test]
async fn seals_got_are_unread_until_opened() {
    let conn = TestConn::new("hanko-unread").await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let work = group_of(&conn, aiko, &[bram, chen]).await;
    let friends = group_of(&conn, bram, &[aiko]).await;
    for p in [aiko, bram, chen] {
        solved(&conn, p, TODAY).await;
    }
    assert_eq!(unread(&conn, aiko, TODAY).await.unwrap(), []);
    stamp(&conn, work, bram, aiko, Wow).await.unwrap();
    stamp(&conn, work, chen, aiko, Fast).await.unwrap();
    stamp(&conn, friends, bram, aiko, Cheers).await.unwrap();
    // The giver's seals aren't theirs to read
    assert_eq!(unread(&conn, bram, TODAY).await.unwrap(), []);
    assert_eq!(
        unread(&conn, aiko, TODAY).await.unwrap(),
        [
            UnreadHanko {
                group_id: work,
                count: 2
            },
            UnreadHanko {
                group_id: friends,
                count: 1
            }
        ]
    );

    // The group page opens the group's seals: new the first time, newest first
    let got = received(&conn, aiko, Some(work), TODAY).await.unwrap();
    assert_eq!(
        got.iter()
            .map(|h| (h.giver, h.kind, h.fresh))
            .collect::<Vec<_>>(),
        [(chen, Fast, true), (bram, Wow, true)]
    );
    assert_eq!(got[0].group_name, "Team Hashi");
    assert_eq!(
        unread(&conn, aiko, TODAY).await.unwrap(),
        [UnreadHanko {
            group_id: friends,
            count: 1
        }]
    );
    let again = received(&conn, aiko, Some(work), TODAY).await.unwrap();
    assert!(again.iter().all(|h| !h.fresh));
    // The daily result opens all of them
    let all = received(&conn, aiko, None, TODAY).await.unwrap();
    assert_eq!(all.iter().filter(|h| h.fresh).count(), 1);
    assert_eq!(unread(&conn, aiko, TODAY).await.unwrap(), []);

    // Taken back and pressed again: new again
    press(&conn, work, bram, aiko, TODAY, Wow, false, TODAY)
        .await
        .unwrap();
    stamp(&conn, work, bram, aiko, Wow).await.unwrap();
    assert_eq!(unread(&conn, aiko, TODAY).await.unwrap().len(), 1);

    // Seals of days gone by aren't news any more
    assert_eq!(unread(&conn, aiko, TODAY + 2).await.unwrap(), []);
    assert_eq!(received(&conn, aiko, None, TODAY + 2).await.unwrap(), []);
    // Someone else's group can't be opened
    let other = group_of(&conn, chen, &[]).await;
    assert_eq!(
        error(received(&conn, aiko, Some(other), TODAY).await),
        ApiError::GroupNotFound
    );
}

/// A banned member's seals disappear with their results.
#[tokio::test]
async fn banned_members_seals_are_hidden() {
    let conn = TestConn::new("hanko-banned").await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, aiko, &[bram]).await;
    solved(&conn, aiko, TODAY).await;
    solved(&conn, bram, TODAY).await;
    stamp(&conn, group, bram, aiko, Wow).await.unwrap();
    conn.execute(
        "UPDATE users SET banned_at = 1 WHERE id = ?1",
        params![bram],
    )
    .await
    .unwrap();
    assert_eq!(board(&conn, group, aiko).await, []);
    assert_eq!(unread(&conn, aiko, TODAY).await.unwrap(), []);
    assert_eq!(received(&conn, aiko, None, TODAY).await.unwrap(), []);
}

/// Leaving a group, or being removed, takes one's seals there along, given and got.
#[tokio::test]
async fn leaving_takes_the_seals_along() {
    let conn = TestConn::new("hanko-leave").await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let group = group_of(&conn, aiko, &[bram, chen]).await;
    let other = group_of(&conn, bram, &[aiko]).await;
    for p in [aiko, bram, chen] {
        solved(&conn, p, TODAY).await;
    }
    stamp(&conn, group, bram, aiko, Wow).await.unwrap();
    stamp(&conn, group, aiko, bram, Fast).await.unwrap();
    stamp(&conn, group, chen, aiko, Close).await.unwrap();
    stamp(&conn, other, bram, aiko, Cheers).await.unwrap();

    leave(&conn, group, bram).await.unwrap();
    assert_eq!(board(&conn, group, aiko).await, [mark(aiko, chen, Close)]);
    // In the other group they stay
    assert_eq!(board(&conn, other, aiko).await, [mark(aiko, bram, Cheers)]);

    remove(&conn, group, aiko, chen).await.unwrap();
    assert_eq!(board(&conn, group, aiko).await, []);
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM group_hanko").await, 1);

    // The last one out takes the group and what's left of it
    leave(&conn, other, bram).await.unwrap();
    leave(&conn, other, aiko).await.unwrap();
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM group_hanko").await, 0);
}

/// Deleting an account deletes the seals it pressed and got, in every group; abandoned guests
/// too.
#[tokio::test]
async fn deleting_an_account_deletes_its_seals() {
    let conn = TestConn::new("hanko-erase").await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let group = group_of(&conn, aiko, &[bram, chen, guest]).await;
    for p in [aiko, bram, chen, guest] {
        solved(&conn, p, TODAY).await;
    }
    stamp(&conn, group, aiko, bram, Wow).await.unwrap();
    stamp(&conn, group, bram, aiko, Fast).await.unwrap();
    stamp(&conn, group, chen, bram, Close).await.unwrap();
    stamp(&conn, group, guest, chen, Funny).await.unwrap();
    stamp(&conn, group, chen, guest, Grit).await.unwrap();

    erase(&conn, aiko).await.unwrap();
    let left = board(&conn, group, chen).await;
    assert_eq!(
        left,
        [
            mark(bram, chen, Close),
            mark(chen, guest, Funny),
            mark(guest, chen, Grit)
        ]
    );
    assert_eq!(
        number(
            &conn,
            &format!(
                "SELECT COUNT(*) FROM group_hanko WHERE giver_id = {aiko} OR target_id = {aiko}"
            )
        )
        .await,
        0
    );

    // The guest nobody can sign in to any more goes with theirs
    assert_eq!(cleanup_guests(&conn).await.unwrap(), 1);
    assert_eq!(board(&conn, group, chen).await, [mark(bram, chen, Close)]);
}

/// A guest who signs in brings the seals they pressed and got along.
#[tokio::test]
async fn signing_in_takes_the_guests_seals_along() {
    let conn = TestConn::new("hanko-merge").await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let token = insert_session(&conn, guest).await.unwrap();
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let group = group_of(&conn, bram, &[guest]).await;
    // aiko is in this one already, with the guest
    let both = group_of(&conn, bram, &[guest, aiko]).await;
    for p in [guest, bram] {
        solved(&conn, p, TODAY).await;
    }
    stamp(&conn, group, guest, bram, Wow).await.unwrap();
    stamp(&conn, group, bram, guest, Cheers).await.unwrap();
    stamp(&conn, both, guest, bram, Fast).await.unwrap();

    merge_guest(&conn, guest, aiko, &token).await.unwrap();
    // The guest's daily is aiko's now: the seals on it and by it are hers
    assert_eq!(
        board(&conn, group, aiko).await,
        [mark(bram, aiko, Wow), mark(aiko, bram, Cheers)]
    );
    assert_eq!(board(&conn, both, aiko).await, [mark(bram, aiko, Fast)]);
    assert_eq!(
        number(
            &conn,
            &format!(
                "SELECT COUNT(*) FROM group_hanko WHERE giver_id = {guest} OR target_id = {guest}"
            )
        )
        .await,
        0
    );
}
