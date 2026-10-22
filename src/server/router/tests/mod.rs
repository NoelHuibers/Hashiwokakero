//! Tests of the HTTP layer: the real router on a local port, clients that keep their cookies
//! like a browser, and server functions called the way the browser calls them. What the
//! handlers compute is tested next to them (server/daily/tests.rs, server/dojo/tests.rs), these
//! tests are about URLs, codecs, status codes, cookies and headers.

// Each test holds its turn (see `Server`) until it ends, on purpose
#![expect(
    clippy::significant_drop_tightening,
    reason = "a `Server` keeps the lock that makes the tests take turns"
)]

mod accounts;
mod admin;
mod app_sessions;
mod apps;
mod build;
mod contact;
mod dojo;
mod groups;
mod kadai;
mod og;
mod pages;
mod playing;
mod rivals;
mod site;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::{Difficulty, GenerateParams};
use leptos::config::{Env, LeptosOptions};
use leptos::prelude::ServerFnError;
use leptos::server_fn::ServerFn;
use leptos::server_fn::error::FromServerFnError;
use libsql::{Connection, params};
use reqwest::header::{ACCEPT, CONTENT_TYPE, COOKIE, HeaderMap};
use reqwest::{Response, StatusCode, cookie::CookieStore};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::{Mutex, MutexGuard, OnceCell};

use crate::model::ApiError;
use crate::server::{auth, daily, db};

/// The admin listed by username, see [`env_var`].
const ADMIN: &str = "http_admin";
/// The admin listed by user id, see `accounts::admins_by_id_change_their_names`.
const ADMIN_ID: i64 = 900_001;
/// `HASHI_ADMIN_TOKEN` of the tests.
const ADMIN_TOKEN: &str = "admin-token-for-tests";
const PASSWORD: &str = "correct horse";
/// `DOJO_SEASON` of the tests.
const CURRENT_SEASON: &str = "2";

/// App settings from these variables, the others unset.
fn app_settings(vars: &[(&str, &str)]) -> crate::server::apps::AppSettings {
    crate::server::apps::AppSettings::from_env(|name| {
        vars.iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| (*v).to_string())
    })
    .unwrap()
}

/// The settings the tests run with, instead of environment variables (see `server::env_var`).
pub(in crate::server) fn env_var(name: &str) -> Option<&'static str> {
    match name {
        "HASHI_ADMINS" => Some("http_admin, 900001"),
        "FLY_APP_NAME" => Some("hashi-http-tests"),
        "HASHI_ADMIN_TOKEN" => Some(ADMIN_TOKEN),
        // Not the first season, so there is a finished one to archive (see `dojo::seasons`)
        "DOJO_SEASON" => Some(CURRENT_SEASON),
        _ => None,
    }
}

/// The site root with a few static files, and the database every request uses (it can only be
/// set once per process). Today's and tomorrow's dailies are made easy up front: generating a
/// hard one takes a while, and a test running at midnight finds tomorrow's ready too.
async fn setup() -> &'static Path {
    static SITE: OnceCell<PathBuf> = OnceCell::const_new();
    SITE.get_or_init(|| async {
        // One directory per test process, which the server uses until the process ends. A
        // static is never dropped and the test harness has no hook at its end, so a guard
        // can't remove it. Instead the next test process removes it: `test_temp_path` sweeps
        // the `hashi-test-<pid>-*` entries of processes that no longer run. So at most one site
        // per finished run stays behind, until the next run.
        let dir = db::test_temp_path("http");
        let _ = std::fs::remove_dir_all(&dir);
        let site = dir.join("site");
        std::fs::create_dir_all(site.join("pkg")).unwrap();
        std::fs::create_dir_all(site.join("fonts")).unwrap();
        std::fs::write(
            site.join("pkg/hashi.0123abcd.js"),
            "export const x = 1;\n".repeat(200),
        )
        .unwrap();
        std::fs::write(site.join("fonts/fonts.css"), "@font-face {}\n".repeat(50)).unwrap();
        std::fs::write(site.join("fonts/murecho.0123abcd.woff2"), [7u8; 4096]).unwrap();
        // What cargo-leptos writes next to the server binary
        std::fs::write(
            site.join("hash.txt"),
            "js: 0123abcd\nwasm: 0123abcd\ncss: 0123abcd\n",
        )
        .unwrap();

        let database = db::open_local(dir.join("hashi.db").to_str().unwrap())
            .await
            .unwrap();
        db::install(database).await.unwrap();
        let conn = db::conn().unwrap();
        for day in [daily::today(), daily::today() + 1] {
            easy_daily(&conn, day).await;
        }
        site
    })
    .await
}

