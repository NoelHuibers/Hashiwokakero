//! Tests of groups against a fresh database file.

use libsql::{Connection, params};

use super::*;
use crate::model::{DOUBLE_BRIDGE_DAYS, MS_PER_DAY};
use crate::server::auth::{cleanup_guests, erase};
use crate::server::db::TestConn;

/// 2026-10-21, a Wednesday.
const TODAY: i64 = 20_747;

async fn db() -> TestConn {
    TestConn::new("groups").await
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

/// A daily of `day` finished in `time_ms`, `finished_at` ms into the day.
async fn finished(conn: &Connection, user: i64, day: i64, time_ms: i64, finished_at: i64) {
    let at = day * MS_PER_DAY + finished_at;
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, hints, checks, time_ms)
         VALUES (?1, ?2, ?3, ?4, 1, 2, ?5)",
        params![user, day, at - time_ms, at, time_ms],
    )
    .await
    .unwrap();
}

async fn invite(conn: &Connection, group: i64) -> String {
    let mut rows = conn
        .query("SELECT invite FROM groups WHERE id = ?1", params![group])
        .await
        .unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

async fn number(conn: &Connection, sql: &str) -> i64 {
    let mut rows = conn.query(sql, ()).await.unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

async fn owner_of(conn: &Connection, group: i64) -> Option<i64> {
    let mut rows = conn
        .query("SELECT owner_id FROM groups WHERE id = ?1", params![group])
        .await
        .unwrap();
    rows.next().await.unwrap().map(|r| r.get(0).unwrap())
}

#[track_caller]
fn error<T: std::fmt::Debug>(result: Result<T, ServerFnError>) -> ApiError {
    let e = result.expect_err("should have failed");
    ApiError::from_message(&e.to_string()).unwrap_or_else(|| panic!("{e}"))
}

/// A group of `owner` that `others` joined, one after another.
async fn group_of(conn: &Connection, owner: i64, others: &[i64]) -> i64 {
    let id = create(conn, owner, "Team Hashi").await.unwrap();
    let code = invite(conn, id).await;
    for (i, &other) in others.iter().enumerate() {
        assert_eq!(join(conn, &code, other).await.unwrap(), id);
        // A second apart: joins in the same millisecond would be ordered by user id
        conn.execute(
            "UPDATE group_members SET joined_at = joined_at + ?3 WHERE group_id = ?1 AND user_id = ?2",
            params![id, other, i64::try_from(i + 1).unwrap() * 1000],
        )
        .await
        .unwrap();
    }
    id
}

fn ids(members: &[GroupMember]) -> Vec<i64> {
    members.iter().map(|m| m.user_id).collect()
}

#[tokio::test]
async fn a_group_is_made_with_its_owner_in_it() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let id = create(&conn, aiko, "  Team   Hashi ").await.unwrap();
    let page = page(&conn, id, aiko, TODAY).await.unwrap();
    assert_eq!(
        (
            page.name.as_str(),
            page.owner_id,
            page.me,
            ids(&page.members)
        ),
        ("Team Hashi", aiko, aiko, vec![aiko])
    );
    assert!(page.is_owner());
    assert!(valid_invite(&page.invite), "{}", page.invite);
    assert_eq!(page.members[0].colour, 0);
    assert!(page.members[0].owner);

    // Names: not empty, not too long, none of the unwelcome words
    for (name, expected) in [
        ("   ", ApiError::GroupNameInvalid),
        (&"x".repeat(33), ApiError::GroupNameInvalid),
        ("Fuck Hashi", ApiError::GroupNameNotAllowed),
    ] {
        assert_eq!(error(create(&conn, aiko, name).await), expected, "{name}");
    }
}

#[tokio::test]
async fn invite_codes_are_random_and_unguessable() {
    let codes: HashSet<String> = (0..200).map(|_| new_invite()).collect();
    assert_eq!(codes.len(), 200);
    assert!(codes.iter().all(|c| valid_invite(c)));
    // Every character of the alphabet turns up
    let used: HashSet<char> = codes.iter().flat_map(|c| c.chars()).collect();
    assert_eq!(used.len(), INVITE_ALPHABET.len());
}

#[tokio::test]
async fn joining_takes_the_link_and_gives_a_colour_of_ones_own() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let id = create(&conn, aiko, "Team").await.unwrap();
    let code = invite(&conn, id).await;

    // The link shows the group before joining
    let seen = preview(&conn, &code, Some(bram), TODAY)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            seen.name.as_str(),
            ids(&seen.members),
            seen.joined,
            seen.full
        ),
        ("Team", vec![aiko], None, false)
    );
    assert_eq!(
        preview(&conn, &code, None, TODAY)
            .await
            .unwrap()
            .unwrap()
            .joined,
        None
    );
    assert_eq!(
        preview(&conn, "nonsense", Some(bram), TODAY).await.unwrap(),
        None
    );
    assert_eq!(
        preview(&conn, "abcdefgh23", Some(bram), TODAY)
            .await
            .unwrap(),
        None
    );

    assert_eq!(join(&conn, &code, bram).await.unwrap(), id);
    // Guests join like everybody
    assert_eq!(join(&conn, &code, guest).await.unwrap(), id);
    // Twice is once
    assert_eq!(join(&conn, &code, bram).await.unwrap(), id);
    let members = page(&conn, id, bram, TODAY).await.unwrap().members;
    assert_eq!(ids(&members), [aiko, bram, guest]);
    let colours: Vec<u8> = members.iter().map(|m| m.colour).collect();
    assert_eq!(colours, [0, 1, 2]);
    assert!(members[2].guest);
    let seen = preview(&conn, &code, Some(bram), TODAY)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(seen.joined, Some(id));

    // A colour freed by someone leaving goes to the next to join
    leave(&conn, id, bram).await.unwrap();
    let chen = player(&conn, "chen").await;
    join(&conn, &code, chen).await.unwrap();
    let members = page(&conn, id, chen, TODAY).await.unwrap().members;
    let colours: Vec<(i64, u8)> = members.iter().map(|m| (m.user_id, m.colour)).collect();
    assert_eq!(colours, [(aiko, 0), (guest, 2), (chen, 1)]);

    assert_eq!(
        error(join(&conn, "abcdefgh23", chen).await),
        ApiError::GroupNotFound
    );
}

