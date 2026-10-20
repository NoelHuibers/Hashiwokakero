//! Users and sessions.
//!
//! Everyone who starts a daily puzzle gets a user: a guest with a generated name, identified by
//! a session cookie. Registering gives that same user a username and password, so results and
//! streaks carry over. The cookie holds a random token, the database only its SHA-256.
//!
//! Sessions last a year and are renewed while the player keeps coming back. A guest without a
//! valid session can never get back in, so guests are deleted a year after their last visit
//! ([`start_cleanup`]). Deleting keeps the ranked times without a name ([`erase`]), so old
//! rankings and everyone else's medals stay as they were.

use std::collections::HashMap;
use std::sync::Mutex;

use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash};
use axum::http::{HeaderMap, HeaderValue, header};
use leptos::config::{Env, LeptosOptions};
use leptos::prelude::*;
use libsql::{Connection, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use super::db::db_err;
use super::names;
use super::{hex, now_ms, run_auth_blocking};
use crate::model::{ApiError, MS_PER_DAY, RENAME_EVERY_DAYS, UserInfo};

const COOKIE: &str = "hashi_session";
const SESSION_DAYS: i64 = 365;
/// A session is renewed (and its cookie sent again) once it is this old, so at most once a day.
const RENEW_AFTER_MS: i64 = MS_PER_DAY;
/// Failed logins per username within the window before further attempts are refused.
const MAX_FAILED_LOGINS: u32 = 8;
const LOGIN_WINDOW_MS: i64 = 15 * 60 * 1000;

#[derive(Clone, Debug)]
pub struct User {
    pub id: i64,
    pub username: Option<String>,
    pub name: String,
    pub created_at: i64,
    pub renamed_at: Option<i64>,
}

impl User {
    pub fn info(&self) -> UserInfo {
        UserInfo {
            id: self.id,
            name: self.name.clone(),
            guest: self.username.is_none(),
            created_at: self.created_at,
            admin: super::admin::is_admin(self),
            name_locked: super::admin::listing(self) == Some(super::admin::Listed::Name),
            renamed_at: self.renamed_at,
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
    // LEPTOS_ENV=PROD, as the router's settings say. Local development has no HTTPS.
    let secure = use_context::<LeptosOptions>().is_some_and(|o| o.env == Env::PROD);
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
        renamed_at: row.get(4)?,
    })
}

pub async fn user_by_id(conn: &Connection, id: i64) -> Result<Option<User>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT id, username, name, created_at, renamed_at FROM users WHERE id = ?1",
            params![id],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => Ok(Some(row_to_user(&row).map_err(db_err)?)),
        None => Ok(None),
    }
}

/// The signed in user (or guest) of this request. Renews the session, at most once a day.
pub async fn current_user(conn: &Connection) -> Result<Option<User>, ServerFnError> {
    let Some(token) = request_token().await else {
        return Ok(None);
    };
    let Some((user, expires_at)) = user_by_token(conn, &token).await? else {
        return Ok(None);
    };
    let now = now_ms();
    let renewed = now + SESSION_DAYS * MS_PER_DAY;
    if renewed - expires_at >= RENEW_AFTER_MS {
        conn.execute(
            "UPDATE sessions SET expires_at = ?1 WHERE token_hash = ?2",
            params![renewed, hash_token(&token)],
        )
        .await
        .map_err(db_err)?;
        set_cookie(&token, SESSION_DAYS * 24 * 3600);
    }
    Ok(Some(user))
}

/// The signed in user, for plain axum handlers outside of Leptos.
pub async fn user_from_headers(
    conn: &Connection,
    headers: &HeaderMap,
) -> Result<Option<User>, ServerFnError> {
    match token_from_headers(headers) {
        Some(token) => Ok(user_by_token(conn, &token).await?.map(|(user, _)| user)),
        None => Ok(None),
    }
}

/// The user of a session and when the session expires. Banned users have no sessions, and one
/// that slipped through (a sign in at the moment of the ban) doesn't count.
async fn user_by_token(
    conn: &Connection,
    token: &str,
) -> Result<Option<(User, i64)>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT u.id, u.username, u.name, u.created_at, u.renamed_at, s.expires_at FROM sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ?1 AND s.expires_at > ?2 AND u.banned_at IS NULL",
            params![hash_token(token), now_ms()],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => Ok(Some((
            row_to_user(&row).map_err(db_err)?,
            row.get(5).map_err(db_err)?,
        ))),
        None => Ok(None),
    }
}

