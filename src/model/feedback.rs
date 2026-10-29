//! Feedback: what a message of the feedback form (`/kontakt`) is about, what the page sends
//! along with it, and where it stands in the admins' inbox.

use serde::{Deserialize, Serialize};

/// What a message is about, picked on the form.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeedbackKind {
    Bug,
    Idea,
    Question,
    /// Praise, data protection, anything else; also every message from before the types.
    #[default]
    Other,
}

impl FeedbackKind {
    pub const ALL: [Self; 4] = [Self::Bug, Self::Idea, Self::Question, Self::Other];

    /// As stored in the database, and in the form's link (`/kontakt?type=bug`).
    pub const fn id(self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Idea => "idea",
            Self::Question => "question",
            Self::Other => "other",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }
}

/// Where a message stands in the admins' inbox.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeedbackStatus {
    /// Nobody looked at it yet.
    #[default]
    New,
    /// A GitHub issue was made of it.
    Issue,
    Answered,
    /// Nothing (more) to do: praise, spam, done.
    Closed,
}

impl FeedbackStatus {
    pub const ALL: [Self; 4] = [Self::New, Self::Issue, Self::Answered, Self::Closed];

    /// As stored in the database.
    pub const fn id(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Issue => "issue",
            Self::Answered => "answered",
            Self::Closed => "closed",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }
}

/// Characters of a text of the context at most, of its page, and of each error.
pub const MAX_CONTEXT_TEXT: usize = 120;
pub const MAX_CONTEXT_PAGE: usize = 200;
pub const MAX_CONTEXT_ERROR: usize = 300;
/// Errors in the context at most, the latest ones.
pub const MAX_CONTEXT_ERRORS: usize = 5;

/// What the page sends along with a message, shown to the writer before they send it. Nothing
/// that isn't listed here: no address, no device id. Stored as JSON with the message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FeedbackContext {
    /// The app's version (`1.1.0`), `None` on the website.
    pub app: Option<String>,
    /// `android` or `ios` in the app, `web` on the website.
    pub platform: String,
    /// The operating system and the browser, from the user agent (`Android 15`, `Chrome 141`).
    pub os: String,
    pub browser: String,
    /// The page's language (`de`).
    pub language: String,
    /// The page the form was opened from, without the language prefix (`/practice`).
    pub page: Option<String>,
    /// Today's daily number, as the page counts.
    pub daily: Option<i64>,
    /// The writer's player id, guests too. The server takes it from the session, whatever the
    /// page said.
    pub player: Option<i64>,
    /// The window's size in CSS pixels and its pixel ratio: `390×844 @3x`.
    pub screen: String,
    /// The last errors the page showed or ran into, the oldest first.
    pub errors: Vec<String>,
}

/// A line of [`FeedbackContext`] as the form, the admins and the mail list it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextField {
    App,
    Platform,
    System,
    Browser,
    Language,
    Page,
    Daily,
    Player,
    Screen,
    Errors,
}

impl FeedbackContext {
    /// Without control characters and cut to the lengths above, at most the latest
    /// [`MAX_CONTEXT_ERRORS`] errors; a page that isn't a path of the site is dropped.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let errors = self.errors.len().saturating_sub(MAX_CONTEXT_ERRORS);
        Self {
            app: self
                .app
                .map(|a| clip(&a, MAX_CONTEXT_TEXT))
                .filter(|a| !a.is_empty()),
            platform: clip(&self.platform, MAX_CONTEXT_TEXT),
            os: clip(&self.os, MAX_CONTEXT_TEXT),
            browser: clip(&self.browser, MAX_CONTEXT_TEXT),
            language: clip(&self.language, MAX_CONTEXT_TEXT),
            page: self
                .page
                .map(|p| clip(&p, MAX_CONTEXT_PAGE))
                .filter(|p| p.starts_with('/') && !p.starts_with("//")),
            daily: self.daily.filter(|d| (1..1_000_000).contains(d)),
            player: self.player,
            screen: clip(&self.screen, MAX_CONTEXT_TEXT),
            errors: self.errors[errors..]
                .iter()
                .map(|e| clip(e, MAX_CONTEXT_ERROR))
                .filter(|e| !e.is_empty())
                .collect(),
        }
    }

    /// The lines worth listing, in order: the version always (an empty value is the website),
    /// the others when they say something. The errors are one line each.
    pub fn rows(&self) -> Vec<(ContextField, String)> {
        let mut rows = vec![(ContextField::App, self.app.clone().unwrap_or_default())];
        let texts = [
            (ContextField::Platform, platform_name(&self.platform)),
            (ContextField::System, self.os.clone()),
            (ContextField::Browser, self.browser.clone()),
            (ContextField::Language, self.language.clone()),
            (ContextField::Page, self.page.clone().unwrap_or_default()),
            (
                ContextField::Daily,
                self.daily.map(|d| format!("#{d}")).unwrap_or_default(),
            ),
            (
                ContextField::Player,
                self.player.map(|p| format!("#{p}")).unwrap_or_default(),
            ),
            (ContextField::Screen, self.screen.clone()),
        ];
        rows.extend(texts.into_iter().filter(|(_, v)| !v.is_empty()));
        rows.extend(
            self.errors
                .iter()
                .map(|e| (ContextField::Errors, e.clone())),
        );
        rows
    }
}

