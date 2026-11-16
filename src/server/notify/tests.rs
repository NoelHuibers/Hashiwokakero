//! Tests of notifications against a fresh database file.

use std::sync::Mutex;

use common::Difficulty;

use super::*;
use crate::model::{ApiError, days_from_civil};
use crate::server::auth::{erase, insert_session, merge_guest};
use crate::server::db::TestConn;
use crate::server::groups::race;

async fn db(name: &str) -> TestConn {
    TestConn::new(&format!("notify-{name}")).await
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

async fn guest(conn: &Connection, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO users (username, name, created_at) VALUES (NULL, ?1, 0)",
        params![name],
    )
    .await
    .unwrap();
    conn.last_insert_rowid()
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

/// A winter day: no daylight saving time in the north.
fn day() -> i64 {
    days_from_civil(2027, 1, 14)
}

/// `h` hours into the day.
fn at(day: i64, hours: f64) -> i64 {
    #[expect(clippy::cast_possible_truncation, reason = "a few hours in ms")]
    let ms = (hours * 3_600_000.0) as i64;
    day * MS_PER_DAY + ms
}

/// A finished daily of `user` on `day` in `time` ms, ranked unless `flag`.
async fn finished(conn: &Connection, user: i64, day: i64, time: i64, flag: Option<&str>) {
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms, flag)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            user,
            day,
            day * MS_PER_DAY,
            day * MS_PER_DAY + time,
            time,
            flag
        ],
    )
    .await
    .unwrap();
}

async fn solved(conn: &Connection, user: i64, day: i64, time: i64) {
    finished(conn, user, day, time, None).await;
}

/// Everything on, in `zone`, with an Android device that has the returned secret.
async fn wants_all(conn: &Connection, user: i64, zone: Option<&str>) -> DeviceRegistration {
    let all = NotificationPrefs::default().with_master(true);
    save_prefs(conn, user, all, zone, 0).await.unwrap();
    register_device(
        conn,
        user,
        DeviceKind::Android,
        None,
        None,
        Locale::en,
        zone,
        0,
    )
    .await
    .unwrap()
}

/// What `user` has in the outbox: (kind, collapse key, args), oldest first.
async fn outbox(conn: &Connection, user: i64) -> Vec<(String, String, Args)> {
    let mut rows = conn
        .query(
            "SELECT kind, collapse_key, args FROM notification_outbox WHERE user_id = ?1 ORDER BY id",
            params![user],
        )
        .await
        .unwrap();
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        out.push((
            row.get(0).unwrap(),
            row.get(1).unwrap(),
            Args::from_json(&row.get::<String>(2).unwrap()),
        ));
    }
    out
}

async fn kinds(conn: &Connection, user: i64) -> Vec<String> {
    outbox(conn, user).await.into_iter().map(|o| o.0).collect()
}

async fn group_of(conn: &Connection, name: &str, members: &[i64]) -> i64 {
    conn.execute(
        "INSERT INTO groups (name, invite, owner_id, created_at) VALUES (?1, ?1, ?2, 0)",
        params![name, members[0]],
    )
    .await
    .unwrap();
    let group = conn.last_insert_rowid();
    for (colour, &member) in members.iter().enumerate() {
        conn.execute(
            "INSERT INTO group_members (group_id, user_id, joined_at, colour) VALUES (?1, ?2, 0, ?3)",
            params![group, member, count(colour)],
        )
        .await
        .unwrap();
    }
    group
}

fn count(n: usize) -> i64 {
    crate::server::count(n)
}

async fn block(conn: &Connection, user: i64, blocked: i64) {
    crate::server::blocks::set(conn, user, blocked, true)
        .await
        .unwrap();
}

// --- Settings ---

#[tokio::test]
async fn settings_are_off_until_chosen_and_keep_the_time_zone() {
    let conn = db("prefs").await;
    let noel = player(&conn, "noel").await;
    assert_eq!(
        prefs(&conn, noel).await.unwrap(),
        NotificationPrefs::default()
    );
    assert_eq!(time_zone(&conn, noel).await.unwrap(), None);

    // On with nothing chosen: everything
    let on = NotificationPrefs {
        enabled: true,
        ..NotificationPrefs::default()
    };
    let saved = save_prefs(&conn, noel, on, Some("Asia/Tokyo"), 1)
        .await
        .unwrap();
    assert!(NotifyKind::ALL.into_iter().all(|k| saved.wants(k)));
    assert_eq!(prefs(&conn, noel).await.unwrap(), saved);
    assert_eq!(
        time_zone(&conn, noel).await.unwrap().as_deref(),
        Some("Asia/Tokyo")
    );
    // An unknown zone, or none, keeps the one stored
    let fewer = saved.with_kind(NotifyKind::Shared, false);
    save_prefs(&conn, noel, fewer, Some("Nowhere/Land"), 2)
        .await
        .unwrap();
    save_prefs(&conn, noel, fewer, None, 3).await.unwrap();
    assert_eq!(prefs(&conn, noel).await.unwrap(), fewer);
    assert_eq!(
        time_zone(&conn, noel).await.unwrap().as_deref(),
        Some("Asia/Tokyo")
    );
    note_time_zone(&conn, noel, Some("Europe/Berlin"), 4)
        .await
        .unwrap();
    assert_eq!(
        time_zone(&conn, noel).await.unwrap().as_deref(),
        Some("Europe/Berlin")
    );
    assert_eq!(prefs(&conn, noel).await.unwrap(), fewer, "the zone alone");
}

