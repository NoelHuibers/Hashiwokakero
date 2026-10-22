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
    /// The account is banned, it can't sign in.
    Banned,
    /// A group name that is empty, too long or has control characters.
    GroupNameInvalid,
    /// A group name with a word of the list of names that aren't welcome.
    GroupNameNotAllowed,
    /// No such group, or the player isn't a member: the same answer, so nobody learns which
    /// groups there are. Also an invite link that doesn't work (any more).
    GroupNotFound,
    /// The group has `MAX_MEMBERS` already.
    GroupFull,
    /// The player is in `MAX_GROUPS` already.
    TooManyGroups,
    /// Only the group's owner can do that.
    GroupOwnerOnly,
    /// Marking yourself as your own rival.
    RivalSelf,
    /// Already `MAX_RIVALS` rivals.
    TooManyRivals,
    /// No such player (deleted, banned or never there).
    NoSuchPlayer,
    /// A hanko seal that can't be pressed (any more): on one's own result, on a result not
    /// finished, before finishing the daily oneself, or after its days.
    HankoNotAllowed,
    /// A group puzzle wider or taller than `MAX_SIDE`.
    PuzzleTooBig,
    /// A group puzzle without a solution.
    PuzzleUnsolvable,
    /// A group puzzle with more than one solution, or one the solver couldn't prove unique.
    PuzzleNotUnique,
    /// Already `MAX_OPEN` open puzzles in the group.
    TooManyPuzzles,
    /// A puzzle title that is too long or has control characters.
    PuzzleTitleInvalid,
    /// A group puzzle that closed (or was withdrawn): it can't be played any more.
    PuzzleClosed,
    /// The author of a group puzzle can't play it.
    PuzzleOwn,
    /// Our solvers don't touch a group puzzle the player is still to finish.
    PuzzleLocked,
    /// Only a group puzzle's author can do that (withdraw it).
    PuzzleAuthorOnly,
}

impl ApiError {
    pub const ALL: [Self; 42] = [
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
        Self::Banned,
        Self::GroupNameInvalid,
        Self::GroupNameNotAllowed,
        Self::GroupNotFound,
        Self::GroupFull,
        Self::TooManyGroups,
        Self::GroupOwnerOnly,
        Self::RivalSelf,
        Self::TooManyRivals,
        Self::NoSuchPlayer,
        Self::HankoNotAllowed,
        Self::PuzzleTooBig,
        Self::PuzzleUnsolvable,
        Self::PuzzleNotUnique,
        Self::TooManyPuzzles,
        Self::PuzzleTitleInvalid,
        Self::PuzzleClosed,
        Self::PuzzleOwn,
        Self::PuzzleLocked,
        Self::PuzzleAuthorOnly,
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
            Self::Banned => "banned",
            Self::GroupNameInvalid => "group_name_invalid",
            Self::GroupNameNotAllowed => "group_name_not_allowed",
            Self::GroupNotFound => "group_not_found",
            Self::GroupFull => "group_full",
            Self::TooManyGroups => "too_many_groups",
            Self::GroupOwnerOnly => "group_owner_only",
            Self::RivalSelf => "rival_self",
            Self::TooManyRivals => "too_many_rivals",
            Self::NoSuchPlayer => "no_such_player",
            Self::HankoNotAllowed => "hanko_not_allowed",
            Self::PuzzleTooBig => "puzzle_too_big",
            Self::PuzzleUnsolvable => "puzzle_unsolvable",
            Self::PuzzleNotUnique => "puzzle_not_unique",
            Self::TooManyPuzzles => "too_many_puzzles",
            Self::PuzzleTitleInvalid => "puzzle_title_invalid",
            Self::PuzzleClosed => "puzzle_closed",
            Self::PuzzleOwn => "puzzle_own",
            Self::PuzzleLocked => "puzzle_locked",
            Self::PuzzleAuthorOnly => "puzzle_author_only",
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

/// Whether a failed server function call means the tab runs an older build of the site than the
/// server.
///
/// A web page came back ([`is_outdated_client`]), or the server that answered runs another
/// build than the page (`api::client`) and the two didn't understand each other. The
/// server couldn't read the arguments, or the tab the answer: a server function that kept its
/// name but takes or returns other types. Reloading the page fixes both.
///
/// The errors the server meant (`E:<code>`) and network errors stay what they are, also from
/// another build. So does every error when a build is unknown: pages of the versions before the
/// builds were compared, and sites without hashed file names (no hash.txt).
pub fn is_outdated_error(
    error: &leptos::prelude::ServerFnError,
    page_build: Option<&str>,
    server_build: Option<&str>,
) -> bool {
    use leptos::prelude::ServerFnError as E;
    if is_outdated_client(&error.to_string()) {
        return true;
    }
    let (Some(page), Some(server)) = (page_build, server_build) else {
        return false;
    };
    page != server
        && matches!(
            error,
            E::Args(_) | E::MissingArg(_) | E::Deserialization(_) | E::Registration(_)
        )
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
    use leptos::prelude::ServerFnError as E;

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

    /// The errors of a server function that kept its name but takes or returns other types:
    /// the server can't read the arguments, the tab can't read the answer. Decoded the way the
    /// browser decodes an error response.
    fn mismatched_types() -> [E; 4] {
        use leptos::server_fn::error::FromServerFnError as _;
        [
            E::de("Args|missing field `month` at line 1 column 12".into()),
            E::de("MissingArg|day".into()),
            E::de("Registration|Could not find a server function".into()),
            E::Deserialization("invalid type: map, expected a sequence".into()),
        ]
    }

    #[test]
    fn errors_from_another_build_mean_an_outdated_client() {
        for error in mismatched_types() {
            assert!(
                matches!(
                    error,
                    E::Args(_) | E::MissingArg(_) | E::Registration(_) | E::Deserialization(_)
                ),
                "{error:?}"
            );
            assert!(
                is_outdated_error(&error, Some("old"), Some("new")),
                "{error}"
            );
            // The same build: a bug, not an outdated tab
            assert!(
                !is_outdated_error(&error, Some("new"), Some("new")),
                "{error}"
            );
            // A build unknown: a page of a version that didn't name its build, or a server
            // that has none (no hash.txt), or an answer without the header
            assert!(!is_outdated_error(&error, None, Some("new")), "{error}");
            assert!(!is_outdated_error(&error, Some("old"), None), "{error}");
            assert!(!is_outdated_error(&error, None, None), "{error}");
        }
    }

    /// What the server meant to say, and calls that never reached it, stay themselves.
    #[test]
    fn real_errors_from_another_build_stay() {
        for error in [
            E::new("E:wrong_login"),
            E::new("E:username_taken"),
            E::Request("TypeError: Failed to fetch".into()),
            E::Serialization("key must be a string".into()),
        ] {
            assert!(
                !is_outdated_error(&error, Some("old"), Some("new")),
                "{error}"
            );
        }
    }

    /// The 404 page is an outdated tab whatever the builds say.
    #[test]
    fn web_pages_mean_an_outdated_client_without_builds() {
        let error =
            E::Deserialization("Invalid format: missing delimiter in \"<!DOCTYPE html>\"".into());
        for (page, server) in [(None, None), (Some("a"), Some("a")), (Some("a"), Some("b"))] {
            assert!(is_outdated_error(&error, page, server));
        }
    }
}
