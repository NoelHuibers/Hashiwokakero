//! The recovery email of an account, and resetting a forgotten password with it.
//!
//! Hashi needs no address: guests have none, and an account only gets one if its player adds it on
//! the account page, behind the password. The address counts once the player opened the link
//! mailed to it ([`CONFIRM_VALID_MS`], once); until then the account keeps the address it had. An
//! address is the recovery email of one account at most, whatever its case.
//!
//! "Forgot password?" mails a link to the confirmed address of the account with that username or
//! address ([`RESET_VALID_MS`], once, a newer link replaces it). The page always answers the same,
//! whether there is such an account or not, and the work happens after the answer, so not even
//! the time it takes tells. The link sets a new password, which signs out every device, and signs
//! in the browser that opened it.
//!
//! Links are 32 random bytes, the database keeps their SHA-256 (`account_tokens`), like sessions.
//! Mails per account, per network and in all are limited ([`budget_left`]): nobody fills someone's
//! inbox, or uses up the mail server's quota.

use leptos::prelude::{ServerFnError, use_context};
use leptos_i18n::Locale as _;
use libsql::params::IntoParams;
use libsql::{Connection, TransactionBehavior, params};

use super::auth::{self, User};
use super::db::db_err;
use super::mail::{self, Mail};
use super::{ClientIdentity, now_ms};
use crate::i18n::{Locale, td_string};
use crate::lang::localized;
use crate::model::{ApiError, MAX_EMAIL, MS_PER_DAY, RecoveryEmail};
use crate::seo::SITE_URL;

const HOUR_MS: i64 = 60 * 60 * 1000;
/// How long a confirmation link works. The mails say so (`account.mail.confirm_valid`).
pub const CONFIRM_VALID_MS: i64 = 24 * HOUR_MS;
/// How long a reset link works. The mails say so (`account.mail.reset_valid`).
pub const RESET_VALID_MS: i64 = HOUR_MS;
/// Confirmation mails per account within a day.
const CONFIRMS_PER_DAY: i64 = 5;
/// Reset mails per account within a day.
const RESETS_PER_DAY: i64 = 3;
/// Mails with a link per (hashed) network within a day.
const PER_NETWORK: i64 = 10;
/// Mails with a link to everyone within a day, far below what the mail server takes.
const PER_DAY: i64 = 300;

/// The pages the links open, in the language of the page that asked for them.
pub const CONFIRM_PATH: &str = "/account/confirm";
pub const RESET_PATH: &str = "/account/reset";

/// For `auth::set_password`, `?1` the user, `?2` now: a new password uses up the reset links
/// still open, they were for the old one.
pub(super) const USE_UP_RESETS: &str = "UPDATE account_tokens SET used_at = ?2
     WHERE user_id = ?1 AND kind = 'reset' AND used_at IS NULL";

/// Calls off the confirmation of an address still waiting for it, `?1` the user, `?2` now.
const CALL_OFF_CONFIRMATIONS: &str = "UPDATE account_tokens SET used_at = ?2, email = NULL
     WHERE user_id = ?1 AND kind = 'confirm' AND used_at IS NULL";

/// Uses up every link of an account, `?1` the user, `?2` now: its address changed or went, links
/// sent before were for what it was.
const USE_UP_ALL: &str = "UPDATE account_tokens SET used_at = ?2, email = NULL
     WHERE user_id = ?1 AND used_at IS NULL";

/// What a link is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Confirms an address as the account's recovery email.
    Confirm,
    /// Sets a new password.
    Reset,
}

impl Kind {
    const fn id(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Reset => "reset",
        }
    }

    const fn valid_ms(self) -> i64 {
        match self {
            Self::Confirm => CONFIRM_VALID_MS,
            Self::Reset => RESET_VALID_MS,
        }
    }

    const fn per_account(self) -> i64 {
        match self {
            Self::Confirm => CONFIRMS_PER_DAY,
            Self::Reset => RESETS_PER_DAY,
        }
    }
}

/// A link as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Link {
    user: i64,
    /// The address a confirmation link confirms; gone once it is used.
    email: Option<String>,
    locale: Option<String>,
    expires_at: i64,
    used_at: Option<i64>,
}

impl Link {
    /// Whether it still works at `now`: not used, not replaced, not expired.
    const fn open(&self, now: i64) -> bool {
        self.used_at.is_none() && now < self.expires_at
    }
}

