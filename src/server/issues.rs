//! Feedback that became a GitHub issue.
//!
//! The admins make one of a message after editing it (`create`), and once the issue is closed as
//! completed the writer hears that it is fixed, once, in their language (`notify_fixed`, from the
//! cleanup every few hours).
//!
//! Only what the admin posted goes to GitHub, never the writer's name, address or player id
//! (`model::issue_draft` leaves them out of the draft). Without `GITHUB_FEEDBACK_TOKEN` (see
//! `github`) or without SMTP (see `contact`) nothing happens.

#[cfg(test)]
mod tests;

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::db::db_err;
use super::github::{GitHub, IssueState, Issues};
use super::now_ms;
use crate::i18n::{Locale, td_string};
use crate::model::{
    ApiError, FeedbackContext, FeedbackKind, MAX_ISSUE_BODY, MAX_ISSUE_TITLE, issue_labels,
};

/// Issues looked at in one run of the cleanup, those looked at longest ago first: a few
/// requests every six hours, far below GitHub's limits.
const CHECKS_PER_RUN: i64 = 20;
/// Characters of the message quoted in the mail.
const QUOTE: usize = 300;

/// Posts message `id` as an issue, with the admin's `title` and `body` and the labels of its
/// type, and marks the message `Issue`. Returns the issue's number.
pub async fn create(
    conn: &Connection,
    issues: &impl Issues,
    id: i64,
    title: &str,
    body: &str,
) -> Result<i64, ServerFnError> {
    let title = title.trim();
    let body = body.trim();
    if title.is_empty()
        || title.chars().count() > MAX_ISSUE_TITLE
        || title.chars().any(char::is_control)
        || body.chars().count() > MAX_ISSUE_BODY
    {
        return Err(ApiError::MessageInvalid.into());
    }
    let mut rows = conn
        .query(
            "SELECT category, github_issue FROM contact_messages WHERE id = ?1",
            params![id],
        )
        .await
        .map_err(db_err)?;
    let (kind, issue) = match rows.next().await.map_err(db_err)? {
        Some(row) => (
            row.get::<String>(0).map_err(db_err)?,
            row.get::<Option<i64>>(1).map_err(db_err)?,
        ),
        None => return Err(ServerFnError::new(format!("no message #{id}"))),
    };
    drop(rows);
    // Twice would make two issues
    if let Some(issue) = issue {
        return Err(ServerFnError::new(format!(
            "#{id} is issue #{issue} already"
        )));
    }
    let labels = issue_labels(FeedbackKind::from_id(&kind).unwrap_or_default());
    let number = issues.create(title, body, &labels).await.map_err(|e| {
        eprintln!("issues: #{id} not posted: {e}");
        ServerFnError::new(format!("GitHub: {e}"))
    })?;
    conn.execute(
        "UPDATE contact_messages SET status = 'issue', github_issue = ?2, handled_at = ?3
         WHERE id = ?1",
        params![id, number, now_ms()],
    )
    .await
    .map_err(db_err)?;
    Ok(number)
}