#[tokio::test]
async fn groups_and_memberships_have_limits() {
    let conn = db().await;
    let owner = player(&conn, "owner").await;
    let id = create(&conn, owner, "Big").await.unwrap();
    let code = invite(&conn, id).await;
    for i in 1..MAX_MEMBERS {
        let u = player(&conn, &format!("member{i}")).await;
        join(&conn, &code, u).await.unwrap();
    }
    let late = player(&conn, "late").await;
    assert_eq!(error(join(&conn, &code, late).await), ApiError::GroupFull);
    assert!(
        preview(&conn, &code, Some(late), TODAY)
            .await
            .unwrap()
            .unwrap()
            .full
    );

    // Ten groups each at most, made or joined
    let busy = player(&conn, "busy").await;
    for i in 0..MAX_GROUPS {
        create(&conn, busy, &format!("Group {i}")).await.unwrap();
    }
    assert_eq!(
        error(create(&conn, busy, "One more").await),
        ApiError::TooManyGroups
    );
    let other = create(&conn, late, "Other").await.unwrap();
    let other_code = invite(&conn, other).await;
    assert_eq!(
        error(join(&conn, &other_code, busy).await),
        ApiError::TooManyGroups
    );
    assert_eq!(mine(&conn, busy, TODAY).await.unwrap().len(), MAX_GROUPS);
}

/// Nobody who isn't a member sees anything of a group, or can tell it from one that doesn't
/// exist.
#[tokio::test]
async fn only_members_see_a_group() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let stranger = player(&conn, "stranger").await;
    let id = group_of(&conn, aiko, &[]).await;
    for group in [id, id + 100] {
        assert_eq!(
            error(page(&conn, group, stranger, TODAY).await),
            ApiError::GroupNotFound
        );
        assert_eq!(
            error(week(&conn, group, stranger, None, TODAY).await),
            ApiError::GroupNotFound
        );
        assert_eq!(
            error(leave(&conn, group, stranger).await),
            ApiError::GroupNotFound
        );
        assert_eq!(
            error(rotate_invite(&conn, group, stranger).await),
            ApiError::GroupNotFound
        );
        assert_eq!(
            error(remove(&conn, group, stranger, aiko).await),
            ApiError::GroupNotFound
        );
    }
    assert_eq!(require_member(&conn, id, aiko).await.unwrap().id, id);
}

