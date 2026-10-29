//! The weekly banzuke 番付 of a group and its awards 賞.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::Banzuke;
#[cfg(feature = "ssr")]
use crate::server::{auth, blocks, daily, db, groups};

/// A group's banzuke of the week of `monday` (the current one if `None`), for a member.
#[server(input = Json, client = HashiClient)]
pub async fn load_banzuke(id: i64, monday: Option<i64>) -> Result<Banzuke, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn)
        .await?
        .ok_or_else(|| crate::model::ApiError::SessionExpired.fail())?;
    let banzuke = groups::banzuke::banzuke(&conn, id, user.id, monday, daily::today()).await?;
    blocks::hidden(&conn, Some(user.id), banzuke).await
}
