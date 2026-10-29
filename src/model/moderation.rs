//! Reports and blocks: a player reports another one to us (a name that isn't welcome, cheating),
//! or blocks them, so their name isn't shown to them anywhere any more.

use serde::{Deserialize, Serialize};

/// Characters of a report's note at most.
pub const MAX_REPORT_NOTE: usize = 500;

/// What a report is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReportReason {
    /// An offensive or inappropriate name.
    Name,
    Cheating,
    Other,
}

impl ReportReason {
    pub const ALL: [Self; 3] = [Self::Name, Self::Cheating, Self::Other];

    /// As stored in the database.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Cheating => "cheating",
            Self::Other => "other",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.id() == id)
    }
}

/// What an admin did about a report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportOutcome {
    /// The player got the neutral name (`neutral_name`).
    Renamed,
    Banned,
    /// Nothing to do.
    Dismissed,
}

impl ReportOutcome {
    /// As stored in the database.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Renamed => "renamed",
            Self::Banned => "banned",
            Self::Dismissed => "dismissed",
        }
    }
}

/// An open report, for the admin dashboard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminReport {
    pub id: i64,
    pub player: i64,
    /// The player's name now, and when they were reported (it may have changed since).
    pub player_name: String,
    pub reported_name: String,
    /// `None` for guests.
    pub player_username: Option<String>,
    pub banned: bool,
    pub reason: ReportReason,
    /// Empty if the reporter wrote none.
    pub note: String,
    pub reporter: i64,
    pub reporter_name: String,
    /// When it was first sent, and last (a report again updates it).
    pub created_at: i64,
    pub updated_at: i64,
}

/// A player the viewer blocked, for the list on the account page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedPlayer {
    pub id: i64,
    pub name: String,
    pub guest: bool,
    pub blocked_at: i64,
}

/// The name the server sends instead of a blocked player's to whoever blocked them.
///
/// Everywhere other players' names reach them (`server::blocks`). The pages show "blocked
/// player" instead, without a link to the profile (see [`is_blocked_name`]); an app from before
/// blocks shows the sign as it is. No name can be it: usernames are ASCII, guest names are made
/// by the server.
pub const BLOCKED_NAME: &str = "\u{1F6AB}";

/// Whether a name on a board is one the viewer blocked ([`BLOCKED_NAME`]).
pub fn is_blocked_name(name: &str) -> bool {
    name == BLOCKED_NAME
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_are_stored_by_their_ids() {
        for reason in ReportReason::ALL {
            assert_eq!(ReportReason::from_id(reason.id()), Some(reason));
        }
        assert_eq!(ReportReason::from_id("spam"), None);
    }

    /// No name a player can have is the sign that stands for a blocked one.
    #[test]
    fn the_blocked_sign_is_no_name() {
        assert!(is_blocked_name(BLOCKED_NAME));
        assert!(!is_blocked_name(""));
        assert!(!is_blocked_name("Player 17"));
        assert!(!BLOCKED_NAME.is_ascii());
    }
}
