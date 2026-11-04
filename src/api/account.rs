//! Accounts: the current user, registering, logging in and out, renaming, changing the password,
//! the recovery email and resetting a forgotten password, deleting, statistics, public profiles.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{PlayerProfile, RecoveryEmail, Stamp, UserInfo, UserStats};
#[cfg(feature = "ssr")]
use crate::server::{auth, daily, db, recovery};

#[server(input = Json, client = HashiClient)]
pub async fn current_user() -> Result<Option<UserInfo>, ServerFnError> {
    let conn = db::conn()?;
    Ok(auth::current_user(&conn).await?.map(|u| u.info()))
}

/// Creates an account. A guest keeps their results and streak.
#[server(input = Json, client = HashiClient)]
pub async fn register(username: String, password: String) -> Result<UserInfo, ServerFnError> {
    let conn = db::conn()?;
    Ok(auth::register(&conn, username.trim(), password)
        .await?
        .info())
}

#[server(input = Json, client = HashiClient)]
pub async fn login(username: String, password: String) -> Result<UserInfo, ServerFnError> {
    let conn = db::conn()?;
    Ok(auth::login(&conn, username.trim(), password).await?.info())
}

/// Changes the username of the signed in account, behind its password and once a month.
#[server(input = Json, client = HashiClient)]
pub async fn change_username(
    username: String,
    password: String,
) -> Result<UserInfo, ServerFnError> {
    let conn = db::conn()?;
    Ok(auth::change_username(&conn, username.trim(), password)
        .await?
        .info())
}

/// Changes the password of the signed in account, behind the current one. Signs the account out
/// everywhere else, this session stays.
#[server(input = Json, client = HashiClient)]
pub async fn change_password(current: String, new: String) -> Result<(), ServerFnError> {
    auth::change_password(&db::conn()?, current, new).await
}

/// The recovery email of the signed in account, one waiting for its confirmation, and whether
/// the server can send mail at all.
#[server(input = Json, client = HashiClient)]
pub async fn load_recovery_email() -> Result<RecoveryEmail, ServerFnError> {
    recovery::load(&db::conn()?).await
}

/// Mails a confirmation link to `email`, behind the password. The address becomes the account's
/// recovery email once the link is opened. `locale`: the page's language, the mail's.
#[server(input = Json, client = HashiClient)]
pub async fn set_recovery_email(
    email: String,
    password: String,
    locale: String,
) -> Result<RecoveryEmail, ServerFnError> {
    recovery::set_email(&db::conn()?, &email, password, &locale).await
}

/// Removes the recovery email of the signed in account, behind the password.
#[server(input = Json, client = HashiClient)]
pub async fn remove_recovery_email(
    password: String,
    locale: String,
) -> Result<RecoveryEmail, ServerFnError> {
    recovery::remove_email(&db::conn()?, password, &locale).await
}

/// Opens the link of a confirmation mail, returns the address it confirmed.
#[server(input = Json, client = HashiClient)]
pub async fn confirm_recovery_email(token: String) -> Result<String, ServerFnError> {
    recovery::confirm(&db::conn()?, &token).await
}

/// Whether "Forgot password?" can mail anything: not without a mail server.
#[server(input = Json, client = HashiClient)]
pub async fn password_reset_available() -> Result<bool, ServerFnError> {
    // Nothing to wait for. `#[server]` copies attributes onto every item it generates, so an
    // `#[expect(clippy::unused_async)]` would be unfulfilled on the others
    Ok(std::future::ready(recovery::available()).await)
}

/// Mails a reset link to the recovery email of the account with this username or address, if
/// there is one. The answer is the same either way.
#[server(input = Json, client = HashiClient)]
pub async fn forgot_password(login: String, locale: String) -> Result<(), ServerFnError> {
    recovery::forgot(&db::conn()?, &login, &locale).await
}

/// Opens the link of a reset mail: a new password, every device signed out, this one signed in.
#[server(input = Json, client = HashiClient)]
pub async fn reset_password(token: String, password: String) -> Result<UserInfo, ServerFnError> {
    Ok(recovery::reset(&db::conn()?, &token, password)
        .await?
        .info())
}

#[server(input = Json, client = HashiClient)]
pub async fn logout() -> Result<(), ServerFnError> {
    auth::logout(&db::conn()?).await
}

/// Deletes the current account and signs out. A registered account needs its password.
#[server(input = Json, client = HashiClient)]
pub async fn delete_account(password: Option<String>) -> Result<(), ServerFnError> {
    auth::delete_account(&db::conn()?, password).await
}

/// Statistics of the current user, `None` without a session.
#[server(input = Json, client = HashiClient)]
pub async fn my_stats() -> Result<Option<UserStats>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => Ok(Some(daily::stats(&conn, user.id).await?)),
        None => Ok(None),
    }
}

/// The public profile of any player, `None` if there is no such player. Not `player_profile`:
/// its generated struct would clash with the model type.
#[server(input = Json, client = HashiClient)]
pub async fn load_player(id: i64) -> Result<Option<PlayerProfile>, ServerFnError> {
    daily::cached_profile(id).await
}

/// Shows one of the current user's stamps next to their name on the leaderboards, or none.
#[server(input = Json, client = HashiClient)]
pub async fn show_stamp(stamp: Option<Stamp>) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn)
        .await?
        .ok_or_else(|| crate::model::ApiError::SessionExpired.fail())?;
    daily::show_stamp(&conn, user.id, stamp).await
}
