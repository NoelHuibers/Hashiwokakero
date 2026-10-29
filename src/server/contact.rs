//! The feedback form (`/kontakt`): messages are stored, mailed to us over SMTP, and deleted
//! after half a year.
//!
//! A message says what it is about (`FeedbackKind`) and brings what the page sends along with it
//! (`FeedbackContext`: the app's version, platform, system, language, the page it came from, the
//! last errors), which the writer sees before sending. The admins go through them in their inbox
//! (`/admin?tab=messages`): each is new until they answer it, close it or make a GitHub issue of
//! it (`FeedbackStatus`).
//!
//! Mail goes out when these are set (Proton Mail: `smtp.protonmail.ch`, port 587, a custom domain
//! address as user and its SMTP token as password):
//!
//! - `SMTP_HOST`, `SMTP_PORT` (default 587: STARTTLS, 465: TLS), `SMTP_USERNAME`, `SMTP_PASSWORD`
//! - `SMTP_FROM`: the sender, default `SMTP_USERNAME`
//! - `CONTACT_TO`: where messages go
//!
//! Without them messages are only stored, the admin dashboard shows them either way.

use axum::Extension;
use leptos::prelude::ServerFnError;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use libsql::params::IntoParams;
use libsql::{Connection, TransactionBehavior, params};

use super::db::db_err;
use super::{ClientIdentity, now_ms};
use crate::model::{
    ApiError, ContactMessage, ContextField, FeedbackContext, FeedbackKind, FeedbackStatus,
    MS_PER_DAY,
};

const MAX_NAME: usize = 80;
const MAX_EMAIL: usize = 254;
pub const MIN_MESSAGE: usize = 10;
pub const MAX_MESSAGE: usize = 5000;
/// Messages per network and day, and from everyone within a day.
const PER_NETWORK: i64 = 5;
const PER_DAY: i64 = 100;
const KEEP_DAYS: i64 = 180;

/// A message as the form sends it.
#[derive(Clone, Debug, Default)]
pub struct Feedback {
    pub name: String,
    pub email: String,
    pub message: String,
    /// The field people don't see, see [`send`].
    pub website: String,
    pub kind: FeedbackKind,
    pub context: Option<FeedbackContext>,
}

