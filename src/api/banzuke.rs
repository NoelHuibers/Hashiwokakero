//! The weekly banzuke 番付 of a group and its awards 賞, and its all time table.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{Banzuke, GroupAllTime};
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

/// A group's all time table: daily wins, race wins, yokozuna weeks and streaks, for a member.
#[server(input = Json, client = HashiClient)]
pub async fn load_group_all_time(id: i64) -> Result<GroupAllTime, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn)
        .await?
        .ok_or_else(|| crate::model::ApiError::SessionExpired.fail())?;
    let table = groups::all_time::all_time(&conn, id, user.id, daily::today()).await?;
    blocks::hidden(&conn, Some(user.id), table).await
}