#[tokio::test]
async fn the_offer_is_answered_once() {
    let conn = db("offer").await;
    let noel = player(&conn, "noel").await;
    let keiko = player(&conn, "keiko").await;
    let yes = answer_offer(&conn, noel, true, Some("Europe/Berlin"), 1)
        .await
        .unwrap();
    assert!(yes.offer_shown && NotifyKind::ALL.into_iter().all(|k| yes.wants(k)));
    let no = answer_offer(&conn, keiko, false, None, 1).await.unwrap();
    assert!(no.offer_shown && !no.enabled);
    // A page that saves its settings with an old copy doesn't bring the offer back
    let stale = NotificationPrefs::default().with_kind(NotifyKind::Race, true);
    assert!(
        save_prefs(&conn, keiko, stale, None, 2)
            .await
            .unwrap()
            .offer_shown
    );
}

// --- Devices ---

#[tokio::test]
async fn devices_are_checked_and_keep_only_a_hash_of_their_secret() {
    let conn = db("devices").await;
    let noel = player(&conn, "noel").await;
    let register = |kind, endpoint: Option<&'static str>, keys: Option<&'static str>| {
        let conn = &conn;
        async move { register_device(conn, noel, kind, endpoint, keys, Locale::de, None, 1).await }
    };
    for (kind, endpoint, keys) in [
        (DeviceKind::Web, Some("http://push.example/1"), Some("{}")),
        (DeviceKind::Web, Some("https://push.example/1"), None),
        (
            DeviceKind::Apns,
            Some("not hex at all, not hex at all!!"),
            None,
        ),
        (DeviceKind::Apns, Some("abcd"), None),
        (DeviceKind::Android, Some("https://push.example/1"), None),
    ] {
        assert_eq!(
            error(register(kind, endpoint, keys).await),
            ApiError::Forbidden
        );
    }
    let web = register(
        DeviceKind::Web,
        Some("https://push.example/1"),
        Some(r#"{"auth":"x"}"#),
    )
    .await
    .unwrap();
    let token = "ab12".repeat(16);
    let apns = register_device(
        &conn,
        noel,
        DeviceKind::Apns,
        Some(&token),
        None,
        Locale::ja,
        None,
        1,
    )
    .await
    .unwrap();
    assert_eq!(web.secret.len(), 64);
    let stored: String = {
        let mut rows = conn
            .query(
                "SELECT secret_hash FROM notification_devices WHERE id = ?1",
                params![web.id],
            )
            .await
            .unwrap();
        rows.next().await.unwrap().unwrap().get(0).unwrap()
    };
    assert_ne!(stored, web.secret);
    let found = device(&conn, apns.id, &apns.secret).await.unwrap().unwrap();
    assert_eq!(
        (found.kind, found.locale, found.endpoint.as_deref()),
        (DeviceKind::Apns, Locale::ja, Some(token.as_str()))
    );
    assert_eq!(device(&conn, apns.id, &web.secret).await.unwrap(), None);

    // The same browser again (another account here): replaces it
    let keiko = player(&conn, "keiko").await;
    let again = register_device(
        &conn,
        keiko,
        DeviceKind::Web,
        Some("https://push.example/1"),
        Some("{}"),
        Locale::en,
        None,
        2,
    )
    .await
    .unwrap();
    assert_eq!(device(&conn, web.id, &web.secret).await.unwrap(), None);
    assert_eq!(
        device(&conn, again.id, &again.secret)
            .await
            .unwrap()
            .unwrap()
            .user_id,
        keiko
    );

    // Unregistering takes the secret
    assert!(!unregister_device(&conn, apns.id, "wrong").await.unwrap());
    assert!(
        unregister_device(&conn, apns.id, &apns.secret)
            .await
            .unwrap()
    );
    assert_eq!(device(&conn, apns.id, &apns.secret).await.unwrap(), None);
}

#[tokio::test]
async fn a_player_has_a_limited_number_of_devices() {
    let conn = db("max-devices").await;
    let noel = player(&conn, "noel").await;
    let mut ids = Vec::new();
    for i in 0..=MAX_DEVICES {
        let r = register_device(
            &conn,
            noel,
            DeviceKind::Android,
            None,
            None,
            Locale::en,
            None,
            i,
        )
        .await
        .unwrap();
        ids.push(r.id);
    }
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM notification_devices").await,
        MAX_DEVICES
    );
    // The one not seen the longest went
    assert_eq!(
        number(&conn, "SELECT MIN(id) FROM notification_devices").await,
        ids[1]
    );
}