/// Stores a message and mails it in the background. `website` is a field people don't see:
/// bots that fill it in get a thank you, but nothing is stored.
///
/// The context is cut to size (`FeedbackContext::sanitized`), its player is `player`, the
/// session's, whatever the page said.
pub async fn send(
    conn: &Connection,
    feedback: Feedback,
    player: Option<i64>,
) -> Result<(), ServerFnError> {
    let name: String = feedback
        .name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME)
        .collect();
    let name = name.trim().to_string();
    let email = feedback.email.trim().to_string();
    let message = feedback.message.trim().to_string();
    if !email.is_empty() && (email.len() > MAX_EMAIL || email.parse::<Address>().is_err()) {
        return Err(ApiError::EmailInvalid.into());
    }
    if !(MIN_MESSAGE..=MAX_MESSAGE).contains(&message.chars().count()) {
        return Err(ApiError::MessageInvalid.into());
    }
    if !feedback.website.is_empty() {
        return Ok(());
    }
    let context = feedback.context.map(|c| FeedbackContext {
        player,
        ..c.sanitized()
    });
    let locale = context.as_ref().and_then(|c| known_locale(&c.language));
    let context_json = context
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| ApiError::Internal.fail())?;
    let kind = feedback.kind;

    let now = now_ms();
    // In-process service callers have no transport identity. HTTP calls must extract the
    // boundary's extension from Leptos' request Parts, never retry resolution from headers.
    let ip_hash = if leptos::prelude::use_context::<axum::http::request::Parts>().is_some() {
        let Extension(client): Extension<ClientIdentity> = leptos_axum::extract().await?;
        Some(
            client
                .network_hash(now.div_euclid(MS_PER_DAY))
                .map_err(ServerFnError::new)?,
        )
    } else {
        None
    };
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(db_err)?;
    let since = now - MS_PER_DAY;
    let from_network = scalar(
        &tx,
        "SELECT COUNT(*) FROM contact_messages WHERE created_at > ?1 AND ip_hash = ?2",
        params![since, ip_hash.clone()],
    )
    .await?;
    let from_all = scalar(
        &tx,
        "SELECT COUNT(*) FROM contact_messages WHERE created_at > ?1",
        params![since],
    )
    .await?;
    if from_network >= PER_NETWORK || from_all >= PER_DAY {
        return Err(ApiError::TooManyMessages.into());
    }

    tx.execute(
        "INSERT INTO contact_messages (name, email, message, created_at, ip_hash, category, context, locale)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            name.clone(),
            email.clone(),
            message.clone(),
            now,
            ip_hash,
            kind.id(),
            context_json,
            locale
        ],
    )
    .await
    .map_err(db_err)?;
    let id = tx.last_insert_rowid();
    tx.commit().await.map_err(db_err)?;

    let feedback = Feedback {
        name,
        email,
        message,
        website: String::new(),
        kind,
        context,
    };
    // The player doesn't wait for the mail server, the message is safe in the database
    tokio::spawn(async move {
        match mail(id, &feedback).await {
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

/// `language` if it is one of the site's (`de`, `zh-Hant`), to answer in.
fn known_locale(language: &str) -> Option<String> {
    use leptos_i18n::Locale as _;
    crate::i18n::Locale::get_all()
        .iter()
        .map(|l| l.as_str())
        .find(|l| *l == language)
        .map(String::from)
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
async fn mail(id: i64, feedback: &Feedback) -> Result<bool, String> {
    let Some(smtp) = Smtp::from_vars(env)? else {
        return Ok(false);
    };
    smtp.send(smtp.compose(id, feedback)?).await?;
    Ok(true)
}

/// The type of a message in the mail's subject.
const fn kind_de(kind: FeedbackKind) -> &'static str {
    match kind {
        FeedbackKind::Bug => "Bug",
        FeedbackKind::Idea => "Idee",
        FeedbackKind::Question => "Frage",
        FeedbackKind::Other => "Nachricht",
    }
}

/// A line of the context in the mail.
const fn field_de(field: ContextField) -> &'static str {
    match field {
        ContextField::App => "Version",
        ContextField::Platform => "Plattform",
        ContextField::System => "System",
        ContextField::Browser => "Browser",
        ContextField::Language => "Sprache",
        ContextField::Page => "Seite",
        ContextField::Daily => "Tagesrätsel",
        ContextField::Player => "Spieler",
        ContextField::Screen => "Bildschirm",
        ContextField::Errors => "Fehler",
    }
}

/// The context as the mail lists it, a line each.
fn context_lines(context: &FeedbackContext) -> String {
    context
        .rows()
        .into_iter()
        .map(|(field, value)| {
            let value = match (field, value.is_empty()) {
                (ContextField::App, true) => "Website".to_string(),
                (ContextField::App, false) => format!("App {value}"),
                _ => value,
            };
            format!("{}: {value}", field_de(field))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Mails a note to us (`CONTACT_TO`), like a report of a player (`server::reports`).
/// `Ok(false)`: SMTP isn't set up.
pub(super) async fn notify(subject: &str, body: &str) -> Result<bool, String> {
    let Some(smtp) = Smtp::from_vars(env)? else {
        return Ok(false);
    };
    smtp.send(smtp.note(subject, body)?).await?;
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

    fn compose(&self, id: i64, feedback: &Feedback) -> Result<Message, String> {
        let Feedback {
            name,
            email,
            message,
            kind,
            context,
            ..
        } = feedback;
        let sender = if name.is_empty() { "anonym" } else { name };
        let mut builder = Message::builder()
            .from(self.from.clone())
            .to(self.to.clone())
            .subject(format!("Hashi: {} von {sender} (#{id})", kind_de(*kind)))
            .header(ContentType::TEXT_PLAIN);
        if let Ok(address) = email.parse::<Address>() {
            let name = (!name.is_empty()).then(|| name.clone());
            builder = builder.reply_to(Mailbox::new(name, address));
        }
        let context = context.as_ref().map_or_else(
            || "Gesendet ohne Angaben zur Seite".to_string(),
            |c| format!("Gesendet mit:\n{}", context_lines(c)),
        );
        let body = format!(
            "{} #{id} über das Feedbackformular von hashi.rs\n\nName: {}\nE-Mail: {}\n\n{message}\n\n{context}\n\nhttps://hashi.rs/admin?tab=messages\n",
            kind_de(*kind),
            if name.is_empty() { "-" } else { name },
            if email.is_empty() { "-" } else { email },
        );
        builder.body(body).map_err(|e| e.to_string())
    }

    /// A note from the site itself, no answer expected.
    fn note(&self, subject: &str, body: &str) -> Result<Message, String> {
        Message::builder()
            .from(self.from.clone())
            .to(self.to.clone())
            .subject(format!("Hashi: {subject}"))
            .header(ContentType::TEXT_PLAIN)
            .body(format!("{body}\n"))
            .map_err(|e| e.to_string())
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
    page(conn, None, None, None).await
}

/// The messages with `status` and of `kind` (`None`: any), newest first, 100 at a time.
///
/// Keyset pagination stays stable while newer messages arrive or earlier ones are deleted.
pub async fn page(
    conn: &Connection,
    before: Option<i64>,
    status: Option<FeedbackStatus>,
    kind: Option<FeedbackKind>,
) -> Result<Vec<ContactMessage>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT id, name, email, message, created_at, mailed_at IS NOT NULL, category, context,
                    locale, status, github_issue, handled_at
             FROM contact_messages
             WHERE (?1 IS NULL OR id < ?1) AND (?2 IS NULL OR status = ?2)
                 AND (?3 IS NULL OR category = ?3)
             ORDER BY id DESC LIMIT 100",
            params![
                before,
                status.map(FeedbackStatus::id),
                kind.map(FeedbackKind::id)
            ],
        )
        .await
        .map_err(db_err)?;
    let mut messages = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let kind: String = row.get(6).map_err(db_err)?;
        let context: Option<String> = row.get(7).map_err(db_err)?;
        let status: String = row.get(9).map_err(db_err)?;
        messages.push(ContactMessage {
            id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            email: row.get(2).map_err(db_err)?,
            message: row.get(3).map_err(db_err)?,
            created_at: row.get(4).map_err(db_err)?,
            mailed: row.get::<i64>(5).map_err(db_err)? != 0,
            kind: FeedbackKind::from_id(&kind).unwrap_or_default(),
            context: context.and_then(|c| serde_json::from_str(&c).ok()),
            locale: row.get(8).map_err(db_err)?,
            status: FeedbackStatus::from_id(&status).unwrap_or_default(),
            github_issue: row.get(10).map_err(db_err)?,
            handled_at: row.get(11).map_err(db_err)?,
        });
    }
    Ok(messages)
}

/// Marks a message as answered or closed, or new again. `Issue` only comes with an issue.
/// Returns whether there is such a message.
pub async fn set_status(
    conn: &Connection,
    id: i64,
    status: FeedbackStatus,
) -> Result<bool, ServerFnError> {
    if status == FeedbackStatus::Issue {
        return Err(ApiError::Internal.into());
    }
    let handled = (status != FeedbackStatus::New).then(now_ms);
    let changed = conn
        .execute(
            "UPDATE contact_messages SET status = ?2, handled_at = ?3 WHERE id = ?1",
            params![id, status.id(), handled],
        )
        .await
        .map_err(db_err)?;
    Ok(changed != 0)
}

pub async fn delete(conn: &Connection, id: i64) -> Result<(), ServerFnError> {
    conn.execute("DELETE FROM contact_messages WHERE id = ?1", params![id])
        .await
        .map_err(db_err)?;
    Ok(())
}

/// For `auth::erase`, `?1` the user: their messages stay (they have no account in them), but
/// lose the player id of their context.
pub const ERASE: [&str; 1] = [
    "UPDATE contact_messages SET context = json_remove(context, '$.player')
     WHERE json_extract(context, '$.player') = ?1",
];

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

        let feedback = |name: &str, email: &str, message: &str| Feedback {
            name: name.into(),
            email: email.into(),
            message: message.into(),
            ..Feedback::default()
        };

        let mail = text(
            s.compose(
                7,
                &feedback("Alice", "alice@example.org", "The bridges are lovely"),
            )
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
        // From an app that sends no context
        assert!(mail.contains("Gesendet ohne Angaben"), "{mail}");

        // Anonymous, and an address that doesn't parse gets no Reply-To
        let mail = text(
            s.compose(8, &feedback("", "", "No name given here"))
                .unwrap(),
        );
        assert!(
            mail.contains("Subject: Hashi: Nachricht von anonym (#8)"),
            "{mail}"
        );
        assert!(mail.contains("Name: -\nE-Mail: -"), "{mail}");
        assert!(!mail.contains("Reply-To"), "{mail}");
        let mail = text(
            s.compose(9, &feedback("", "bob@example.org", "Hello there!"))
                .unwrap(),
        );
        assert!(mail.contains("Reply-To: bob@example.org\n"), "{mail}");

        // The type in the subject, the context under the message
        let bug = Feedback {
            kind: FeedbackKind::Bug,
            context: Some(FeedbackContext {
                app: Some("1.1.0".into()),
                platform: "android".into(),
                page: Some("/practice".into()),
                player: Some(17),
                errors: vec!["/practice E:database".into()],
                ..FeedbackContext::default()
            }),
            ..feedback("Kim", "", "The board froze after a hint")
        };
        let mail = text(s.compose(10, &bug).unwrap());
        assert!(
            mail.contains("Subject: Hashi: Bug von Kim (#10)\n"),
            "{mail}"
        );
        for line in [
            "Version: App 1.1.0",
            "Plattform: Android",
            "Seite: /practice",
            "Spieler: #17",
            "Fehler: /practice E:database",
            // The `=` is quoted-printable
            "https://hashi.rs/admin?tab",
        ] {
            assert!(mail.contains(line), "{line}: {mail}");
        }
        let website = Feedback {
            kind: FeedbackKind::Idea,
            context: Some(FeedbackContext::default()),
            ..feedback("", "", "A 5x5 daily for beginners")
        };
        let mail = text(s.compose(11, &website).unwrap());
        assert!(
            mail.contains("Subject: Hashi: Idee von anonym (#11)\n"),
            "{mail}"
        );
        assert!(mail.contains("Version: Website\n"), "{mail}");

        // A note of the site, a report say
        let mail = text(s.note("Meldung #3", "Spieler #17").unwrap());
        assert!(mail.contains("From: Hashi <hello@hashi.rs>\n"), "{mail}");
        assert!(mail.contains("To: admin@hashi.rs\n"), "{mail}");
        assert!(mail.contains("Subject: Hashi: Meldung #3\n"), "{mail}");
        assert!(mail.contains("Spieler #17"), "{mail}");
        assert!(!mail.contains("Reply-To"), "{mail}");
    }

    fn sample(kind: FeedbackKind, message: &str) -> Feedback {
        Feedback {
            name: "Kim".into(),
            message: message.into(),
            kind,
            context: Some(FeedbackContext {
                platform: "web".into(),
                language: "de".into(),
                page: Some("/practice".into()),
                // The page's claim, the server knows better
                player: Some(999),
                errors: (0..9).map(|i| format!("error {i}")).collect(),
                ..FeedbackContext::default()
            }),
            ..Feedback::default()
        }
    }

    /// The type, the context (cut to size, with the session's player) and the language to answer
    /// in are stored with the message, which starts out new.
    #[tokio::test]
    async fn feedback_keeps_its_type_and_context() {
        let conn = crate::server::db::TestConn::new("feedback-context").await;
        send(
            &conn,
            sample(FeedbackKind::Bug, "The hint button does nothing"),
            Some(7),
        )
        .await
        .unwrap();
        // An app from before sends neither type nor context; a guest without a session
        send(
            &conn,
            Feedback {
                message: "Thanks for the dailies!".into(),
                ..Feedback::default()
            },
            None,
        )
        .await
        .unwrap();
        let unknown = Feedback {
            context: Some(FeedbackContext {
                language: "xx".into(),
                ..FeedbackContext::default()
            }),
            ..sample(FeedbackKind::Question, "Which language is this?")
        };
        send(&conn, unknown, None).await.unwrap();

        let list = list(&conn).await.unwrap();
        let [question, thanks, bug] = &list[..] else {
            panic!("{list:?}");
        };
        assert_eq!(
            (bug.kind, bug.status, bug.locale.as_deref(), bug.handled_at),
            (FeedbackKind::Bug, FeedbackStatus::New, Some("de"), None)
        );
        let context = bug.context.as_ref().unwrap();
        assert_eq!(context.player, Some(7));
        assert_eq!(context.page.as_deref(), Some("/practice"));
        assert_eq!(context.errors.len(), crate::model::MAX_CONTEXT_ERRORS);
        assert_eq!(context.errors.last().map(String::as_str), Some("error 8"));
        assert_eq!(
            (thanks.kind, &thanks.context, &thanks.locale),
            (FeedbackKind::Other, &None, &None)
        );
        assert_eq!(
            (question.kind, question.locale.as_deref()),
            (FeedbackKind::Question, None)
        );
        assert_eq!(question.context.as_ref().unwrap().player, None);
    }

    /// The admins answer, close and reopen messages; the inbox filters by status and type.
    #[tokio::test]
    async fn feedback_moves_through_the_inbox() {
        let conn = crate::server::db::TestConn::new("feedback-status").await;
        for (kind, message) in [
            (FeedbackKind::Bug, "The board froze after a hint"),
            (FeedbackKind::Idea, "A 5x5 daily for beginners"),
            (FeedbackKind::Bug, "Music stops after a minute"),
        ] {
            send(&conn, sample(kind, message), None).await.unwrap();
        }
        let ids: Vec<i64> = list(&conn).await.unwrap().iter().map(|m| m.id).collect();
        let (music, idea, froze) = (ids[0], ids[1], ids[2]);

        assert!(
            set_status(&conn, froze, FeedbackStatus::Answered)
                .await
                .unwrap()
        );
        assert!(
            set_status(&conn, idea, FeedbackStatus::Closed)
                .await
                .unwrap()
        );
        assert!(
            !set_status(&conn, 9999, FeedbackStatus::Closed)
                .await
                .unwrap()
        );
        // An issue needs its issue
        assert!(
            set_status(&conn, music, FeedbackStatus::Issue)
                .await
                .is_err()
        );

        let ids_of =
            |messages: Vec<ContactMessage>| messages.iter().map(|m| m.id).collect::<Vec<_>>();
        let new = page(&conn, None, Some(FeedbackStatus::New), None)
            .await
            .unwrap();
        assert_eq!(ids_of(new), [music]);
        let bugs = page(&conn, None, None, Some(FeedbackKind::Bug))
            .await
            .unwrap();
        assert_eq!(ids_of(bugs), [music, froze]);
        let answered_bugs = page(
            &conn,
            None,
            Some(FeedbackStatus::Answered),
            Some(FeedbackKind::Bug),
        )
        .await
        .unwrap();
        assert_eq!(answered_bugs.len(), 1);
        assert!(answered_bugs[0].handled_at.is_some());
        assert_eq!(
            ids_of(page(&conn, Some(music), None, None).await.unwrap()),
            [idea, froze]
        );
        let count = crate::server::admin::overview(&conn)
            .await
            .unwrap()
            .messages;
        assert_eq!(count, 1);

        // Reopened, it is new again
        assert!(set_status(&conn, idea, FeedbackStatus::New).await.unwrap());
        let idea = page(&conn, None, None, Some(FeedbackKind::Idea))
            .await
            .unwrap();
        assert_eq!(
            (idea[0].status, idea[0].handled_at),
            (FeedbackStatus::New, None)
        );
    }

    /// Deleting an account takes its player id out of the context of its messages, and only
    /// of its messages.
    #[tokio::test]
    async fn erasing_a_player_forgets_them_in_feedback() {
        let conn = crate::server::db::TestConn::new("feedback-erase").await;
        send(
            &conn,
            sample(FeedbackKind::Bug, "Mine, from player seven"),
            Some(7),
        )
        .await
        .unwrap();
        send(
            &conn,
            sample(FeedbackKind::Bug, "Someone else's message"),
            Some(8),
        )
        .await
        .unwrap();
        for sql in ERASE {
            conn.execute(sql, params![7]).await.unwrap();
        }
        let players: Vec<_> = list(&conn)
            .await
            .unwrap()
            .iter()
            .map(|m| m.context.as_ref().map(|c| (c.player, c.page.clone())))
            .collect();
        let page = Some("/practice".to_string());
        assert_eq!(players, [Some((Some(8), page.clone())), Some((None, page))]);
    }
}
