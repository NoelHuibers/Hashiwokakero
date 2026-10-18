//! Users and sessions.
//!
//! Everyone who starts a daily puzzle gets a user: a guest with a generated name, identified by
//! a session cookie. Registering gives that same user a username and password, so results and
//! streaks carry over. The cookie holds a random token, the database only its SHA-256.

use std::collections::HashMap;
use std::sync::Mutex;

use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash};
use axum::http::{HeaderMap, HeaderValue, header};
use leptos::prelude::*;
use libsql::{Connection, params};
use sha2::{Digest, Sha256};

use super::db::db_err;
use super::{hex, now_ms, run_blocking};
use crate::model::{ApiError, MS_PER_DAY, UserInfo};

const COOKIE: &str = "hashi_session";
const SESSION_DAYS: i64 = 365;
/// Failed logins per username within the window before further attempts are refused.
const MAX_FAILED_LOGINS: u32 = 8;
const LOGIN_WINDOW_MS: i64 = 15 * 60 * 1000;

#[derive(Clone, Debug)]
pub struct User {
    pub id: i64,
    pub username: Option<String>,
    pub name: String,
    pub created_at: i64,
}

impl User {
    pub fn info(&self) -> UserInfo {
        UserInfo {
            id: self.id,
            name: self.name.clone(),
            guest: self.username.is_none(),
            created_at: self.created_at,
            admin: super::admin::is_admin(self),
        }
    }
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("no system randomness");
    buf
}

fn hash_token(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

/// The session token from the request's cookie header.
async fn request_token() -> Option<String> {
    let headers: HeaderMap = leptos_axum::extract().await.ok()?;
    token_from_headers(&headers)
}

fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value.to_string())
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn set_cookie(value: &str, max_age_s: i64) {
    let secure = std::env::var("LEPTOS_ENV").is_ok_and(|e| e.eq_ignore_ascii_case("prod"));
    let cookie = format!(
        "{COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_s}{}",
        if secure { "; Secure" } else { "" }
    );
    if let (Some(response), Ok(value)) = (
        use_context::<leptos_axum::ResponseOptions>(),
        HeaderValue::from_str(&cookie),
    ) {
        response.append_header(header::SET_COOKIE, value);
    }
}

fn row_to_user(row: &libsql::Row) -> Result<User, libsql::Error> {
    Ok(User {
        id: row.get(0)?,
        username: row.get(1)?,
        name: row.get(2)?,
        created_at: row.get(3)?,
    })
}

pub async fn user_by_id(conn: &Connection, id: i64) -> Result<Option<User>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT id, username, name, created_at FROM users WHERE id = ?1",
            params![id],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => Ok(Some(row_to_user(&row).map_err(db_err)?)),
        None => Ok(None),
    }
}

/// The signed in user (or guest) of this request.
pub async fn current_user(conn: &Connection) -> Result<Option<User>, ServerFnError> {
    match request_token().await {
        Some(token) => user_by_token(conn, &token).await,
        None => Ok(None),
    }
}

/// The signed in user, for plain axum handlers outside of Leptos.
pub async fn user_from_headers(
    conn: &Connection,
    headers: &HeaderMap,
) -> Result<Option<User>, ServerFnError> {
    match token_from_headers(headers) {
        Some(token) => user_by_token(conn, &token).await,
        None => Ok(None),
    }
}

async fn user_by_token(conn: &Connection, token: &str) -> Result<Option<User>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT u.id, u.username, u.name, u.created_at FROM sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ?1 AND s.expires_at > ?2",
            params![hash_token(token), now_ms()],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => Ok(Some(row_to_user(&row).map_err(db_err)?)),
        None => Ok(None),
    }
}

async fn create_session(conn: &Connection, user_id: i64) -> Result<(), ServerFnError> {
    let token = hex(&random_bytes::<32>());
    let now = now_ms();
    conn.execute(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        params![hash_token(&token), user_id, now, now + SESSION_DAYS * MS_PER_DAY],
    )
    .await
    .map_err(db_err)?;
    // Housekeeping, cheap thanks to the primary key
    conn.execute("DELETE FROM sessions WHERE expires_at < ?1", params![now])
        .await
        .map_err(db_err)?;
    set_cookie(&token, SESSION_DAYS * 24 * 3600);
    Ok(())
}

