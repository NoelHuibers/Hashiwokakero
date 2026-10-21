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

/// Direct clients cannot choose their quota key, even by sending Fly's header themselves.
#[tokio::test]
async fn direct_contact_quota_uses_the_socket_peer_despite_spoofed_headers() {
    let server = Server::direct(true).await;
    for i in 0..6 {
        let mut headers = HeaderMap::new();
        let fake = format!("192.0.2.{}", i + 1);
        headers.insert("fly-client-ip", fake.parse().unwrap());
        headers.insert("x-forwarded-for", fake.parse().unwrap());
        headers.insert("forwarded", format!("for={fake}").parse().unwrap());
        let browser = server.browser_at(headers, Some("127.0.0.80".parse().unwrap()));
        let result = browser
            .call(message("", &format!("Direct contact quota regression {i}")))
            .await;
        if i < 5 {
            result.unwrap();
        } else {
            assert_eq!(api_error(result), ApiError::TooManyMessages);
        }
    }
    server
        .browser_at(HeaderMap::new(), Some("127.0.0.81".parse().unwrap()))
        .call(message("", "Direct contact from a distinct peer"))
        .await
        .unwrap();
    let conn = conn().await;
    let mut rows = conn
        .query(
            "SELECT ip_hash, COUNT(*) FROM contact_messages WHERE message LIKE 'Direct contact %' GROUP BY ip_hash",
            (),
        )
        .await
        .unwrap();
    let mut counts = std::collections::HashMap::new();
    while let Some(row) = rows.next().await.unwrap() {
        counts.insert(row.get::<String>(0).unwrap(), row.get::<i64>(1).unwrap());
    }
    let day = daily::today();
    assert_eq!(counts.len(), 2);
    assert_eq!(counts[&crate::server::network_hash("127.0.0.80", day)], 5);
    assert_eq!(counts[&crate::server::network_hash("127.0.0.81", day)], 1);
}

#[tokio::test]
async fn fly_contact_clients_share_an_ipv6_prefix_budget() {
    let server = Server::start().await;
    for i in 1..=5 {
        server
            .browser_from(&format!("2001:db8:1234:5678::{i}"))
            .call(message("", "IPv6 Fly contact quota regression"))
            .await
            .unwrap();
    }
    assert_eq!(
        api_error(
            server
                .browser_from("2001:0db8:1234:5678::ffff")
                .call(message("", "IPv6 prefix budget is already used"))
                .await
        ),
        ApiError::TooManyMessages
    );
    server
        .browser_from("2001:db8:1234:5679::1")
        .call(message("", "IPv6 neighboring prefix is independent"))
        .await
        .unwrap();
}

#[tokio::test]
async fn missing_transport_identity_cannot_store_an_unlimited_contact_message() {
    // Some service harnesses do not install ConnectInfo. That is an unknown identity,
    // rather than loopback, a payload address, or a nullable quota key.
    let server = Server::direct(false).await;
    let browser = server.browser_from("203.0.113.123");
    let result = browser
        .call(message("", "Missing transport identity regression"))
        .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Client address unavailable")
    );
    let mut rows = conn()
        .await
        .query("SELECT COUNT(*) FROM contact_messages WHERE message = 'Missing transport identity regression'", ())
        .await
        .unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        0
    );
    let res = browser
        .post("/dojo/benchmarks/submit")
        .body("not a form")
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::SERVICE_UNAVAILABLE);
}