async fn easy_daily(conn: &Connection, day: i64) {
    let (width, height) = Difficulty::Easy.size();
    let generated = backend::api::generate(&GenerateParams {
        width,
        height,
        difficulty: Difficulty::Easy,
        seed: day.unsigned_abs(),
    });
    conn.execute(
        "INSERT OR IGNORE INTO dailies (day, difficulty, puzzle, solution, created_at) VALUES (?1, ?2, ?3, ?4, 0)",
        params![
            day,
            Difficulty::Easy.id(),
            serde_json::to_string(&generated.puzzle).unwrap(),
            serde_json::to_string(&generated.solution).unwrap()
        ],
    )
    .await
    .unwrap();
}

/// A connection to the database of the tests, to look behind the server's back.
async fn conn() -> Connection {
    setup().await;
    db::conn().unwrap()
}

/// The server on a free local port. Tests take turns: they share the database, and SQLite
/// refuses concurrent writes from separate connections instead of waiting.
struct Server {
    url: String,
    fly: bool,
    _turn: MutexGuard<'static, ()>,
}

impl Server {
    async fn start() -> Self {
        Self::with(|_| {}).await
    }

    /// With changed settings, as cargo-leptos or the LEPTOS_* variables would make them.
    async fn with(change: impl FnOnce(&mut LeptosOptions)) -> Self {
        Self::with_transport(change, true, true, &app_settings(&[])).await
    }

    /// With these app settings (`HASHI_APP_*`, see `server::apps`) and changed Leptos settings.
    async fn with_apps(settings: &[(&str, &str)], change: impl FnOnce(&mut LeptosOptions)) -> Self {
        Self::with_transport(change, true, true, &app_settings(settings)).await
    }

    async fn direct(connect_info: bool) -> Self {
        Self::with_transport(|_| {}, false, connect_info, &app_settings(&[])).await
    }

    async fn with_transport(
        change: impl FnOnce(&mut LeptosOptions),
        fly: bool,
        connect_info: bool,
        apps: &crate::server::apps::AppSettings,
    ) -> Self {
        static TURN: Mutex<()> = Mutex::const_new(());
        let turn = TURN.lock().await;
        let site = setup().await;
        let mut options = LeptosOptions::builder()
            .output_name("hashi")
            .site_root(site.to_str().unwrap())
            .site_pkg_dir("pkg")
            .hash_files(true)
            .env(Env::DEV)
            .build();
        // Read next to the binary, unless the path is absolute
        options.hash_file = site.join("hash.txt").to_str().unwrap().into();
        change(&mut options);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = super::router_with_client_source(options, crate::app::shell, fly, apps);
        tokio::spawn(async move {
            if connect_info {
                axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
                )
                .await
            } else {
                axum::serve(listener, app.into_make_service()).await
            }
        });
        Self {
            url,
            fly,
            _turn: turn,
        }
    }

    /// A new browser, without cookies.
    fn browser(&self) -> Browser {
        self.browser_sending(HeaderMap::new())
    }

    /// A client that sends this `Cookie` header, whatever it is.
    fn sending_cookie(&self, cookie: &str) -> Browser {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, cookie.parse().unwrap());
        self.browser_sending(headers)
    }

    /// A browser behind the fly.io proxy, which says where requests come from.
    fn browser_from(&self, ip: &str) -> Browser {
        let mut headers = HeaderMap::new();
        headers.insert("fly-client-ip", ip.parse().unwrap());
        self.browser_sending(headers)
    }

    fn browser_sending(&self, headers: HeaderMap) -> Browser {
        self.browser_at(headers, None)
    }

    fn browser_at(&self, mut headers: HeaderMap, peer: Option<std::net::IpAddr>) -> Browser {
        // Default HTTP journeys model independent Fly clients. Direct-deployment regressions
        // instead bind real loopback peers, and tests without ConnectInfo remain explicit.
        if self.fly && !headers.contains_key("fly-client-ip") {
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let ip =
                std::net::Ipv4Addr::from(u32::from(std::net::Ipv4Addr::new(198, 18, 0, 0)) + n);
            headers.insert("fly-client-ip", ip.to_string().parse().unwrap());
        }
        let jar = Arc::new(reqwest::cookie::Jar::default());
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .local_address(peer)
            .cookie_provider(jar.clone())
            // Redirects are part of what is tested
            .redirect(reqwest::redirect::Policy::none())
            // A test that hangs holds up all the others
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap();
        Browser {
            url: self.url.clone(),
            http,
            jar,
        }
    }
}