/// `android` -> `Android`, `ios` -> `iOS`, `web` -> `Web`; anything else as it is.
pub fn platform_name(platform: &str) -> String {
    match platform {
        "android" => "Android".into(),
        "ios" => "iOS".into(),
        "web" => "Web".into(),
        other => other.into(),
    }
}

/// `text` without control characters, trimmed and cut to `max` characters.
fn clip(text: &str, max: usize) -> String {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    text.trim().chars().take(max).collect()
}

/// The operating system and the browser a user agent names, with their major versions.
///
/// `("Android 15", "Chrome 141")`, `("iOS 18.1", "Safari 18")`; empty where it doesn't tell.
/// The app's `WebView` is `WebView` (Android) or `WebKit` (iOS, its user agent has no Safari).
pub fn user_agent_parts(ua: &str) -> (String, String) {
    (user_agent_os(ua), user_agent_browser(ua))
}

fn user_agent_os(ua: &str) -> String {
    if let Some(version) = after(ua, "Android ") {
        return format!(
            "Android {}",
            leading(version, |c| c.is_ascii_digit() || c == '.')
        );
    }
    if ua.contains("Android") {
        return "Android".into();
    }
    // `iPhone OS 18_1` on phones, `CPU OS 18_1` on iPads that ask for the mobile site
    if ["iPhone", "iPad", "iPod"].iter().any(|d| ua.contains(d)) {
        let version = after(ua, " OS ")
            .map(|v| leading(v, |c| c.is_ascii_digit() || c == '_').replace('_', "."))
            .unwrap_or_default();
        return format!("iOS {version}").trim_end().to_string();
    }
    if ua.contains("Windows") {
        "Windows".into()
    } else if ua.contains("CrOS") {
        "ChromeOS".into()
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") {
        "macOS".into()
    } else if ua.contains("Linux") {
        "Linux".into()
    } else {
        String::new()
    }
}

fn user_agent_browser(ua: &str) -> String {
    // The ones that also say Chrome or Safari first
    for (token, name) in [
        ("EdgA/", "Edge"),
        ("EdgiOS/", "Edge"),
        ("Edg/", "Edge"),
        ("SamsungBrowser/", "Samsung Internet"),
        ("OPR/", "Opera"),
        ("FxiOS/", "Firefox"),
        ("Firefox/", "Firefox"),
        ("CriOS/", "Chrome"),
    ] {
        if let Some(version) = after(ua, token) {
            return format!("{name} {}", major(version));
        }
    }
    if let Some(version) = after(ua, "Chrome/") {
        // Android's WebView says `; wv)`
        let name = if ua.contains("; wv)") {
            "WebView"
        } else {
            "Chrome"
        };
        return format!("{name} {}", major(version));
    }
    if ua.contains("Safari/") {
        return match after(ua, "Version/") {
            Some(version) => format!("Safari {}", major(version)),
            None => "Safari".into(),
        };
    }
    if ua.contains("AppleWebKit") {
        return "WebKit".into();
    }
    String::new()
}

/// What follows `token` in `text`.
fn after<'a>(text: &'a str, token: &str) -> Option<&'a str> {
    text.find(token).map(|i| &text[i + token.len()..])
}

/// The start of `text` while `keep` holds.
fn leading(text: &str, keep: impl Fn(char) -> bool) -> String {
    text.chars().take_while(|&c| keep(c)).collect()
}

/// The major version at the start of `version`: `141.0.7390.54` -> `141`.
fn major(version: &str) -> String {
    leading(version, |c| c.is_ascii_digit())
}

/// Characters of an issue's title and body at most (GitHub takes 256 and 65 536).
pub const MAX_ISSUE_TITLE: usize = 200;
pub const MAX_ISSUE_BODY: usize = 20_000;

/// A GitHub issue made of a message, as the admin edits it before posting it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueDraft {
    pub title: String,
    pub body: String,
}

/// The labels of an issue made of a message of `kind`: the type's (`bug`, `idea`, `question`)
/// and `feedback`.
pub fn issue_labels(kind: FeedbackKind) -> Vec<&'static str> {
    let kind = match kind {
        FeedbackKind::Bug => Some("bug"),
        FeedbackKind::Idea => Some("idea"),
        FeedbackKind::Question => Some("question"),
        FeedbackKind::Other => None,
    };
    kind.into_iter().chain(["feedback"]).collect()
}