async fn create_session(conn: &Connection, user_id: i64) -> Result<(), ServerFnError> {
    let token = insert_session(conn, user_id).await?;
    set_cookie(&token, SESSION_DAYS * 24 * 3600);
    Ok(())
}

/// Stores a session without publishing its cookie: a caller in a transaction must commit
/// before the browser receives a token that depends on that transaction.
pub(super) async fn insert_session(
    conn: &Connection,
    user_id: i64,
) -> Result<String, ServerFnError> {
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
    Ok(token)
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
        renamed_at: None,
    })
}

/// 3 to 20 letters, digits, `_` and `-`, none of the words the boards don't want, and nothing
/// that looks official unless `official` (admins, see `server/names.rs`).
pub fn validate_username(username: &str, official: bool) -> Result<(), ServerFnError> {
    let ok_len = (3..=20).contains(&username.chars().count());
    let ok_chars = username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !(ok_len && ok_chars) {
        return Err(ApiError::UsernameInvalid.into());
    }
    if names::blocked(username).is_some() || (names::reserved(username).is_some() && !official) {
        return Err(ApiError::UsernameNotAllowed.into());
    }
    Ok(())
}

pub fn validate_password(password: &str) -> Result<(), ServerFnError> {
    match password.chars().count() {
        0..=7 => Err(ApiError::PasswordTooShort.into()),
        8..=128 => Ok(()),
        _ => Err(ApiError::PasswordTooLong.into()),
    }
}

async fn hash_password(password: String) -> Result<String, ServerFnError> {
    run_auth_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|e| e.to_string())
    })
    .await
}