/// The current user, or a new guest with a session.
pub async fn ensure_user(conn: &Connection) -> Result<User, ServerFnError> {
    if let Some(user) = current_user(conn).await? {
        return Ok(user);
    }
    let name = guest_name();
    let now = now_ms();
    conn.execute(
        "INSERT INTO users (name, created_at) VALUES (?1, ?2)",
        params![name.clone(), now],
    )
    .await
    .map_err(db_err)?;
    let id = conn.last_insert_rowid();
    create_session(conn, id).await?;
    Ok(User {
        id,
        username: None,
        name,
        created_at: now,
    })
}

pub fn validate_username(username: &str) -> Result<(), ServerFnError> {
    let ok_len = (3..=20).contains(&username.chars().count());
    let ok_chars = username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok_len && ok_chars {
        Ok(())
    } else {
        Err(ApiError::UsernameInvalid.into())
    }
}

pub fn validate_password(password: &str) -> Result<(), ServerFnError> {
    match password.chars().count() {
        0..=7 => Err(ApiError::PasswordTooShort.into()),
        8..=128 => Ok(()),
        _ => Err(ApiError::PasswordTooLong.into()),
    }
}

async fn hash_password(password: String) -> Result<String, ServerFnError> {
    run_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|e| e.to_string())
    })
    .await
}