#[tokio::test]
async fn the_owner_removes_members_and_replaces_the_link() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let id = group_of(&conn, aiko, &[bram, chen]).await;
    let old = invite(&conn, id).await;

    // Only the owner
    assert_eq!(
        error(remove(&conn, id, bram, chen).await),
        ApiError::GroupOwnerOnly
    );
    assert_eq!(
        error(rotate_invite(&conn, id, bram).await),
        ApiError::GroupOwnerOnly
    );

    remove(&conn, id, aiko, chen).await.unwrap();
    assert_eq!(
        ids(&page(&conn, id, aiko, TODAY).await.unwrap().members),
        [aiko, bram]
    );
    assert_eq!(
        error(page(&conn, id, chen, TODAY).await),
        ApiError::GroupNotFound
    );

    // A new link: the old one leads nowhere
    let new = rotate_invite(&conn, id, aiko).await.unwrap();
    assert_ne!(new, old);
    assert_eq!(invite(&conn, id).await, new);
    assert_eq!(preview(&conn, &old, Some(chen), TODAY).await.unwrap(), None);
    assert_eq!(
        error(join(&conn, &old, chen).await),
        ApiError::GroupNotFound
    );
    assert_eq!(join(&conn, &new, chen).await.unwrap(), id);
}

#[tokio::test]
async fn the_longest_standing_member_takes_over_and_the_last_turns_off_the_light() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let id = group_of(&conn, aiko, &[bram, chen]).await;

    leave(&conn, id, aiko).await.unwrap();
    assert_eq!(owner_of(&conn, id).await, Some(bram));
    let page = page(&conn, id, chen, TODAY).await.unwrap();
    assert_eq!(ids(&page.members), [bram, chen]);
    assert!(page.members[0].owner);

    // Someone not the owner leaves: the owner stays
    leave(&conn, id, chen).await.unwrap();
    assert_eq!(owner_of(&conn, id).await, Some(bram));
    // Removing oneself is leaving
    remove(&conn, id, bram, bram).await.unwrap();
    assert_eq!(owner_of(&conn, id).await, None);
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM group_members").await, 0);
}

/// Before finishing today's daily a member only sees who's done; after, the ranking. Given up
/// and flagged results are done but not ranked, like on the global board.
#[tokio::test]
async fn the_board_is_spoiler_safe() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let dana = player(&conn, "dana").await;
    let emre = player(&conn, "emre").await;
    let id = group_of(&conn, aiko, &[bram, chen, dana, emre]).await;
    finished(&conn, bram, TODAY, 90_000, 5_000_000).await;
    finished(&conn, chen, TODAY, 60_000, 9_000_000).await;
    // Dana gave up, Emre's result is flagged
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, revealed) VALUES (?1, ?2, 0, 1)",
        params![dana, TODAY],
    )
    .await
    .unwrap();
    finished(&conn, emre, TODAY, 30_000, 1_000).await;
    conn.execute(
        "UPDATE attempts SET flag = 'pace' WHERE user_id = ?1",
        params![emre],
    )
    .await
    .unwrap();
    // Yesterday doesn't count
    finished(&conn, aiko, TODAY - 1, 10_000, 1_000).await;

    let closed = page(&conn, id, aiko, TODAY).await.unwrap().today;
    assert!(!closed.open);
    assert_eq!(closed.done(), 4);
    let seen: Vec<_> = closed
        .entries
        .iter()
        .map(|e| (e.user_id, e.state))
        .collect();
    assert_eq!(
        seen,
        [
            (bram, TodayState::Done),
            (chen, TodayState::Done),
            (dana, TodayState::Done),
            (emre, TodayState::Done),
            (aiko, TodayState::Waiting),
        ]
    );
    // Not a time anywhere in what goes to the browser
    let json = serde_json::to_string(&page(&conn, id, aiko, TODAY).await.unwrap()).unwrap();
    assert!(
        !json.contains("60000") && !json.contains("time_ms"),
        "{json}"
    );
    let members = page(&conn, id, aiko, TODAY).await.unwrap().members;
    let done: Vec<bool> = members.iter().map(|m| m.done_today).collect();
    assert_eq!(done, [false, true, true, true, true]);

    // Chen finished: the full ranking
    let open = page(&conn, id, chen, TODAY).await.unwrap().today;
    assert!(open.open);
    let seen: Vec<_> = open.entries.iter().map(|e| (e.user_id, e.state)).collect();
    let ranked = |rank, time_ms| TodayState::Ranked {
        rank,
        time_ms,
        hints: 1,
        checks: 2,
    };
    assert_eq!(
        seen,
        [
            (chen, ranked(1, 60_000)),
            (bram, ranked(2, 90_000)),
            (dana, TodayState::Unranked),
            (emre, TodayState::Unranked),
            (aiko, TodayState::Waiting),
        ]
    );
    // Giving up counts as finished: the board opens
    assert!(page(&conn, id, dana, TODAY).await.unwrap().today.open);
    // The daily page gets every group's board
    let all = boards(&conn, chen, TODAY).await.unwrap();
    assert_eq!(all, vec![open]);
    let summary = mine(&conn, aiko, TODAY).await.unwrap();
    assert_eq!(
        summary
            .iter()
            .map(|s| (s.id, s.members, s.done_today, s.owner))
            .collect::<Vec<_>>(),
        [(id, 5, 4, true)]
    );
}