/// `input` as a recovery email: trimmed, at most [`MAX_EMAIL`] bytes, an address as mail servers
/// take it (`user@domain`, nothing around it).
pub fn parse_email(input: &str) -> Result<String, ApiError> {
    let email = input.trim();
    let valid =
        !email.is_empty() && email.len() <= MAX_EMAIL && email.parse::<lettre::Address>().is_ok();
    if valid {
        Ok(email.to_string())
    } else {
        Err(ApiError::EmailInvalid)
    }
}

/// Whether a token could be one of ours: 64 hex digits. Anything else is no link at all.
fn plausible(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether mail can go out, so recovery emails and resets work at all.
pub fn available() -> bool {
    mail::available()
}

async fn scalar(
    conn: &Connection,
    sql: &str,
    params: impl IntoParams,
) -> Result<i64, ServerFnError> {
    let mut rows = conn.query(sql, params).await.map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err),
        None => Ok(0),
    }
}

async fn text(
    conn: &Connection,
    sql: &str,
    params: impl IntoParams,
) -> Result<Option<String>, ServerFnError> {
    let mut rows = conn.query(sql, params).await.map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err),
        None => Ok(None),
    }
}

/// The (hashed, per day) network of the request, `None` for calls from within the server.
async fn network() -> Result<Option<String>, ServerFnError> {
    if use_context::<axum::http::request::Parts>().is_none() {
        return Ok(None);
    }
    let axum::Extension(client): axum::Extension<ClientIdentity> = leptos_axum::extract().await?;
    client
        .network_hash(now_ms().div_euclid(MS_PER_DAY))
        .map(Some)
        .map_err(ServerFnError::new)
}

/// Whether another mail with a link of `kind` may go to `user`: fewer than its limit within a day
/// for the account, fewer than [`PER_NETWORK`] from the network and [`PER_DAY`] in all. Links used
/// or replaced count too, their rows stay for a day ([`delete_old`]). The browser tests switch the
/// count per network off, like the other limits per network (`limits::network_limits_off`).
async fn budget_left(
    conn: &Connection,
    user: i64,
    kind: Kind,
    network: Option<&str>,
    now: i64,
) -> Result<bool, ServerFnError> {
    let since = now - MS_PER_DAY;
    let mine = scalar(
        conn,
        "SELECT COUNT(*) FROM account_tokens WHERE user_id = ?1 AND kind = ?2 AND created_at > ?3",
        params![user, kind.id(), since],
    )
    .await?;
    let from_network = match network.filter(|_| !super::limits::network_limits_off()) {
        Some(network) => {
            scalar(
                conn,
                "SELECT COUNT(*) FROM account_tokens WHERE ip_hash = ?1 AND created_at > ?2",
                params![network, since],
            )
            .await?
        }
        None => 0,
    };
    let all = scalar(
        conn,
        "SELECT COUNT(*) FROM account_tokens WHERE created_at > ?1",
        params![since],
    )
    .await?;
    Ok(mine < kind.per_account() && from_network < PER_NETWORK && all < PER_DAY)
}

/// Stores a new link of `kind` for `user`, returns its token.
async fn insert_link(
    conn: &Connection,
    user: i64,
    kind: Kind,
    email: Option<&str>,
    locale: Locale,
    network: Option<String>,
    now: i64,
) -> Result<String, ServerFnError> {
    let token = auth::new_token();
    conn.execute(
        "INSERT INTO account_tokens (token_hash, user_id, kind, email, locale, ip_hash, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            auth::hash_token(&token),
            user,
            kind.id(),
            email,
            locale.as_str(),
            network,
            now,
            now + kind.valid_ms()
        ],
    )
    .await
    .map_err(db_err)?;
    Ok(token)
}

