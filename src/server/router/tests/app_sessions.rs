//! The app's sessions: the same tokens as the cookie's, sent as `Authorization: Bearer hs1.<token>`
//! and handed out in `x-hashi-session`, since the app's `WebView` blocks the site's cookie.

use std::sync::Mutex;

use reqwest::header::{AUTHORIZATION, CACHE_CONTROL, HeaderValue, SET_COOKIE};

use super::*;
use crate::api::{
    APP_HEADER, CurrentUser, DailyToday, DeleteAccount, LoadPlayer, Login, Logout, Register,
    SESSION_HEADER, StartDaily,
};
use crate::model::{MS_PER_DAY, UserInfo};

/// The app: it marks its requests, keeps no cookies and sends the token it was given last.
struct App {
    browser: Browser,
    token: Mutex<Option<String>>,
}

impl App {
    fn new(server: &Server) -> Self {
        Self::with_token(server, None)
    }

    fn with_token(server: &Server, token: Option<String>) -> Self {
        Self {
            browser: server.browser(),
            token: Mutex::new(token),
        }
    }

    fn token(&self) -> Option<String> {
        self.token.lock().unwrap().clone()
    }

    async fn call<F>(&self, args: F) -> Result<F::Output, ServerFnError>
    where
        F: ServerFn + Serialize,
        F::Output: DeserializeOwned,
    {
        self.call_with_headers(args).await.result
    }

    /// A call like the app makes it. The app never gets a cookie, and nothing stores a
    /// response that hands it a token.
    async fn call_with_headers<F>(&self, args: F) -> Called<F::Output>
    where
        F: ServerFn + Serialize,
        F::Output: DeserializeOwned,
    {
        let mut headers = HeaderMap::new();
        headers.insert(APP_HEADER, HeaderValue::from_static("android/1.0.0"));
        if let Some(token) = self.token() {
            headers.insert(AUTHORIZATION, bearer(&token));
        }
        let called = self.browser.call_sending(args, headers).await;
        assert!(called.headers.get(SET_COOKIE).is_none(), "{}", F::PATH);
        if let Some(sent) = called.headers.get(SESSION_HEADER) {
            assert_eq!(called.headers[CACHE_CONTROL], "no-store", "{}", F::PATH);
            let sent = sent.to_str().unwrap();
            *self.token.lock().unwrap() = Some(sent.to_string()).filter(|t| !t.is_empty());
        }
        called
    }
}

/// The app's `Authorization` header for a session token.
fn bearer(token: &str) -> HeaderValue {
    format!("Bearer hs1.{token}").parse().unwrap()
}

/// A client that sends only this `Authorization` header (and this cookie).
fn sending_auth(server: &Server, authorization: &str, cookie: Option<&str>) -> Browser {
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, authorization.parse().unwrap());
    if let Some(cookie) = cookie {
        headers.insert(COOKIE, cookie.parse().unwrap());
    }
    server.browser_sending(headers)
}

/// The token in `x-hashi-session`, which must be there.
#[track_caller]
fn session_header(headers: &HeaderMap) -> String {
    headers
        .get(SESSION_HEADER)
        .expect("no session header")
        .to_str()
        .unwrap()
        .to_string()
}

async fn current(app: &App) -> Option<UserInfo> {
    app.call(CurrentUser {}).await.unwrap()
}

fn register(username: &str) -> Register {
    Register {
        username: username.into(),
        password: PASSWORD.into(),
    }
}

fn login(username: &str) -> Login {
    Login {
        username: username.into(),
        password: PASSWORD.into(),
    }
}

