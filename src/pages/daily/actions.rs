//! Paid intents survive reloads and closed tabs. A failed transport leaves an explicit retry,
//! with the same ID and original board, instead of guessing whether a penalty was committed.

use common::Puzzle;
use leptos::prelude::*;

use super::state::Daily;
use crate::api::{PaidAction, PaidIntent as Intent, PaidOutcome, PaidRecovery, PaidReply};
use crate::state::Auth;

const INTENT_KEY: &str = "hashi.daily.action";

// Separate localStorage keys preserve every tab's unresolved intent: concurrent writes never
// overwrite another intent, and closing a tab does not lose an ambiguous successful purchase.
// Storage failure is fail-closed. An accepted ID is never replaced just because saving failed.
#[cfg(not(test))]
mod storage {
    use leptos::prelude::window;

    pub(super) fn entries() -> Vec<(String, String)> {
        let Ok(Some(storage)) = window().local_storage() else {
            return vec![];
        };
        (0..storage.length().unwrap_or(0))
            .filter_map(|index| {
                let key = storage.key(index).ok()??;
                if !key.starts_with(super::INTENT_KEY) {
                    return None;
                }
                let value = storage.get_item(&key).ok()??;
                Some((key, value))
            })
            .collect()
    }

    pub(super) fn set(key: &str, value: Option<&str>) -> bool {
        let Ok(Some(storage)) = window().local_storage() else {
            return false;
        };
        match value {
            Some(value) => {
                storage.set_item(key, value).is_ok()
                    && storage.get_item(key).ok().flatten().as_deref() == Some(value)
            }
            None => storage.remove_item(key).is_ok(),
        }
    }
}

