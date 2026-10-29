//! The feedback form.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{FeedbackContext, FeedbackKind};

#[cfg(feature = "ssr")]
use crate::server::{auth, contact, db};

/// Sends a message to us. `website` is the field only bots fill in.
///
/// `kind` and `context` came with the feedback form: apps from before send neither (a message
/// of `FeedbackKind::Other` without a context). The context's player is the session's.
#[server(input = Json, client = HashiClient)]
pub async fn send_message(
    name: String,
    email: String,
    message: String,
    website: String,
    #[server(default)] kind: FeedbackKind,
    #[server(default)] context: Option<FeedbackContext>,
) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    // Only for the context: a visitor without a session stays without one
    let player = match context {
        Some(_) => auth::current_user(&conn).await?.map(|u| u.id),
        None => None,
    };
    let feedback = contact::Feedback {
        name,
        email,
        message,
        website,
        kind,
        context,
    };
    contact::send(&conn, feedback, player).await
}