#[tokio::test]
async fn the_app_gets_its_guest_session_in_a_header() {
    let server = Server::start().await;
    let app = App::new(&server);
    let called = app.call_with_headers(CurrentUser {}).await;
    assert_eq!(called.result.unwrap(), None);
    assert!(called.headers.get(SESSION_HEADER).is_none());

    // Playing makes a guest, whose token comes in the header
    let started = app.call_with_headers(StartDaily { device: None }).await;
    started.result.unwrap();
    let token = session_header(&started.headers);
    assert_eq!(token.len(), 64);
    assert!(app.browser.session().is_none());
    let guest = current(&app).await.unwrap();
    assert!(guest.guest);
    // A fresh session isn't sent again
    let called = app.call_with_headers(DailyToday {}).await;
    assert!(called.result.unwrap().attempt.is_some());
    assert!(called.headers.get(SESSION_HEADER).is_none());
    assert_eq!(called.headers[CACHE_CONTROL], "no-cache");

    // The bearer alone is enough, without the app's header
    let bare = sending_auth(&server, &format!("Bearer hs1.{token}"), None);
    let me = bare.call(CurrentUser {}).await.unwrap().unwrap();
    assert_eq!(me.id, guest.id);
    // It's the same session the cookie would hold
    let web = server.sending_cookie(&format!("hashi_session={token}"));
    assert_eq!(
        web.call(CurrentUser {}).await.unwrap().unwrap().id,
        guest.id
    );
}

#[tokio::test]
async fn app_sessions_are_renewed_in_the_header() {
    let server = Server::start().await;
    let app = App::new(&server);
    let registered = app.call_with_headers(register("app_renewer")).await;
    let info = registered.result.unwrap();
    let token = session_header(&registered.headers);
    let conn = conn().await;
    conn.execute(
        "UPDATE sessions SET expires_at = expires_at - 2 * ?2 WHERE user_id = ?1",
        params![info.id, MS_PER_DAY],
    )
    .await
    .unwrap();
    // A bearer without the app's header counts as the app too
    let bare = sending_auth(&server, &format!("Bearer hs1.{token}"), None);
    let renewed = bare.call_with_headers(CurrentUser {}).await;
    assert_eq!(renewed.result.unwrap().unwrap().id, info.id);
    assert_eq!(session_header(&renewed.headers), token);
    assert_eq!(renewed.headers[CACHE_CONTROL], "no-store");
    assert!(renewed.headers.get(SET_COOKIE).is_none());
    // Renewed once, not again on the next call
    let called = app.call_with_headers(CurrentUser {}).await;
    assert!(called.result.unwrap().is_some());
    assert!(called.headers.get(SESSION_HEADER).is_none());
}

#[tokio::test]
async fn the_app_registers_and_signs_in() {
    let server = Server::start().await;
    // A guest who registers keeps the same user, under a new token
    let app = App::new(&server);
    app.call(StartDaily { device: None }).await.unwrap();
    let guest = current(&app).await.unwrap();
    let guest_token = app.token().unwrap();
    let registered = app.call_with_headers(register("app_registrant")).await;
    let info = registered.result.unwrap();
    assert_eq!((info.id, info.guest), (guest.id, false));
    let token = session_header(&registered.headers);
    assert_ne!(token, guest_token);
    assert_eq!(current(&app).await, Some(info.clone()));
    let stale = App::with_token(&server, Some(guest_token));
    assert_eq!(current(&stale).await, None);

    // A guest who signs in brings today's attempt along, and the guest is gone
    let other = App::new(&server);
    other.call(StartDaily { device: None }).await.unwrap();
    let guest = current(&other).await.unwrap();
    let guest_token = other.token().unwrap();
    let signed_in = other.call_with_headers(login("app_registrant")).await;
    assert_eq!(signed_in.result.unwrap().id, info.id);
    let second = session_header(&signed_in.headers);
    assert!(second != guest_token && second != token);
    assert!(other.call(DailyToday {}).await.unwrap().attempt.is_some());
    assert_eq!(other.call(LoadPlayer { id: guest.id }).await.unwrap(), None);
    let stale = App::with_token(&server, Some(guest_token));
    assert_eq!(current(&stale).await, None);
    // Both sessions work
    assert_eq!(current(&other).await.unwrap().id, info.id);
    assert_eq!(current(&app).await.unwrap().id, info.id);

    // Signing in without a guest
    let fresh = App::new(&server);
    let signed_in = fresh.call_with_headers(login("app_registrant")).await;
    assert_eq!(signed_in.result.unwrap().id, info.id);
    assert_eq!(session_header(&signed_in.headers).len(), 64);
    assert_eq!(current(&fresh).await.unwrap().id, info.id);
}

