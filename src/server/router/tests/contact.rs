//! The contact form. SMTP isn't set up here, messages are only stored.

use super::*;
use crate::api::{AdminMessages, DeleteAdminMessage, SendMessage};

fn message(email: &str, text: &str) -> SendMessage {
    SendMessage {
        name: "Kim".into(),
        email: email.into(),
        message: text.into(),
        website: String::new(),
    }
}

#[tokio::test]
async fn messages_reach_the_admins() {
    let server = Server::start().await;
    let visitor = server.browser_from("203.0.113.7");
    for (email, text, error) in [
        (
            "not an address",
            "Hello there, nice puzzles!",
            ApiError::EmailInvalid,
        ),
        ("kim@example.com", "Hi", ApiError::MessageInvalid),
        ("", &"x".repeat(5001), ApiError::MessageInvalid),
    ] {
        assert_eq!(api_error(visitor.call(message(email, text)).await), error);
    }
    let text = "The bridges on day 300 crossed, I think.";
    visitor
        .call(message("kim@example.com", text))
        .await
        .unwrap();
    // Without an address too: an answer isn't always wanted
    visitor
        .call(message("", "Just saying thanks for the dailies."))
        .await
        .unwrap();
    // A bot that filled in the hidden field is thanked, but nothing is kept
    let bot_text = "Buy cheap watches at our shop now";
    visitor
        .call(SendMessage {
            website: "https://spam.example".into(),
            ..message("bot@example.com", bot_text)
        })
        .await
        .unwrap();

    assert_eq!(
        api_error(visitor.call(AdminMessages { before: None }).await),
        ApiError::Forbidden
    );
    assert_eq!(
        api_error(visitor.call(DeleteAdminMessage { id: 1 }).await),
        ApiError::Forbidden
    );
    let admin = Browser::admin(&server).await;
    let messages = admin.call(AdminMessages { before: None }).await.unwrap();
    let sent = messages.iter().find(|m| m.message == text).unwrap();
    assert_eq!((&*sent.name, &*sent.email), ("Kim", "kim@example.com"));
    assert!(!sent.mailed);
    assert!(messages.iter().all(|m| m.message != bot_text));
    assert!(
        admin
            .call(crate::api::LoadAdminOverview {})
            .await
            .unwrap()
            .messages
            >= 2
    );
    admin
        .call(DeleteAdminMessage { id: sent.id })
        .await
        .unwrap();
    let messages = admin.call(AdminMessages { before: None }).await.unwrap();
    assert!(messages.iter().all(|m| m.id != sent.id));
}

#[tokio::test]
async fn a_network_sends_five_messages_a_day() {
    let server = Server::start().await;
    let visitor = server.browser_from("198.51.100.23");
    for i in 0..5 {
        visitor
            .call(message("", &format!("Message number {i} from here")))
            .await
            .unwrap();
    }
    assert_eq!(
        api_error(visitor.call(message("", "One message too many")).await),
        ApiError::TooManyMessages
    );
    // Another network still gets through
    server
        .browser_from("198.51.100.24")
        .call(message("", "From somewhere else entirely"))
        .await
        .unwrap();
}
