//! Signing up, in and out: the session cookie and what the server functions make of it.

use super::*;
use crate::api::{
    ChangeUsername, CurrentUser, DeleteAccount, LoadPlayer, Login, Logout, MyStats, Register,
    ShowStamp, StartDaily,
};
use crate::model::MS_PER_DAY;

fn login(username: &str, password: &str) -> Login {
    Login {
        username: username.into(),
        password: password.into(),
    }
}

#[tokio::test]
async fn sign_up_in_and_out() {
    let server = Server::start().await;
    let browser = server.browser();
    assert_eq!(browser.call(CurrentUser {}).await.unwrap(), None);

    let signed_up = browser
        .call_with_headers(Register {
            username: "  Alice_1 ".into(),
            password: PASSWORD.into(),
        })
        .await;
    let info = signed_up.result.unwrap();
    assert_eq!(info.name, "Alice_1");
    assert!(!info.guest && !info.admin);
    // What keeps the session safe: not readable by scripts, not sent along from other sites
    let cookie = session_cookie(&signed_up.headers);
    let token = browser.session().unwrap();
    assert_eq!(token.len(), 64);
    assert_eq!(
        cookie,
        format!("hashi_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=31536000")
    );
    let me = browser.call(CurrentUser {}).await.unwrap().unwrap();
    assert_eq!((me.id, me.guest), (info.id, false));
    assert!(browser.call(MyStats {}).await.unwrap().is_some());

    // A wrong password and an unknown username get the same answer
    let other = server.browser();
    assert_eq!(
        api_error(other.call(login("alice_1", "wrong password")).await),
        ApiError::WrongLogin
    );
    assert_eq!(
        api_error(other.call(login("nobody_here", PASSWORD)).await),
        ApiError::WrongLogin
    );
    assert!(other.session().is_none());
    // Usernames don't care about case
    let signed_in = other.call(login("alice_1", PASSWORD)).await.unwrap();
    assert_eq!(signed_in.id, info.id);
    let second = other.session().unwrap();
    assert_ne!(second, token);

    // Signing out ends that session only, and the browser forgets the cookie
    let out = other.call_with_headers(Logout {}).await;
    out.result.unwrap();
    assert!(session_cookie(&out.headers).starts_with("hashi_session=; Path=/;"));
    assert!(session_cookie(&out.headers).contains("Max-Age=0"));
    assert!(other.session().is_none());
    assert_eq!(other.call(CurrentUser {}).await.unwrap(), None);
    assert_eq!(other.call(MyStats {}).await.unwrap(), None);
    let stale = server.sending_cookie(&format!("hashi_session={second}"));
    assert_eq!(stale.call(CurrentUser {}).await.unwrap(), None);
    assert!(browser.call(CurrentUser {}).await.unwrap().is_some());
}

#[tokio::test]
async fn cookies_are_secure_in_production() {
    let server = Server::with(|options| options.env = Env::PROD).await;
    let browser = server.browser();
    let signed_up = browser
        .call_with_headers(Register {
            username: "secure_sam".into(),
            password: PASSWORD.into(),
        })
        .await;
    signed_up.result.unwrap();
    assert!(session_cookie(&signed_up.headers).ends_with("; Secure"));
}

#[tokio::test]
async fn broken_and_expired_sessions_are_signed_out() {
    let server = Server::start().await;
    for cookie in [
        "hashi_session=garbage",
        "hashi_session=",
        "hashi_session",
        // The right length, but no such session
        &format!("hashi_session={}", "ab".repeat(32)),
        &format!("other=1; hashi_session={}%00", "ab".repeat(31)),
        &format!("hashi_session=' OR 1=1 --{}", "x".repeat(53)),
    ] {
        let browser = server.sending_cookie(cookie);
        assert_eq!(
            browser.call(CurrentUser {}).await.unwrap(),
            None,
            "{cookie}"
        );
    }

    let browser = Browser::signed_up(&server, "expiring_eve").await;
    let conn = conn().await;
    let id = user_id(&conn, "expiring_eve").await;
    conn.execute(
        "UPDATE sessions SET expires_at = 1 WHERE user_id = ?1",
        params![id],
    )
    .await
    .unwrap();
    assert_eq!(browser.call(CurrentUser {}).await.unwrap(), None);
    // Without a session, playing makes a new guest instead of using the old account
    let started = browser.call_with_headers(StartDaily { device: None }).await;
    started.result.unwrap();
    session_cookie(&started.headers);
    let guest = browser.call(CurrentUser {}).await.unwrap().unwrap();
    assert!(guest.guest && guest.id != id);
}