/// The link of `token`, if it is one of `kind`.
async fn find_link(
    conn: &Connection,
    token: &str,
    kind: Kind,
) -> Result<Option<Link>, ServerFnError> {
    if !plausible(token) {
        return Ok(None);
    }
    let mut rows = conn
        .query(
            "SELECT user_id, email, locale, expires_at, used_at FROM account_tokens
             WHERE token_hash = ?1 AND kind = ?2",
            params![auth::hash_token(token), kind.id()],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    Ok(Some(Link {
        user: row.get(0).map_err(db_err)?,
        email: row.get(1).map_err(db_err)?,
        locale: row.get(2).map_err(db_err)?,
        expires_at: row.get(3).map_err(db_err)?,
        used_at: row.get(4).map_err(db_err)?,
    }))
}

/// The signed in account, with its username. Guests have no recovery email.
async fn registered(conn: &Connection) -> Result<(User, String), ServerFnError> {
    let user = auth::current_user(conn)
        .await?
        .ok_or_else(|| ApiError::SessionExpired.fail())?;
    let Some(username) = user.username.clone() else {
        return Err(ApiError::Forbidden.into());
    };
    Ok((user, username))
}

/// The recovery email of `user` as the account page shows it.
async fn status(conn: &Connection, user: i64) -> Result<RecoveryEmail, ServerFnError> {
    let address = text(conn, "SELECT email FROM users WHERE id = ?1", params![user]).await?;
    let pending = text(
        conn,
        "SELECT email FROM account_tokens
         WHERE user_id = ?1 AND kind = 'confirm' AND used_at IS NULL AND expires_at > ?2
             AND email IS NOT NULL
         ORDER BY created_at DESC LIMIT 1",
        params![user, now_ms()],
    )
    .await?;
    Ok(RecoveryEmail {
        address,
        pending,
        mail: available(),
    })
}

/// The recovery email of the signed in account.
pub async fn load(conn: &Connection) -> Result<RecoveryEmail, ServerFnError> {
    let (user, _) = registered(conn).await?;
    status(conn, user.id).await
}

/// Mails a confirmation link to `email`, the account's recovery email once the link is opened.
///
/// Behind the account's password, which counts as a sign in when it is wrong. An address still
/// waiting for its confirmation is called off; the account's address stays until then.
///
/// Whether another account has the address already isn't said here, only the confirmation link
/// says it, to whoever reads that mailbox: nobody learns who has an account by trying addresses.
pub async fn set_email(
    conn: &Connection,
    email: &str,
    password: String,
    locale: &str,
) -> Result<RecoveryEmail, ServerFnError> {
    let (user, username) = registered(conn).await?;
    if !available() {
        return Err(ApiError::MailUnavailable.into());
    }
    let email = parse_email(email)?;
    if auth::check_password(conn, &username, password)
        .await?
        .is_none()
    {
        return Err(ApiError::WrongPassword.into());
    }
    let now = now_ms();
    let current = status(conn, user.id).await?;
    if current
        .address
        .as_deref()
        .is_some_and(|a| a.eq_ignore_ascii_case(&email))
    {
        // The address it has: nothing to confirm, a change on its way is called off
        conn.execute(CALL_OFF_CONFIRMATIONS, params![user.id, now])
            .await
            .map_err(db_err)?;
        return status(conn, user.id).await;
    }
    let locale = super::issues::locale(Some(locale));
    let network = network().await?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    if !budget_left(&tx, user.id, Kind::Confirm, network.as_deref(), now).await? {
        return Err(ApiError::TooManyMessages.into());
    }
    tx.execute(CALL_OFF_CONFIRMATIONS, params![user.id, now])
        .await
        .map_err(db_err)?;
    let token = insert_link(
        &tx,
        user.id,
        Kind::Confirm,
        Some(&email),
        locale,
        network,
        now,
    )
    .await?;
    tx.commit().await.map_err(db_err)?;

    // Waited for: the player wants to know if it didn't go out
    if let Err(e) = mail::send(confirm_mail(locale, &username, &email, &token)).await {
        eprintln!("recovery: confirmation for #{} not sent: {e}", user.id);
        conn.execute(
            "DELETE FROM account_tokens WHERE token_hash = ?1",
            params![auth::hash_token(&token)],
        )
        .await
        .map_err(db_err)?;
        return Err(ApiError::MailUnavailable.into());
    }
    status(conn, user.id).await
}

/// Removes the recovery email of the signed in account, and calls off a new one waiting for its
/// confirmation, behind the password. The address removed is told.
pub async fn remove_email(
    conn: &Connection,
    password: String,
    locale: &str,
) -> Result<RecoveryEmail, ServerFnError> {
    let (user, username) = registered(conn).await?;
    if auth::check_password(conn, &username, password)
        .await?
        .is_none()
    {
        return Err(ApiError::WrongPassword.into());
    }
    let now = now_ms();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let old = text(
        &tx,
        "SELECT email FROM users WHERE id = ?1",
        params![user.id],
    )
    .await?;
    tx.execute(
        "UPDATE users SET email = NULL WHERE id = ?1",
        params![user.id],
    )
    .await
    .map_err(db_err)?;
    tx.execute(USE_UP_ALL, params![user.id, now])
        .await
        .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    if let Some(old) = old {
        let locale = super::issues::locale(Some(locale));
        mail::send_later(
            "removal notice",
            notice_mail(locale, &username, &old, Notice::Removed),
        );
    }
    status(conn, user.id).await
}

/// Opens a confirmation link: its address becomes the recovery email of its account. Returns the
/// address. Needs no session, the link may be opened anywhere.
///
/// Every other link of the account is used up: one waiting for confirmation, and the reset links
/// that went to the old address. The old address is told.
pub async fn confirm(conn: &Connection, token: &str) -> Result<String, ServerFnError> {
    let now = now_ms();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let link = find_link(&tx, token, Kind::Confirm)
        .await?
        .filter(|l| l.open(now))
        .ok_or_else(|| ApiError::LinkExpired.fail())?;
    let email = link
        .email
        .clone()
        .ok_or_else(|| ApiError::LinkExpired.fail())?;
    let mut rows = tx
        .query(
            "SELECT username, email FROM users
             WHERE id = ?1 AND username IS NOT NULL AND deleted_at IS NULL",
            params![link.user],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Err(ApiError::LinkExpired.into());
    };
    let username: String = row.get(0).map_err(db_err)?;
    let old: Option<String> = row.get(1).map_err(db_err)?;
    drop(rows);
    tx.execute(
        "UPDATE users SET email = ?1 WHERE id = ?2",
        params![email.clone(), link.user],
    )
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            ApiError::EmailTaken.fail()
        } else {
            db_err(e)
        }
    })?;
    tx.execute(USE_UP_ALL, params![link.user, now])
        .await
        .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    if let Some(old) = old.filter(|old| !old.eq_ignore_ascii_case(&email)) {
        let locale = super::issues::locale(link.locale.as_deref());
        mail::send_later(
            "change notice",
            notice_mail(locale, &username, &old, Notice::Changed),
        );
    }
    Ok(email)
}

