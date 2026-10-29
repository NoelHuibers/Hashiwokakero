//! The last errors of this page, for the feedback form's context: the error toasts it showed
//! and a panic. Only in memory, only the last few, gone with the tab; never sent unless the
//! player sends a message and sees them listed first.

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::model::{MAX_CONTEXT_ERROR, MAX_CONTEXT_ERRORS};

/// The last [`MAX_CONTEXT_ERRORS`] errors, each cut to [`MAX_CONTEXT_ERROR`] characters, the
/// oldest first. The same error again right after itself counts once.
#[derive(Debug, Default)]
pub(super) struct RecentErrors(VecDeque<String>);

impl RecentErrors {
    pub(super) const fn new() -> Self {
        Self(VecDeque::new())
    }

    pub(super) fn push(&mut self, error: &str) {
        let error: String = error.trim().chars().take(MAX_CONTEXT_ERROR).collect();
        if error.is_empty() || self.0.back() == Some(&error) {
            return;
        }
        if self.0.len() == MAX_CONTEXT_ERRORS {
            self.0.pop_front();
        }
        self.0.push_back(error);
    }

    pub(super) fn list(&self) -> Vec<String> {
        self.0.iter().cloned().collect()
    }
}

/// This page's errors. A browser has one page per process, the server keeps none.
static RECENT: Mutex<RecentErrors> = Mutex::new(RecentErrors::new());

/// Notes an error of the page, with the path of the page it happened on. Nothing on the server,
/// where one process serves everyone.
pub fn note_error(error: &str) {
    if cfg!(feature = "ssr") {
        return;
    }
    #[cfg(feature = "browser")]
    let error = {
        let path = leptos::prelude::window()
            .location()
            .pathname()
            .unwrap_or_default();
        format!("{} {error}", crate::lang::unlocalized(&path))
    };
    #[cfg(not(feature = "browser"))]
    let error = error.to_string();
    // Not waiting: a panic while the list is being written would wait for itself
    if let Ok(mut recent) = RECENT.try_lock() {
        recent.push(&error);
    }
}

/// The page's last errors, the oldest first.
pub fn recent_errors() -> Vec<String> {
    RECENT.lock().map(|r| r.list()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_last_few_errors_stay() {
        let mut recent = RecentErrors::new();
        assert_eq!(recent.list(), Vec::<String>::new());
        for i in 0..8 {
            recent.push(&format!("/practice E:error_{i}"));
        }
        assert_eq!(
            recent.list(),
            (3..8)
                .map(|i| format!("/practice E:error_{i}"))
                .collect::<Vec<_>>()
        );
        // The same again is no new error, a long one is cut, an empty one isn't one
        recent.push("/practice E:error_7");
        recent.push(&"x".repeat(1000));
        recent.push("  ");
        let list = recent.list();
        assert_eq!(list.len(), MAX_CONTEXT_ERRORS);
        assert_eq!(list[3], "/practice E:error_7");
        assert_eq!(list[4].chars().count(), MAX_CONTEXT_ERROR);
    }

    /// The server notes nothing: its one process serves every player.
    #[test]
    fn the_server_keeps_no_errors() {
        note_error("E:database");
        assert_eq!(recent_errors(), Vec::<String>::new());
    }
}