#[tokio::test]
async fn sign_up_errors_reach_the_client() {
    let server = Server::start().await;
    let browser = server.browser();
    let register = |username: &str, password: &str| Register {
        username: username.into(),
        password: password.into(),
    };
    for (username, password, error) in [
        ("x", PASSWORD, ApiError::UsernameInvalid),
        ("with space", PASSWORD, ApiError::UsernameInvalid),
        ("bob_ok", "short", ApiError::PasswordTooShort),
        ("bob_ok", &"x".repeat(129), ApiError::PasswordTooLong),
    ] {
        assert_eq!(
            api_error(browser.call(register(username, password)).await),
            error
        );
    }
    // Words the boards don't want, and names that look official
    for username in [
        "Shit_Happens",
        "BigAss",
        "f_u_c_k",
        "Hashi_Admin",
        "moderator",
        "Player_7",
    ] {
        assert_eq!(
            api_error(browser.call(register(username, PASSWORD)).await),
            ApiError::UsernameNotAllowed,
            "{username}"
        );
    }
    browser.call(register("taken_tom", PASSWORD)).await.unwrap();
    let other = server.browser();
    assert_eq!(
        api_error(other.call(register("TAKEN_TOM", PASSWORD)).await),
        ApiError::UsernameTaken
    );
    // Guessing passwords is cut short
    for _ in 0..8 {
        let _ = other.call(login("taken_tom", "not it at all")).await;
    }
    assert_eq!(
        api_error(other.call(login("taken_tom", PASSWORD)).await),
        ApiError::TooManyAttempts
    );
    // A session is needed for the stamp
    assert_eq!(
        api_error(other.call(ShowStamp { stamp: None }).await),
        ApiError::SessionExpired
    );
}

