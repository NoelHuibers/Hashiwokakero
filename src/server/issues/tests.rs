//! Issues made of feedback and the mail about a fix, with a fake GitHub and a fake mail server.

use std::collections::HashMap;
use std::sync::Mutex;

use super::*;
use crate::server::contact::{self, Feedback};
use crate::server::db::TestConn;

/// GitHub as the tests need it: issues get numbers from 100 on, their states are set by hand.
#[derive(Default)]
struct FakeGitHub {
    created: Mutex<Vec<(String, String, Vec<String>)>>,
    states: Mutex<HashMap<i64, IssueState>>,
    looked_at: Mutex<Vec<i64>>,
    down: bool,
}

impl FakeGitHub {
    fn set(&self, issue: i64, state: IssueState) {
        self.states.lock().unwrap().insert(issue, state);
    }

    fn looked_at(&self) -> Vec<i64> {
        std::mem::take(&mut *self.looked_at.lock().unwrap())
    }
}

#[expect(clippy::unused_async_trait_impl, reason = "the fake answers at once")]
impl Issues for FakeGitHub {
    async fn create(&self, title: &str, body: &str, labels: &[&str]) -> Result<i64, String> {
        if self.down {
            return Err("503 Service Unavailable".into());
        }
        let mut created = self.created.lock().unwrap();
        created.push((
            title.into(),
            body.into(),
            labels.iter().map(ToString::to_string).collect(),
        ));
        Ok(99 + i64::try_from(created.len()).unwrap())
    }

    async fn state(&self, number: i64) -> Result<IssueState, String> {
        self.looked_at.lock().unwrap().push(number);
        if self.down {
            return Err("503 Service Unavailable".into());
        }
        Ok(self
            .states
            .lock()
            .unwrap()
            .get(&number)
            .copied()
            .unwrap_or(IssueState::Open))
    }
}

#[derive(Default)]
struct FakeMail {
    sent: Mutex<Vec<(String, String, String)>>,
    down: Mutex<bool>,
}

impl FakeMail {
    fn sent(&self) -> Vec<(String, String, String)> {
        std::mem::take(&mut *self.sent.lock().unwrap())
    }
}

#[expect(clippy::unused_async_trait_impl, reason = "the fake sends at once")]
impl Mailer for FakeMail {
    async fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), String> {
        if *self.down.lock().unwrap() {
            return Err("connection refused".into());
        }
        self.sent
            .lock()
            .unwrap()
            .push((to.into(), subject.into(), body.into()));
        Ok(())
    }
}

/// Sends a message and returns its id.
async fn message(
    conn: &Connection,
    kind: FeedbackKind,
    email: &str,
    text: &str,
    context: Option<FeedbackContext>,
) -> i64 {
    let feedback = Feedback {
        name: "Kim".into(),
        email: email.into(),
        message: text.into(),
        kind,
        context,
        ..Feedback::default()
    };
    contact::send(conn, feedback, Some(7)).await.unwrap();
    contact::list(conn).await.unwrap()[0].id
}

fn from_app(language: &str) -> FeedbackContext {
    FeedbackContext {
        app: Some("1.1.0".into()),
        platform: "android".into(),
        language: language.into(),
        ..FeedbackContext::default()
    }
}

