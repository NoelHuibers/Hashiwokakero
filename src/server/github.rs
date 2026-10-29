//! GitHub issues made of feedback (`contact::create_issue`), and whether they were fixed
//! (`contact::notify_fixed`), over GitHub's REST API.
//!
//! - `GITHUB_FEEDBACK_TOKEN`: a fine-grained personal access token for the repository only, with
//!   the permission "Issues: Read and write" (docs/deployment.md)
//! - `GITHUB_FEEDBACK_REPO`: `owner/name`, default `NoelHuibers/Hashiwokakero`
//!
//! Without the token nothing goes to GitHub: the admins' "Create issue" is off, and nobody is
//! told about fixes.

use std::sync::OnceLock;
use std::time::Duration;

use serde::Deserialize;

const DEFAULT_REPO: &str = "NoelHuibers/Hashiwokakero";
const API: &str = "https://api.github.com";

/// What became of an issue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueState {
    Open,
    /// Closed as completed: fixed, or built.
    Completed,
    /// Closed otherwise: not planned, a duplicate.
    Closed,
}

/// Where issues go. [`GitHub`], a fake in the tests.
pub trait Issues: Sync {
    /// Opens an issue, returns its number.
    fn create(
        &self,
        title: &str,
        body: &str,
        labels: &[&str],
    ) -> impl Future<Output = Result<i64, String>> + Send;

    fn state(&self, number: i64) -> impl Future<Output = Result<IssueState, String>> + Send;
}

/// The repository's issues, with the token.
pub struct GitHub {
    token: String,
    repo: String,
}

impl std::fmt::Debug for GitHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the token
        f.debug_struct("GitHub")
            .field("repo", &self.repo)
            .finish_non_exhaustive()
    }
}

impl GitHub {
    /// The settings from `var` (the environment), `None` without a token or with a repository
    /// that isn't `owner/name`.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let var = |name: &str| {
            var(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let token = var("GITHUB_FEEDBACK_TOKEN")?;
        let repo = var("GITHUB_FEEDBACK_REPO").unwrap_or_else(|| DEFAULT_REPO.into());
        let valid = |part: &str| {
            !part.is_empty()
                && !part.starts_with('.')
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        };
        match repo.split_once('/') {
            Some((owner, name)) if valid(owner) && valid(name) => Some(Self { token, repo }),
            _ => {
                eprintln!("github: GITHUB_FEEDBACK_REPO isn't owner/name, issues are off");
                None
            }
        }
    }

    pub fn from_env() -> Option<Self> {
        Self::from_vars(super::env_var)
    }

    /// `owner/name`.
    pub fn repo(&self) -> &str {
        &self.repo
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
        CLIENT
            .get_or_init(reqwest::Client::new)
            .request(method, format!("{API}/repos/{}{path}", self.repo))
            .bearer_auth(&self.token)
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28")
            .header("user-agent", "hashi.rs")
            .timeout(Duration::from_secs(20))
    }
}

/// The answer's body, or an error with its status (GitHub's message, never the token).
async fn answer(response: Result<reqwest::Response, reqwest::Error>) -> Result<String, String> {
    let response = response.map_err(|e| e.without_url().to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if status.is_success() {
        Ok(text)
    } else {
        let message: String = text.chars().take(300).collect();
        Err(format!("{status}: {message}"))
    }
}

impl Issues for GitHub {
    async fn create(&self, title: &str, body: &str, labels: &[&str]) -> Result<i64, String> {
        let json = serde_json::json!({ "title": title, "body": body, "labels": labels });
        let text = answer(
            self.request(reqwest::Method::POST, "/issues")
                .header("content-type", "application/json")
                .body(json.to_string())
                .send()
                .await,
        )
        .await?;
        created_number(&text)
    }

    async fn state(&self, number: i64) -> Result<IssueState, String> {
        let text = answer(
            self.request(reqwest::Method::GET, &format!("/issues/{number}"))
                .send()
                .await,
        )
        .await?;
        issue_state(&text)
    }
}

/// The number of the issue GitHub created.
fn created_number(json: &str) -> Result<i64, String> {
    #[derive(Deserialize)]
    struct Created {
        number: i64,
    }
    serde_json::from_str::<Created>(json)
        .map(|c| c.number)
        .map_err(|e| format!("unexpected answer: {e}"))
}

/// What an issue's JSON says became of it.
fn issue_state(json: &str) -> Result<IssueState, String> {
    #[derive(Deserialize)]
    struct Issue {
        state: String,
        state_reason: Option<String>,
    }
    let issue: Issue = serde_json::from_str(json).map_err(|e| format!("unexpected answer: {e}"))?;
    Ok(
        match (issue.state.as_str(), issue.state_reason.as_deref()) {
            ("closed", Some("completed")) => IssueState::Completed,
            ("closed", _) => IssueState::Closed,
            _ => IssueState::Open,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn github(vars: &[(&str, &str)]) -> Option<GitHub> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        GitHub::from_vars(|name| vars.get(name).cloned())
    }

    #[test]
    fn settings_come_from_the_environment() {
        assert!(github(&[]).is_none());
        assert!(github(&[("GITHUB_FEEDBACK_TOKEN", " ")]).is_none());
        let g = github(&[("GITHUB_FEEDBACK_TOKEN", "github_pat_secret")]).unwrap();
        assert_eq!(g.repo(), "NoelHuibers/Hashiwokakero");
        // The token never shows in a log
        assert!(!format!("{g:?}").contains("secret"));
        let g = github(&[
            ("GITHUB_FEEDBACK_TOKEN", "t"),
            ("GITHUB_FEEDBACK_REPO", "someone/hashi-fork"),
        ])
        .unwrap();
        assert_eq!(g.repo(), "someone/hashi-fork");
        for repo in ["noslash", "a/b/c", "a/", "../x", "a b/c"] {
            assert!(
                github(&[
                    ("GITHUB_FEEDBACK_TOKEN", "t"),
                    ("GITHUB_FEEDBACK_REPO", repo)
                ])
                .is_none(),
                "{repo}"
            );
        }
    }

    #[test]
    fn answers_are_read() {
        assert_eq!(
            created_number(r#"{"id":1,"number":190,"title":"x"}"#),
            Ok(190)
        );
        assert!(created_number(r#"{"message":"Bad credentials"}"#).is_err());
        for (json, state) in [
            (r#"{"state":"open","state_reason":null}"#, IssueState::Open),
            (
                r#"{"state":"open","state_reason":"reopened"}"#,
                IssueState::Open,
            ),
            (
                r#"{"state":"closed","state_reason":"completed"}"#,
                IssueState::Completed,
            ),
            (
                r#"{"state":"closed","state_reason":"not_planned"}"#,
                IssueState::Closed,
            ),
            (
                r#"{"state":"closed","state_reason":"duplicate"}"#,
                IssueState::Closed,
            ),
            (r#"{"state":"closed"}"#, IssueState::Closed),
        ] {
            assert_eq!(issue_state(json), Ok(state), "{json}");
        }
    }
}