// --- The last call ---

#[tokio::test]
async fn the_last_call_comes_in_the_evening_of_the_players_own_day() {
    let conn = db("last-call").await;
    let day = day();
    let tokyo = player(&conn, "tokyo").await;
    let berlin = player(&conn, "berlin").await;
    let nowhere = player(&conn, "nowhere").await;
    for (user, zone) in [
        (tokyo, Some("Asia/Tokyo")),
        (berlin, Some("Europe/Berlin")),
        (nowhere, None),
    ] {
        wants_all(&conn, user, zone).await;
        solved(&conn, user, day - 1, 60_000).await;
    }
    // Tokyo: 21:00 local is 12:00 UTC; Berlin in winter: 22:00 local, 21:00 UTC; no zone: UTC
    assert_eq!(queue_last_calls(&conn, at(day, 11.9)).await.unwrap(), 0);
    assert_eq!(queue_last_calls(&conn, at(day, 12.0)).await.unwrap(), 1);
    assert_eq!(kinds(&conn, tokyo).await, ["last_call"]);
    assert_eq!(queue_last_calls(&conn, at(day, 20.9)).await.unwrap(), 0);
    assert_eq!(queue_last_calls(&conn, at(day, 21.0)).await.unwrap(), 2);
    // Once per daily
    assert_eq!(queue_last_calls(&conn, at(day, 22.0)).await.unwrap(), 0);
    for user in [tokyo, berlin, nowhere] {
        let out = outbox(&conn, user).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, day.to_string());
    }
    // The next day, again (the streak goes on)
    for user in [tokyo, berlin, nowhere] {
        solved(&conn, user, day, 60_000).await;
    }
    assert_eq!(queue_last_calls(&conn, at(day + 1, 21.0)).await.unwrap(), 3);
}

#[tokio::test]
async fn no_last_call_without_a_streak_or_once_the_daily_is_done() {
    let conn = db("last-call-streak").await;
    let day = day();
    let done = player(&conn, "done").await;
    let revealed = player(&conn, "revealed").await;
    let no_streak = player(&conn, "nostreak").await;
    let flagged = player(&conn, "flagged").await;
    let fresh = player(&conn, "fresh").await;
    let unfinished_today = player(&conn, "started").await;
    for user in [done, revealed, no_streak, flagged, fresh, unfinished_today] {
        wants_all(&conn, user, None).await;
    }
    for user in [done, revealed, unfinished_today] {
        solved(&conn, user, day - 1, 60_000).await;
    }
    // Done today: solved, or the solution shown
    solved(&conn, done, day, 60_000).await;
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at, revealed) VALUES (?1, ?2, 0, 1)",
        params![revealed, day],
    )
    .await
    .unwrap();
    // Started today, not finished: still due
    conn.execute(
        "INSERT INTO attempts (user_id, day, started_at) VALUES (?1, ?2, 0)",
        params![unfinished_today, day],
    )
    .await
    .unwrap();
    // A streak that broke the day before yesterday, and a flagged result (not ranked)
    solved(&conn, no_streak, day - 2, 60_000).await;
    finished(&conn, flagged, day - 1, 60_000, Some("review")).await;
    assert_eq!(queue_last_calls(&conn, at(day, 21.5)).await.unwrap(), 1);
    assert_eq!(kinds(&conn, unfinished_today).await, ["last_call"]);
    // Too close to the change
    let late = player(&conn, "late").await;
    wants_all(&conn, late, None).await;
    solved(&conn, late, day - 1, 60_000).await;
    assert_eq!(queue_last_calls(&conn, at(day, 23.9)).await.unwrap(), 0);
}

#[tokio::test]
async fn a_last_call_for_a_daily_done_since_never_goes_out() {
    let conn = db("last-call-done").await;
    let day = day();
    let noel = player(&conn, "noel").await;
    let phone = wants_all(&conn, noel, None).await;
    solved(&conn, noel, day - 1, 60_000).await;
    assert_eq!(queue_last_calls(&conn, at(day, 21.0)).await.unwrap(), 1);
    // Solved on another device before this one asked
    solved(&conn, noel, day, 60_000).await;
    let got = poll(&conn, phone.id, &phone.secret, at(day, 21.1))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, []);
}

// --- Switches, blocks and limits ---