#[tokio::test]
async fn bridges_grow_with_days_both_solved() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let id = group_of(&conn, aiko, &[bram, chen]).await;
    for day in TODAY - DOUBLE_BRIDGE_DAYS + 1..=TODAY {
        finished(&conn, aiko, day, 60_000, 1_000).await;
        finished(&conn, bram, day, 60_000, 2_000).await;
    }
    finished(&conn, chen, TODAY - 2, 60_000, 1_000).await;
    // Too long ago to count
    finished(&conn, chen, TODAY - BRIDGE_DAYS, 60_000, 1_000).await;
    finished(&conn, aiko, TODAY - BRIDGE_DAYS, 60_000, 1_000).await;
    let shared = page(&conn, id, chen, TODAY).await.unwrap().shared;
    assert_eq!(
        shared,
        [
            SharedDays {
                a: aiko,
                b: bram,
                days: DOUBLE_BRIDGE_DAYS
            },
            SharedDays {
                a: aiko,
                b: chen,
                days: 1
            },
            SharedDays {
                a: bram,
                b: chen,
                days: 1
            },
        ]
    );
    assert_eq!(count_shared(&[1, 2], &[(1, 5), (2, 6)]), []);
}

#[tokio::test]
async fn the_week_grid_pages_back_to_the_groups_first_week() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let id = group_of(&conn, aiko, &[bram]).await;
    // Made three weeks ago
    let monday = monday_of(TODAY);
    conn.execute(
        "UPDATE groups SET created_at = ?2 WHERE id = ?1",
        params![id, (monday - 20) * MS_PER_DAY],
    )
    .await
    .unwrap();
    finished(&conn, aiko, monday, 60_000, 1_000).await;
    finished(&conn, bram, TODAY, 60_000, 1_000).await;
    finished(&conn, bram, monday - 7, 60_000, 1_000).await;

    let now = week(&conn, id, aiko, None, TODAY).await.unwrap();
    assert_eq!(
        (now.monday, now.today, now.first),
        (monday, TODAY, monday - 21)
    );
    let rows: Vec<_> = now.rows.iter().map(|r| (r.user_id, r.solved)).collect();
    assert_eq!(
        rows,
        [
            (aiko, [true, false, false, false, false, false, false]),
            (bram, [false, false, true, false, false, false, false]),
        ]
    );
    let before = week(&conn, id, aiko, Some(monday - 3), TODAY)
        .await
        .unwrap();
    assert_eq!(before.monday, monday - 7);
    assert!(before.rows[1].solved[0]);
    // Not before the first week, not after the current one
    assert_eq!(
        week(&conn, id, aiko, Some(monday - 400), TODAY)
            .await
            .unwrap()
            .monday,
        monday - 21
    );
    assert_eq!(
        week(&conn, id, aiko, Some(monday + 30), TODAY)
            .await
            .unwrap()
            .monday,
        monday
    );
}

