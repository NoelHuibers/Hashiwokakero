//! The contact form: messages are stored, mailed to us over SMTP, and deleted after half a year.
//!
//! Mail goes out when these are set (Proton Mail: `smtp.protonmail.ch`, port 587, a custom domain
//! address as user and its SMTP token as password):
//!
//! - `SMTP_HOST`, `SMTP_PORT` (default 587: STARTTLS, 465: TLS), `SMTP_USERNAME`, `SMTP_PASSWORD`
//! - `SMTP_FROM`: the sender, default `SMTP_USERNAME`
//! - `CONTACT_TO`: where messages go
//!
//! Without them messages are only stored, the admin dashboard shows them either way.

use axum::http::HeaderMap;
use leptos::prelude::ServerFnError;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use libsql::params::IntoParams;
use libsql::{Connection, params};

use super::db::db_err;
use super::{client_ip, network_hash, now_ms};
use crate::model::{ApiError, ContactMessage, MS_PER_DAY};

const MAX_NAME: usize = 80;
const MAX_EMAIL: usize = 254;
pub const MIN_MESSAGE: usize = 10;
pub const MAX_MESSAGE: usize = 5000;
/// Messages per network and day, and from everyone within a day.
const PER_NETWORK: i64 = 5;
const PER_DAY: i64 = 100;
const KEEP_DAYS: i64 = 180;

/// Stores a message and mails it in the background. `website` is a field people don't see:
/// bots that fill it in get a thank you, but nothing is stored.
pub async fn send(
    conn: &Connection,
    name: &str,
    email: &str,
    message: &str,
    website: &str,
) -> Result<(), ServerFnError> {
    let name: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME)
        .collect();
    let name = name.trim().to_string();
    let email = email.trim().to_string();
    let message = message.trim().to_string();
    if !email.is_empty() && (email.len() > MAX_EMAIL || email.parse::<Address>().is_err()) {
        return Err(ApiError::EmailInvalid.into());
    }
    if !(MIN_MESSAGE..=MAX_MESSAGE).contains(&message.chars().count()) {
        return Err(ApiError::MessageInvalid.into());
    }
    if !website.is_empty() {
        return Ok(());
    }

    let now = now_ms();
    let headers: Option<HeaderMap> = leptos_axum::extract().await.ok();
    let ip_hash = headers
        .as_ref()
        .and_then(client_ip)
        .map(|ip| network_hash(&ip, now.div_euclid(MS_PER_DAY)));
    let since = now - MS_PER_DAY;
    let from_network = scalar(
        conn,
        "SELECT COUNT(*) FROM contact_messages WHERE created_at > ?1 AND ip_hash = ?2",
        params![since, ip_hash.clone()],
    )
    .await?;
    let from_all = scalar(
        conn,
        "SELECT COUNT(*) FROM contact_messages WHERE created_at > ?1",
        params![since],
    )
    .await?;
    if from_network >= PER_NETWORK || from_all >= PER_DAY {
        return Err(ApiError::TooManyMessages.into());
    }

    conn.execute(
        "INSERT INTO contact_messages (name, email, message, created_at, ip_hash)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name.clone(), email.clone(), message.clone(), now, ip_hash],
    )
    .await
    .map_err(db_err)?;
    let id = conn.last_insert_rowid();

    // The player doesn't wait for the mail server, the message is safe in the database
    tokio::spawn(async move {
        match mail(id, &name, &email, &message).await {
            Ok(true) => {
                let marked = match super::db::conn() {
                    Ok(conn) => conn
                        .execute(
                            "UPDATE contact_messages SET mailed_at = ?1 WHERE id = ?2",
                            params![now_ms(), id],
                        )
                        .await
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                };
                if let Err(e) = marked {
                    eprintln!("contact: #{id} mailed, but not marked: {e}");
                }
            }
            Ok(false) => {}
            Err(e) => eprintln!("contact: mailing #{id} failed: {e}"),
        }
    });
    Ok(())
}

async fn scalar(
    conn: &Connection,
    sql: &str,
    params: impl IntoParams,
) -> Result<i64, ServerFnError> {
    let mut rows = conn.query(sql, params).await.map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err),
        None => Ok(0),
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Mails a message to `CONTACT_TO`, with the sender as Reply-To. `Ok(false)`: SMTP isn't set up.
async fn mail(id: i64, name: &str, email: &str, message: &str) -> Result<bool, String> {
    let (Some(host), Some(username), Some(password), Some(to)) = (
        env("SMTP_HOST"),
        env("SMTP_USERNAME"),
        env("SMTP_PASSWORD"),
        env("CONTACT_TO"),
    ) else {
        return Ok(false);
    };
    let port = env("SMTP_PORT").and_then(|p| p.parse().ok()).unwrap_or(587);
    let from = env("SMTP_FROM").unwrap_or_else(|| username.clone());
    let from: Mailbox = format!("Hashi <{from}>")
        .parse()
        .map_err(|e| format!("SMTP_FROM: {e}"))?;
    let to: Mailbox = to.parse().map_err(|e| format!("CONTACT_TO: {e}"))?;

    let sender = if name.is_empty() { "anonym" } else { name };
    let mut builder = Message::builder()
        .from(from)
        .to(to)
        .subject(format!("Hashi: Nachricht von {sender} (#{id})"))
        .header(ContentType::TEXT_PLAIN);
    if let Ok(address) = email.parse::<Address>() {
        let name = (!name.is_empty()).then(|| name.to_string());
        builder = builder.reply_to(Mailbox::new(name, address));
    }
    let body = format!(
        "Nachricht #{id} über das Kontaktformular von hashi.rs\n\nName: {}\nE-Mail: {}\n\n{message}\n",
        if name.is_empty() { "-" } else { name },
        if email.is_empty() { "-" } else { email },
    );
    let mail = builder.body(body).map_err(|e| e.to_string())?;

    let transport = if port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
    }
    .map_err(|e| e.to_string())?
    .port(port)
    .credentials(Credentials::new(username, password))
    .build();
    transport.send(mail).await.map_err(|e| e.to_string())?;
    Ok(true)
}

/// Every stored message, newest first.
pub async fn list(conn: &Connection) -> Result<Vec<ContactMessage>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT id, name, email, message, created_at, mailed_at IS NOT NULL
             FROM contact_messages ORDER BY created_at DESC",
            (),
        )
        .await
        .map_err(db_err)?;
    let mut messages = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        messages.push(ContactMessage {
            id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            email: row.get(2).map_err(db_err)?,
            message: row.get(3).map_err(db_err)?,
            created_at: row.get(4).map_err(db_err)?,
            mailed: row.get::<i64>(5).map_err(db_err)? != 0,
        });
    }
    Ok(messages)
}

pub async fn delete(conn: &Connection, id: i64) -> Result<(), ServerFnError> {
    conn.execute("DELETE FROM contact_messages WHERE id = ?1", params![id])
        .await
        .map_err(db_err)?;
    Ok(())
}

/// Deletes messages older than half a year.
pub async fn delete_old(conn: &Connection) -> Result<(), ServerFnError> {
    conn.execute(
        "DELETE FROM contact_messages WHERE created_at < ?1",
        params![now_ms() - KEEP_DAYS * MS_PER_DAY],
    )
    .await
    .map_err(db_err)?;
    Ok(())
}