#[tokio::test]
async fn guests_keep_their_results() {
    let server = Server::start().await;
    // A guest who signs up keeps the same user
    let browser = server.browser();
    let started = browser.call(StartDaily { device: None }).await.unwrap();
    assert!(started.attempt.is_some());
    let guest = browser.call(CurrentUser {}).await.unwrap().unwrap();
    assert!(guest.guest);
    let token = browser.session().unwrap();
    let info = browser
        .call(Register {
            username: "former_guest".into(),
            password: PASSWORD.into(),
        })
        .await
        .unwrap();
    assert_eq!(info.id, guest.id);
    assert_eq!(browser.session().unwrap(), token);

    // A guest who signs in to an account brings today's attempt along
    let browser = server.browser();
    browser.call(StartDaily { device: None }).await.unwrap();
    let guest = browser.call(CurrentUser {}).await.unwrap().unwrap();
    let _ = Browser::signed_up(&server, "returning_rita").await;
    let user = browser
        .call(login("returning_rita", PASSWORD))
        .await
        .unwrap();
    let today = browser.call(crate::api::DailyToday {}).await.unwrap();
    assert!(today.attempt.is_some());
    // The guest is gone
    assert_eq!(
        browser.call(LoadPlayer { id: guest.id }).await.unwrap(),
        None
    );
    let profile = browser
        .call(LoadPlayer { id: user.id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(profile.name, "returning_rita");
}

/// When the session of the user `id` expires.
async fn expires_at(conn: &Connection, id: i64) -> i64 {
    let mut rows = conn
        .query(
            "SELECT MAX(expires_at) FROM sessions WHERE user_id = ?1",
            params![id],
        )
        .await
        .unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

#[tokio::test]
async fn sessions_are_renewed_once_a_day() {
    let server = Server::start().await;
    let browser = Browser::signed_up(&server, "regular_ruth").await;
    let conn = conn().await;
    let id = user_id(&conn, "regular_ruth").await;
    // A fresh session isn't sent again on every request
    let called = browser.call_with_headers(CurrentUser {}).await;
    assert!(called.result.unwrap().is_some());
    assert!(called.headers.get(reqwest::header::SET_COOKIE).is_none());

    // Two days later it is good for another year, the cookie too
    conn.execute(
        "UPDATE sessions SET expires_at = expires_at - 2 * ?2 WHERE user_id = ?1",
        params![id, MS_PER_DAY],
    )
    .await
    .unwrap();
    let before = expires_at(&conn, id).await;
    let token = browser.session().unwrap();
    let called = browser.call_with_headers(CurrentUser {}).await;
    assert!(called.result.unwrap().is_some());
    assert_eq!(
        session_cookie(&called.headers),
        format!("hashi_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=31536000")
    );
    let after = expires_at(&conn, id).await;
    assert!(
        after >= before + 2 * MS_PER_DAY - 60_000,
        "{before} {after}"
    );
    assert_eq!(browser.session().unwrap(), token);
}

#[tokio::test]
async fn deleting_an_account() {
    let server = Server::start().await;
    let nobody = server.browser();
    assert_eq!(
        api_error(nobody.call(DeleteAccount { password: None }).await),
        ApiError::SessionExpired
    );

    // Guests have no password to give
    let guest = server.browser();
    guest.call(StartDaily { device: None }).await.unwrap();
    let id = guest.call(CurrentUser {}).await.unwrap().unwrap().id;
    let token = guest.session().unwrap();
    let deleted = guest
        .call_with_headers(DeleteAccount { password: None })
        .await;
    deleted.result.unwrap();
    assert!(session_cookie(&deleted.headers).contains("Max-Age=0"));
    assert!(guest.session().is_none());
    assert_eq!(guest.call(LoadPlayer { id }).await.unwrap(), None);
    let stale = server.sending_cookie(&format!("hashi_session={token}"));
    assert_eq!(stale.call(CurrentUser {}).await.unwrap(), None);

    // An account needs its password
    let browser = Browser::signed_up(&server, "leaving_leo").await;
    for password in [None, Some("not my password".to_string())] {
        assert_eq!(
            api_error(browser.call(DeleteAccount { password }).await),
            ApiError::WrongPassword
        );
    }
    assert!(browser.call(CurrentUser {}).await.unwrap().is_some());
    let deleted = browser
        .call_with_headers(DeleteAccount {
            password: Some(PASSWORD.into()),
        })
        .await;
    deleted.result.unwrap();
    assert!(session_cookie(&deleted.headers).contains("Max-Age=0"));
    assert_eq!(browser.call(CurrentUser {}).await.unwrap(), None);
    assert_eq!(
        api_error(browser.call(login("leaving_leo", PASSWORD)).await),
        ApiError::WrongLogin
    );
    // The username is free again
    Browser::signed_up(&server, "leaving_leo").await;
}

fn rename(username: &str, password: &str) -> ChangeUsername {
    ChangeUsername {
        username: username.into(),
        password: password.into(),
    }
}

#[tokio::test]
async fn changing_the_username_once_a_month() {
    let server = Server::start().await;
    let nobody = server.browser();
    assert_eq!(
        api_error(nobody.call(rename("nobody_new", PASSWORD)).await),
        ApiError::SessionExpired
    );
    // Guests have no username, they get one by registering
    let guest = server.browser();
    guest.call(StartDaily { device: None }).await.unwrap();
    assert_eq!(
        api_error(guest.call(rename("guest_new", PASSWORD)).await),
        ApiError::Forbidden
    );

    let browser = Browser::signed_up(&server, "renaming_rae").await;
    let id = browser.call(CurrentUser {}).await.unwrap().unwrap().id;
    let _ = Browser::signed_up(&server, "taken_tara").await;
    for (username, password, error) in [
        ("rae_new", "not my password", ApiError::WrongPassword),
        ("x", PASSWORD, ApiError::UsernameInvalid),
        ("with space", PASSWORD, ApiError::UsernameInvalid),
        // Taken, whatever the case
        ("TAKEN_TARA", PASSWORD, ApiError::UsernameTaken),
        ("Rae_the_Cunt", PASSWORD, ApiError::UsernameNotAllowed),
        ("rae_official", PASSWORD, ApiError::UsernameNotAllowed),
    ] {
        assert_eq!(
            api_error(browser.call(rename(username, password)).await),
            error,
            "{username}"
        );
    }
    // None of that counted as a change
    let me = browser.call(CurrentUser {}).await.unwrap().unwrap();
    assert_eq!((me.name.as_str(), me.renamed_at), ("renaming_rae", None));
    // The same name again is no change either
    let same = browser
        .call(rename("renaming_rae", PASSWORD))
        .await
        .unwrap();
    assert_eq!(same.renamed_at, None);

    let renamed = browser
        .call(rename("  Rae_Renamed ", PASSWORD))
        .await
        .unwrap();
    assert_eq!((renamed.id, renamed.name.as_str()), (id, "Rae_Renamed"));
    let at = renamed.renamed_at.unwrap();
    assert_eq!(
        renamed.rename_from(at),
        Some(at + crate::model::RENAME_EVERY_DAYS * MS_PER_DAY)
    );
    assert_eq!(
        renamed.rename_from(at + crate::model::RENAME_EVERY_DAYS * MS_PER_DAY),
        None
    );
    // Everywhere the name shows, the new one, and the session stays
    assert_eq!(
        browser.call(CurrentUser {}).await.unwrap().unwrap().name,
        "Rae_Renamed"
    );
    let profile = nobody.call(LoadPlayer { id }).await.unwrap().unwrap();
    assert_eq!(profile.name, "Rae_Renamed");
    // Signing in works with the new name only
    let other = server.browser();
    assert_eq!(
        api_error(other.call(login("renaming_rae", PASSWORD)).await),
        ApiError::WrongLogin
    );
    assert_eq!(
        other.call(login("rae_renamed", PASSWORD)).await.unwrap().id,
        id
    );

    // Not again so soon, even with the password, and not in another browser either
    for b in [&browser, &other] {
        assert_eq!(
            api_error(b.call(rename("rae_again", PASSWORD)).await),
            ApiError::RenameTooSoon
        );
    }
    // A day short of the month is still too soon, a month later it works
    let conn = conn().await;
    let days_ago = |days: i64| {
        let conn = &conn;
        async move {
            conn.execute(
                "UPDATE users SET renamed_at = ?2 WHERE id = ?1",
                params![id, crate::server::now_ms() - days * MS_PER_DAY],
            )
            .await
            .unwrap();
        }
    };
    days_ago(crate::model::RENAME_EVERY_DAYS - 1).await;
    assert_eq!(
        api_error(browser.call(rename("rae_again", PASSWORD)).await),
        ApiError::RenameTooSoon
    );
    days_ago(crate::model::RENAME_EVERY_DAYS).await;
    let again = browser.call(rename("rae_again", PASSWORD)).await.unwrap();
    assert_eq!(again.name, "rae_again");
    // The old names are free for others
    Browser::signed_up(&server, "renaming_rae").await;
    Browser::signed_up(&server, "rae_renamed").await;
}

/// `HASHI_ADMINS` lists usernames: an admin's old name, freed, would make its next owner an admin.
#[tokio::test]
async fn admins_keep_their_names() {
    let server = Server::start().await;
    let admin = Browser::admin(&server).await;
    let name = admin.call(CurrentUser {}).await.unwrap().unwrap().name;
    assert_eq!(
        api_error(admin.call(rename("someone_else", PASSWORD)).await),
        ApiError::Forbidden
    );
    let me = admin.call(CurrentUser {}).await.unwrap().unwrap();
    assert!(me.admin);
    assert_eq!(me.name, name);
}

#[tokio::test]
async fn abandoned_guests_are_signed_out_for_good() {
    let server = Server::start().await;
    let browser = server.browser();
    browser.call(StartDaily { device: None }).await.unwrap();
    let id = browser.call(CurrentUser {}).await.unwrap().unwrap().id;
    // A year later, the session ran out
    let conn = conn().await;
    conn.execute(
        "UPDATE sessions SET expires_at = 1 WHERE user_id = ?1",
        params![id],
    )
    .await
    .unwrap();
    conn.execute("UPDATE users SET created_at = 1 WHERE id = ?1", params![id])
        .await
        .unwrap();
    assert!(auth::cleanup_guests(&conn).await.unwrap() >= 1);
    assert_eq!(browser.call(LoadPlayer { id }).await.unwrap(), None);
    // The old cookie is just no session, playing again makes a new guest
    assert_eq!(browser.call(CurrentUser {}).await.unwrap(), None);
    let started = browser.call_with_headers(StartDaily { device: None }).await;
    started.result.unwrap();
    session_cookie(&started.headers);
    // (SQLite may hand out the id of the deleted guest again, its age tells them apart)
    let guest = browser.call(CurrentUser {}).await.unwrap().unwrap();
    assert!(guest.guest && guest.created_at > 1);
}