/// Sends a mail to a player. SMTP (`contact::Smtp`), a fake in the tests.
pub trait Mailer: Sync {
    fn send(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

/// The language of a message, English if it has none (or one the site no longer has).
fn locale(id: Option<&str>) -> Locale {
    use leptos_i18n::Locale as _;
    Locale::get_all()
        .iter()
        .copied()
        .find(|l| Some(l.as_str()) == id)
        .unwrap_or_default()
}

/// The mail that tells the writer of `message` it is fixed: subject and text, in `locale`. An
/// idea is "done", the rest fixed; from the app it comes with the next update, on the website
/// it is live.
fn fixed_mail(locale: Locale, kind: FeedbackKind, app: bool, message: &str) -> (String, String) {
    let done = if kind == FeedbackKind::Idea {
        td_string!(locale, contact.fixed.done)
    } else {
        td_string!(locale, contact.fixed.fixed)
    };
    let when = if app {
        td_string!(locale, contact.fixed.app)
    } else {
        td_string!(locale, contact.fixed.web)
    };
    let mut quoted: String = message.trim().chars().take(QUOTE).collect();
    if message.trim().chars().count() > QUOTE {
        quoted.push('…');
    }
    let quoted: Vec<String> = quoted.lines().map(|l| format!("> {l}")).collect();
    let body = format!(
        "{done} {when}\n\n{}\n{}\n\n{}\nHashi · https://hashi.rs\n",
        td_string!(locale, contact.fixed.quote),
        quoted.join("\n"),
        td_string!(locale, contact.fixed.thanks),
    );
    (
        format!("Hashi: {}", td_string!(locale, contact.fixed.subject)),
        body,
    )
}

/// Tells the writers of messages whose issue is fixed, returns how many.
///
/// Looks at the issues of messages with an address whose writer wasn't told yet, the longest
/// unchecked first, at most [`CHECKS_PER_RUN`]. Closed as completed: the writer gets the mail,
/// once. Closed otherwise: nobody is told, the issue isn't looked at again. A failed lookup or
/// mail is tried again next time.
pub async fn notify_fixed(
    conn: &Connection,
    issues: &impl Issues,
    mailer: &impl Mailer,
) -> Result<usize, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT id, email, locale, context, category, message, github_issue
             FROM contact_messages
             WHERE github_issue IS NOT NULL AND email != '' AND issue_closed_at IS NULL
                 AND fixed_mailed_at IS NULL
             ORDER BY COALESCE(issue_checked_at, 0), id LIMIT ?1",
            params![CHECKS_PER_RUN],
        )
        .await
        .map_err(db_err)?;
    let mut due = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let context: Option<String> = row.get(3).map_err(db_err)?;
        let context: Option<FeedbackContext> = context.and_then(|c| serde_json::from_str(&c).ok());
        due.push((
            row.get::<i64>(0).map_err(db_err)?,
            row.get::<String>(1).map_err(db_err)?,
            row.get::<Option<String>>(2).map_err(db_err)?,
            context.is_some_and(|c| c.app.is_some()),
            row.get::<String>(4).map_err(db_err)?,
            row.get::<String>(5).map_err(db_err)?,
            row.get::<i64>(6).map_err(db_err)?,
        ));
    }
    drop(rows);

    let mut told = 0;
    for (id, email, language, app, kind, message, issue) in due {
        let now = now_ms();
        let state = match issues.state(issue).await {
            Ok(state) => state,
            Err(e) => {
                eprintln!("issues: issue #{issue} of #{id} not looked at: {e}");
                IssueState::Open
            }
        };
        let (closed_at, fixed_at) = match state {
            IssueState::Open => (None, None),
            IssueState::Closed => (Some(now), None),
            IssueState::Completed => {
                let kind = FeedbackKind::from_id(&kind).unwrap_or_default();
                let (subject, body) = fixed_mail(locale(language.as_deref()), kind, app, &message);
                match mailer.send(&email, &subject, &body).await {
                    Ok(()) => {
                        told += 1;
                        (Some(now), Some(now))
                    }
                    Err(e) => {
                        eprintln!("issues: telling #{id} about issue #{issue} failed: {e}");
                        (None, None)
                    }
                }
            }
        };
        conn.execute(
            "UPDATE contact_messages SET issue_checked_at = ?2, issue_closed_at = ?3,
                 fixed_mailed_at = ?4
             WHERE id = ?1",
            params![id, now, closed_at, fixed_at],
        )
        .await
        .map_err(db_err)?;
    }
    Ok(told)
}

/// [`notify_fixed`] with GitHub and SMTP as configured; nothing without either.
pub async fn notify_fixed_now(conn: &Connection) -> Result<usize, ServerFnError> {
    let Some(github) = GitHub::from_env() else {
        return Ok(0);
    };
    let Some(smtp) = super::contact::smtp().map_err(ServerFnError::new)? else {
        return Ok(0);
    };
    notify_fixed(conn, &github, &smtp).await
}