#[tokio::test]
async fn only_players_who_turned_the_kind_on_and_have_a_device_get_it() {
    let conn = db("gating").await;
    let host = player(&conn, "host").await;
    let all = player(&conn, "all").await;
    let master_off = player(&conn, "masteroff").await;
    let race_off = player(&conn, "raceoff").await;
    let no_device = player(&conn, "nodevice").await;
    let never = player(&conn, "never").await;
    for user in [host, all, master_off, race_off] {
        wants_all(&conn, user, None).await;
    }
    let on = NotificationPrefs::default().with_master(true);
    save_prefs(&conn, master_off, on.with_master(false), None, 0)
        .await
        .unwrap();
    save_prefs(
        &conn,
        race_off,
        on.with_kind(NotifyKind::Race, false),
        None,
        0,
    )
    .await
    .unwrap();
    save_prefs(&conn, no_device, on, None, 0).await.unwrap();
    let group = group_of(
        &conn,
        "Lunch club",
        &[host, all, master_off, race_off, no_device, never],
    )
    .await;
    let now = at(day(), 12.0);
    let id = race::open(&conn, group, host, Difficulty::Easy, now)
        .await
        .unwrap();
    assert_eq!(race_opened(&conn, id, now).await.unwrap(), 1);
    assert_eq!(kinds(&conn, all).await, ["race"]);
    for user in [host, master_off, race_off, no_device, never] {
        assert_eq!(kinds(&conn, user).await, Vec::<String>::new());
    }
    let (_, _, args) = &outbox(&conn, all).await[0];
    assert_eq!(
        (args.name.as_str(), args.group.as_str()),
        ("host", "Lunch club")
    );
    // Switched off after it was queued: it doesn't go out
    let phone = register_device(
        &conn,
        all,
        DeviceKind::Android,
        None,
        None,
        Locale::en,
        None,
        0,
    )
    .await
    .unwrap();
    conn.execute("UPDATE notification_devices SET cursor = 0", ())
        .await
        .unwrap();
    save_prefs(&conn, all, on.with_kind(NotifyKind::Race, false), None, now)
        .await
        .unwrap();
    assert_eq!(
        poll(&conn, phone.id, &phone.secret, now)
            .await
            .unwrap()
            .unwrap(),
        []
    );
}

#[tokio::test]
async fn nothing_comes_from_a_blocked_or_banned_player() {
    let conn = db("blocks").await;
    let troll = player(&conn, "troll").await;
    let noel = player(&conn, "noel").await;
    let keiko = player(&conn, "keiko").await;
    let banned = player(&conn, "banned").await;
    for user in [troll, noel, keiko, banned] {
        wants_all(&conn, user, None).await;
    }
    block(&conn, noel, troll).await;
    let group = group_of(&conn, "Team", &[troll, noel, keiko, banned]).await;
    let now = at(day(), 12.0);
    let id = race::open(&conn, group, troll, Difficulty::Easy, now)
        .await
        .unwrap();
    assert_eq!(race_opened(&conn, id, now).await.unwrap(), 2);
    assert_eq!(kinds(&conn, noel).await, Vec::<String>::new());
    assert_eq!(kinds(&conn, keiko).await, ["race"]);

    // Banned: nothing from them
    conn.execute(
        "UPDATE users SET banned_at = 1 WHERE id = ?1",
        params![banned],
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO group_puzzles (group_id, author_id, title, puzzle, solution, created_at, closes_at)
         VALUES (?1, ?2, NULL, '{}', '[]', 0, 9999999999999)",
        params![group, banned],
    )
    .await
    .unwrap();
    let puzzle = conn.last_insert_rowid();
    assert_eq!(puzzle_posted(&conn, puzzle, now).await.unwrap(), 0);

    // Blocked after it was queued: it doesn't go out either
    let phone = wants_all(&conn, keiko, None).await;
    conn.execute("UPDATE notification_devices SET cursor = 0", ())
        .await
        .unwrap();
    block(&conn, keiko, troll).await;
    assert_eq!(
        poll(&conn, phone.id, &phone.secret, now)
            .await
            .unwrap()
            .unwrap(),
        []
    );
}