/// "Forgot password?": mails a reset link to the recovery email of the account with the username
/// or recovery email `login`. Not to banned or deleted accounts, and only within the limits
/// ([`budget_left`]).
///
/// Answers the same in every case, before anything is looked up: whether there is such an account,
/// whether it has an address, whether a mail went out, nobody learns it here. Only a server
/// without mail says so, that's the same for everyone.
pub async fn forgot(conn: &Connection, login: &str, locale: &str) -> Result<(), ServerFnError> {
    if !available() {
        return Err(ApiError::MailUnavailable.into());
    }
    let network = network().await?;
    let conn = conn.clone();
    let login = login.trim().to_string();
    let locale = super::issues::locale(Some(locale));
    mail::in_background("reset link", async move {
        mail_reset_link(&conn, &login, locale, network)
            .await
            .map_err(|e| e.to_string())
    });
    Ok(())
}

/// The work of [`forgot`], after its answer.
async fn mail_reset_link(
    conn: &Connection,
    login: &str,
    locale: Locale,
    network: Option<String>,
) -> Result<(), ServerFnError> {
    if login.is_empty() || login.len() > MAX_EMAIL {
        return Ok(());
    }
    // Usernames have no `@`, addresses always one: no login is both
    let mut rows = conn
        .query(
            "SELECT id, username, email FROM users
             WHERE (username = ?1 OR email = ?1) AND username IS NOT NULL AND email IS NOT NULL
                 AND deleted_at IS NULL AND banned_at IS NULL
             LIMIT 1",
            params![login],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(());
    };
    let user: i64 = row.get(0).map_err(db_err)?;
    let username: String = row.get(1).map_err(db_err)?;
    let email: String = row.get(2).map_err(db_err)?;
    drop(rows);

    let now = now_ms();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    if !budget_left(&tx, user, Kind::Reset, network.as_deref(), now).await? {
        return Ok(());
    }
    // A new link replaces the ones before
    tx.execute(USE_UP_RESETS, params![user, now])
        .await
        .map_err(db_err)?;
    let token = insert_link(&tx, user, Kind::Reset, None, locale, network, now).await?;
    tx.commit().await.map_err(db_err)?;
    mail::send(reset_mail(locale, &username, &email, &token))
        .await
        .map_err(ServerFnError::new)
}

