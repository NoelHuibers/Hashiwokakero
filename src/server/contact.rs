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
    std::env::var(name).ok()
}

/// Mails a message to `CONTACT_TO`, with the sender as Reply-To. `Ok(false)`: SMTP isn't set up.
async fn mail(id: i64, name: &str, email: &str, message: &str) -> Result<bool, String> {
    let Some(smtp) = Smtp::from_vars(env)? else {
        return Ok(false);
    };
    smtp.send(smtp.compose(id, name, email, message)?).await?;
    Ok(true)
}

/// The SMTP settings (see the top of this file).
#[derive(Debug)]
struct Smtp {
    host: String,
    port: u16,
    username: String,
    password: String,
    from: Mailbox,
    to: Mailbox,
}

impl Smtp {
    /// The settings from `var` (the environment), `None` if they are incomplete.
    fn from_vars(var: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
        let var = |name: &str| var(name).filter(|v| !v.is_empty());
        let (Some(host), Some(username), Some(password), Some(to)) = (
            var("SMTP_HOST"),
            var("SMTP_USERNAME"),
            var("SMTP_PASSWORD"),
            var("CONTACT_TO"),
        ) else {
            return Ok(None);
        };
        let port = var("SMTP_PORT").and_then(|p| p.parse().ok()).unwrap_or(587);
        let from = var("SMTP_FROM").unwrap_or_else(|| username.clone());
        let from = format!("Hashi <{from}>")
            .parse()
            .map_err(|e| format!("SMTP_FROM: {e}"))?;
        let to = to.parse().map_err(|e| format!("CONTACT_TO: {e}"))?;
        Ok(Some(Self {
            host,
            port,
            username,
            password,
            from,
            to,
        }))
    }

    fn compose(&self, id: i64, name: &str, email: &str, message: &str) -> Result<Message, String> {
        let sender = if name.is_empty() { "anonym" } else { name };
        let mut builder = Message::builder()
            .from(self.from.clone())
            .to(self.to.clone())
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
        builder.body(body).map_err(|e| e.to_string())
    }

    async fn send(&self, mail: Message) -> Result<(), String> {
        let transport = if self.port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host)
        }
        .map_err(|e| e.to_string())?
        .port(self.port)
        .credentials(Credentials::new(
            self.username.clone(),
            self.password.clone(),
        ))
        .build();
        transport.send(mail).await.map_err(|e| e.to_string())?;
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn smtp(vars: &[(&str, &str)]) -> Result<Option<Smtp>, String> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        Smtp::from_vars(|name| vars.get(name).cloned())
    }

    const SETTINGS: [(&str, &str); 4] = [
        ("SMTP_HOST", "smtp.example.org"),
        ("SMTP_USERNAME", "hello@hashi.rs"),
        ("SMTP_PASSWORD", "token"),
        ("CONTACT_TO", "admin@hashi.rs"),
    ];

    /// Without all four settings messages are only stored; with them, the defaults fill in.
    #[test]
    fn settings_come_from_the_environment() {
        assert!(smtp(&[]).unwrap().is_none());
        assert!(smtp(&SETTINGS[..3]).unwrap().is_none());
        let mut empty = SETTINGS;
        empty[2].1 = "";
        assert!(smtp(&empty).unwrap().is_none());

        let s = smtp(&SETTINGS).unwrap().unwrap();
        assert_eq!((s.host.as_str(), s.port), ("smtp.example.org", 587));
        assert_eq!(s.from.to_string(), "Hashi <hello@hashi.rs>");
        assert_eq!(s.to.to_string(), "admin@hashi.rs");

        let mut more = SETTINGS.to_vec();
        more.extend([("SMTP_PORT", "465"), ("SMTP_FROM", "kontakt@hashi.rs")]);
        let s = smtp(&more).unwrap().unwrap();
        assert_eq!(s.port, 465);
        assert_eq!(s.from.to_string(), "Hashi <kontakt@hashi.rs>");

        let mut bad = SETTINGS;
        bad[3].1 = "not an address";
        assert!(smtp(&bad).unwrap_err().starts_with("CONTACT_TO: "));
        let mut bad = SETTINGS.to_vec();
        bad.push(("SMTP_FROM", "nope"));
        assert!(smtp(&bad).unwrap_err().starts_with("SMTP_FROM: "));
    }

    /// The mail names the sender, answers go to them, and the message is in it.
    #[test]
    fn mails_say_who_wrote() {
        let s = smtp(&SETTINGS).unwrap().unwrap();
        // Header lines end in CRLF, the body is ours
        let text = |mail: Message| {
            String::from_utf8(mail.formatted())
                .unwrap()
                .replace("\r\n", "\n")
        };

        let mail = text(
            s.compose(7, "Alice", "alice@example.org", "The bridges are lovely")
                .unwrap(),
        );
        assert!(mail.contains("From: Hashi <hello@hashi.rs>\n"), "{mail}");
        assert!(mail.contains("To: admin@hashi.rs\n"), "{mail}");
        assert!(
            mail.contains("Subject: Hashi: Nachricht von Alice (#7)\n"),
            "{mail}"
        );
        assert!(
            mail.contains("Reply-To: Alice <alice@example.org>\n"),
            "{mail}"
        );
        assert!(
            mail.contains("Name: Alice\nE-Mail: alice@example.org"),
            "{mail}"
        );
        assert!(mail.contains("The bridges are lovely"), "{mail}");

        // Anonymous, and an address that doesn't parse gets no Reply-To
        let mail = text(s.compose(8, "", "", "No name given here").unwrap());
        assert!(
            mail.contains("Subject: Hashi: Nachricht von anonym (#8)"),
            "{mail}"
        );
        assert!(mail.contains("Name: -\nE-Mail: -"), "{mail}");
        assert!(!mail.contains("Reply-To"), "{mail}");
        let mail = text(s.compose(9, "", "bob@example.org", "Hello there!").unwrap());
        assert!(mail.contains("Reply-To: bob@example.org\n"), "{mail}");
    }
}