/// Deleting an account takes the player out of their groups: what they own passes on, a group
/// only they were in goes. Their name is nowhere in a group afterwards.
#[tokio::test]
async fn deleting_an_account_leaves_every_group() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let shared = group_of(&conn, aiko, &[bram, chen]).await;
    let joined = group_of(&conn, bram, &[aiko]).await;
    let alone = create(&conn, aiko, "Just me").await.unwrap();
    // A ranked time stays on the boards without a name
    finished(&conn, aiko, TODAY, 60_000, 1_000).await;

    erase(&conn, aiko).await.unwrap();
    assert_eq!(owner_of(&conn, shared).await, Some(bram));
    assert_eq!(owner_of(&conn, joined).await, Some(bram));
    assert_eq!(owner_of(&conn, alone).await, None);
    assert_eq!(
        number(
            &conn,
            &format!("SELECT COUNT(*) FROM group_members WHERE user_id = {aiko}")
        )
        .await,
        0
    );
    let page = page(&conn, shared, chen, TODAY).await.unwrap();
    assert_eq!(ids(&page.members), [bram, chen]);
    assert!(page.today.entries.iter().all(|e| e.user_id != aiko));
    assert_eq!(ids(&page.members[..1]), [bram]);
    assert!(page.members[0].owner);
}

/// Guests nobody can sign in to any more are deleted, and leave their groups like everyone.
#[tokio::test]
async fn abandoned_guests_leave_their_groups() {
    let conn = db().await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let aiko = player(&conn, "aiko").await;
    let owned = group_of(&conn, guest, &[aiko]).await;
    let alone = create(&conn, guest, "Tanuki").await.unwrap();
    assert_eq!(cleanup_guests(&conn).await.unwrap(), 1);
    assert_eq!(owner_of(&conn, owned).await, Some(aiko));
    assert_eq!(owner_of(&conn, alone).await, None);
    assert_eq!(number(&conn, "SELECT COUNT(*) FROM group_members").await, 1);
}

/// A guest who signs in to their account brings their groups along.
#[tokio::test]
async fn signing_in_takes_the_guests_groups_along() {
    let conn = db().await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let owned = group_of(&conn, guest, &[bram]).await;
    let both = group_of(&conn, bram, &[guest, aiko]).await;
    merge_guest(&conn, aiko, guest).await.unwrap();
    assert_eq!(owner_of(&conn, owned).await, Some(aiko));
    assert_eq!(
        ids(&page(&conn, owned, aiko, TODAY).await.unwrap().members),
        [aiko, bram]
    );
    assert_eq!(
        ids(&page(&conn, both, aiko, TODAY).await.unwrap().members),
        [bram, aiko]
    );
    assert_eq!(
        number(
            &conn,
            &format!("SELECT COUNT(*) FROM group_members WHERE user_id = {guest}")
        )
        .await,
        0
    );
}

/// Signing in never takes an account past [`MAX_GROUPS`]: its own groups stay, the guest's newest
/// ones come along while there's room, and the guest leaves the others like a deleted account.
#[tokio::test]
async fn signing_in_keeps_to_the_group_limit() {
    let conn = db().await;
    let limit = crate::server::count(MAX_GROUPS);
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let shared = group_of(&conn, bram, &[aiko, guest]).await;
    for i in 1..MAX_GROUPS {
        create(&conn, aiko, &format!("Aiko {i}")).await.unwrap();
    }
    assert_eq!(groups_of(&conn, aiko).await.unwrap(), limit);
    // The guest's, oldest first: alone, owned with bram, and bram's that the guest joined last
    let alone = create(&conn, guest, "Tanuki").await.unwrap();
    let owned = group_of(&conn, guest, &[bram]).await;
    let newest = group_of(&conn, bram, &[guest]).await;
    for (i, group) in [alone, owned, newest].into_iter().enumerate() {
        conn.execute(
            "UPDATE group_members SET joined_at = ?3 WHERE group_id = ?1 AND user_id = ?2",
            params![group, guest, 10_000 + i64::try_from(i).unwrap()],
        )
        .await
        .unwrap();
    }

    // No room at all: the account's ten stay, the guest leaves theirs
    merge_guest(&conn, aiko, guest).await.unwrap();
    assert_eq!(groups_of(&conn, aiko).await.unwrap(), limit);
    assert_eq!(groups_of(&conn, guest).await.unwrap(), 0);
    assert_eq!(owner_of(&conn, alone).await, None);
    assert_eq!(owner_of(&conn, owned).await, Some(bram));
    assert_eq!(
        ids(&page(&conn, newest, bram, TODAY).await.unwrap().members),
        [bram]
    );
    assert_eq!(
        ids(&page(&conn, shared, bram, TODAY).await.unwrap().members),
        [bram, aiko]
    );
}

