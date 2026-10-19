//! Errors the server reports to players, translated on the client.

/// Errors the server reports to players. They travel as `E:<code>` in the server function error
/// and are translated on the client (`common.errors.<code>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiError {
    Database,
    Internal,
    ArchiveOnlyPast,
    PasswordTooShort,
    PasswordTooLong,
    UsernameInvalid,
    UsernameTaken,
    WrongLogin,
    TooManyAttempts,
    SessionExpired,
    NotStarted,
    NotSolved,
    WrongPuzzle,
    AlreadyFinished,
    /// Our solvers don't touch today's daily before you finished it.
    DailyLocked,
    /// Only for admins.
    Forbidden,
    /// Deleting an account needs its password.
    WrongPassword,
    EmailInvalid,
    MessageInvalid,
    TooManyMessages,
    /// The username was changed less than `RENAME_EVERY_DAYS` ago.
    RenameTooSoon,
    /// A word of the list of names that aren't welcome (`server/blocked_names.txt`).
    UsernameNotAllowed,
}

impl ApiError {
    pub const ALL: [Self; 22] = [
        Self::Database,
        Self::Internal,
        Self::ArchiveOnlyPast,
        Self::PasswordTooShort,
        Self::PasswordTooLong,
        Self::UsernameInvalid,
        Self::UsernameTaken,
        Self::WrongLogin,
        Self::TooManyAttempts,
        Self::SessionExpired,
        Self::NotStarted,
        Self::NotSolved,
        Self::WrongPuzzle,
        Self::AlreadyFinished,
        Self::DailyLocked,
        Self::Forbidden,
        Self::WrongPassword,
        Self::EmailInvalid,
        Self::MessageInvalid,
        Self::TooManyMessages,
        Self::RenameTooSoon,
        Self::UsernameNotAllowed,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Internal => "internal",
            Self::ArchiveOnlyPast => "archive_only_past",
            Self::PasswordTooShort => "password_too_short",
            Self::PasswordTooLong => "password_too_long",
            Self::UsernameInvalid => "username_invalid",
            Self::UsernameTaken => "username_taken",
            Self::WrongLogin => "wrong_login",
            Self::TooManyAttempts => "too_many_attempts",
            Self::SessionExpired => "session_expired",
            Self::NotStarted => "not_started",
            Self::NotSolved => "not_solved",
            Self::WrongPuzzle => "wrong_puzzle",
            Self::AlreadyFinished => "already_finished",
            Self::DailyLocked => "daily_locked",
            Self::Forbidden => "forbidden",
            Self::WrongPassword => "wrong_password",
            Self::EmailInvalid => "email_invalid",
            Self::MessageInvalid => "message_invalid",
            Self::TooManyMessages => "too_many_messages",
            Self::RenameTooSoon => "rename_too_soon",
            Self::UsernameNotAllowed => "username_not_allowed",
        }
    }

    /// Finds the `E:<code>` marker in an error message.
    pub fn from_message(message: &str) -> Option<Self> {
        let code = message.split("E:").nth(1)?;
        let code = code
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .next()?;
        Self::ALL.into_iter().find(|e| e.code() == code)
    }
}

/// Whether a server function got a web page back instead of its answer.
///
/// The server answers paths it doesn't know with its 404 page, so the tab runs the WASM of an
/// older version of the site that calls a server function the new one doesn't have; reloading
/// the page fixes it.
pub fn is_outdated_client(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("<!doctype") || message.contains("<html")
}

#[cfg(feature = "ssr")]
impl From<ApiError> for leptos::prelude::ServerFnError {
    fn from(e: ApiError) -> Self {
        Self::new(format!("E:{}", e.code()))
    }
}

#[cfg(feature = "ssr")]
impl ApiError {
    /// The error as a server function error, where `.into()` can't infer the type.
    pub fn fail(self) -> leptos::prelude::ServerFnError {
        self.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_round_trip() {
        for e in ApiError::ALL {
            let message = format!("error running server function: E:{}", e.code());
            assert_eq!(ApiError::from_message(&message), Some(e));
        }
        assert_eq!(ApiError::from_message("something else"), None);
    }

    #[test]
    fn web_pages_mean_an_outdated_client() {
        for message in [
            "error deserializing server function results: Invalid format: missing delimiter in \"<!DOCTYPE html><html lang=\\\"en\\\">\"",
            "Unknown error type: <!doctype html>",
            "<html><body>404</body></html>",
        ] {
            assert!(is_outdated_client(message), "{message}");
        }
        for message in [
            "error running server function: E:database",
            "error reaching server to call server function: TypeError: Failed to fetch",
            "a < b",
        ] {
            assert!(!is_outdated_client(message), "{message}");
        }
    }
}