#[tokio::test]
async fn signing_out_and_deleting_clear_the_apps_token() {
    let server = Server::start().await;
    let app = App::new(&server);
    let info = app.call(register("app_leaver")).await.unwrap();
    let token = app.token().unwrap();
    let out = app.call_with_headers(Logout {}).await;
    out.result.unwrap();
    assert_eq!(session_header(&out.headers), "");
    assert_eq!(app.token(), None);
    let stale = App::with_token(&server, Some(token));
    assert_eq!(current(&stale).await, None);

    // A guest deletes itself
    let guest = App::new(&server);
    guest.call(StartDaily { device: None }).await.unwrap();
    let id = current(&guest).await.unwrap().id;
    let token = guest.token().unwrap();
    let deleted = guest
        .call_with_headers(DeleteAccount { password: None })
        .await;
    deleted.result.unwrap();
    assert_eq!(session_header(&deleted.headers), "");
    assert_eq!(guest.call(LoadPlayer { id }).await.unwrap(), None);
    let stale = App::with_token(&server, Some(token));
    assert_eq!(current(&stale).await, None);

    // An account whose session is renewed on the way out still ends up without a token
    let account = App::new(&server);
    account.call(login("app_leaver")).await.unwrap();
    let token = account.token().unwrap();
    conn()
        .await
        .execute(
            "UPDATE sessions SET expires_at = expires_at - 2 * ?2 WHERE user_id = ?1",
            params![info.id, MS_PER_DAY],
        )
        .await
        .unwrap();
    let deleted = account
        .call_with_headers(DeleteAccount {
            password: Some(PASSWORD.into()),
        })
        .await;
    deleted.result.unwrap();
    assert_eq!(session_header(&deleted.headers), "");
    let stale = App::with_token(&server, Some(token));
    assert_eq!(current(&stale).await, None);
}

#[tokio::test]
async fn the_bearer_beats_the_cookie() {
    let server = Server::start().await;
    let web = Browser::signed_up(&server, "cookie_carl").await;
    let cookie = format!("hashi_session={}", web.session().unwrap());
    let web_id = web.call(CurrentUser {}).await.unwrap().unwrap().id;
    let app = App::new(&server);
    app.call(StartDaily { device: None }).await.unwrap();
    let app_id = current(&app).await.unwrap().id;
    let token = app.token().unwrap();

    let both = sending_auth(&server, &format!("Bearer hs1.{token}"), Some(&cookie));
    assert_eq!(both.call(CurrentUser {}).await.unwrap().unwrap().id, app_id);

    // A broken bearer is nobody, even with a good cookie
    for broken in [
        "Bearer hs1.garbage".to_string(),
        "Bearer hs1.".to_string(),
        format!("Bearer hs1.{token}0"),
        format!("Bearer hs1.{}", "ab".repeat(32)),
    ] {
        let client = sending_auth(&server, &broken, Some(&cookie));
        assert_eq!(client.call(CurrentUser {}).await.unwrap(), None, "{broken}");
    }
    // Playing with one makes a new guest for the app, the cookie's account is left alone
    let client = sending_auth(&server, "Bearer hs1.garbage", Some(&cookie));
    let started = client.call_with_headers(StartDaily { device: None }).await;
    started.result.unwrap();
    assert!(started.headers.get(SET_COOKIE).is_none());
    let new = App::with_token(&server, Some(session_header(&started.headers)));
    let guest = current(&new).await.unwrap();
    assert!(guest.guest && guest.id != web_id && guest.id != app_id);
    assert_eq!(web.call(CurrentUser {}).await.unwrap().unwrap().id, web_id);
}

