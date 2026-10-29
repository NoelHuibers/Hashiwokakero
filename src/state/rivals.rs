//! The signed in player's rivals, for the markers on every leaderboard and the toggles.

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::{Auth, ToastKind, error_toast, toast};
use crate::i18n::td_string;

/// The rivals of the signed in player (or guest), loaded with the user.
#[derive(Clone, Copy, Debug)]
pub struct Rivals {
    /// Their ids, in the order they were marked.
    pub ids: RwSignal<Vec<i64>>,
    /// Counts the changes made here, for the cards comparing with the rivals to load again.
    pub changes: RwSignal<u32>,
}

impl Default for Rivals {
    fn default() -> Self {
        Self::new()
    }
}

impl Rivals {
    pub fn new() -> Self {
        Self {
            ids: RwSignal::new(Vec::new()),
            changes: RwSignal::new(0),
        }
    }

    /// Whether `id` is a rival (tracked).
    pub fn is(&self, id: i64) -> bool {
        self.ids.with(|ids| ids.contains(&id))
    }

    /// Loads the list of the signed in player, or empties it without one.
    pub fn load(&self, signed_in: bool) {
        let ids = self.ids;
        if !signed_in {
            ids.set(Vec::new());
            return;
        }
        spawn_local(async move {
            match crate::api::my_rivals().await {
                Ok(list) => ids.set(list),
                Err(e) => leptos::logging::warn!("could not load the rivals: {e}"),
            }
        });
    }

    /// Unmarks `id` here, after the server did (blocking them, see `Blocks::set`), and counts
    /// it as a change.
    pub fn forget(&self, id: i64) {
        if self.is(id) {
            self.ids.update(|ids| ids.retain(|&r| r != id));
            self.changes.update(|c| *c += 1);
        }
    }

    /// Marks `id` here at once, or unmarks it. Whether it is marked now.
    fn flip(&self, id: i64) -> bool {
        let mut on = false;
        self.ids.update(|ids| {
            if let Some(i) = ids.iter().position(|&r| r == id) {
                ids.remove(i);
            } else {
                ids.push(id);
                on = true;
            }
        });
        on
    }

    /// The server's answer to a toggle: its list, or the one from before on an error.
    fn settle(&self, answer: Result<Vec<i64>, Vec<i64>>) -> bool {
        match answer {
            Ok(list) => {
                self.ids.set(list);
                self.changes.update(|c| *c += 1);
                true
            }
            Err(before) => {
                self.ids.set(before);
                false
            }
        }
    }

    /// Marks the player `id` (`name`) as a rival, or unmarks them, and tells the server. A
    /// visitor becomes a guest by it, like on the daily's start.
    pub fn toggle(&self, id: i64, name: String) {
        let this = *self;
        let before = self.ids.get_untracked();
        let on = self.flip(id);
        let auth = use_context::<Auth>();
        spawn_local(async move {
            match crate::api::set_rival(id, on).await {
                Ok(list) => {
                    this.settle(Ok(list));
                    toast(ToastKind::Success, move |l| {
                        if on {
                            td_string!(l, rivals.marked, name = name.clone())
                        } else {
                            td_string!(l, rivals.unmarked, name = name.clone())
                        }
                    });
                    if let Some(auth) = auth.filter(|a| a.user.with_untracked(Option::is_none)) {
                        auth.refresh();
                    }
                }
                Err(e) => {
                    this.settle(Err(before));
                    error_toast(e);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_toggle_shows_at_once_and_the_server_has_the_last_word() {
        Owner::new().with(|| {
            let rivals = Rivals::new();
            assert!(!rivals.is(7));
            assert!(rivals.flip(7));
            assert!(rivals.is(7));
            assert!(rivals.flip(9));
            assert_eq!(rivals.ids.get_untracked(), [7, 9]);
            assert!(!rivals.flip(7));
            assert_eq!(rivals.ids.get_untracked(), [9]);

            // The server's list wins and counts as a change
            assert!(rivals.settle(Ok(vec![9, 3])));
            assert_eq!(rivals.ids.get_untracked(), [9, 3]);
            assert_eq!(rivals.changes.get_untracked(), 1);
            // A failed toggle goes back to the list from before, no change
            rivals.flip(5);
            assert!(!rivals.settle(Err(vec![9, 3])));
            assert_eq!(rivals.ids.get_untracked(), [9, 3]);
            assert_eq!(rivals.changes.get_untracked(), 1);
        });
    }

    /// Blocking a rival takes them off the list here, as the server did.
    #[test]
    fn a_blocked_rival_is_forgotten() {
        Owner::new().with(|| {
            let rivals = Rivals::new();
            rivals.ids.set(vec![7, 9]);
            rivals.forget(7);
            assert_eq!(rivals.ids.get_untracked(), [9]);
            assert_eq!(rivals.changes.get_untracked(), 1);
            // Not a rival: nothing changes
            rivals.forget(3);
            assert_eq!(rivals.changes.get_untracked(), 1);
        });
    }

    #[test]
    fn signed_out_there_are_no_rivals() {
        Owner::new().with(|| {
            let rivals = Rivals::new();
            rivals.ids.set(vec![1, 2]);
            rivals.load(false);
            assert_eq!(rivals.ids.get_untracked(), Vec::<i64>::new());
        });
    }
}