#[cfg(test)]
mod storage {
    use std::{cell::RefCell, collections::BTreeMap};
    thread_local! { static ITEMS: RefCell<BTreeMap<String, String>> = RefCell::default(); }
    pub(super) fn entries() -> Vec<(String, String)> {
        ITEMS.with_borrow(|items| items.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }
    pub(super) fn set(key: &str, value: Option<&str>) -> bool {
        ITEMS.with_borrow_mut(|items| match value {
            Some(value) => {
                items.insert(key.to_owned(), value.to_owned());
            }
            None => {
                items.remove(key);
            }
        });
        true
    }
}

fn intent_key(intent: &Intent) -> String {
    format!(
        "{INTENT_KEY}.{}.{}.{}.{}",
        intent.user, intent.day, intent.started_at, intent.id
    )
}

fn saved_intents() -> Vec<Intent> {
    storage::entries()
        .into_iter()
        .filter_map(|(key, value)| {
            let intent = serde_json::from_str::<Intent>(&value).ok()?;
            (intent_key(&intent) == key).then_some(intent)
        })
        .collect()
}

fn save_intent(intent: &Intent) -> bool {
    let key = intent_key(intent);
    let entries = storage::entries();
    // Bound browser retention too, and always permit retry of an existing key at the limit.
    if entries.len() >= 256 && !entries.iter().any(|(k, _)| *k == key) {
        return false;
    }
    let Ok(json) = serde_json::to_string(intent) else {
        return false;
    };
    storage::set(&key, Some(&json))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Scope {
    user: i64,
    day: i64,
    started_at: i64,
    attempt_id: String,
    epoch: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Actions {
    pub pending: RwSignal<bool>,
    pub intent: RwSignal<Option<Intent>>,
    pub recovery: RwSignal<bool>,
    checked_foreign: StoredValue<Vec<Intent>>,
    auth: Auth,
    scope: StoredValue<Option<Scope>>,
    version: RwSignal<u64>,
    sequence: RwSignal<u64>,
}

/// Captured synchronously before dispatch, also used by delayed animation callbacks. Page
/// signals test owner lifetime (the shared game outlives the page), not just puzzle identity.
#[derive(Clone, Debug)]
pub(super) struct Guard {
    user: i64,
    day: i64,
    started_at: i64,
    attempt_id: String,
    epoch: u32,
    version: u64,
    sequence: u64,
    puzzle: Puzzle,
    pub bridges: Vec<u8>,
}

impl Actions {
    pub(super) fn new(auth: Auth) -> Self {
        Self {
            pending: RwSignal::new(false),
            intent: RwSignal::new(None),
            recovery: RwSignal::new(false),
            checked_foreign: StoredValue::new(vec![]),
            auth,
            scope: StoredValue::new(None),
            version: RwSignal::new(0),
            sequence: RwSignal::new(0),
        }
    }

    /// Reactively restore only this player's attempt. Switching day, puzzle or account
    /// invalidates in-flight work immediately, even if an old response has not returned yet.
    pub(super) fn sync(&self, daily: &Daily) {
        let user = self.auth.user.with(|u| u.as_ref().map(|u| u.id));
        let attempt = daily.info.with(|i| {
            i.as_ref().and_then(|i| {
                i.attempt
                    .as_ref()
                    .map(|a| (i.day, a.started_at, a.attempt_id.clone()))
            })
        });
        let epoch = daily.game.epoch.get();
        let scope = user
            .zip(attempt)
            .map(|(user, (day, started_at, attempt_id))| Scope {
                user,
                day,
                started_at,
                attempt_id,
                epoch,
            });
        if self.scope.get_value() == scope {
            return;
        }
        self.scope.set_value(scope.clone());
        self.checked_foreign.set_value(vec![]);
        self.sequence.update(|s| *s += 1);
        self.pending.set(false);
        // An old UTC day cannot be played again from this page. Drop its browser intents;
        // server receipts still remain safe to replay for the lifetime of their attempts.
        if let Some(scope) = scope {
            for old in saved_intents().into_iter().filter(|i| i.day < scope.day) {
                storage::set(&intent_key(&old), None);
            }
        }
        self.restore_intent();
    }

    pub(super) fn user(&self) -> Option<i64> {
        self.auth.user.with_untracked(|u| u.as_ref().map(|u| u.id))
    }

    pub(super) fn refresh_revision(&self) -> u64 {
        self.sequence.get_untracked()
    }

    pub(super) fn locked(&self) -> bool {
        self.pending.get() || self.intent.with(Option::is_some) || self.recovery.get()
    }

    pub(super) fn can_start(&self, daily: &Daily, action: Option<PaidAction>) -> bool {
        !self.pending.get()
            && (!self.recovery.get() || action.is_some())
            && !daily.busy.get()
            && daily.started()
            && !daily.finished()
            && (daily.game.status.get() == crate::state::Status::Playing
                || (daily.game.status.get() == crate::state::Status::Won
                    && (self.recovery.get() && action.is_some()
                        || self
                            .intent
                            .with(|i| i.as_ref().is_some_and(|i| Some(i.action) == action)))))
            && self
                .intent
                .with(|intent| intent.as_ref().is_none_or(|i| Some(i.action) == action))
            && self.auth.user.with(Option::is_some)
    }

    /// All daily editing paths (including keyboard cycling) share the synchronous lock and
    /// revision. A board changed and then undone still invalidates older callbacks.
    pub(super) fn edit(&self, daily: &Daily, edit: impl FnOnce()) {
        if untrack(|| self.can_start(daily, None)) {
            self.version.update(|v| *v += 1);
            daily.game.hint.set(None);
            daily.game.wrong.set(vec![]);
            edit();
        }
    }

    pub(super) fn begin(&self, daily: &Daily, action: Option<PaidAction>) -> Option<Guard> {
        untrack(|| self.sync(daily));
        if !self.pending.get_untracked() && self.intent.with_untracked(Option::is_none) {
            self.restore_intent();
        }
        if !untrack(|| self.can_start(daily, action)) {
            return None;
        }
        self.pending.set(true);
        self.sequence.update(|s| *s += 1);
        daily.game.hint.set(None);
        daily.game.wrong.set(vec![]);
        self.capture(daily)
    }

    pub(super) fn capture(&self, daily: &Daily) -> Option<Guard> {
        let user = self.auth.user.get_untracked()?.id;
        let (day, started_at, attempt_id) = daily.info.with_untracked(|i| {
            i.as_ref().and_then(|i| {
                i.attempt
                    .as_ref()
                    .map(|a| (i.day, a.started_at, a.attempt_id.clone()))
            })
        })?;
        Some(Guard {
            user,
            day,
            started_at,
            attempt_id,
            epoch: daily.game.epoch.get_untracked(),
            version: self.version.get_untracked(),
            sequence: self.sequence.get_untracked(),
            puzzle: daily.game.board.with_untracked(|b| b.puzzle.clone()),
            bridges: daily.game.bridges.get_untracked(),
        })
    }

    pub(super) fn purchase(
        &self,
        guard: &Guard,
        action: PaidAction,
        id: impl FnOnce() -> String,
    ) -> Option<Intent> {
        let intent = self.intent.get_untracked().unwrap_or_else(|| Intent {
            attempt_id: guard.attempt_id.clone(),
            user: guard.user,
            day: guard.day,
            started_at: guard.started_at,
            action,
            bridges: guard.bridges.clone(),
            id: id(),
        });
        if !save_intent(&intent) {
            return None;
        }
        self.intent.set(Some(intent.clone()));
        Some(intent)
    }

    pub(super) fn failed(&self, daily: &Daily, guard: &Guard, error: &ServerFnError) {
        // Only the action transaction can prove there is no accepted receipt. In particular,
        // session expiry on a retry says nothing about an earlier ambiguous success: retain
        // its ID so signing back in cannot turn that retry into another purchase.
        if guard.current(daily)
            && crate::model::ApiError::from_message(&error.to_string()).is_some_and(|e| {
                matches!(
                    e,
                    crate::model::ApiError::WrongPuzzle
                        | crate::model::ApiError::NotStarted
                        | crate::model::ApiError::AlreadyFinished
                        | crate::model::ApiError::TooManyAttempts
                )
            })
        {
            self.resolved();
        }
        self.finish(guard);
    }

    fn restore_intent(&self) {
        let scope = self.scope.get_value();
        self.intent.set(saved_intents().into_iter().find(|intent| {
            scope.as_ref().is_some_and(|scope| {
                (intent.user, intent.day, intent.started_at)
                    == (scope.user, scope.day, scope.started_at)
                    && !intent.attempt_id.is_empty()
                    && intent.attempt_id == scope.attempt_id
            })
        }));
        self.recovery.set(!self.recovery_intents().is_empty());
    }

    /// Foreign intents are candidates only. They cannot be adopted or sent to the purchase
    /// endpoint: only a read-only, authenticated server receipt can prove a real transfer.
    pub(super) fn recovery_intents(&self) -> Vec<Intent> {
        if let Some(intent) = self.intent.get_untracked() {
            if self.scope.get_value().is_some_and(|scope| {
                intent.user != scope.user
                    || intent.attempt_id != scope.attempt_id
                    || scope.attempt_id.is_empty()
            }) {
                return vec![intent];
            }
            return vec![];
        }
        let scope = self.scope.get_value();
        saved_intents()
            .into_iter()
            .filter(|i| {
                scope.as_ref().is_some_and(|scope| {
                    (i.user != scope.user
                        || i.attempt_id != scope.attempt_id
                        || scope.attempt_id.is_empty())
                        && i.day == scope.day
                        && i.started_at == scope.started_at
                }) && !self
                    .checked_foreign
                    .with_value(|checked| checked.contains(i))
            })
            .collect()
    }

    pub(super) fn recovered(&self, recovery: Option<&PaidRecovery>, candidates: &[Intent]) {
        if recovery.is_none() {
            // A successful negative lookup unlocks this scope, but never deletes another
            // account's durable intent or silently turns a recovery click into a purchase.
            self.checked_foreign.update_value(|checked| {
                if checked.len() + candidates.len() > 256 {
                    checked.clear();
                }
                checked.extend_from_slice(candidates);
            });
        }
        // Keep the original storage key until the outcome is actually consumed. Reloads and
        // storage failures therefore remain read-only retries, even on the new account.
        self.intent.set(recovery.map(|r| r.intent.clone()));
        self.recovery.set(false);
    }

    pub(super) fn resolved(&self) {
        if let Some(intent) = self.intent.get_untracked() {
            // Delete only this intent. Another tab's unresolved intent must remain recoverable.
            // If removal fails, restoring it safely replays the receipt without charging.
            storage::set(&intent_key(&intent), None);
        }
        self.restore_intent();
    }

    pub(super) fn finish(&self, guard: &Guard) {
        if self.sequence.try_get_untracked() == Some(guard.sequence) {
            self.pending.set(false);
        }
    }
}

impl Guard {
    /// Reconcile counters once, and return feedback only for the board that bought it.
    pub(super) fn accept(
        &self,
        daily: &Daily,
        purchased: &[u8],
        reply: PaidReply,
    ) -> Option<PaidOutcome> {
        if !self.current(daily) {
            return None;
        }
        daily.update_attempt(|a| {
            a.hints = a.hints.max(reply.hints);
            a.checks = a.checks.max(reply.checks);
        });
        (self.bridges == purchased
            && daily.game.status.get_untracked() == crate::state::Status::Playing)
            .then_some(reply.outcome)
    }

    pub(super) fn current(&self, daily: &Daily) -> bool {
        daily.actions.sequence.try_get_untracked() == Some(self.sequence)
            && daily.actions.version.try_get_untracked() == Some(self.version)
            && daily
                .actions
                .auth
                .user
                .try_with_untracked(|u| u.as_ref().map(|u| u.id))
                == Some(Some(self.user))
            && daily.info.try_with_untracked(|i| {
                i.as_ref().and_then(|i| {
                    i.attempt
                        .as_ref()
                        .map(|a| (i.day, a.started_at, a.attempt_id.clone()))
                })
            }) == Some(Some((self.day, self.started_at, self.attempt_id.clone())))
            && daily.game.epoch.try_get_untracked() == Some(self.epoch)
            && daily
                .game
                .board
                .try_with_untracked(|b| b.puzzle == self.puzzle)
                == Some(true)
            && daily
                .game
                .bridges
                .try_with_untracked(|b| *b == self.bridges)
                == Some(true)
    }

    pub(super) fn clear_hint(&self, daily: &Daily, edge: usize) {
        if self.current(daily) && daily.game.hint.get_untracked() == Some(edge) {
            daily.game.hint.set(None);
        }
    }

    pub(super) fn clear_wrong(&self, daily: &Daily) {
        if self.current(daily) {
            daily.game.wrong.set(vec![]);
        }
    }

    pub(super) fn reveal_step(&self, daily: &Daily, edge: usize, value: u8) {
        if self.current(daily)
            && daily.game.status.get_untracked() == crate::state::Status::Revealed
        {
            daily.game.bridges.update(|b| b[edge] = value);
        }
    }

    pub(super) fn user(&self) -> i64 {
        self.user
    }

    pub(super) fn started_at(&self) -> i64 {
        self.started_at
    }

    pub(super) fn day(&self) -> i64 {
        self.day
    }
}

#[cfg(test)]
mod tests {
    use common::{Board, Difficulty, Island, Pos, Puzzle};

    use super::*;
    use crate::model::{AttemptInfo, DailyInfo, UserInfo};
    use crate::state::{Game, Status};

    fn page(game: &Game, auth: Auth) -> Daily {
        let daily = Daily {
            game: *game,
            actions: Actions::new(auth),
            info: RwSignal::new(Some(DailyInfo {
                day: 100,
                difficulty: Difficulty::Easy,
                puzzle: Some(game.board.with_untracked(|b| b.puzzle.clone())),
                attempt: Some(AttemptInfo {
                    attempt_id: "test-attempt-identity".into(),
                    started_at: 1000,
                    finished_at: None,
                    hints: 0,
                    checks: 0,
                    revealed: false,
                    time_ms: None,
                    bridges: None,
                    flag: None,
                }),
                server_now: 2000,
                finishers: 0,
                stamps: vec![],
            })),
            skew: RwSignal::new(0),
            now: RwSignal::new(2000),
            result: RwSignal::new(None),
            ranking: RwSignal::new(None),
            busy: RwSignal::new(false),
            logged: StoredValue::new(None),
            refreshing: StoredValue::new(false),
        };
        daily.actions.sync(&daily);
        daily
    }

    fn tab(test: impl FnOnce(Daily, Auth)) {
        Owner::new().with(|| {
            for (key, _) in storage::entries() {
                storage::set(&key, None);
            }
            let game = Game::new("test.daily.actions");
            game.board.set(Board::new(Puzzle::new(
                3,
                1,
                vec![
                    Island {
                        pos: Pos { x: 0, y: 0 },
                        required: 1,
                    },
                    Island {
                        pos: Pos { x: 2, y: 0 },
                        required: 1,
                    },
                ],
            )));
            game.bridges.set(vec![0]);
            game.status.set(Status::Playing);
            let auth = Auth {
                user: RwSignal::new(Some(UserInfo {
                    id: 42,
                    name: "player".into(),
                    guest: true,
                    created_at: 0,
                    admin: false,
                    renamed_at: None,
                    name_locked: false,
                })),
                loaded: RwSignal::new(true),
            };
            test(page(&game, auth), auth);
        });
    }

    #[test]
    fn pending_is_synchronous_and_blocks_double_taps_all_actions_and_edits() {
        tab(|d, _| {
            let first = d.actions.begin(&d, Some(PaidAction::Hint)).unwrap();
            assert!(d.actions.pending.get_untracked());
            for action in [Some(PaidAction::Hint), Some(PaidAction::Check), None] {
                assert!(d.actions.begin(&d, action).is_none());
            }
            d.actions.edit(&d, || d.game.cycle(0, 1));
            assert_eq!(d.game.bridges.get_untracked(), [0]);
            assert_eq!(d.game.moves.get_untracked(), 0);
            d.actions.finish(&first);
            d.actions.edit(&d, || d.game.cycle(0, 1));
            assert_eq!(d.game.bridges.get_untracked(), [1]);
            assert_eq!(d.game.moves.get_untracked(), 1);
        });
    }

    #[test]
    fn an_ambiguous_failure_clears_pending_but_only_retries_the_durable_intent() {
        tab(|d, _| {
            let first = d.actions.begin(&d, Some(PaidAction::Hint)).unwrap();
            let intent = d
                .actions
                .purchase(&first, PaidAction::Hint, || "first-intent-id-0001".into())
                .unwrap();
            d.actions.finish(&first); // transport failed after the server may have committed
            assert!(!d.actions.pending.get_untracked());
            assert!(d.actions.begin(&d, Some(PaidAction::Check)).is_none());
            d.actions.edit(&d, || d.game.cycle(0, 1));
            assert_eq!(d.game.bridges.get_untracked(), [0]);
            let retry = d.actions.begin(&d, Some(PaidAction::Hint)).unwrap();
            let retried = d
                .actions
                .purchase(&retry, PaidAction::Hint, || {
                    panic!("a retry must reuse the ID")
                })
                .unwrap();
            assert_eq!(intent, retried);
            d.actions.resolved();
            d.actions.finish(&retry);
            assert!(!d.actions.locked());
            assert_eq!(saved_intents(), Vec::new());
            assert!(d.actions.begin(&d, Some(PaidAction::Check)).is_some());
        });
    }

    #[test]
    fn reload_recovers_the_intent_and_a_solved_board_can_retry_before_submission() {
        tab(|d, auth| {
            let first = d.actions.begin(&d, Some(PaidAction::Hint)).unwrap();
            let intent = d
                .actions
                .purchase(&first, PaidAction::Hint, || "lost-response-id-001".into())
                .unwrap();
            // A new page owner has only durable storage, not the previous pending signal.
            let reloaded = page(&d.game, auth);
            assert!(!reloaded.actions.pending.get_untracked());
            assert_eq!(
                reloaded.actions.intent.get_untracked(),
                Some(intent.clone())
            );
            reloaded.game.status.set(Status::Won);
            let retry = reloaded
                .actions
                .begin(&reloaded, Some(PaidAction::Hint))
                .unwrap();
            assert_eq!(
                reloaded
                    .actions
                    .purchase(&retry, PaidAction::Hint, || panic!("new ID"))
                    .unwrap(),
                intent
            );
            reloaded.actions.resolved();
            reloaded.actions.finish(&retry);
            assert!(!reloaded.actions.locked()); // auto-submit can now flush the final move
        });
    }

    #[test]
    fn error_completion_releases_pending_and_preserves_only_ambiguous_intents() {
        tab(|d, _| {
            for (message, ambiguous) in [
                ("transport failed", true),
                ("E:database", true),
                ("E:session_expired", true),
                ("E:forbidden", true),
                ("E:wrong_puzzle", false),
            ] {
                let guard = d.actions.begin(&d, Some(PaidAction::Check)).unwrap();
                d.actions
                    .purchase(&guard, PaidAction::Check, || "error-intent-id-001".into())
                    .unwrap();
                d.actions.failed(&d, &guard, &ServerFnError::new(message));
                assert!(!d.actions.pending.get_untracked());
                assert_eq!(d.actions.intent.get_untracked().is_some(), ambiguous);
                assert_eq!(!saved_intents().is_empty(), ambiguous);
            }
            assert!(d.actions.can_start(&d, None));
        });
    }

    #[test]
    fn reply_counters_reconcile_after_reload_and_stale_replies_cannot_publish() {
        tab(|d, _| {
            let guard = d.actions.begin(&d, Some(PaidAction::Hint)).unwrap();
            let reply = PaidReply {
                outcome: PaidOutcome::Hint(Some((0, 1))),
                hints: 1,
                checks: 2,
            };
            assert!(guard.accept(&d, &[0], reply.clone()).is_some());
            assert!(guard.accept(&d, &[0], reply.clone()).is_some());
            let counters = || {
                d.info.with_untracked(|i| {
                    let a = i.as_ref().unwrap().attempt.as_ref().unwrap();
                    (a.hints, a.checks)
                })
            };
            assert_eq!(counters(), (1, 2));
            // The server's reply to a retry still belongs to its original purchased board.
            assert!(guard.accept(&d, &[1], reply.clone()).is_none());
            d.info.update(|i| i.as_mut().unwrap().day += 1);
            let stale = PaidReply {
                hints: 99,
                checks: 99,
                ..reply
            };
            assert!(guard.accept(&d, &[0], stale).is_none());
            assert_eq!(counters(), (1, 2));
            assert_eq!(d.game.bridges.get_untracked(), [0]);
        });
    }

    #[test]
    fn delayed_callbacks_mutate_only_the_expected_frame_of_the_current_page() {
        tab(|d, _| {
            let first = d.actions.begin(&d, None).unwrap();
            d.game.status.set(Status::Revealed);
            first.reveal_step(&d, 0, 1);
            assert_eq!(d.game.bridges.get_untracked(), [1]);
            first.reveal_step(&d, 0, 2); // the old frame no longer matches
            assert_eq!(d.game.bridges.get_untracked(), [1]);
            let frame = d.actions.capture(&d).unwrap();
            d.game.hint.set(Some(0));
            d.game.wrong.set(vec![0]);
            d.game.epoch.update(|e| *e += 1);
            frame.reveal_step(&d, 0, 2);
            frame.clear_hint(&d, 0);
            frame.clear_wrong(&d);
            assert_eq!(d.game.bridges.get_untracked(), [1]);
            assert_eq!(d.game.hint.get_untracked(), Some(0));
            assert_eq!(d.game.wrong.get_untracked(), [0]);
            let current = d.actions.capture(&d).unwrap();
            current.clear_hint(&d, 0);
            current.clear_wrong(&d);
            assert_eq!(d.game.hint.get_untracked(), None);
            assert_eq!(d.game.wrong.get_untracked(), Vec::<usize>::new());
        });
    }

    #[test]
    fn independent_tab_intents_survive_each_others_completion_and_storage_is_bounded() {
        tab(|d, auth| {
            let guard = d.actions.begin(&d, Some(PaidAction::Hint)).unwrap();
            let first = d
                .actions
                .purchase(&guard, PaidAction::Hint, || "parallel-intent-0000".into())
                .unwrap();
            // Two tabs can start simultaneously; separate keys preserve both IDs rather than
            // letting the last writer erase another tab's unresolved successful purchase.
            let second = Intent {
                id: "parallel-intent-0001".into(),
                ..first.clone()
            };
            assert!(save_intent(&second));
            d.actions.resolved();
            assert_eq!(saved_intents(), vec![second.clone()]);
            let reloaded = page(&d.game, auth);
            assert_eq!(
                reloaded.actions.intent.get_untracked(),
                Some(second.clone())
            );
            for n in 2..257 {
                assert!(save_intent(&Intent {
                    id: format!("parallel-intent-{n:04}"),
                    ..first.clone()
                }));
            }
            assert!(!save_intent(&first)); // a new ID at the retention limit is fail-closed
            assert!(save_intent(&second)); // existing IDs are always retryable at the limit
            reloaded.actions.resolved();
            assert_eq!(saved_intents().len(), 255);
        });
    }

    #[test]
    fn an_old_intent_is_not_restored_for_another_player_or_attempt() {
        tab(|d, auth| {
            let guard = d.actions.begin(&d, Some(PaidAction::Check)).unwrap();
            d.actions
                .purchase(&guard, PaidAction::Check, || "scoped-intent-id-01".into())
                .unwrap();
            auth.user.update(|u| u.as_mut().unwrap().id += 1);
            assert!(page(&d.game, auth).actions.intent.get_untracked().is_none());
            auth.user.update(|u| u.as_mut().unwrap().id -= 1);
            d.info.update(|i| i.as_mut().unwrap().day += 1);
            d.actions.sync(&d);
            assert!(d.actions.intent.get_untracked().is_none());
            assert!(!guard.current(&d));
        });
    }

    #[test]
    fn reused_user_and_start_time_cannot_restore_an_intent_or_accept_old_callbacks() {
        tab(|d, auth| {
            for action in [PaidAction::Hint, PaidAction::Check] {
                let guard = d.actions.begin(&d, Some(action)).unwrap();
                let intent = d
                    .actions
                    .purchase(&guard, action, || "reused-identity-01".into())
                    .unwrap();
                // The shared cookie now names a replacement guest with exactly the same ID.
                // Until refresh arrives, the session error retains the original purchase.
                d.actions
                    .failed(&d, &guard, &ServerFnError::new("E:session_expired"));
                assert_eq!(saved_intents(), std::slice::from_ref(&intent));
                d.update_attempt(|a| a.attempt_id = "replacement-attempt".into());
                assert!(!guard.current(&d));
                d.actions.sync(&d);
                assert!(d.actions.intent.get_untracked().is_none());
                // Even coincident timestamps can only attempt a read-only recovery.
                assert_eq!(d.actions.recovery_intents(), std::slice::from_ref(&intent));
                d.actions.recovered(None, std::slice::from_ref(&intent));
                assert_eq!(saved_intents(), std::slice::from_ref(&intent));
                assert!(!d.actions.locked());
                // Authenticating to the receiving account exposes the transferred attempt.
                auth.user.update(|u| u.as_mut().unwrap().id += 1);
                d.update_attempt(|a| a.attempt_id.clone_from(&intent.attempt_id));
                d.actions.sync(&d);
                assert_eq!(d.actions.recovery_intents(), std::slice::from_ref(&intent));
                let recovered = PaidRecovery {
                    intent: intent.clone(),
                    reply: PaidReply {
                        outcome: PaidOutcome::Check(vec![]),
                        hints: 1,
                        checks: 1,
                    },
                };
                d.actions
                    .recovered(Some(&recovered), std::slice::from_ref(&intent));
                d.actions.resolved();
                assert_eq!(saved_intents(), Vec::<Intent>::new());
            }
        });
    }

    #[test]
    fn pre_token_intents_are_read_only_even_for_the_same_user() {
        tab(|d, auth| {
            let guard = d.actions.begin(&d, Some(PaidAction::Check)).unwrap();
            let mut intent = d
                .actions
                .purchase(&guard, PaidAction::Check, || "legacy-intent-id-01".into())
                .unwrap();
            intent.attempt_id.clear();
            assert!(save_intent(&intent));
            let reloaded = page(&d.game, auth);
            assert!(reloaded.actions.intent.get_untracked().is_none());
            assert_eq!(reloaded.actions.recovery_intents(), [intent.clone()]);
            reloaded
                .actions
                .recovered(None, std::slice::from_ref(&intent));
            assert_eq!(saved_intents(), [intent]);
            assert!(!reloaded.actions.locked());
        });
    }

    #[test]
    fn a_foreign_intent_needs_read_only_proof_and_unknown_recovery_stays_locked() {
        tab(|d, auth| {
            let first = d.actions.begin(&d, Some(PaidAction::Hint)).unwrap();
            let intent = d
                .actions
                .purchase(&first, PaidAction::Hint, || "merged-lost-hint-01".into())
                .unwrap();
            auth.user.update(|u| u.as_mut().unwrap().id += 1);
            let new_page = page(&d.game, auth);
            assert!(new_page.actions.intent.get_untracked().is_none());
            assert!(new_page.actions.recovery.get_untracked());
            assert!(new_page.actions.locked());
            new_page
                .actions
                .edit(&new_page, || new_page.game.cycle(0, 1));
            assert_eq!(new_page.game.bridges.get_untracked(), [0]);
            let lookup = new_page
                .actions
                .begin(&new_page, Some(PaidAction::Check))
                .unwrap();
            assert_eq!(
                new_page.actions.recovery_intents(),
                std::slice::from_ref(&intent)
            );
            // A failed lookup must not manufacture an intent for the new identity.
            new_page.actions.finish(&lookup);
            assert!(new_page.actions.recovery.get_untracked());
            assert!(new_page.actions.locked());
            let proven = PaidRecovery {
                intent: intent.clone(),
                reply: PaidReply {
                    outcome: PaidOutcome::Hint(Some((0, 1))),
                    hints: 1,
                    checks: 0,
                },
            };
            new_page
                .actions
                .recovered(Some(&proven), std::slice::from_ref(&intent));
            assert_eq!(
                new_page.actions.intent.get_untracked(),
                Some(intent.clone())
            );
            assert!(
                new_page
                    .actions
                    .begin(&new_page, Some(PaidAction::Check))
                    .is_none()
            );
            new_page.game.status.set(Status::Won);
            let retry = new_page
                .actions
                .begin(&new_page, Some(PaidAction::Hint))
                .unwrap();
            assert_eq!(
                new_page
                    .actions
                    .purchase(&retry, PaidAction::Hint, || panic!(
                        "no new ID after transfer"
                    )),
                Some(intent.clone())
            );
            assert_eq!(new_page.actions.recovery_intents(), [intent]);
            assert!(retry.accept(&new_page, &[0], proven.reply).is_none());
            assert_eq!(
                new_page
                    .info
                    .get_untracked()
                    .unwrap()
                    .attempt
                    .unwrap()
                    .hints,
                1
            );
            new_page.actions.resolved();
            new_page.actions.finish(&retry);
            assert!(!new_page.actions.locked());
            assert_eq!(saved_intents(), Vec::<Intent>::new());
        });
    }

    #[test]
    fn absent_foreign_receipts_do_not_adopt_or_delete_another_accounts_intents() {
        tab(|d, auth| {
            let first = d.actions.begin(&d, Some(PaidAction::Check)).unwrap();
            let intent = d
                .actions
                .purchase(&first, PaidAction::Check, || "unrelated-intent-01".into())
                .unwrap();
            auth.user.update(|u| u.as_mut().unwrap().id += 1);
            let new_page = page(&d.game, auth);
            let lookup = new_page
                .actions
                .begin(&new_page, Some(PaidAction::Hint))
                .unwrap();
            new_page
                .actions
                .recovered(None, std::slice::from_ref(&intent)); // authenticated lookup found no exact receipt
            new_page.actions.finish(&lookup);
            assert!(!new_page.actions.locked());
            let next = new_page
                .actions
                .begin(&new_page, Some(PaidAction::Hint))
                .unwrap();
            assert_eq!(new_page.actions.recovery_intents(), Vec::<Intent>::new());
            new_page.actions.finish(&next);
            assert_eq!(saved_intents(), std::slice::from_ref(&intent));
            assert_eq!(new_page.game.bridges.get_untracked(), [0]);
            auth.user.update(|u| u.as_mut().unwrap().id -= 1);
            let original = page(&d.game, auth);
            assert_eq!(original.actions.intent.get_untracked(), Some(intent));
        });
    }

    #[test]
    fn replies_and_callbacks_reject_day_epoch_board_and_revision_changes() {
        tab(|d, _| {
            let guard = d.actions.begin(&d, None).unwrap();
            assert!(guard.current(&d));
            d.info.update(|i| i.as_mut().unwrap().day += 1);
            assert!(!guard.current(&d));
            d.info.update(|i| i.as_mut().unwrap().day -= 1);
            d.game.epoch.update(|e| *e += 1);
            assert!(!guard.current(&d));
            d.game.epoch.update(|e| *e -= 1);
            d.game.board.update(|b| b.puzzle.islands[0].required += 1);
            assert!(!guard.current(&d));
            d.game.board.update(|b| b.puzzle.islands[0].required -= 1);
            d.game.bridges.set(vec![1]);
            assert!(!guard.current(&d));
            d.game.bridges.set(vec![0]);
            d.actions.finish(&guard);
            // Returning to identical bridges via edit/undo is still a newer revision.
            d.actions.edit(&d, || d.game.cycle(0, 1));
            d.actions.edit(&d, || d.game.undo());
            assert_eq!(d.game.bridges.get_untracked(), guard.bridges);
            assert!(!guard.current(&d));
        });
    }

    #[test]
    fn a_new_request_invalidates_older_highlight_and_animation_callbacks() {
        tab(|d, _| {
            let old = d.actions.begin(&d, None).unwrap();
            d.actions.finish(&old);
            let new = d.actions.begin(&d, None).unwrap();
            assert!(!old.current(&d));
            assert!(new.current(&d));
            // An obsolete completion must not clear the new request's pending flag.
            d.actions.finish(&old);
            assert!(d.actions.pending.get_untracked());
            d.actions.finish(&new);
            assert!(!d.actions.pending.get_untracked());
        });
    }

    #[test]
    fn disposed_page_owners_cannot_mutate_the_persistent_game() {
        tab(|d, auth| {
            let owner = Owner::new();
            let (page, guard) = owner.with(|| {
                let page = page(&d.game, auth);
                (page, page.actions.begin(&page, None).unwrap())
            });
            owner.cleanup();
            assert!(!guard.current(&page));
            guard.clear_hint(&page, 0);
            guard.clear_wrong(&page);
            guard.reveal_step(&page, 0, 1);
            assert!(
                guard
                    .accept(
                        &page,
                        &[0],
                        PaidReply {
                            outcome: PaidOutcome::Hint(Some((0, 1))),
                            hints: 9,
                            checks: 9
                        }
                    )
                    .is_none()
            );
            page.actions.finish(&guard); // no panic when page signals are disposed
            assert_eq!(d.game.bridges.get_untracked(), [0]);
        });
    }
}
