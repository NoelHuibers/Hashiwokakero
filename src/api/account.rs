//! Accounts: the current user, registering, logging in and out, deleting, statistics, public
//! profiles.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use crate::model::{PlayerProfile, Stamp, UserInfo, UserStats};
#[cfg(feature = "ssr")]
use crate::server::{auth, daily, db};

#[server(input = Json)]
pub async fn current_user() -> Result<Option<UserInfo>, ServerFnError> {
    let conn = db::conn()?;
    Ok(auth::current_user(&conn).await?.map(|u| u.info()))
}

/// Creates an account. A guest keeps their results and streak.
#[server(input = Json)]
pub async fn register(username: String, password: String) -> Result<UserInfo, ServerFnError> {
    let conn = db::conn()?;
    Ok(auth::register(&conn, username.trim(), password)
        .await?
        .info())
}

#[server(input = Json)]
pub async fn login(username: String, password: String) -> Result<UserInfo, ServerFnError> {
    let conn = db::conn()?;
    Ok(auth::login(&conn, username.trim(), password).await?.info())
}

#[server(input = Json)]
pub async fn logout() -> Result<(), ServerFnError> {
    auth::logout(&db::conn()?).await
}

/// Deletes the current account and signs out. A registered account needs its password.
#[server(input = Json)]
pub async fn delete_account(password: Option<String>) -> Result<(), ServerFnError> {
    auth::delete_account(&db::conn()?, password).await
}

/// Statistics of the current user, `None` without a session.
#[server(input = Json)]
pub async fn my_stats() -> Result<Option<UserStats>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => Ok(Some(daily::stats(&conn, user.id).await?)),
        None => Ok(None),
    }
}

/// The public profile of any player, `None` if there is no such player. Not `player_profile`:
/// its generated struct would clash with the model type.
#[server(input = Json)]
pub async fn load_player(id: i64) -> Result<Option<PlayerProfile>, ServerFnError> {
    daily::profile(&db::conn()?, id).await
}

/// Shows one of the current user's stamps next to their name on the leaderboards, or none.
#[server(input = Json)]
pub async fn show_stamp(stamp: Option<Stamp>) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn)
        .await?
        .ok_or_else(|| crate::model::ApiError::SessionExpired.fail())?;
    daily::show_stamp(&conn, user.id, stamp).await
}