#[tokio::test]
async fn a_handful_a_day_at_most() {
    let conn = db("limits").await;
    let me = player(&conn, "me").await;
    wants_all(&conn, me, None).await;
    let now = at(day(), 12.0);
    let mut queued = 0;
    // Many group puzzles: four a day
    for i in 0..10 {
        let note = Note {
            kind: NotifyKind::Group,
            key: format!("puzzle:{i}"),
            actor: None,
            args: Args::default(),
            path: "/".into(),
            expires_at: now + RETENTION_MS,
        };
        queued += enqueue(&conn, &note, Recipients::Users(&[me]), now + i)
            .await
            .unwrap();
    }
    assert_eq!(queued, 4);
    // All kinds together: SOCIAL_PER_DAY
    for kind in [NotifyKind::Race, NotifyKind::Rival, NotifyKind::Shared] {
        for i in 0..5 {
            let note = Note {
                kind,
                key: format!("k{i}"),
                actor: None,
                args: Args::default(),
                path: "/".into(),
                expires_at: now + RETENTION_MS,
            };
            enqueue(&conn, &note, Recipients::Users(&[me]), now + 100 + i)
                .await
                .unwrap();
        }
    }
    let social = number(
        &conn,
        "SELECT COUNT(*) FROM notification_outbox WHERE kind != 'last_call'",
    )
    .await;
    assert_eq!(social, SOCIAL_PER_DAY);
    // The last call isn't held back by the others
    solved(&conn, me, day() - 1, 60_000).await;
    assert_eq!(queue_last_calls(&conn, at(day(), 21.0)).await.unwrap(), 1);
    // A day later there's room again
    let note = Note {
        kind: NotifyKind::Group,
        key: "puzzle:99".into(),
        actor: None,
        args: Args::default(),
        path: "/".into(),
        expires_at: now + 2 * RETENTION_MS,
    };
    assert_eq!(
        enqueue(
            &conn,
            &note,
            Recipients::Users(&[me]),
            now + MS_PER_DAY + 1000
        )
        .await
        .unwrap(),
        1
    );
}

// --- The events ---

#[tokio::test]
async fn a_race_is_told_once_an_hour_per_group_and_expires_with_its_lobby() {
    let conn = db("race").await;
    let host = player(&conn, "host").await;
    let noel = player(&conn, "noel").await;
    for user in [host, noel] {
        wants_all(&conn, user, None).await;
    }
    let group = group_of(&conn, "Team", &[host, noel]).await;
    let now = at(day(), 12.0);
    let id = race::open(&conn, group, host, Difficulty::Easy, now)
        .await
        .unwrap();
    assert_eq!(race_opened(&conn, id, now).await.unwrap(), 1);
    // Called off and opened again: not again within the hour
    race::leave(&conn, id, host, now + 60_000).await.unwrap();
    let again = race::open(&conn, group, host, Difficulty::Easy, now + 120_000)
        .await
        .unwrap();
    assert_eq!(race_opened(&conn, again, now + 120_000).await.unwrap(), 0);
    let phone = wants_all(&conn, noel, None).await;
    conn.execute("UPDATE notification_devices SET cursor = 0", ())
        .await
        .unwrap();
    // The lobby expired: no use telling
    assert_eq!(
        poll(&conn, phone.id, &phone.secret, now + LOBBY_MS)
            .await
            .unwrap()
            .unwrap(),
        []
    );
}

#[tokio::test]
async fn a_new_group_puzzle_goes_to_the_other_members() {
    let conn = db("kadai").await;
    let author = player(&conn, "author").await;
    let noel = player(&conn, "noel").await;
    for user in [author, noel] {
        wants_all(&conn, user, None).await;
    }
    let group = group_of(&conn, "Team", &[author, noel]).await;
    conn.execute(
        "INSERT INTO group_puzzles (group_id, author_id, title, puzzle, solution, created_at, closes_at)
         VALUES (?1, ?2, 'Spiral', '{}', '[]', 0, 9999999999999)",
        params![group, author],
    )
    .await
    .unwrap();
    let puzzle = conn.last_insert_rowid();
    let now = at(day(), 12.0);
    assert_eq!(puzzle_posted(&conn, puzzle, now).await.unwrap(), 1);
    assert_eq!(
        puzzle_posted(&conn, puzzle, now + 1).await.unwrap(),
        0,
        "once"
    );
    let (kind, key, args) = &outbox(&conn, noel).await[0];
    assert_eq!(
        (kind.as_str(), key.as_str()),
        ("group", &*format!("puzzle:{puzzle}"))
    );
    assert_eq!(
        (args.name.as_str(), args.title.as_str()),
        ("author", "Spiral")
    );
    assert_eq!(kinds(&conn, author).await, Vec::<String>::new());
}

