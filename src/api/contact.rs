//! The contact form.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;

#[cfg(feature = "ssr")]
use crate::server::{contact, db};

/// Sends a message to us. `website` is the field only bots fill in.
#[server(input = Json, client = HashiClient)]
pub async fn send_message(
    name: String,
    email: String,
    message: String,
    website: String,
) -> Result<(), ServerFnError> {
    contact::send(&db::conn()?, &name, &email, &message, &website).await
}
