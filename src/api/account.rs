//! Accounts: the current user, registering, logging in and out, statistics.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use crate::model::{UserInfo, UserStats};
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

/// Statistics of the current user, `None` without a session.
#[server(input = Json)]
pub async fn my_stats() -> Result<Option<UserStats>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => Ok(Some(daily::stats(&conn, user.id).await?)),
        None => Ok(None),
    }
}