#[tokio::test]
async fn a_rival_who_beats_your_time_is_told_once() {
    let conn = db("rival").await;
    let day = day();
    let fast = player(&conn, "fast").await;
    let slow = player(&conn, "slow").await;
    let faster = player(&conn, "faster").await;
    let unfinished = player(&conn, "unfinished").await;
    let not_marked = player(&conn, "notmarked").await;
    for user in [fast, slow, faster, unfinished, not_marked] {
        wants_all(&conn, user, None).await;
    }
    for user in [slow, faster, unfinished] {
        crate::server::daily::set_rival(&conn, user, fast, true)
            .await
            .unwrap();
    }
    solved(&conn, slow, day, 90_000).await;
    solved(&conn, faster, day, 30_000).await;
    solved(&conn, not_marked, day, 90_000).await;
    solved(&conn, fast, day, 60_000).await;
    let now = at(day, 12.0);
    assert_eq!(rival_finished(&conn, fast, day, now).await.unwrap(), 1);
    let (_, _, args) = &outbox(&conn, slow).await[0];
    assert_eq!((args.name.as_str(), args.time_ms), ("fast", 60_000));
    // The submit sent again: nothing new
    assert_eq!(rival_finished(&conn, fast, day, now + 1).await.unwrap(), 0);
    for user in [faster, unfinished, not_marked, fast] {
        assert_eq!(kinds(&conn, user).await, Vec::<String>::new());
    }
    // A flagged result beats nobody
    let cheat = player(&conn, "cheat").await;
    crate::server::daily::set_rival(&conn, slow, cheat, true)
        .await
        .unwrap();
    finished(&conn, cheat, day, 1_000, Some("review")).await;
    assert_eq!(rival_finished(&conn, cheat, day, now).await.unwrap(), 0);
}

/// A shared puzzle `code` of `sharer`, its link's id.
async fn share(conn: &Connection, sharer: i64, code: &str) -> i64 {
    conn.execute(
        "INSERT INTO shared_puzzles (hash, puzzle, solution, created_at) VALUES (?1, '{}', '[]', 0)",
        params![code],
    )
    .await
    .unwrap();
    let puzzle = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO puzzle_shares (code, puzzle_id, user_id, created_at) VALUES (?1, ?2, ?3, 0)",
        params![code, puzzle, sharer],
    )
    .await
    .unwrap();
    conn.last_insert_rowid()
}

async fn solve_shared(conn: &Connection, share: i64, user: i64, at: i64) {
    conn.execute(
        "INSERT INTO shared_results (puzzle_id, user_id, share_id, time_ms, moves, hints, score_ms, finished_at)
         SELECT puzzle_id, ?2, id, 1000, 1, 0, 1000, ?3 FROM puzzle_shares WHERE id = ?1",
        params![share, user, at],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn a_popular_shared_puzzle_is_told_every_few_hours_with_a_count() {
    let conn = db("shared").await;
    let sharer = player(&conn, "sharer").await;
    wants_all(&conn, sharer, None).await;
    let link = share(&conn, sharer, "k7m2xq9a").await;
    let now = at(day(), 8.0);
    // Their own solve doesn't count
    solve_shared(&conn, link, sharer, now).await;
    assert_eq!(shared_solved(&conn, link, now).await.unwrap(), 0);
    let ada = player(&conn, "ada").await;
    solve_shared(&conn, link, ada, now + 1).await;
    assert_eq!(
        shared_solved_code(&conn, "k7m2xq9a", now + 2)
            .await
            .unwrap(),
        1
    );
    // Three more within the hour, one of them blocked: no new notification yet
    let troll = player(&conn, "troll").await;
    block(&conn, sharer, troll).await;
    for (i, name) in ["bea", "troll", "cy"].into_iter().enumerate() {
        let user = if name == "troll" {
            troll
        } else {
            player(&conn, name).await
        };
        let t = now + 600_000 * (i64::try_from(i).unwrap() + 1);
        solve_shared(&conn, link, user, t).await;
        assert_eq!(shared_solved(&conn, link, t + 1).await.unwrap(), 0);
        assert_eq!(flush_shared(&conn, t + 2).await.unwrap(), 0);
    }
    // Three hours after the first: the job tells the two that count
    let later = now + 2 + Rule::of(NotifyKind::Shared).window_ms + 1;
    assert_eq!(flush_shared(&conn, later).await.unwrap(), 1);
    assert_eq!(flush_shared(&conn, later + 1).await.unwrap(), 0, "once");
    let out = outbox(&conn, sharer).await;
    assert_eq!(out.len(), 2);
    assert_eq!((out[0].2.count, out[0].2.name.as_str()), (1, "ada"));
    assert_eq!((out[1].2.count, out[1].2.name.as_str()), (2, "cy"));
    assert!(out.iter().all(|o| o.1 == format!("share:{link}")));
}

// --- Delivery ---

#[tokio::test]
async fn a_device_gets_each_notification_once_in_its_language() {
    let conn = db("poll").await;
    let host = player(&conn, "host").await;
    let noel = player(&conn, "noel").await;
    wants_all(&conn, host, None).await;
    let first = wants_all(&conn, noel, None).await;
    let german = register_device(
        &conn,
        noel,
        DeviceKind::Android,
        None,
        None,
        Locale::de,
        None,
        0,
    )
    .await
    .unwrap();
    let group = group_of(&conn, "Team", &[host, noel]).await;
    let now = at(day(), 12.0);
    let id = race::open(&conn, group, host, Difficulty::Easy, now)
        .await
        .unwrap();
    race_opened(&conn, id, now).await.unwrap();

    let got = poll(&conn, first.id, &first.secret, now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].kind, "race");
    assert_eq!(got[0].title, "host opened a race");
    assert_eq!(
        got[0].url,
        format!("https://hashi.rs/groups/{group}/races/{id}")
    );
    assert_eq!(got[0].tag, format!("race:group:{group}"));
    assert_eq!(got[0].expires_at, now + LOBBY_MS);
    // Once per device, every device
    assert_eq!(
        poll(&conn, first.id, &first.secret, now)
            .await
            .unwrap()
            .unwrap(),
        []
    );
    let got = poll(&conn, german.id, &german.secret, now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got[0].title, "host hat ein Rennen gestartet");
    assert_eq!(
        got[0].url,
        format!("https://hashi.rs/de/groups/{group}/races/{id}")
    );
    // A device that isn't there
    assert_eq!(poll(&conn, first.id, "wrong", now).await.unwrap(), None);
}

/// A channel that notes what it sends, and answers `Gone` for one device.
struct Fake {
    sent: Mutex<Vec<(i64, String)>>,
    gone: i64,
}

impl Channel for Fake {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Web
    }

    fn send<'a>(&'a self, device: &'a Device, message: &'a PushMessage) -> BoxFuture<'a, Delivery> {
        Box::pin(async move {
            if device.id == self.gone {
                return Delivery::Gone;
            }
            self.sent
                .lock()
                .unwrap()
                .push((device.id, message.title.clone()));
            Delivery::Sent
        })
    }
}