/// The issue a message would make, for the admin to edit.
///
/// Its first line is the title, the message and a table of its context the body. Never the writer's name, address or player
/// id; what they wrote in the message itself is for the admin to take out.
pub fn issue_draft(id: i64, message: &str, context: Option<&FeedbackContext>) -> IssueDraft {
    use std::fmt::Write as _;
    let first = message
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .trim();
    let mut title: String = first.chars().take(80).collect();
    if first.chars().count() > 80 {
        title.push('…');
    }
    let mut body = format!(
        "{}\n\n---\n\nFrom the feedback form on hashi.rs (#{id}).\n",
        message.trim()
    );
    if let Some(context) = context {
        let mut errors = Vec::new();
        let mut rows = Vec::new();
        for (field, value) in context.rows() {
            let name = match field {
                // Who wrote it stays with us
                ContextField::Player => continue,
                ContextField::Errors => {
                    errors.push(format!("`{}`", value.replace('`', "'")));
                    continue;
                }
                ContextField::App => {
                    let value = if value.is_empty() {
                        "website".into()
                    } else {
                        format!("app {value}")
                    };
                    rows.push(("Version", value));
                    continue;
                }
                ContextField::Platform => "Platform",
                ContextField::System => "System",
                ContextField::Browser => "Browser",
                ContextField::Language => "Language",
                ContextField::Page => "Page",
                ContextField::Daily => "Daily",
                ContextField::Screen => "Screen",
            };
            rows.push((name, value));
        }
        if !errors.is_empty() {
            rows.push(("Recent errors", errors.join("<br>")));
        }
        body.push_str("\n| | |\n|---|---|\n");
        for (name, value) in rows {
            let value = value.replace('|', "\\|").replace(['\n', '\r'], " ");
            let _ = writeln!(body, "| {name} | {value} |");
        }
    }
    IssueDraft { title, body }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_statuses_are_stored_by_their_ids() {
        for kind in FeedbackKind::ALL {
            assert_eq!(FeedbackKind::from_id(kind.id()), Some(kind));
        }
        for status in FeedbackStatus::ALL {
            assert_eq!(FeedbackStatus::from_id(status.id()), Some(status));
        }
        assert_eq!(FeedbackKind::from_id("spam"), None);
        assert_eq!(FeedbackStatus::from_id("open"), None);
        // Messages from before the types are something else, and new
        assert_eq!(FeedbackKind::default(), FeedbackKind::Other);
        assert_eq!(FeedbackStatus::default(), FeedbackStatus::New);
    }

    #[test]
    fn user_agents_name_their_system_and_browser() {
        for (ua, os, browser) in [
            (
                "Mozilla/5.0 (Linux; Android 15; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.7390.54 Mobile Safari/537.36",
                "Android 15",
                "Chrome 141",
            ),
            // The app on Android
            (
                "Mozilla/5.0 (Linux; Android 14; SM-S911B Build/UP1A.231005.007; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/140.0.0.0 Mobile Safari/537.36",
                "Android 14",
                "WebView 140",
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1",
                "iOS 18.1",
                "Safari 18",
            ),
            // The app on iOS
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148",
                "iOS 17.4",
                "WebKit",
            ),
            (
                "Mozilla/5.0 (iPad; CPU OS 16_7 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/130.0.6723.90 Mobile/15E148 Safari/604.1",
                "iOS 16.7",
                "Chrome 130",
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.3537.71",
                "Windows",
                "Edge 141",
            ),
            (
                "Mozilla/5.0 (X11; Linux x86_64; rv:144.0) Gecko/20100101 Firefox/144.0",
                "Linux",
                "Firefox 144",
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15",
                "macOS",
                "Safari 26",
            ),
            (
                "Mozilla/5.0 (Linux; Android 13; SAMSUNG SM-A515F) AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/28.0 Chrome/130.0.0.0 Mobile Safari/537.36",
                "Android 13",
                "Samsung Internet 28",
            ),
            (
                "Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 OPR/120.0.0.0",
                "ChromeOS",
                "Opera 120",
            ),
            ("curl/8.9.1", "", ""),
            ("", "", ""),
        ] {
            assert_eq!(
                user_agent_parts(ua),
                (os.to_string(), browser.to_string()),
                "{ua}"
            );
        }
    }

    /// Whatever a page sends, the context stays small: texts cut, the latest errors only, no
    /// control characters, and no page that isn't one of the site.
    #[test]
    fn contexts_are_cut_to_size() {
        let long = "x".repeat(1000);
        let context = FeedbackContext {
            app: Some("  ".into()),
            platform: "web\n".into(),
            os: long.clone(),
            browser: "Firefox 144".into(),
            language: "de".into(),
            page: Some(format!("/{long}")),
            daily: Some(380),
            player: Some(7),
            screen: "390×844 @3x".into(),
            errors: (0..8).map(|i| format!("error {i}\u{0}")).collect(),
        }
        .sanitized();
        assert_eq!(context.app, None);
        assert_eq!(context.platform, "web");
        assert_eq!(context.os.chars().count(), MAX_CONTEXT_TEXT);
        assert_eq!(
            context.page.as_ref().map(|p| p.chars().count()),
            Some(MAX_CONTEXT_PAGE)
        );
        assert_eq!(context.screen, "390×844 @3x");
        assert_eq!(
            context.errors,
            ["error 3", "error 4", "error 5", "error 6", "error 7"]
        );
        let error = FeedbackContext {
            errors: vec![long],
            ..FeedbackContext::default()
        }
        .sanitized();
        assert_eq!(error.errors[0].chars().count(), MAX_CONTEXT_ERROR);

        for page in ["https://evil.example", "//evil.example/x", "practice"] {
            let context = FeedbackContext {
                page: Some(page.into()),
                ..FeedbackContext::default()
            };
            assert_eq!(context.sanitized().page, None, "{page}");
        }
        let nonsense = FeedbackContext {
            daily: Some(-3),
            ..FeedbackContext::default()
        };
        assert_eq!(nonsense.sanitized().daily, None);
    }

    #[test]
    fn rows_list_what_says_something() {
        let website = FeedbackContext {
            platform: "web".into(),
            os: "Linux".into(),
            language: "en".into(),
            ..FeedbackContext::default()
        };
        assert_eq!(
            website.rows(),
            [
                (ContextField::App, String::new()),
                (ContextField::Platform, "Web".into()),
                (ContextField::System, "Linux".into()),
                (ContextField::Language, "en".into()),
            ]
        );
        let app = FeedbackContext {
            app: Some("1.1.0".into()),
            platform: "ios".into(),
            page: Some("/practice".into()),
            daily: Some(12),
            player: Some(40),
            errors: vec!["/ E:database".into(), "/practice panic".into()],
            ..FeedbackContext::default()
        };
        assert_eq!(
            app.rows(),
            [
                (ContextField::App, "1.1.0".into()),
                (ContextField::Platform, "iOS".into()),
                (ContextField::Page, "/practice".into()),
                (ContextField::Daily, "#12".into()),
                (ContextField::Player, "#40".into()),
                (ContextField::Errors, "/ E:database".into()),
                (ContextField::Errors, "/practice panic".into()),
            ]
        );
    }

    /// An issue has the message and its context, never who wrote it.
    #[test]
    fn issues_leave_the_writer_out() {
        let context = FeedbackContext {
            app: Some("1.1.0".into()),
            platform: "android".into(),
            os: "Android 15".into(),
            browser: "WebView 141".into(),
            language: "de".into(),
            page: Some("/practice".into()),
            daily: Some(14),
            player: Some(4711),
            screen: "390×844 @3x".into(),
            errors: vec!["/practice E:database".into(), "panic: a | b `c`".into()],
        };
        let draft = issue_draft(
            12,
            "\n  The hint button does nothing\nafter the second check.",
            Some(&context),
        );
        assert_eq!(draft.title, "The hint button does nothing");
        assert!(
            draft
                .body
                .starts_with("The hint button does nothing\nafter the second check.\n")
        );
        for line in [
            "From the feedback form on hashi.rs (#12).",
            "| Version | app 1.1.0 |",
            "| Platform | Android |",
            "| Page | /practice |",
            "| Daily | #14 |",
            "| Screen | 390×844 @3x |",
            "| Recent errors | `/practice E:database`<br>`panic: a \\| b 'c'` |",
        ] {
            assert!(draft.body.contains(line), "{line}\n{}", draft.body);
        }
        assert!(!draft.body.contains("4711"), "{}", draft.body);
        assert!(!draft.body.contains("Player"), "{}", draft.body);

        // A long first line is cut, a message without context has no table
        let long = "x".repeat(100);
        let draft = issue_draft(3, &long, None);
        assert_eq!(draft.title.chars().count(), 81);
        assert!(!draft.body.contains('|'));
    }

    #[test]
    fn issues_are_labelled_by_type() {
        assert_eq!(issue_labels(FeedbackKind::Bug), ["bug", "feedback"]);
        assert_eq!(issue_labels(FeedbackKind::Idea), ["idea", "feedback"]);
        assert_eq!(
            issue_labels(FeedbackKind::Question),
            ["question", "feedback"]
        );
        assert_eq!(issue_labels(FeedbackKind::Other), ["feedback"]);
    }
}