struct Browser {
    url: String,
    http: reqwest::Client,
    jar: Arc<reqwest::cookie::Jar>,
}

/// The response to a server function call: the decoded output, or the error the way the
/// browser decodes it, with the response's headers.
struct Called<T> {
    result: Result<T, ServerFnError>,
    headers: HeaderMap,
}

impl Browser {
    /// Calls a server function like the browser does: a JSON body posted to its URL, an error
    /// status means the body is the encoded error.
    async fn call<F>(&self, args: F) -> Result<F::Output, ServerFnError>
    where
        F: ServerFn + Serialize,
        F::Output: DeserializeOwned,
    {
        self.call_with_headers(args).await.result
    }

    async fn call_with_headers<F>(&self, args: F) -> Called<F::Output>
    where
        F: ServerFn + Serialize,
        F::Output: DeserializeOwned,
    {
        self.call_sending(args, HeaderMap::new()).await
    }

    /// With these headers on top of the browser's.
    async fn call_sending<F>(&self, args: F, headers: HeaderMap) -> Called<F::Output>
    where
        F: ServerFn + Serialize,
        F::Output: DeserializeOwned,
    {
        let res = self
            .http
            .post(format!("{}{}", self.url, F::PATH))
            .headers(headers)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .body(serde_json::to_vec(&args).unwrap())
            .send()
            .await
            .unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let body = res.bytes().await.unwrap();
        let result = if status.is_client_error() || status.is_server_error() {
            Err(ServerFnError::de(body))
        } else {
            Ok(serde_json::from_slice(&body)
                .unwrap_or_else(|e| panic!("{}: {e}: {}", F::PATH, String::from_utf8_lossy(&body))))
        };
        Called { result, headers }
    }

    async fn get(&self, path: &str) -> Response {
        self.http
            .get(format!("{}{path}", self.url))
            .send()
            .await
            .unwrap()
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.http.post(format!("{}{path}", self.url))
    }

    /// The session cookie the browser holds.
    fn session(&self) -> Option<String> {
        let url = self.url.parse().unwrap();
        let cookies = self.jar.cookies(&url)?;
        cookies
            .to_str()
            .unwrap()
            .split("; ")
            .find_map(|c| c.strip_prefix("hashi_session="))
            .map(String::from)
    }

    /// A browser that signed up as `username`.
    async fn signed_up(server: &Server, username: &str) -> Self {
        let browser = server.browser();
        browser
            .call(crate::api::Register {
                username: username.into(),
                password: PASSWORD.into(),
            })
            .await
            .unwrap();
        browser
    }

    /// A browser signed in as the admin. `HASHI_ADMINS` doesn't care about case.
    async fn admin(server: &Server) -> Self {
        let browser = server.browser();
        let username = ADMIN.to_uppercase();
        // Admin grants are provisioned out of band. Public registration cannot claim an
        // allowlisted legacy name, including after its previous owner deleted the account.
        let hash = auth::test_password_hash(PASSWORD);
        conn().await.execute(
            "INSERT OR IGNORE INTO users (username, name, password_hash, created_at) VALUES (?1, ?1, ?2, 0)",
            params![username.clone(), hash],
        ).await.unwrap();
        browser
            .call(crate::api::Login {
                username,
                password: PASSWORD.into(),
            })
            .await
            .unwrap();
        browser
    }
}

/// The error a server function failed with, as the client reads it (see `lang::error_text`).
#[track_caller]
fn api_error<T: std::fmt::Debug>(result: Result<T, ServerFnError>) -> ApiError {
    let error = result.expect_err("the call should have failed");
    ApiError::from_message(&error.to_string()).unwrap_or_else(|| panic!("not an ApiError: {error}"))
}

/// The `Set-Cookie` header of the session cookie.
#[track_caller]
fn session_cookie(headers: &HeaderMap) -> String {
    headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .find(|c| c.starts_with("hashi_session="))
        .expect("no session cookie set")
}

/// A user's id by username.
async fn user_id(conn: &Connection, username: &str) -> i64 {
    let mut rows = conn
        .query(
            "SELECT id FROM users WHERE username = ?1",
            params![username],
        )
        .await
        .unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

#[track_caller]
fn assert_status(res: &Response, status: StatusCode) {
    assert_eq!(res.status(), status, "{}", res.url());
}