#[tokio::test]
async fn channels_push_to_their_devices_once_and_forget_the_gone_ones() {
    let conn = db("deliver").await;
    let host = player(&conn, "host").await;
    let noel = player(&conn, "noel").await;
    wants_all(&conn, host, None).await;
    let android = wants_all(&conn, noel, None).await;
    let browser = |n: usize| {
        let conn = &conn;
        async move {
            register_device(
                conn,
                noel,
                DeviceKind::Web,
                Some(&format!("https://push.example/{n}")),
                Some("{}"),
                Locale::en,
                None,
                0,
            )
            .await
            .unwrap()
        }
    };
    let kept = browser(1).await;
    let gone = browser(2).await;
    let group = group_of(&conn, "Team", &[host, noel]).await;
    let now = at(day(), 12.0);
    let id = race::open(&conn, group, host, Difficulty::Easy, now)
        .await
        .unwrap();
    race_opened(&conn, id, now).await.unwrap();

    let fake = Fake {
        sent: Mutex::new(Vec::new()),
        gone: gone.id,
    };
    let fakes: Vec<Box<dyn Channel>> = vec![Box::new(fake)];
    assert_eq!(deliver(&conn, &fakes, now).await.unwrap(), 1);
    assert_eq!(deliver(&conn, &fakes, now).await.unwrap(), 0, "once");
    assert_eq!(device(&conn, gone.id, &gone.secret).await.unwrap(), None);
    assert!(
        device(&conn, kept.id, &kept.secret)
            .await
            .unwrap()
            .is_some()
    );
    // The Android app isn't pushed to, it asks
    let got = poll(&conn, android.id, &android.secret, now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.len(), 1);
    // The empty list of this build sends nothing
    assert_eq!(channels().len(), 0);
}

#[tokio::test]
async fn two_machines_claim_a_notification_once() {
    let conn = db("claim").await;
    let host = player(&conn, "host").await;
    let noel = player(&conn, "noel").await;
    wants_all(&conn, host, None).await;
    let phone = wants_all(&conn, noel, None).await;
    let group = group_of(&conn, "Team", &[host, noel]).await;
    let now = at(day(), 12.0);
    let id = race::open(&conn, group, host, Difficulty::Easy, now)
        .await
        .unwrap();
    race_opened(&conn, id, now).await.unwrap();
    // Both read the device before either claims
    let seen = device(&conn, phone.id, &phone.secret)
        .await
        .unwrap()
        .unwrap();
    let one = take(&conn, &seen, now).await.unwrap();
    let other = take(&conn, &seen, now).await.unwrap();
    assert_eq!((one.len(), other.len()), (1, 0));
}

// --- Keeping it tidy ---

