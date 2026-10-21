//! Explicit, bounded recovery. No error state schedules another request by itself.

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::state::{Daily, LOG_KEY, SavedLog, browser_now, browser_utc_offset};
use crate::api::{DailyOwner, daily_attempt_info, submit_daily_attempt};
use crate::components::stamps::announce;
use crate::model::DailyInfo;
use crate::state::{Status, load};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Phase {
    #[default]
    Idle,
    Pending,
    Error,
}

/// A synchronous pending guard and a generation to reject old completions, including when
/// identity changes start a replacement request before the previous one completes.
#[derive(Clone, Copy, Debug)]
pub(super) struct Fetch {
    pub(super) phase: RwSignal<Phase>,
    generation: StoredValue<u64>,
}

impl Fetch {
    pub(super) fn new() -> Self {
        Self {
            phase: RwSignal::new(Phase::Idle),
            generation: StoredValue::new(0),
        }
    }

    pub(super) fn cancel(self) {
        self.generation.update_value(|g| *g += 1);
        self.phase.set(Phase::Idle);
    }

    pub(super) fn begin(self, replace: bool) -> Option<u64> {
        if !replace && self.phase.get_untracked() == Phase::Pending {
            return None;
        }
        self.generation.update_value(|g| *g += 1);
        self.phase.set(Phase::Pending);
        Some(self.generation.get_value())
    }

    pub(super) fn current(self, generation: u64) -> bool {
        self.generation.try_get_value() == Some(generation)
    }

    pub(super) fn finish(self, generation: u64, failed: bool) {
        if self.current(generation) {
            self.phase
                .set(if failed { Phase::Error } else { Phase::Idle });
        }
    }
}

/// Freeze the original log before dispatch. Another tab may rewrite the shared log key
/// while this page waits for reconciliation; explicit retries must keep the original payload.
#[derive(Clone, Debug)]
pub(super) struct Submission {
    moves: u32,
    log: Vec<crate::model::MoveEvent>,
    utc_offset: i32,
}

/// Preserve the existing reload/sign-in guidance, without passing arbitrary exception text
/// through the general-purpose error toast. The persistent recovery messages stay generic.
pub(super) fn error_feedback(error: &ServerFnError) {
    if crate::api::is_outdated(error)
        || matches!(
            crate::model::ApiError::from_message(&error.to_string()),
            Some(crate::model::ApiError::SessionExpired)
        )
    {
        crate::state::error_toast(error.clone());
    }
}

impl Daily {
    /// Capture pending before spawning: two clicks in one event turn can only dispatch once.
    pub(super) fn begin_submit(&self, retry: bool) -> Option<super::actions::Guard> {
        untrack(|| self.actions.sync(self));
        if self.busy.get_untracked()
            || self.finished()
            || self.actions.locked()
            || self.game.status.get_untracked() != Status::Won
            || (self.submission.get_untracked() == Phase::Error && !retry)
        {
            return None;
        }
        let guard = self.actions.submit_guard(self)?;
        self.busy.set(true);
        self.submission.set(Phase::Pending);
        // Effect ordering cannot omit the last move or append it twice.
        untrack(|| self.record_moves());
        Some(guard)
    }

    /// The original solved board and saved log stay locked until an exact authoritative
    /// attempt says it finished. A running reply permits a guarded retry, never an unlock.
    pub(super) fn reconcile(&self, guard: &super::actions::Guard, info: DailyInfo) -> bool {
        if !guard.current(self) || !guard.matches(&info) {
            return false;
        }
        if info
            .attempt
            .as_ref()
            .is_some_and(crate::model::AttemptInfo::finished)
        {
            self.busy.set(false);
            self.submission.set(Phase::Idle);
            self.submit_payload.set_value(None);
            self.show(info);
            return true;
        }
        false
    }

    pub(super) fn submit(&self, retry: bool) {
        let Some(guard) = self.begin_submit(retry) else {
            return;
        };
        let this = *self;
        let day = guard.day();
        let payload = self
            .submit_payload
            .get_value()
            .unwrap_or_else(|| Submission {
                moves: self.game.moves.get_untracked(),
                log: load::<SavedLog>(LOG_KEY)
                    .filter(|l| l.day == day)
                    .map(|l| l.events)
                    .unwrap_or_default(),
                utc_offset: browser_utc_offset(),
            });
        self.submit_payload.set_value(Some(payload.clone()));
        let owner = DailyOwner {
            user: guard.user(),
            attempt: guard.attempt_id().to_owned(),
        };
        spawn_local(async move {
            if retry {
                // Unknown and mismatched replies cannot fall through to a write.
                let info = daily_attempt_info(day, owner.clone()).await;
                if !guard.current(&this) {
                    return;
                }
                if let Err(error) = &info {
                    error_feedback(error);
                }
                match info {
                    Ok(info) if guard.matches(&info) => {
                        if this.reconcile(&guard, info) {
                            this.refresh_ranking();
                            return;
                        }
                    }
                    _ => {
                        this.busy.set(false);
                        this.submission.set(Phase::Error);
                        return;
                    }
                }
            }
            let reply = submit_daily_attempt(
                day,
                guard.bridges.clone(),
                payload.moves,
                payload.log,
                Some(payload.utc_offset),
                owner.clone(),
            )
            .await;
            if !guard.current(&this) {
                return;
            }
            if let Err(error) = &reply {
                error_feedback(error);
            }
            if let Ok(result) = reply {
                let end = browser_now() + this.skew.get_untracked();
                this.update_attempt(|a| {
                    a.finished_at = Some(end);
                    a.time_ms = Some(result.time_ms);
                });
                announce(&result.stamps);
                this.result.set(Some(result));
                this.submission.set(Phase::Idle);
                this.submit_payload.set_value(None);
                this.busy.set(false);
                this.refresh_ranking();
            } else {
                // Commit may have succeeded before ranking/stamps or transport failed.
                // One read is bounded recovery, not a request loop.
                let info = daily_attempt_info(day, owner).await;
                if !guard.current(&this) {
                    return;
                }
                if let Err(error) = &info {
                    error_feedback(error);
                }
                if let Ok(info) = info
                    && this.reconcile(&guard, info)
                {
                    this.refresh_ranking();
                    return;
                }
                this.busy.set(false);
                this.submission.set(Phase::Error);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_guards_are_immediate_and_only_the_current_generation_can_finish() {
        let owner = Owner::new();
        let fetch = owner.with(Fetch::new);
        let first = fetch.begin(false).unwrap();
        assert!(fetch.begin(false).is_none());
        let replacement = fetch.begin(true).unwrap();
        fetch.finish(first, true);
        assert_eq!(fetch.phase.get_untracked(), Phase::Pending);
        fetch.finish(replacement, true);
        assert_eq!(fetch.phase.get_untracked(), Phase::Error);
        let retry = fetch.begin(false).unwrap();
        assert!(fetch.begin(false).is_none());
        fetch.finish(retry, false);
        assert_eq!(fetch.phase.get_untracked(), Phase::Idle);
        owner.cleanup();
        assert!(!fetch.current(retry));
        fetch.finish(retry, true);
    }
}