/// With room for some, the guest's newest groups come along, the group both are in doesn't
/// count, and the account takes over the groups the guest owned that it is in now.
#[tokio::test]
async fn signing_in_brings_the_newest_groups_that_fit() {
    let conn = db().await;
    let limit = crate::server::count(MAX_GROUPS);
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let shared = group_of(&conn, guest, &[aiko]).await;
    for i in 2..MAX_GROUPS {
        create(&conn, aiko, &format!("Aiko {i}")).await.unwrap();
    }
    assert_eq!(groups_of(&conn, aiko).await.unwrap(), limit - 1);
    let alone = create(&conn, guest, "Tanuki").await.unwrap();
    let owned = group_of(&conn, guest, &[bram]).await;
    let newest = group_of(&conn, bram, &[guest]).await;
    for (i, group) in [alone, owned, newest].into_iter().enumerate() {
        conn.execute(
            "UPDATE group_members SET joined_at = ?3 WHERE group_id = ?1 AND user_id = ?2",
            params![group, guest, 10_000 + i64::try_from(i).unwrap()],
        )
        .await
        .unwrap();
    }

    merge_guest(&conn, aiko, guest).await.unwrap();
    assert_eq!(groups_of(&conn, aiko).await.unwrap(), limit);
    assert_eq!(groups_of(&conn, guest).await.unwrap(), 0);
    // The one both were in: the account owns it now, alone in it
    assert_eq!(owner_of(&conn, shared).await, Some(aiko));
    assert_eq!(
        ids(&page(&conn, shared, aiko, TODAY).await.unwrap().members),
        [aiko]
    );
    // The newest came along, with the day the guest joined; the older ones were left
    assert_eq!(
        ids(&page(&conn, newest, aiko, TODAY).await.unwrap().members),
        [aiko, bram]
    );
    assert_eq!(owner_of(&conn, owned).await, Some(bram));
    assert_eq!(owner_of(&conn, alone).await, None);
}

async fn ban(conn: &Connection, user: i64) {
    conn.execute(
        "UPDATE users SET banned_at = 1, ban_reason = 'a bot' WHERE id = ?1",
        params![user],
    )
    .await
    .unwrap();
}

/// How many rows of `group` each table of a group has: memberships, seals, river levels and
/// crews.
async fn rows_of(conn: &Connection, group: i64) -> [i64; 4] {
    let mut counts = [0; 4];
    for (count, table) in counts.iter_mut().zip([
        "group_members",
        "group_hanko",
        "group_river",
        "group_river_crew",
    ]) {
        *count = number(
            conn,
            &format!("SELECT COUNT(*) FROM {table} WHERE group_id = {group}"),
        )
        .await;
    }
    counts
}

/// A seal `banned` pressed on `other`'s daily, a completed level of the river and its crew.
async fn seal_and_river(conn: &Connection, group: i64, banned: i64, other: i64) {
    conn.execute(
        "INSERT INTO group_hanko (group_id, day, target_id, giver_id, kind, created_at)
         VALUES (?1, ?4, ?3, ?2, 'wow', 0)",
        params![group, banned, other, TODAY],
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO group_river (group_id, level, target, completed_at) VALUES (?1, 0, 40, 0)",
        params![group],
    )
    .await
    .unwrap();
    for user in [banned, other] {
        conn.execute(
            "INSERT INTO group_river_crew (group_id, level, user_id) VALUES (?1, 0, ?2)",
            params![group, user],
        )
        .await
        .unwrap();
    }
}

