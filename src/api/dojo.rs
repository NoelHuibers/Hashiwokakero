//! The SAT Dojo: leaderboard and submissions.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{DojoBoard, DojoMe, DojoSeasons, DojoSubmission};
#[cfg(feature = "ssr")]
use crate::server::{auth, db, dojo};

/// The leaderboard of the current season.
#[server(input = Json, client = HashiClient)]
pub async fn load_dojo_board() -> Result<DojoBoard, ServerFnError> {
    dojo::board(&db::conn()?).await
}

/// The current season's commitment and every finished season with its seed and final board.
#[server(input = Json, client = HashiClient)]
pub async fn load_dojo_seasons() -> Result<DojoSeasons, ServerFnError> {
    dojo::seasons(&db::conn()?, dojo::season()).await
}

/// One submission with its score, `None` if there is no such submission.
#[server(input = Json, client = HashiClient)]
pub async fn load_dojo_submission(id: i64) -> Result<Option<DojoSubmission>, ServerFnError> {
    let conn = db::conn()?;
    let me = auth::current_user(&conn).await?.map(|u| u.id);
    dojo::submission(&conn, id, me).await
}

/// The current user's submissions and how many they can still make today.
#[server(input = Json, client = HashiClient)]
pub async fn load_dojo_me() -> Result<DojoMe, ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn).await?;
    dojo::me(&conn, user.as_ref()).await
}
