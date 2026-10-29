//! Groups (仲間 nakama): making, joining and leaving them, the group page and the boards of
//! today of the player's groups.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{GroupPage, GroupSummary, GroupToday, GroupWeek, InvitePreview};
#[cfg(feature = "ssr")]
use crate::server::{auth, blocks, daily, db, groups};

#[cfg(feature = "ssr")]
async fn signed_in() -> Result<(libsql::Connection, auth::User), ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn)
        .await?
        .ok_or_else(|| crate::model::ApiError::SessionExpired.fail())?;
    Ok((conn, user))
}

/// The groups of the current player, none without a session.
#[server(input = Json, client = HashiClient)]
pub async fn my_groups() -> Result<Vec<GroupSummary>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => groups::mine(&conn, user.id, daily::today()).await,
        None => Ok(Vec::new()),
    }
}

/// Makes a group, the visitor becomes a guest if they aren't one yet. Returns its page, for the
/// invite step.
#[server(input = Json, client = HashiClient)]
pub async fn create_group(name: String) -> Result<GroupPage, ServerFnError> {
    // Before a guest is made for nothing
    groups::check_name(&name)?;
    let conn = db::conn()?;
    let user = auth::ensure_user(&conn).await?;
    let id = groups::create(&conn, user.id, &name).await?;
    groups::page(&conn, id, user.id, daily::today()).await
}

/// What an invite link shows, `None` if it doesn't work (any more).
#[server(input = Json, client = HashiClient)]
pub async fn group_invite(code: String) -> Result<Option<InvitePreview>, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn).await?.map(|u| u.id);
    let preview = groups::preview(&conn, &code, user, daily::today()).await?;
    blocks::hidden(&conn, user, preview).await
}

/// Joins the group of an invite link, as a new guest for a visitor without a user. Returns the
/// group's id.
#[server(input = Json, client = HashiClient)]
pub async fn join_group(code: String) -> Result<i64, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::ensure_user(&conn).await?;
    groups::join(&conn, &code, user.id).await
}

/// A group's page, `None` for a group the visitor isn't in (or that doesn't exist).
#[server(input = Json, client = HashiClient)]
pub async fn load_group(id: i64) -> Result<Option<GroupPage>, ServerFnError> {
    let conn = db::conn()?;
    let Some(user) = auth::current_user(&conn).await? else {
        return Ok(None);
    };
    match groups::page(&conn, id, user.id, daily::today()).await {
        Ok(page) => blocks::hidden(&conn, Some(user.id), Some(page)).await,
        Err(e) if is_not_found(&e) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(feature = "ssr")]
fn is_not_found(e: &ServerFnError) -> bool {
    crate::model::ApiError::from_message(&e.to_string())
        == Some(crate::model::ApiError::GroupNotFound)
}

/// The week grid of a group, the week of the day `monday` (the current one if `None`). Not
/// `group_week`: its generated struct would clash with the model type.
#[server(input = Json, client = HashiClient)]
pub async fn load_group_week(id: i64, monday: Option<i64>) -> Result<GroupWeek, ServerFnError> {
    let (conn, user) = signed_in().await?;
    let week = groups::week(&conn, id, user.id, monday, daily::today()).await?;
    blocks::hidden(&conn, Some(user.id), week).await
}

#[server(input = Json, client = HashiClient)]
pub async fn leave_group(id: i64) -> Result<(), ServerFnError> {
    let (conn, user) = signed_in().await?;
    groups::leave(&conn, id, user.id).await
}

/// The owner takes a member out of the group.
#[server(input = Json, client = HashiClient)]
pub async fn remove_member(id: i64, member: i64) -> Result<(), ServerFnError> {
    let (conn, user) = signed_in().await?;
    groups::remove(&conn, id, user.id, member).await
}

/// The owner replaces the invite link, the old one stops working. Returns the new code.
#[server(input = Json, client = HashiClient)]
pub async fn rotate_invite(id: i64) -> Result<String, ServerFnError> {
    let (conn, user) = signed_in().await?;
    groups::rotate_invite(&conn, id, user.id).await
}

/// Today's boards of all the player's groups, for the daily page (spoiler safe).
#[server(input = Json, client = HashiClient)]
pub async fn group_boards() -> Result<Vec<GroupToday>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => {
            let boards = groups::boards(&conn, user.id, daily::today()).await?;
            blocks::hidden(&conn, Some(user.id), boards).await
        }
        None => Ok(Vec::new()),
    }
}