/// Banned players are hidden from the group and don't keep it alive: the last one who isn't
/// banned takes it along, with everything in it, the banned players' memberships too.
#[tokio::test]
async fn the_last_member_who_isnt_banned_turns_off_the_light() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let dana = player(&conn, "dana").await;
    let id = group_of(&conn, aiko, &[chen, dana, bram]).await;
    assert!(!page_of(&conn, id, aiko).await.unwrap().last_one());
    // Another group of the banned two stays as it is
    let other = group_of(&conn, chen, &[dana]).await;
    seal_and_river(&conn, id, chen, dana).await;
    seal_and_river(&conn, other, chen, dana).await;
    ban(&conn, chen).await;
    ban(&conn, dana).await;

    leave(&conn, id, bram).await.unwrap();
    assert_eq!(owner_of(&conn, id).await, Some(aiko));
    let page = page(&conn, id, aiko, TODAY).await.unwrap();
    assert_eq!(ids(&page.members), [aiko]);
    // The page asks before deleting the group
    assert!(page.last_one());
    assert_eq!(mine(&conn, aiko, TODAY).await.unwrap()[0].members, 1);

    // Aiko is the last one there: the group goes
    leave(&conn, id, aiko).await.unwrap();
    assert_eq!(owner_of(&conn, id).await, None);
    assert_eq!(rows_of(&conn, id).await, [0; 4]);
    assert_eq!(owner_of(&conn, other).await, Some(chen));
    assert_eq!(rows_of(&conn, other).await, [2, 1, 1, 2]);
    // Lifting the ban brings nothing back
    conn.execute("UPDATE users SET banned_at = NULL", ())
        .await
        .unwrap();
    assert_eq!(groups_of(&conn, chen).await.unwrap(), 1);
    assert_eq!(
        error(page_of(&conn, id, chen).await),
        ApiError::GroupNotFound
    );
}

async fn page_of(conn: &Connection, group: i64, user: i64) -> Result<GroupPage, ServerFnError> {
    page(conn, group, user, TODAY).await
}

/// The owner removing the last other member who isn't banned leaves the group to the owner.
#[tokio::test]
async fn removing_the_last_other_member_keeps_the_group() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let id = group_of(&conn, aiko, &[chen, bram]).await;
    ban(&conn, chen).await;
    remove(&conn, id, aiko, bram).await.unwrap();
    assert_eq!(owner_of(&conn, id).await, Some(aiko));
    assert_eq!(rows_of(&conn, id).await[0], 2);
    // Removing oneself is leaving: the last one there
    remove(&conn, id, aiko, aiko).await.unwrap();
    assert_eq!(rows_of(&conn, id).await, [0; 4]);
}

/// A group never passes to a banned player, and one a banned player owns passes on with the next
/// member who goes.
#[tokio::test]
async fn a_group_never_passes_to_a_banned_member() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let dana = player(&conn, "dana").await;
    let id = group_of(&conn, aiko, &[chen, bram, dana]).await;
    ban(&conn, chen).await;

    // Chen joined before bram, but is banned
    leave(&conn, id, aiko).await.unwrap();
    assert_eq!(owner_of(&conn, id).await, Some(bram));
    assert!(page_of(&conn, id, dana).await.unwrap().members[0].owner);

    // Bram is banned now: the next one who goes passes the group on
    ban(&conn, bram).await;
    let erin = player(&conn, "erin").await;
    let fumi = player(&conn, "fumi").await;
    let code = invite(&conn, id).await;
    for (i, member) in [erin, fumi].into_iter().enumerate() {
        join(&conn, &code, member).await.unwrap();
        // After the others of `group_of`, a few seconds apart
        conn.execute(
            "UPDATE group_members SET joined_at = joined_at + ?3 WHERE group_id = ?1 AND user_id = ?2",
            params![id, member, 10_000 + i64::try_from(i).unwrap() * 1000],
        )
        .await
        .unwrap();
    }
    assert_eq!(owner_of(&conn, id).await, Some(bram));
    leave(&conn, id, fumi).await.unwrap();
    assert_eq!(owner_of(&conn, id).await, Some(dana));
    assert_eq!(
        ids(&page_of(&conn, id, erin).await.unwrap().members),
        [dana, erin]
    );
}