/// Opens a reset link: a new password for its account, and this browser signed in to it.
///
/// The new password ends every session of the account. Signing in here is as signing in is: the
/// results of a guest playing here move to the account. The link and the account's other reset
/// links are used up.
pub async fn reset(
    conn: &Connection,
    token: &str,
    password: String,
) -> Result<User, ServerFnError> {
    // Before the link is used up: a password that can't be stored keeps it working
    auth::validate_password(&password)?;
    let now = now_ms();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let link = find_link(&tx, token, Kind::Reset)
        .await?
        .filter(|l| l.open(now))
        .ok_or_else(|| ApiError::LinkExpired.fail())?;
    let allowed = scalar(
        &tx,
        "SELECT COUNT(*) FROM users
         WHERE id = ?1 AND username IS NOT NULL AND deleted_at IS NULL AND banned_at IS NULL",
        params![link.user],
    )
    .await?;
    if allowed == 0 {
        return Err(ApiError::LinkExpired.into());
    }
    tx.execute(
        "UPDATE account_tokens SET used_at = ?2 WHERE token_hash = ?1",
        params![auth::hash_token(token), now],
    )
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;

    if let Err(e) = auth::set_password(conn, link.user, password, None).await {
        // Not stored (the server too busy to hash it, a ban meanwhile): the link works again,
        // until it expires
        conn.execute(
            "UPDATE account_tokens SET used_at = NULL WHERE token_hash = ?1 AND used_at = ?2",
            params![auth::hash_token(token), now],
        )
        .await
        .map_err(db_err)?;
        return Err(match ApiError::from_message(&e.to_string()) {
            Some(ApiError::SessionExpired) => ApiError::LinkExpired.into(),
            _ => e,
        });
    }
    auth::sign_in_here(conn, link.user).await?;
    auth::user_by_id(conn, link.user)
        .await?
        .ok_or_else(|| ApiError::Internal.fail())
}

/// Deletes the links that expired a while ago: kept for a day after they were made to count the
/// mails ([`budget_left`]), an address waiting for its confirmation until its link expires.
pub async fn delete_old(conn: &Connection) -> Result<(), ServerFnError> {
    let now = now_ms();
    conn.execute(
        "DELETE FROM account_tokens WHERE expires_at < ?1 AND created_at < ?2",
        params![now, now - MS_PER_DAY],
    )
    .await
    .map_err(db_err)?;
    Ok(())
}

/// The page of a link: in the player's language, the token after the `#`, which browsers never
/// send to a server (no logs have it, no `Referer`).
fn page_link(locale: Locale, path: &str, token: &str) -> String {
    format!("{SITE_URL}{}#{token}", localized(locale, path))
}

/// The end of every mail.
const SIGNATURE: &str = "Hashi · https://hashi.rs";

fn confirm_mail(locale: Locale, username: &str, to: &str, token: &str) -> Mail {
    let body = format!(
        "{}\n\n{}\n{}\n\n{}\n{}\n\n{SIGNATURE}\n",
        td_string!(locale, account.mail.hello, name = username),
        td_string!(locale, account.mail.confirm_text),
        page_link(locale, CONFIRM_PATH, token),
        td_string!(locale, account.mail.confirm_valid),
        td_string!(locale, account.mail.ignore),
    );
    Mail {
        to: to.to_string(),
        subject: format!(
            "Hashi: {}",
            td_string!(locale, account.mail.confirm_subject)
        ),
        body,
    }
}

fn reset_mail(locale: Locale, username: &str, to: &str, token: &str) -> Mail {
    let body = format!(
        "{}\n\n{}\n{}\n\n{}\n{}\n\n{SIGNATURE}\n",
        td_string!(locale, account.mail.hello, name = username),
        td_string!(locale, account.mail.reset_text),
        page_link(locale, RESET_PATH, token),
        td_string!(locale, account.mail.reset_valid),
        td_string!(locale, account.mail.ignore),
    );
    Mail {
        to: to.to_string(),
        subject: format!("Hashi: {}", td_string!(locale, account.mail.reset_subject)),
        body,
    }
}

