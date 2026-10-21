//! The hanko 判子 the signed in player got and didn't see yet, for the dot on the groups'
//! navigation entry and the badges of the group list.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::model::UnreadHanko;

/// Unread seals per group, loaded with the user and again on the group list.
#[derive(Clone, Copy, Debug)]
pub struct HankoInbox {
    pub unread: RwSignal<Vec<UnreadHanko>>,
}

impl Default for HankoInbox {
    fn default() -> Self {
        Self::new()
    }
}

impl HankoInbox {
    pub fn new() -> Self {
        Self {
            unread: RwSignal::new(Vec::new()),
        }
    }

    /// Unread seals in all groups (tracked).
    pub fn total(&self) -> i64 {
        self.unread.with(|u| u.iter().map(|g| g.count).sum())
    }

    /// Unread seals in one group (tracked).
    pub fn of(&self, group: i64) -> i64 {
        self.unread.with(|u| {
            u.iter()
                .find(|g| g.group_id == group)
                .map_or(0, |g| g.count)
        })
    }

    /// The seals of one group (`Some`) or all were seen: opening them told the server so.
    pub fn seen(&self, group: Option<i64>) {
        self.unread.update(|u| match group {
            Some(group) => u.retain(|g| g.group_id != group),
            None => u.clear(),
        });
    }

    /// Asks the server, or empties the list without a player.
    pub fn load(&self, signed_in: bool) {
        let unread = self.unread;
        if !signed_in {
            unread.set(Vec::new());
            return;
        }
        spawn_local(async move {
            match crate::api::unread_hanko_counts().await {
                Ok(list) => unread.set(list),
                Err(e) => leptos::logging::warn!("could not load the unread hanko: {e}"),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeing_a_group_clears_its_seals_only() {
        Owner::new().with(|| {
            let inbox = HankoInbox::new();
            assert_eq!(inbox.total(), 0);
            inbox.unread.set(vec![
                UnreadHanko {
                    group_id: 3,
                    count: 2,
                },
                UnreadHanko {
                    group_id: 8,
                    count: 1,
                },
            ]);
            assert_eq!(
                (inbox.total(), inbox.of(3), inbox.of(8), inbox.of(5)),
                (3, 2, 1, 0)
            );
            inbox.seen(Some(3));
            assert_eq!((inbox.total(), inbox.of(3), inbox.of(8)), (1, 0, 1));
            // Seeing a group without unread ones changes nothing
            inbox.seen(Some(5));
            assert_eq!(inbox.total(), 1);
            // The daily result opens all of them
            inbox.seen(None);
            assert_eq!(inbox.total(), 0);
        });
    }

    #[test]
    fn signed_out_nothing_is_unread() {
        Owner::new().with(|| {
            let inbox = HankoInbox::new();
            inbox.unread.set(vec![UnreadHanko {
                group_id: 1,
                count: 4,
            }]);
            inbox.load(false);
            assert_eq!(inbox.total(), 0);
        });
    }
}