async fn verify_password(password: String, hash: String) -> Result<bool, ServerFnError> {
    run_auth_blocking(move || {
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
    // Legacy admin names may be used by their existing owner, never claimed by a new account.
    if super::admin::admin_username(username) {
        return Err(ApiError::UsernameTaken.into());
    }
    validate_username(username, false)?;
    validate_password(&password)?;
    // Capture the guest before password work can yield to another registration. Never treat
    // a revoked token as an anonymous signup: a competing upgrade must not create a second
    // account or replace the successful response's cookie.
    let current = match request_token().await {
        Some(token) => {
            let (user, _) = user_by_token(conn, &token)
                .await?
                .ok_or_else(|| ApiError::SessionExpired.fail())?;
            Some((user, token))
        }
        None => None,
    };
    let hash = hash_password(password).await?;
    let (user, token) = register_hashed(conn, current, username, hash).await?;
    set_cookie(&token, SESSION_DAYS * 24 * 3600);
    super::daily::invalidate_profiles();
    Ok(user)
}

/// Claims a captured guest, revokes its sessions and stores the replacement atomically.
async fn register_hashed(
    conn: &Connection,
    current: Option<(User, String)>,
    username: &str,
    hash: String,
) -> Result<(User, String), ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let signup_error = |e: libsql::Error| {
        if e.to_string().contains("UNIQUE") {
            ApiError::UsernameTaken.fail()
        } else {
            db_err(e)
        }
    };
    let id = match current {
        Some((user, token)) if user.username.is_none() => {
            let changed = tx
                .execute(
                    "UPDATE users SET username = ?1, name = ?1, password_hash = ?2
                 WHERE id = ?3 AND username IS NULL AND deleted_at IS NULL AND banned_at IS NULL
                   AND EXISTS (SELECT 1 FROM sessions WHERE token_hash = ?4
                               AND user_id = ?3 AND expires_at > ?5)",
                    params![username, hash, user.id, hash_token(&token), now_ms()],
                )
                .await
                .map_err(signup_error)?;
            if changed != 1 {
                return Err(ApiError::SessionExpired.fail());
            }
            tx.execute("DELETE FROM sessions WHERE user_id = ?1", params![user.id])
                .await
                .map_err(db_err)?;
            user.id
        }
        _ => {
            tx.execute(
                "INSERT INTO users (username, name, password_hash, created_at) VALUES (?1, ?1, ?2, ?3)",
                params![username, hash, now_ms()],
            )
            .await.map_err(signup_error)?;
            tx.last_insert_rowid()
        }
    };
    let token = insert_session(&tx, id).await?;
    let user = user_by_id(&tx, id)
        .await?
        .ok_or_else(|| ApiError::Internal.fail())?;
    tx.commit().await.map_err(db_err)?;
    Ok((user, token))
}

/// Gives the signed in account another username, behind its password.
///
/// The username is also the name on the leaderboards, old ones included, so it can only change
/// once in [`RENAME_EVERY_DAYS`]: names on the boards don't keep changing under the players.
///
/// Admins listed by username in `HASHI_ADMINS` keep their names: freed, one would make whoever
/// registers it next an admin. Listed by id, they rename like everyone else.
pub async fn change_username(
    conn: &Connection,
    username: &str,
    password: String,
) -> Result<User, ServerFnError> {
    let user = current_user(conn)
        .await?
        .ok_or_else(|| ApiError::SessionExpired.fail())?;
    let Some(current) = user.username.clone() else {
        return Err(ApiError::Forbidden.into());
    };
    let admin = super::admin::listing(&user);
    if super::admin::admin_username(username)
        && !user
            .username
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(username))
    {
        return Err(ApiError::UsernameTaken.into());
    }
    validate_username(username, admin.is_some())?;
    if username == current {
        return Ok(user);
    }
    if admin == Some(super::admin::Listed::Name) {
        return Err(ApiError::Forbidden.into());
    }
    let now = now_ms();
    let earliest = now - RENAME_EVERY_DAYS * MS_PER_DAY;
    if user.renamed_at.is_some_and(|at| at > earliest) {
        return Err(ApiError::RenameTooSoon.into());
    }
    if check_password(conn, &current, password).await?.is_none() {
        return Err(ApiError::WrongPassword.into());
    }
    // The condition again, for two renames at the same moment
    let changed = conn
        .execute(
            "UPDATE users SET username = ?1, name = ?1, renamed_at = ?2
             WHERE id = ?3 AND username IS NOT NULL AND (renamed_at IS NULL OR renamed_at <= ?4)",
            params![username, now, user.id, earliest],
        )
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                ApiError::UsernameTaken.fail()
            } else {
                db_err(e)
            }
        })?;
    if changed == 0 {
        return Err(ApiError::RenameTooSoon.into());
    }
    super::daily::invalidate_profiles();
    user_by_id(conn, user.id)
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
///
/// A banned account is told so, but only with the right password: nobody else learns about it.
pub async fn login(
    conn: &Connection,
    username: &str,
    password: String,
) -> Result<User, ServerFnError> {
    let Some(user) = check_password(conn, username, password).await? else {
        return Err(ApiError::WrongLogin.into());
    };
    if super::admin::is_banned(conn, user.id).await? {
        return Err(ApiError::Banned.into());
    }

    if let Some(token) = request_token().await {
        if let Some((guest, _)) = user_by_token(conn, &token)
            .await?
            .filter(|(u, _)| u.username.is_none())
        {
            merge_guest(conn, guest.id, user.id, &token).await?;
        }
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

/// The account if the password is right, counting failed attempts per username.
async fn check_password(
    conn: &Connection,
    username: &str,
    password: String,
) -> Result<Option<User>, ServerFnError> {
    let key = username.to_lowercase();
    if !login_allowed(&key) {
        return Err(ApiError::TooManyAttempts.into());
    }
    // The query ends before the hash is checked, which can take seconds on a busy server: an
    // unfinished query keeps a read open, on a local file in rollback journal mode one that
    // blocks every write (see `signing_in_holds_no_lock_while_the_password_is_checked`)
    let found = {
        let mut rows = conn
            .query(
                "SELECT id, username, name, created_at, renamed_at, password_hash FROM users WHERE username = ?1",
                params![username],
            )
            .await
            .map_err(db_err)?;
        match rows.next().await.map_err(db_err)? {
            Some(row) => Some((
                row_to_user(&row).map_err(db_err)?,
                row.get::<Option<String>>(5).map_err(db_err)?,
            )),
            None => None,
        }
    };
    let valid = match &found {
        Some((_, Some(hash))) => verify_password(password, hash.clone()).await?,
        _ => false,
    };
    if !valid {
        record_failed_login(&key);
    }
    Ok(found.filter(|_| valid).map(|(user, _)| user))
}

pub(super) async fn merge_guest(
    conn: &Connection,
    guest: i64,
    user: i64,
    token: &str,
) -> Result<(), ServerFnError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    // Registration can claim this identity while login is waiting for the write lock. Never
    // transfer a registered account's attempt based on the earlier guest snapshot. The
    // captured session also excludes a replacement guest that reused the deleted guest's ID.
    let still_guest = tx
        .query(
            "SELECT 1 FROM users WHERE id = ?1 AND username IS NULL AND deleted_at IS NULL
             AND EXISTS (SELECT 1 FROM sessions WHERE token_hash = ?2 AND user_id = ?1 AND expires_at > ?3)",
            params![guest, hash_token(token), now_ms()],
        )
        .await
        .map_err(db_err)?
        .next()
        .await
        .map_err(db_err)?
        .is_some();
    if !still_guest {
        tx.commit().await.map_err(db_err)?;
        return Ok(());
    }
    // Annotate pre-provenance receipts using their actual owner, under the same lock as
    // transfer. Existing provenance is preserved; conflicting attempts are never combined.
    tx.execute(
        "UPDATE attempts SET paid_actions = (
            SELECT json_group_array(json_set(value, '$.origin_user',
                COALESCE(json_extract(value, '$.origin_user'), ?1)))
            FROM json_each(attempts.paid_actions)
         ) WHERE user_id = ?1 AND paid_actions != '[]'",
        params![guest],
    )
    .await
    .map_err(db_err)?;
    for sql in [
        "UPDATE OR IGNORE attempts SET user_id = ?1 WHERE user_id = ?2",
        "UPDATE OR IGNORE stamps SET user_id = ?1 WHERE user_id = ?2",
    ] {
        tx.execute(sql, params![user, guest])
            .await
            .map_err(db_err)?;
    }
    for sql in [
        "DELETE FROM attempts WHERE user_id = ?1",
        "DELETE FROM stamps WHERE user_id = ?1",
        "DELETE FROM sessions WHERE user_id = ?1",
        "DELETE FROM users WHERE id = ?1 AND username IS NULL",
    ] {
        tx.execute(sql, params![guest]).await.map_err(db_err)?;
    }
    tx.commit().await.map_err(db_err)?;
    super::daily::invalidate_profiles();
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

/// Deletes the current account (a registered one needs its password) and signs out.
pub async fn delete_account(
    conn: &Connection,
    password: Option<String>,
) -> Result<(), ServerFnError> {
    let user = current_user(conn)
        .await?
        .ok_or_else(|| ApiError::SessionExpired.fail())?;
    if let Some(username) = &user.username {
        let password = password.unwrap_or_default();
        if check_password(conn, username, password).await?.is_none() {
            return Err(ApiError::WrongPassword.into());
        }
    }
    erase(conn, user.id).await?;
    set_cookie("", 0);
    Ok(())
}

/// Deletes everything personal of a user.
///
/// That is name, password, sessions, stamps, Dojo submissions, and of the daily results the
/// boards, move logs, devices and unranked attempts. What stays are
/// the ranked times and penalties, under a user without a name (`deleted_at`), so the rankings of
/// old days and the medals of everyone else don't change. A user without ranked times remains
/// as an anonymous tombstone, so numeric admin grants can never transfer to a reused id.
/// Archived Dojo seasons keep the numbers of the user's solvers, without their names, links or
/// a way back to the user.
pub async fn erase(conn: &Connection, user: i64) -> Result<(), ServerFnError> {
    let tx = conn.transaction().await.map_err(db_err)?;
    for sql in [
        "DELETE FROM sessions WHERE user_id = ?1",
        "DELETE FROM stamps WHERE user_id = ?1",
        // Later copies of a deleted submission are no copies any more
        "UPDATE dojo_submissions SET similar_to = NULL
         WHERE similar_to IN (SELECT id FROM dojo_submissions WHERE user_id = ?1)",
        "DELETE FROM dojo_submissions WHERE user_id = ?1",
        // Community benchmarks go, unless a season drew them: those are published with its set,
        // without a name
        "DELETE FROM dojo_benchmarks WHERE user_id = ?1 AND id NOT IN (SELECT benchmark_id FROM dojo_season_pool)",
        "UPDATE dojo_benchmarks SET user_id = NULL, ip_hash = NULL WHERE user_id = ?1",
        "UPDATE dojo_archive SET user_id = NULL, name = '', source_url = NULL WHERE user_id = ?1",
        "DELETE FROM attempts
         WHERE user_id = ?1 AND (finished_at IS NULL OR revealed != 0 OR flag IS NOT NULL)",
        "UPDATE attempts SET bridges = NULL, log = NULL, device = NULL, run = NULL, daypart = NULL, paid_actions = '[]'
         WHERE user_id = ?1",
    ] {
        tx.execute(sql, params![user]).await.map_err(db_err)?;
    }
    tx.execute(
        "UPDATE users SET username = NULL, name = '', password_hash = NULL, stamp = NULL,
            renamed_at = NULL, deleted_at = ?2
         WHERE id = ?1",
        params![user, now_ms()],
    )
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    super::daily::invalidate_profiles();
    Ok(())
}

/// Deletes the guests nobody can sign in to any more: their last session expired (a year after
/// their last visit) or is gone. Returns how many.
pub async fn cleanup_guests(conn: &Connection) -> Result<usize, ServerFnError> {
    let now = now_ms();
    conn.execute("DELETE FROM sessions WHERE expires_at < ?1", params![now])
        .await
        .map_err(db_err)?;
    let mut rows = conn
        .query(
            "SELECT id FROM users u
             WHERE username IS NULL AND deleted_at IS NULL AND created_at < ?1
               AND NOT EXISTS (SELECT 1 FROM sessions s WHERE s.user_id = u.id)",
            // A day of slack for a guest whose session is just being created
            params![now - MS_PER_DAY],
        )
        .await
        .map_err(db_err)?;
    let mut guests = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        guests.push(row.get::<i64>(0).map_err(db_err)?);
    }
    for &guest in &guests {
        erase(conn, guest).await?;
    }
    Ok(guests.len())
}