#[tokio::test]
async fn session_and_admin_tokens_dont_mix() {
    let server = Server::start().await;
    let admin = Browser::admin(&server).await;
    let session = admin.session().unwrap();
    let day = daily::today();
    let attempt = format!("/admin/attempt?user=1&day={day}");

    // A session token is no admin token, with the app's prefix or without
    for authorization in [format!("Bearer {session}"), format!("Bearer hs1.{session}")] {
        let client = sending_auth(&server, &authorization, None);
        assert_status(&client.get(&attempt).await, StatusCode::UNAUTHORIZED);
        let res = client
            .post(&format!("/admin/flag?user=1&day={day}&flag=review"))
            .send()
            .await
            .unwrap();
        assert_status(&res, StatusCode::UNAUTHORIZED);
    }
    // Without the prefix it's no session either
    let plain = sending_auth(&server, &format!("Bearer {session}"), None);
    assert_eq!(plain.call(CurrentUser {}).await.unwrap(), None);

    // The admin token is no session, with the prefix or without
    for authorization in [
        format!("Bearer hs1.{ADMIN_TOKEN}"),
        format!("Bearer {ADMIN_TOKEN}"),
    ] {
        let client = sending_auth(&server, &authorization, None);
        assert_eq!(client.call(CurrentUser {}).await.unwrap(), None);
        let res = client
            .post("/admin/house")
            .multipart(reqwest::multipart::Form::new().text("name", ""))
            .send()
            .await
            .unwrap();
        assert_status(&res, StatusCode::NOT_FOUND);
    }
    // The admin token works where it always did
    let script = sending_auth(&server, &format!("Bearer {ADMIN_TOKEN}"), None);
    assert_ne!(
        script.get(&attempt).await.status(),
        StatusCode::UNAUTHORIZED
    );
    // And the admin's session as the app's bearer on the plain routes that take sessions
    let app = sending_auth(&server, &format!("Bearer hs1.{session}"), None);
    let res = app
        .post("/admin/house")
        .multipart(reqwest::multipart::Form::new().text("name", ""))
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::SEE_OTHER);
    assert!(
        res.headers()["location"]
            .to_str()
            .unwrap()
            .starts_with("/admin?tab=dojo&error=")
    );
}

#[tokio::test]
async fn the_web_keeps_its_cookie() {
    let server = Server::start().await;
    let browser = server.browser();
    let started = browser.call_with_headers(StartDaily { device: None }).await;
    started.result.unwrap();
    session_cookie(&started.headers);
    let registered = browser.call_with_headers(register("web_wendy")).await;
    let info = registered.result.unwrap();
    for headers in [&started.headers, &registered.headers] {
        assert!(headers.get(SESSION_HEADER).is_none());
        assert_eq!(headers[CACHE_CONTROL], "no-cache");
    }
    // Renewal and signing out too
    conn()
        .await
        .execute(
            "UPDATE sessions SET expires_at = expires_at - 2 * ?2 WHERE user_id = ?1",
            params![info.id, MS_PER_DAY],
        )
        .await
        .unwrap();
    let renewed = browser.call_with_headers(CurrentUser {}).await;
    renewed.result.unwrap();
    session_cookie(&renewed.headers);
    assert!(renewed.headers.get(SESSION_HEADER).is_none());
    // Somebody else's bearer token doesn't change whose cookie it is
    let cookie = format!("hashi_session={}", browser.session().unwrap());
    let scripted = sending_auth(&server, &format!("Bearer {ADMIN_TOKEN}"), Some(&cookie));
    let called = scripted.call_with_headers(CurrentUser {}).await;
    assert_eq!(called.result.unwrap().unwrap().id, info.id);
    assert!(called.headers.get(SESSION_HEADER).is_none());
    // An empty app header is no app
    let mut empty = HeaderMap::new();
    empty.insert(APP_HEADER, HeaderValue::from_static(""));
    let out = browser.call_sending(Logout {}, empty).await;
    out.result.unwrap();
    assert!(session_cookie(&out.headers).contains("Max-Age=0"));
    assert!(out.headers.get(SESSION_HEADER).is_none());
    assert!(browser.session().is_none());
}
