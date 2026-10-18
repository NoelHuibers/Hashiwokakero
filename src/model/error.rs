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
}

impl ApiError {
    pub const ALL: [Self; 16] = [
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
}