#[tokio::test]
async fn old_notifications_and_idle_devices_go() {
    let conn = db("cleanup").await;
    let host = player(&conn, "host").await;
    let noel = player(&conn, "noel").await;
    wants_all(&conn, host, None).await;
    wants_all(&conn, noel, None).await;
    let group = group_of(&conn, "Team", &[host, noel]).await;
    let now = at(day(), 12.0);
    let id = race::open(&conn, group, host, Difficulty::Easy, now)
        .await
        .unwrap();
    race_opened(&conn, id, now).await.unwrap();
    // One device seen now, the other at the start of the day
    conn.execute(
        "UPDATE notification_devices SET last_seen = ?1 WHERE user_id = ?2",
        params![now, noel],
    )
    .await
    .unwrap();
    conn.execute(
        "UPDATE notification_devices SET last_seen = ?1 WHERE user_id = ?2",
        params![day() * MS_PER_DAY, host],
    )
    .await
    .unwrap();
    delete_old(&conn, now + RETENTION_MS).await.unwrap();
    assert_eq!(queued(&conn, noel).await.unwrap(), 1);
    delete_old(&conn, now + RETENTION_MS + 1).await.unwrap();
    assert_eq!(queued(&conn, noel).await.unwrap(), 0);
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM notification_devices").await,
        2
    );
    delete_old(&conn, now + DEVICE_IDLE_MS).await.unwrap();
    assert_eq!(
        number(&conn, "SELECT user_id FROM notification_devices").await,
        noel
    );
    // The job's round runs on an empty database too
    assert_eq!(run_once(&conn, &channels(), now).await.unwrap(), (0, 0));
}

#[tokio::test]
async fn deleting_an_account_deletes_its_settings_devices_and_notifications() {
    let conn = db("erase").await;
    let host = player(&conn, "host").await;
    let noel = player(&conn, "noel").await;
    let keiko = player(&conn, "keiko").await;
    for user in [host, noel, keiko] {
        wants_all(&conn, user, Some("Europe/Berlin")).await;
    }
    let group = group_of(&conn, "Team", &[host, noel, keiko]).await;
    let now = at(day(), 12.0);
    let id = race::open(&conn, group, host, Difficulty::Easy, now)
        .await
        .unwrap();
    race_opened(&conn, id, now).await.unwrap();
    assert_eq!(queued(&conn, noel).await.unwrap(), 1);
    erase(&conn, noel).await.unwrap();
    for table in [
        "notification_prefs",
        "notification_devices",
        "notification_outbox",
    ] {
        assert_eq!(
            number(
                &conn,
                &format!("SELECT COUNT(*) FROM {table} WHERE user_id = {noel}")
            )
            .await,
            0,
            "{table}"
        );
    }
    // The host's deleted: the notifications with their name go too
    erase(&conn, host).await.unwrap();
    assert_eq!(queued(&conn, keiko).await.unwrap(), 0);
    assert_eq!(
        prefs(&conn, keiko).await.unwrap(),
        NotificationPrefs::default().with_master(true)
    );
}

#[tokio::test]
async fn abandoned_guests_take_theirs_along() {
    let conn = db("guests").await;
    let old = guest(&conn, "Sleepy Koi 12").await;
    wants_all(&conn, old, None).await;
    let n = crate::server::auth::cleanup_guests(&conn).await.unwrap();
    assert_eq!(n, 1);
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM notification_prefs").await,
        0
    );
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM notification_devices").await,
        0
    );
}

#[tokio::test]
async fn signing_in_brings_the_guests_devices_and_settings_along() {
    let conn = db("merge").await;
    let account = player(&conn, "noel").await;
    let browser = guest(&conn, "Brave Heron 42").await;
    answer_offer(&conn, browser, true, Some("Asia/Tokyo"), 0)
        .await
        .unwrap();
    let device = register_device(
        &conn,
        browser,
        DeviceKind::Android,
        None,
        None,
        Locale::ja,
        None,
        0,
    )
    .await
    .unwrap();
    let token = insert_session(&conn, browser).await.unwrap();
    merge_guest(&conn, browser, account, &token).await.unwrap();
    let moved = super::device(&conn, device.id, &device.secret)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(moved.user_id, account);
    let prefs = prefs(&conn, account).await.unwrap();
    assert!(prefs.offer_shown && prefs.wants(NotifyKind::LastCall));
    assert_eq!(
        time_zone(&conn, account).await.unwrap().as_deref(),
        Some("Asia/Tokyo")
    );
    // An account with settings of its own keeps them
    let other = guest(&conn, "Quiet Owl 7").await;
    wants_all(&conn, other, None).await;
    let token = insert_session(&conn, other).await.unwrap();
    let mine = NotificationPrefs::default().with_kind(NotifyKind::Shared, true);
    save_prefs(&conn, account, mine, None, 1).await.unwrap();
    merge_guest(&conn, other, account, &token).await.unwrap();
    assert!(
        !super::prefs(&conn, account)
            .await
            .unwrap()
            .wants(NotifyKind::Race)
    );
    assert_eq!(
        number(&conn, "SELECT COUNT(*) FROM notification_devices").await,
        2
    );
}
