//! The players the signed in player blocked (`server::blocks`): for the buttons on their
//! profiles and the list on the account page.

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::{Auth, Rivals, ToastKind, error_toast, toast};
use crate::i18n::td_string;
use crate::model::BlockedPlayer;

#[derive(Clone, Copy, Debug)]
pub struct Blocks {
    /// The latest first, as the server lists them.
    pub list: RwSignal<Vec<BlockedPlayer>>,
}

impl Default for Blocks {
    fn default() -> Self {
        Self::new()
    }
}

impl Blocks {
    pub fn new() -> Self {
        Self {
            list: RwSignal::new(Vec::new()),
        }
    }

    /// Whether `id` is blocked (tracked).
    pub fn is(&self, id: i64) -> bool {
        self.list.with(|list| list.iter().any(|b| b.id == id))
    }

    /// Loads the list of the signed in player, or empties it without one.
    pub fn load(&self, signed_in: bool) {
        let list = self.list;
        if !signed_in {
            list.set(Vec::new());
            return;
        }
        spawn_local(async move {
            match crate::api::my_blocks().await {
                Ok(blocked) => list.set(blocked),
                Err(e) => leptos::logging::warn!("could not load the blocked players: {e}"),
            }
        });
    }

    /// Blocks the player `id` (`name`), or unblocks them, and tells the server. Blocking takes
    /// them off the rivals. A visitor becomes a guest by it, like by marking a rival.
    pub fn set(&self, id: i64, name: String, on: bool) {
        let list = self.list;
        let auth = use_context::<Auth>();
        let rivals = use_context::<Rivals>();
        spawn_local(async move {
            match crate::api::block_player(id, on).await {
                Ok(blocked) => {
                    list.set(blocked);
                    if on && let Some(rivals) = rivals {
                        rivals.forget(id);
                    }
                    toast(ToastKind::Success, move |l| {
                        if on {
                            td_string!(l, ranks.player.block.done, name = name.clone())
                        } else {
                            td_string!(l, ranks.player.block.undone, name = name.clone())
                        }
                    });
                    if let Some(auth) = auth.filter(|a| a.user.with_untracked(Option::is_none)) {
                        auth.refresh();
                    }
                }
                Err(e) => error_toast(e),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whom_the_player_blocked() {
        Owner::new().with(|| {
            let blocks = Blocks::new();
            assert!(!blocks.is(7));
            blocks.list.set(vec![BlockedPlayer {
                id: 7,
                name: "troll".into(),
                guest: false,
                blocked_at: 0,
            }]);
            assert!(blocks.is(7));
            assert!(!blocks.is(8));
        });
    }
}