/// Deletes abandoned guests and old contact messages now and then every few hours.
pub fn start_cleanup() {
    tokio::spawn(async {
        loop {
            match super::db::conn() {
                Ok(conn) => {
                    match cleanup_guests(&conn).await {
                        Ok(0) => {}
                        Ok(n) => println!("cleanup: deleted {n} abandoned guests"),
                        Err(e) => eprintln!("cleanup: {e}"),
                    }
                    if let Err(e) = super::contact::delete_old(&conn).await {
                        eprintln!("cleanup: {e}");
                    }
                }
                Err(e) => eprintln!("cleanup: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_hours(6)).await;
        }
    });
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
pub(super) fn test_password_hash(password: &str) -> String {
    Argon2::default()
        .hash_password(password.as_bytes())
        .unwrap()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert!(validate_username("noel", false).is_ok());
        assert!(validate_username("Bridge_Builder-99", false).is_ok());
        assert!(validate_username("ab", false).is_err());
        assert!(validate_username("has space", false).is_err());
        assert!(validate_username("ünïcode", false).is_err());
        assert!(validate_username(&"x".repeat(21), false).is_err());
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

    /// A hash as `register` stored it, made with argon2 0.6.0 and its defaults.
    const STORED_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$8C0eovpPoTDFAHE5YNGFOw$zACBMxyBbkdhJXVVpw8lwGNZiK7KA0NYK7ItMmvyUIg";

    /// Both requests observed the same guest before hashing. A barrier fixes that interleaving
    /// without depending on HTTP scheduling or password-hash timing.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn competing_guest_registrations_have_one_winner() {
        use std::sync::Arc;

        use crate::server::db;

        let file = db::TempDb::new("register-race");
        let database = db::open_local(file.as_str()).await.unwrap();
        let conn = db::connect(&database).unwrap();
        db::migrate(&conn).await.unwrap();
        conn.execute(
            "INSERT INTO users (name, created_at) VALUES ('Guest', 1)",
            (),
        )
        .await
        .unwrap();
        let guest_id = conn.last_insert_rowid();
        let token = insert_session(&conn, guest_id).await.unwrap();
        let guest = user_by_id(&conn, guest_id).await.unwrap().unwrap();
        let other_hash = hash_password("different horse".into()).await.unwrap();
        let barrier = Arc::new(tokio::sync::Barrier::new(3));
        let mut requests = Vec::new();
        for (name, password, hash) in [
            ("first_claim", "correct horse", STORED_HASH.to_string()),
            ("second_claim", "different horse", other_hash),
        ] {
            let conn = db::connect(&database).unwrap();
            let captured = Some((guest.clone(), token.clone()));
            let ready = barrier.clone();
            requests.push(tokio::spawn(async move {
                ready.wait().await;
                (
                    name,
                    password,
                    register_hashed(&conn, captured, name, hash).await,
                )
            }));
        }
        barrier.wait().await;
        let mut winners = Vec::new();
        let mut losers = Vec::new();
        for request in requests {
            let (name, password, result) = request.await.unwrap();
            match result {
                Ok((user, fresh)) => {
                    assert_eq!(user.id, guest_id);
                    assert_eq!(user.username.as_deref(), Some(name));
                    winners.push((name, password, fresh));
                }
                Err(error) => {
                    assert_eq!(
                        ApiError::from_message(&error.to_string()),
                        Some(ApiError::SessionExpired)
                    );
                    losers.push((name, password));
                }
            }
        }
        assert_eq!(winners.len(), 1);
        assert_eq!(losers.len(), 1);
        let (winner, password, fresh) = &winners[0];
        assert!(
            check_password(&conn, winner, (*password).into())
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            check_password(&conn, winner, losers[0].1.into())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            check_password(&conn, losers[0].0, losers[0].1.into())
                .await
                .unwrap()
                .is_none()
        );
        assert!(user_by_token(&conn, &token).await.unwrap().is_none());
        assert_eq!(
            user_by_token(&conn, fresh).await.unwrap().unwrap().0.id,
            guest_id
        );
        let mut rows = conn
            .query(
                "SELECT (SELECT COUNT(*) FROM users), (SELECT COUNT(*) FROM sessions)",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        assert_eq!(row.get::<i64>(0).unwrap(), 1);
        assert_eq!(row.get::<i64>(1).unwrap(), 1);
    }

    #[tokio::test]
    async fn captured_registration_and_transfer_cannot_claim_a_reused_guest_id() {
        use crate::server::{daily, db};
        let db = db::TestConn::new("captured-guest-reuse").await;
        let conn = &*db;
        conn.execute(
            "INSERT INTO users (username, name, created_at) VALUES ('owner', 'Owner', 1)",
            (),
        )
        .await
        .unwrap();
        let owner = conn.last_insert_rowid();
        let guest = ensure_user(conn).await.unwrap();
        let token = insert_session(conn, guest.id).await.unwrap();
        daily::start(conn, guest.id, daily::today(), None)
            .await
            .unwrap();
        merge_guest(conn, guest.id, owner, &token).await.unwrap();
        let replacement = ensure_user(conn).await.unwrap();
        assert_eq!(replacement.id, guest.id);
        let fresh = insert_session(conn, replacement.id).await.unwrap();
        daily::start(conn, replacement.id, daily::today(), None)
            .await
            .unwrap();
        let captured = Some((guest.clone(), token.clone()));
        let error = register_hashed(conn, captured, "stale_claim", STORED_HASH.into())
            .await
            .unwrap_err();
        assert_eq!(
            ApiError::from_message(&error.to_string()),
            Some(ApiError::SessionExpired)
        );
        merge_guest(conn, guest.id, owner, &token).await.unwrap();
        assert!(
            user_by_id(conn, replacement.id)
                .await
                .unwrap()
                .unwrap()
                .username
                .is_none()
        );
        assert!(user_by_token(conn, &fresh).await.unwrap().is_some());
        assert!(
            daily::attempt(conn, replacement.id, daily::today())
                .await
                .unwrap()
                .is_some()
        );
        assert!(user_by_token(conn, &token).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn stored_password_hashes_still_verify() {
        assert!(
            verify_password("correct horse".into(), STORED_HASH.into())
                .await
                .unwrap(),
            "a password hash in the database doesn't verify any more: nobody could sign in"
        );
        assert!(
            !verify_password("correct hose".into(), STORED_HASH.into())
                .await
                .unwrap(),
            "a wrong password verifies: anybody could sign in to any account"
        );
    }

    #[tokio::test]
    async fn new_password_hashes_keep_their_parameters() {
        let hash = hash_password("correct horse".into()).await.unwrap();
        let parsed = PasswordHash::new(&hash).unwrap();
        // Argon2id with 19 MiB, 2 passes and one lane, a 16 byte salt and a 32 byte hash. A new
        // default of argon2 would show up here: weaker parameters weaken new accounts, stronger
        // ones cost the single shared CPU more on every sign in
        assert_eq!(
            hash.split('$').take(4).collect::<Vec<_>>(),
            ["", "argon2id", "v=19", "m=19456,t=2,p=1"],
            "{hash}: other argon2 parameters for new passwords"
        );
        assert_eq!(parsed.salt.map(|s| s.len()), Some(16), "{hash}");
        assert_eq!(parsed.hash.map(|h| h.len()), Some(32), "{hash}");
        assert!(
            verify_password("correct horse".into(), hash.clone())
                .await
                .unwrap()
        );
        // A new salt every time
        assert_ne!(hash, hash_password("correct horse".into()).await.unwrap());
    }

    /// Signing in while another request writes. The password hash is checked on the blocking
    /// threads, where it can wait its turn behind other password checks for seconds. It
    /// did that with its query still open, which on a local file kept a read lock: the other
    /// request's write couldn't commit and failed with "database is locked" after the busy
    /// timeout (two sign ins of the browser tests did). The file is in SQLite's default rollback
    /// journal mode here, where any lock held shows up as a write that fails.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn signing_in_holds_no_lock_while_the_password_is_checked() {
        use std::time::{Duration, Instant};

        use crate::server::db;

        let file = db::TempDb::new("sign-in");
        let database = libsql::Builder::new_local(file.as_str())
            .build()
            .await
            .unwrap();
        let signing_in = db::connect(&database).unwrap();
        let other = db::connect(&database).unwrap();
        db::migrate(&other).await.unwrap();
        let mut rows = other.query("PRAGMA journal_mode", ()).await.unwrap();
        let mode: String = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(mode, "delete");
        drop(rows);
        other
            .execute(
                "INSERT INTO users (username, name, password_hash, created_at) VALUES ('lockless', 'lockless', ?1, 1)",
                params![STORED_HASH],
            )
            .await
            .unwrap();

        // Both password permits taken, like by two concurrent sign-ins
        let (started_tx, mut started) = tokio::sync::mpsc::unbounded_channel();
        let mut releases = Vec::new();
        for _ in 0..2 {
            let (release, wait) = std::sync::mpsc::channel::<()>();
            releases.push(release);
            let started_tx = started_tx.clone();
            tokio::spawn(run_auth_blocking(move || {
                started_tx.send(()).unwrap();
                let _ = wait.recv();
                Ok(())
            }));
        }
        for _ in 0..2 {
            started.recv().await.unwrap();
        }
        let sign_in = tokio::spawn(async move {
            check_password(&signing_in, "lockless", "correct horse".into())
                .await
                .map(|user| user.map(|u| u.id))
        });
        // The query takes a moment, then the hash waits for a permit
        tokio::time::sleep(Duration::from_millis(300)).await;

        let writing = Instant::now();
        let wrote = other
            .execute(
                "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES ('ab', 1, 1, 2)",
                (),
            )
            .await;
        let waited = writing.elapsed();
        drop(releases);
        assert!(
            wrote.is_ok(),
            "a write while someone signs in: {:?}",
            wrote.err()
        );
        assert!(
            waited < Duration::from_secs(1),
            "the write waited for the sign in: {waited:?}"
        );
        assert_eq!(sign_in.await.unwrap().unwrap(), Some(1));
    }

    #[test]
    fn session_tokens_hash_the_same() {
        // Sessions are looked up by this hash of the cookie: another one signs everybody out
        assert_eq!(
            hash_token("0123456789abcdef"),
            "9f9f5111f7b27a781f1f1ddde5ebc2dd2b796bfc7365c9c28b548e564176929f",
            "the hash of session tokens changed: every session would end"
        );
    }

    #[test]
    fn guest_names_look_nice() {
        let name = guest_name();
        assert_eq!(name.split(' ').count(), 3, "{name}");
    }
}