#[tokio::test]
async fn issues_carry_the_admins_text_and_the_labels_of_their_type() {
    let conn = TestConn::new("issues-create").await;
    let github = FakeGitHub::default();
    let bug = message(
        &conn,
        FeedbackKind::Bug,
        "kim@example.com",
        "The hint froze the board",
        None,
    )
    .await;
    let other = message(
        &conn,
        FeedbackKind::Other,
        "",
        "A thought about the colours",
        None,
    )
    .await;

    assert_eq!(
        create(&conn, &github, bug, " Hint froze the board ", "Steps: …")
            .await
            .unwrap(),
        100
    );
    assert_eq!(
        create(&conn, &github, other, "Colours", "").await.unwrap(),
        101
    );
    let created = github.created.lock().unwrap().clone();
    assert_eq!(
        created,
        [
            (
                "Hint froze the board".into(),
                "Steps: …".into(),
                vec!["bug".into(), "feedback".into()]
            ),
            ("Colours".into(), String::new(), vec!["feedback".into()]),
        ]
    );
    let list = contact::list(&conn).await.unwrap();
    let posted = list.iter().find(|m| m.id == bug).unwrap();
    assert_eq!(
        (
            posted.status,
            posted.github_issue,
            posted.handled_at.is_some()
        ),
        (crate::model::FeedbackStatus::Issue, Some(100), true)
    );

    // Not twice, not without a title, not for a message that isn't there
    assert!(create(&conn, &github, bug, "Again", "").await.is_err());
    let third = message(
        &conn,
        FeedbackKind::Idea,
        "",
        "A dark mode for the editor",
        None,
    )
    .await;
    for title in ["", "   ", "two\nlines", &"x".repeat(MAX_ISSUE_TITLE + 1)] {
        assert!(
            create(&conn, &github, third, title, "").await.is_err(),
            "{title:?}"
        );
    }
    assert!(
        create(
            &conn,
            &github,
            third,
            "Too long",
            &"x".repeat(MAX_ISSUE_BODY + 1)
        )
        .await
        .is_err()
    );
    assert!(create(&conn, &github, 9999, "Nothing", "").await.is_err());
    assert_eq!(github.created.lock().unwrap().len(), 2);

    // GitHub down: the message stays as it was
    let down = FakeGitHub {
        down: true,
        ..FakeGitHub::default()
    };
    assert!(
        create(&conn, &down, third, "Dark editor", "")
            .await
            .is_err()
    );
    let list = contact::list(&conn).await.unwrap();
    let idea = list.iter().find(|m| m.id == third).unwrap();
    assert_eq!(
        (idea.status, idea.github_issue),
        (crate::model::FeedbackStatus::New, None)
    );
}