async fn verify_password(password: String, hash: String) -> Result<bool, ServerFnError> {
    run_blocking(move || {
        let parsed = PasswordHash::new(&hash).map_err(|e| e.to_string())?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
}

/// Gives the current guest a username and password, or creates a new account.
pub async fn register(
    conn: &Connection,
    username: &str,
    password: String,
) -> Result<User, ServerFnError> {
    validate_username(username)?;
    validate_password(&password)?;
    let hash = hash_password(password).await?;
    let current = current_user(conn).await?;
    let result = match &current {
        Some(user) if user.username.is_none() => {
            conn.execute(
                "UPDATE users SET username = ?1, name = ?1, password_hash = ?2 WHERE id = ?3",
                params![username, hash, user.id],
            )
            .await
        }
        _ => {
            conn.execute(
                "INSERT INTO users (username, name, password_hash, created_at) VALUES (?1, ?1, ?2, ?3)",
                params![username, hash, now_ms()],
            )
            .await
        }
    };
    if let Err(e) = result {
        return Err(if e.to_string().contains("UNIQUE") {
            ApiError::UsernameTaken.into()
        } else {
            db_err(e)
        });
    }
    let id = match current {
        Some(user) if user.username.is_none() => user.id,
        _ => {
            let id = conn.last_insert_rowid();
            create_session(conn, id).await?;
            id
        }
    };
    user_by_id(conn, id)
        .await?
        .ok_or_else(|| ApiError::Internal.fail())
}

static FAILED_LOGINS: Mutex<Option<HashMap<String, (u32, i64)>>> = Mutex::new(None);

fn login_allowed(username: &str) -> bool {
    let now = now_ms();
    let mut guard = FAILED_LOGINS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let map = guard.get_or_insert_with(HashMap::new);
    map.retain(|_, (_, since)| now - *since < LOGIN_WINDOW_MS);
    let allowed = map
        .get(username)
        .is_none_or(|(count, _)| *count < MAX_FAILED_LOGINS);
    drop(guard);
    allowed
}

fn record_failed_login(username: &str) {
    let now = now_ms();
    FAILED_LOGINS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .entry(username.to_string())
        .or_insert((0, now))
        .0 += 1;
}

/// Signs in. Daily results the player made as a guest in this browser move to the account,
/// unless the account already has a result for that day.
pub async fn login(
    conn: &Connection,
    username: &str,
    password: String,
) -> Result<User, ServerFnError> {
    let key = username.to_lowercase();
    if !login_allowed(&key) {
        return Err(ApiError::TooManyAttempts.into());
    }
    let mut rows = conn
        .query(
            "SELECT id, username, name, created_at, password_hash FROM users WHERE username = ?1",
            params![username],
        )
        .await
        .map_err(db_err)?;
    let found = match rows.next().await.map_err(db_err)? {
        Some(row) => Some((
            row_to_user(&row).map_err(db_err)?,
            row.get::<Option<String>>(4).map_err(db_err)?,
        )),
        None => None,
    };
    let valid = match &found {
        Some((_, Some(hash))) => verify_password(password, hash.clone()).await?,
        _ => false,
    };
    let Some((user, _)) = found.filter(|_| valid) else {
        record_failed_login(&key);
        return Err(ApiError::WrongLogin.into());
    };

    if let Some(guest) = current_user(conn).await?.filter(|u| u.username.is_none()) {
        merge_guest(conn, guest.id, user.id).await?;
    }
    if let Some(token) = request_token().await {
        conn.execute(
            "DELETE FROM sessions WHERE token_hash = ?1",
            params![hash_token(&token)],
        )
        .await
        .map_err(db_err)?;
    }
    create_session(conn, user.id).await?;
    Ok(user)
}

async fn merge_guest(conn: &Connection, guest: i64, user: i64) -> Result<(), ServerFnError> {
    conn.execute(
        "UPDATE OR IGNORE attempts SET user_id = ?1 WHERE user_id = ?2",
        params![user, guest],
    )
    .await
    .map_err(db_err)?;
    for sql in [
        "DELETE FROM attempts WHERE user_id = ?1",
        "DELETE FROM sessions WHERE user_id = ?1",
        "DELETE FROM users WHERE id = ?1 AND username IS NULL",
    ] {
        conn.execute(sql, params![guest]).await.map_err(db_err)?;
    }
    Ok(())
}

pub async fn logout(conn: &Connection) -> Result<(), ServerFnError> {
    if let Some(token) = request_token().await {
        conn.execute(
            "DELETE FROM sessions WHERE token_hash = ?1",
            params![hash_token(&token)],
        )
        .await
        .map_err(db_err)?;
    }
    set_cookie("", 0);
    Ok(())
}

/// "Sleepy Tanuki 27"
pub fn guest_name() -> String {
    const ADJECTIVES: [&str; 20] = [
        "Sleepy", "Brave", "Quiet", "Swift", "Clever", "Gentle", "Lucky", "Misty", "Sunny",
        "Rainy", "Cosy", "Bold", "Tiny", "Curious", "Merry", "Calm", "Wise", "Nimble", "Dreamy",
        "Jolly",
    ];
    const CREATURES: [&str; 20] = [
        "Crane", "Koi", "Tanuki", "Kitsune", "Heron", "Frog", "Otter", "Sparrow", "Turtle", "Owl",
        "Deer", "Macaque", "Rabbit", "Firefly", "Seal", "Shiba", "Panda", "Carp", "Swallow",
        "Badger",
    ];
    let [a, b, c] = random_bytes::<3>();
    format!(
        "{} {} {}",
        ADJECTIVES[a as usize % ADJECTIVES.len()],
        CREATURES[b as usize % CREATURES.len()],
        10 + c as usize % 90
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert!(validate_username("noel").is_ok());
        assert!(validate_username("Bridge_Builder-99").is_ok());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("has space").is_err());
        assert!(validate_username("ünïcode").is_err());
        assert!(validate_username(&"x".repeat(21)).is_err());
    }

    #[test]
    fn passwords() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("long enough").is_ok());
    }

    #[test]
    fn tokens_are_hashed() {
        let token = hex(&random_bytes::<32>());
        assert_eq!(token.len(), 64);
        assert_eq!(hash_token(&token).len(), 64);
        assert_ne!(hash_token(&token), token);
    }

    #[test]
    fn guest_names_look_nice() {
        let name = guest_name();
        assert_eq!(name.split(' ').count(), 3, "{name}");
    }
}
