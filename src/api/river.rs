//! The river 川 of a group: the bridge its members build together (see `model::river`).

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::GroupRiver;
#[cfg(feature = "ssr")]
use crate::server::{auth, daily, db, groups};

/// A group's river, for a member. Looking at it completes the levels that are across and
/// remembers what the member saw: the planks laid since show as new only once, a level completed
/// in between is celebrated once. Not `group_river`: its generated struct would clash with the
/// model type.
#[server(input = Json, client = HashiClient)]
pub async fn load_group_river(id: i64) -> Result<GroupRiver, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn)
        .await?
        .ok_or_else(|| crate::model::ApiError::SessionExpired.fail())?;
    groups::river::river(&conn, id, user.id, daily::today()).await
}