/// The writer hears about a fix once, in their language; issues closed otherwise and messages
/// without an address tell nobody, and closed issues aren't looked at again.
#[tokio::test]
async fn writers_hear_about_a_fix_once() {
    let conn = TestConn::new("issues-fixed").await;
    let github = FakeGitHub::default();
    let mail = FakeMail::default();
    let fixed = message(
        &conn,
        FeedbackKind::Bug,
        "kim@example.com",
        "Die Musik bricht ab",
        Some(from_app("de")),
    )
    .await;
    let idea = message(
        &conn,
        FeedbackKind::Idea,
        "ada@example.com",
        "A 5x5 daily for beginners",
        None,
    )
    .await;
    let anonymous = message(&conn, FeedbackKind::Bug, "", "No address given here", None).await;
    let declined = message(
        &conn,
        FeedbackKind::Idea,
        "bob@example.com",
        "Ads please",
        None,
    )
    .await;
    let unposted = message(
        &conn,
        FeedbackKind::Bug,
        "eve@example.com",
        "Not an issue yet",
        None,
    )
    .await;
    let mut numbers = HashMap::new();
    for id in [fixed, idea, anonymous, declined] {
        numbers.insert(
            id,
            create(&conn, &github, id, "Title", "Body").await.unwrap(),
        );
    }
    let _ = unposted;
    github.set(numbers[&fixed], IssueState::Completed);
    github.set(numbers[&anonymous], IssueState::Completed);
    github.set(numbers[&declined], IssueState::Closed);

    assert_eq!(notify_fixed(&conn, &github, &mail).await.unwrap(), 1);
    let sent = mail.sent();
    assert_eq!(sent.len(), 1);
    let (to, subject, body) = &sent[0];
    assert_eq!(to, "kim@example.com");
    assert_eq!(
        *subject,
        format!("Hashi: {}", td_string!(Locale::de, contact.fixed.subject))
    );
    assert!(
        body.starts_with(td_string!(Locale::de, contact.fixed.fixed)),
        "{body}"
    );
    // From the app: with the next update
    assert!(
        body.contains(td_string!(Locale::de, contact.fixed.app)),
        "{body}"
    );
    assert!(body.contains("> Die Musik bricht ab"), "{body}");
    // Without an address nobody is asked about
    let mut looked = github.looked_at();
    looked.sort_unstable();
    assert_eq!(
        looked,
        [numbers[&fixed], numbers[&idea], numbers[&declined]]
    );

    // Next time: only the open one is looked at, nobody is told twice
    assert_eq!(notify_fixed(&conn, &github, &mail).await.unwrap(), 0);
    assert_eq!(mail.sent(), []);
    assert_eq!(github.looked_at(), [numbers[&idea]]);

    // The idea is built: done, live on the website, in English
    github.set(numbers[&idea], IssueState::Completed);
    assert_eq!(notify_fixed(&conn, &github, &mail).await.unwrap(), 1);
    let sent = mail.sent();
    assert_eq!(sent[0].0, "ada@example.com");
    assert!(
        sent[0]
            .2
            .starts_with(td_string!(Locale::en, contact.fixed.done)),
        "{}",
        sent[0].2
    );
    assert!(
        sent[0]
            .2
            .contains(td_string!(Locale::en, contact.fixed.web)),
        "{}",
        sent[0].2
    );
    assert_eq!(github.looked_at(), [numbers[&idea]]);
    // And then nothing is left to look at
    assert_eq!(notify_fixed(&conn, &github, &mail).await.unwrap(), 0);
    assert_eq!(mail.sent(), []);
    assert_eq!(github.looked_at(), Vec::<i64>::new());
}

/// A mail that doesn't go out, or GitHub not answering, is tried again next time.
#[tokio::test]
async fn failures_are_tried_again() {
    let conn = TestConn::new("issues-retry").await;
    let github = FakeGitHub::default();
    let mail = FakeMail::default();
    let id = message(
        &conn,
        FeedbackKind::Bug,
        "kim@example.com",
        "The share button fails",
        None,
    )
    .await;
    let issue = create(&conn, &github, id, "Share fails", "").await.unwrap();
    github.set(issue, IssueState::Completed);

    *mail.down.lock().unwrap() = true;
    assert_eq!(notify_fixed(&conn, &github, &mail).await.unwrap(), 0);
    let down = FakeGitHub {
        down: true,
        ..FakeGitHub::default()
    };
    *mail.down.lock().unwrap() = false;
    assert_eq!(notify_fixed(&conn, &down, &mail).await.unwrap(), 0);
    assert_eq!(down.looked_at(), [issue]);
    assert_eq!(mail.sent(), []);
    assert_eq!(notify_fixed(&conn, &github, &mail).await.unwrap(), 1);
    assert_eq!(mail.sent().len(), 1);
}

/// Every language has the mail, an unknown one gets English.
#[test]
fn fixed_mails_speak_every_language() {
    use leptos_i18n::Locale as _;
    for &l in Locale::get_all() {
        let (subject, body) = fixed_mail(l, FeedbackKind::Bug, false, "It broke");
        assert!(
            subject.len() > "Hashi: ".len() && body.contains("> It broke"),
            "{}",
            l.as_str()
        );
    }
    assert_eq!(locale(Some("xx")), Locale::en);
    assert_eq!(locale(None), Locale::en);
    assert_eq!(locale(Some("zh-Hant")), Locale::zh_Hant);
    let long = "y".repeat(1000);
    let (_, body) = fixed_mail(Locale::en, FeedbackKind::Question, true, &long);
    assert!(
        body.contains(&format!("> {}…", "y".repeat(QUOTE))),
        "{body}"
    );
}