/// Deleting an account and the guest cleanup go by the same rules: a group only banned players
/// are left in goes, a group passes to the longest standing member who isn't banned, and one a
/// banned player owns passes on too.
#[tokio::test]
async fn deleted_accounts_and_guests_leave_banned_members_behind() {
    let conn = db().await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let lonely = group_of(&conn, aiko, &[chen]).await;
    let shared = group_of(&conn, aiko, &[chen, bram]).await;
    let banned_owner = group_of(&conn, chen, &[aiko, bram]).await;
    let guest_lonely = group_of(&conn, guest, &[chen]).await;
    let guest_shared = group_of(&conn, guest, &[chen, bram]).await;
    seal_and_river(&conn, lonely, chen, aiko).await;
    seal_and_river(&conn, guest_lonely, chen, guest).await;
    ban(&conn, chen).await;

    erase(&conn, aiko).await.unwrap();
    assert_eq!(rows_of(&conn, lonely).await, [0; 4]);
    assert_eq!(owner_of(&conn, lonely).await, None);
    assert_eq!(owner_of(&conn, shared).await, Some(bram));
    assert_eq!(owner_of(&conn, banned_owner).await, Some(bram));
    assert_eq!(rows_of(&conn, shared).await[0], 2);

    assert_eq!(cleanup_guests(&conn).await.unwrap(), 1);
    assert_eq!(rows_of(&conn, guest_lonely).await, [0; 4]);
    assert_eq!(owner_of(&conn, guest_lonely).await, None);
    assert_eq!(owner_of(&conn, guest_shared).await, Some(bram));
    // Chen is in the groups that are left, bram owns all of them
    assert_eq!(groups_of(&conn, chen).await.unwrap(), 3);
    assert_eq!(
        number(
            &conn,
            &format!("SELECT COUNT(*) FROM groups WHERE owner_id != {bram}")
        )
        .await,
        0
    );
}

/// The groups a guest leaves on signing in (no room for them) go by the same rules.
#[tokio::test]
async fn signing_in_leaves_banned_members_behind() {
    let conn = db().await;
    let guest = user(&conn, "Sleepy Tanuki 27", None).await;
    let aiko = player(&conn, "aiko").await;
    let bram = player(&conn, "bram").await;
    let chen = player(&conn, "chen").await;
    for i in 0..MAX_GROUPS {
        create(&conn, aiko, &format!("Aiko {i}")).await.unwrap();
    }
    let lonely = group_of(&conn, guest, &[chen]).await;
    let shared = group_of(&conn, guest, &[chen, bram]).await;
    seal_and_river(&conn, lonely, chen, guest).await;
    ban(&conn, chen).await;

    merge_guest(&conn, aiko, guest).await.unwrap();
    assert_eq!(rows_of(&conn, lonely).await, [0; 4]);
    assert_eq!(owner_of(&conn, lonely).await, None);
    assert_eq!(owner_of(&conn, shared).await, Some(bram));
    assert_eq!(
        ids(&page_of(&conn, shared, bram).await.unwrap().members),
        [bram]
    );
}

#[tokio::test]
async fn group_daily_and_week_share_the_same_hundredth_places() {
    let conn = db().await;
    let a = player(&conn, "Noel").await;
    let b = player(&conn, "Nele").await;
    let c = player(&conn, "Third").await;
    let group_id = group_of(&conn, a, &[b, c]).await;
    let day = crate::model::SHARED_PLACES_DAY;
    finished(&conn, a, day, 7826, 10_000).await;
    finished(&conn, b, day, 7831, 20_000).await;
    finished(&conn, c, day, 8803, 30_000).await;
    let group = require_member(&conn, group_id, a).await.unwrap();
    let members = members(&conn, &group, day).await.unwrap();
    let board = today_board(&conn, &group, &members, a, day).await.unwrap();
    let places: Vec<_> = board
        .entries
        .iter()
        .filter_map(|e| match e.state {
            TodayState::Ranked { rank, .. } => Some(rank),
            _ => None,
        })
        .collect();
    assert_eq!(places, [1, 1, 3]);
    let solves = [(a, 7826), (b, 7831), (c, 8803)].map(|(user, time_ms)| crate::model::WeekSolve {
        user,
        day,
        time_ms,
        finished_at: day * MS_PER_DAY + time_ms,
        clean: true,
    });
    let standings = crate::model::standings(&[a, b, c], &solves, monday_of(day));
    assert_eq!(standings[0].points, standings[1].points);
    assert_eq!(standings[0].wins, standings[1].wins);
}