/// What happened to an address, for the mail that tells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Notice {
    /// Another address took its place.
    Changed,
    Removed,
}

/// Tells an address that it no longer is the recovery email of its account. The new one isn't in
/// it.
fn notice_mail(locale: Locale, username: &str, to: &str, notice: Notice) -> Mail {
    let (subject, text) = match notice {
        Notice::Changed => (
            td_string!(locale, account.mail.changed_subject),
            td_string!(locale, account.mail.changed_text),
        ),
        Notice::Removed => (
            td_string!(locale, account.mail.removed_subject),
            td_string!(locale, account.mail.removed_text),
        ),
    };
    let body = format!(
        "{}\n\n{text}\n\n{}\n{SITE_URL}{}\n\n{SIGNATURE}\n",
        td_string!(locale, account.mail.hello, name = username),
        td_string!(locale, account.mail.not_you),
        localized(locale, "/kontakt"),
    );
    Mail {
        to: to.to_string(),
        subject: format!("Hashi: {subject}"),
        body,
    }
}

/// For `auth::erase`, `?1` the user: the links of the account. Its address goes with the rest of
/// the account's row.
pub const ERASE: [&str; 1] = ["DELETE FROM account_tokens WHERE user_id = ?1"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_checked_and_trimmed() {
        for (input, address) in [
            ("ada@example.org", "ada@example.org"),
            (
                "  Ada.Lovelace+hashi@Example.ORG \n",
                "Ada.Lovelace+hashi@Example.ORG",
            ),
            ("kim@localhost", "kim@localhost"),
        ] {
            assert_eq!(parse_email(input).as_deref(), Ok(address), "{input:?}");
        }
        // One byte over, every part of it fine on its own
        let long = format!(
            "{}@{}.{}.{}.org",
            "a".repeat(64),
            "x".repeat(63),
            "y".repeat(63),
            "z".repeat(58)
        );
        assert_eq!(long.len(), MAX_EMAIL + 1);
        for input in [
            "",
            "   ",
            "ada",
            "ada@",
            "@example.org",
            "ada@@example.org",
            "Ada <ada@example.org>",
            "ada@example.org, bob@example.org",
            "ada lovelace@example.org",
            "ada@exa mple.org",
            "ada\n@example.org",
            "ada@example.org\r\nBcc: eve@example.org",
            long.as_str(),
        ] {
            assert_eq!(parse_email(input), Err(ApiError::EmailInvalid), "{input:?}");
        }
        // The longest there can be: 64 before the `@`, labels of at most 63
        let domain = format!(
            "{}.{}.{}.org",
            "x".repeat(63),
            "y".repeat(63),
            "z".repeat(57)
        );
        let longest = format!("{}@{domain}", "a".repeat(64));
        assert_eq!(longest.len(), MAX_EMAIL);
        assert!(parse_email(&longest).is_ok());
        let longer = format!("{}@{domain}", "a".repeat(65));
        assert_eq!(parse_email(&longer), Err(ApiError::EmailInvalid));
    }

    #[test]
    fn links_work_once_until_they_expire() {
        let link = Link {
            user: 7,
            email: None,
            locale: None,
            expires_at: 1_000 + RESET_VALID_MS,
            used_at: None,
        };
        assert!(link.open(1_000));
        assert!(link.open(1_000 + RESET_VALID_MS - 1));
        assert!(!link.open(1_000 + RESET_VALID_MS));
        let used = Link {
            used_at: Some(1_500),
            ..link
        };
        assert!(!used.open(1_600));
        // An hour to reset, a day to confirm: what the mails say
        assert_eq!(Kind::Reset.valid_ms(), 60 * 60 * 1000);
        assert_eq!(Kind::Confirm.valid_ms(), MS_PER_DAY);
    }

    /// The cleanup deletes links that expired and are more than a day old: used ones too, but
    /// not before a day, they count for the limits.
    #[tokio::test]
    async fn old_links_are_deleted() {
        let conn = crate::server::db::TestConn::new("recovery-cleanup").await;
        let now = now_ms();
        for (hash, created, expires) in [
            // A reset link from two days ago, and a confirmation from yesterday that just expired
            (
                "gone-reset",
                now - 2 * MS_PER_DAY,
                now - 2 * MS_PER_DAY + RESET_VALID_MS,
            ),
            ("gone-confirm", now - MS_PER_DAY - 1, now - 1),
            // A reset link of an hour ago, expired, still counted
            ("counted", now - HOUR_MS - 1, now - 1),
            ("open", now, now + CONFIRM_VALID_MS),
        ] {
            conn.execute(
                "INSERT INTO account_tokens (token_hash, user_id, kind, created_at, expires_at, used_at)
                 VALUES (?1, 7, 'reset', ?2, ?3, ?2)",
                params![hash, created, expires],
            )
            .await
            .unwrap();
        }
        delete_old(&conn).await.unwrap();
        let mut rows = conn
            .query(
                "SELECT token_hash FROM account_tokens ORDER BY token_hash",
                (),
            )
            .await
            .unwrap();
        let mut left = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            left.push(row.get::<String>(0).unwrap());
        }
        assert_eq!(left, ["counted", "open"]);
    }

    /// Turso refuses statements given more parameters than they use: the ones run with a fixed
    /// list of parameters take exactly those.
    #[tokio::test]
    async fn statements_take_their_parameters() {
        let conn = crate::server::db::TestConn::new("recovery-parameters").await;
        for (sql, given) in [
            (USE_UP_RESETS, 2),
            (CALL_OFF_CONFIRMATIONS, 2),
            (USE_UP_ALL, 2),
            (ERASE[0], 1),
        ] {
            let statement = conn.prepare(sql).await.unwrap();
            assert_eq!(statement.parameter_count(), given, "{sql}");
        }
    }

    #[test]
    fn only_tokens_of_ours_are_looked_up() {
        assert!(plausible(&auth::new_token()));
        for token in [
            String::new(),
            "abc".into(),
            "zz".repeat(32),
            "ab".repeat(33),
            format!("{} ", "ab".repeat(32)),
        ] {
            assert!(!plausible(&token), "{token}");
        }
    }

    /// The mails, in the player's language: their name, the link to the page in that language
    /// with the token after the `#`, how long it works, and what to do if it wasn't them.
    #[test]
    fn mails_say_what_to_do() {
        let token = "ab".repeat(32);
        let mail = reset_mail(Locale::de, "noel", "noel@example.org", &token);
        assert_eq!(mail.to, "noel@example.org");
        assert_eq!(
            mail.subject,
            format!(
                "Hashi: {}",
                td_string!(Locale::de, account.mail.reset_subject)
            )
        );
        assert!(mail.body.starts_with("Hallo noel,\n\n"), "{}", mail.body);
        assert!(
            mail.body
                .contains(&format!("\nhttps://hashi.rs/de/account/reset#{token}\n")),
            "{}",
            mail.body
        );
        for line in [
            td_string!(Locale::de, account.mail.reset_valid),
            td_string!(Locale::de, account.mail.ignore),
            SIGNATURE,
        ] {
            assert!(mail.body.contains(line), "{line}: {}", mail.body);
        }

        let mail = confirm_mail(Locale::en, "kim", "kim@example.org", &token);
        assert!(
            mail.body
                .contains(&format!("\nhttps://hashi.rs/account/confirm#{token}\n")),
            "{}",
            mail.body
        );
        assert!(mail.body.contains("24 hours"), "{}", mail.body);
        assert_eq!(mail.subject, "Hashi: Confirm your recovery email");

        let mail = notice_mail(Locale::ja, "kim", "old@example.org", Notice::Removed);
        assert_eq!(mail.to, "old@example.org");
        assert!(
            mail.body.contains("https://hashi.rs/ja/kontakt"),
            "{}",
            mail.body
        );
        assert!(!mail.body.contains('#'), "{}", mail.body);

        // Every language has every text, its variables filled in
        for &locale in Locale::get_all() {
            for mail in [
                confirm_mail(locale, "kim", "kim@example.org", &token),
                reset_mail(locale, "kim", "kim@example.org", &token),
                notice_mail(locale, "kim", "kim@example.org", Notice::Changed),
                notice_mail(locale, "kim", "kim@example.org", Notice::Removed),
            ] {
                assert!(mail.body.contains("kim"), "{locale:?}: {}", mail.body);
                assert!(
                    !mail.body.contains("{{") && !mail.subject.contains("{{"),
                    "{locale:?}: {}",
                    mail.body
                );
                assert!(mail.subject.len() > "Hashi: ".len(), "{locale:?}");
            }
        }
    }
}
