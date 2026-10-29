//! Toasts: short messages in a corner of the screen that follow the language picker.

use std::time::Duration;

use leptos::prelude::*;

use crate::i18n::{Locale, td_string};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

/// The button of a toast that says the client is outdated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastAction {
    /// Reloads the page: the tab runs an outdated version of the site.
    Reload,
    /// Opens the app's store page (`api::update_url`): the app is outdated, and reloading
    /// would load the same bundle again.
    Update,
}

/// The text of a toast in a given language, so it follows the language picker.
pub type ToastText = std::sync::Arc<dyn Fn(Locale) -> String + Send + Sync>;

#[derive(Clone)]
pub struct Toast {
    pub id: u32,
    pub kind: ToastKind,
    pub text: ToastText,
    /// A button for a client that is outdated.
    pub action: Option<ToastAction>,
}

impl std::fmt::Debug for Toast {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Toast")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("action", &self.action)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Toasts(
    pub RwSignal<Vec<Toast>>,
    /// The app said it's offline, which it says once until the network is back.
    StoredValue<bool>,
);

impl Default for Toasts {
    fn default() -> Self {
        Self::new()
    }
}

impl Toasts {
    pub fn new() -> Self {
        Self(RwSignal::new(Vec::new()), StoredValue::new(false))
    }

    /// The network is back (the window's `online` event): the next call that can't reach the
    /// server says so again.
    pub fn back_online(&self) {
        self.1.set_value(false);
    }

    pub fn push(&self, kind: ToastKind, text: impl Fn(Locale) -> String + Send + Sync + 'static) {
        self.show(kind, None, std::sync::Arc::new(text));
    }

    /// A server function call that failed, see `lang::error_text`. When the tab runs an outdated
    /// version of the site (`api::is_outdated`: the server didn't know the function, or runs
    /// another build that didn't understand the call), the toast offers a reload and stays a
    /// while; in an outdated app (`api::needs_update`) an update.
    ///
    /// In the app, a call that didn't reach the server shows a calm notice instead, once while
    /// offline: without a network every call fails, and practice works on.
    ///
    /// Every error is noted for the feedback form's context (`note_error`), its code if it has
    /// one.
    pub fn push_error(&self, error: ServerFnError) {
        super::note_error(&error_note(&error));
        match self.noted(cfg!(feature = "csr"), &error) {
            Shown::Error => {}
            Shown::Offline => {
                self.push(ToastKind::Info, |l| {
                    td_string!(l, common.toast.offline).to_string()
                });
                return;
            }
            Shown::Nothing => return,
        }
        let action = error_action(
            crate::api::needs_update(&error),
            crate::api::is_outdated(&error),
        );
        let text = move |l: Locale| crate::lang::error_text(l, &error);
        self.show(ToastKind::Error, action, std::sync::Arc::new(text));
    }

    /// What a failed call shows in the app (`app`) or on the website, see [`shown`]. Notes the
    /// offline notice, which isn't shown again until [`Self::back_online`].
    fn noted(&self, app: bool, error: &ServerFnError) -> Shown {
        let shown = shown(app, error, self.1.get_value());
        if shown == Shown::Offline {
            self.1.set_value(true);
        }
        shown
    }

    fn show(&self, kind: ToastKind, action: Option<ToastAction>, text: ToastText) {
        let list = self.0;
        let Some(Some(id)) = list.try_update(|l| add(l, kind, action, text)) else {
            return;
        };
        let shown = if action.is_some() { 12_000 } else { 3600 };
        set_timeout(
            move || list.update(|l| l.retain(|t| t.id != id)),
            Duration::from_millis(shown),
        );
    }
}

/// What a failed call shows.
#[derive(Debug, PartialEq, Eq)]
enum Shown {
    Error,
    /// The app's notice that it's offline.
    Offline,
    /// Nothing more: the app said it's offline already.
    Nothing,
}

/// What the failed call shows in the app (`app`) or on the website, with the offline notice
/// shown already or not (`noted`). On the website an error, as always: a page there doesn't
/// work without the server either.
fn shown(app: bool, error: &ServerFnError, noted: bool) -> Shown {
    if !(app && matches!(error, ServerFnError::Request(_))) {
        Shown::Error
    } else if noted {
        Shown::Nothing
    } else {
        Shown::Offline
    }
}

/// How the feedback form's context lists a failed call: `E:<code>` of an error the server
/// reported, the error itself (cut) otherwise.
fn error_note(error: &ServerFnError) -> String {
    let text = error.to_string();
    match crate::model::ApiError::from_message(&text) {
        Some(e) => format!("E:{}", e.code()),
        None => text,
    }
}

/// The button of an error's toast: an update for an app that needs one, else a reload for an
/// outdated tab.
fn error_action(update: bool, outdated: bool) -> Option<ToastAction> {
    if update {
        Some(ToastAction::Update)
    } else if outdated {
        Some(ToastAction::Reload)
    } else {
        None
    }
}

/// Adds a toast to the list, which keeps the last three. Its id, or `None` when it asks for a
/// reload or an update that is already asked for: an outdated tab or app fails every call at once.
fn add(
    list: &mut Vec<Toast>,
    kind: ToastKind,
    action: Option<ToastAction>,
    text: ToastText,
) -> Option<u32> {
    if action.is_some() && list.iter().any(|t| t.action == action) {
        return None;
    }
    let id = list.iter().map(|t| t.id + 1).max().unwrap_or(0);
    list.push(Toast {
        id,
        kind,
        text,
        action,
    });
    if list.len() > 3 {
        list.remove(0);
    }
    Some(id)
}

/// Shows a toast, e.g. `toast(ToastKind::Info, |l| td_string!(l, common.toast.copied).to_string())`.
pub fn toast(kind: ToastKind, text: impl Fn(Locale) -> String + Send + Sync + 'static) {
    if let Some(t) = use_context::<Toasts>() {
        t.push(kind, text);
    }
}

/// Shows a server function's error, see `Toasts::push_error`.
pub fn error_toast(error: ServerFnError) {
    if let Some(t) = use_context::<Toasts>() {
        t.push_error(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &'static str) -> ToastText {
        std::sync::Arc::new(move |_| s.to_string())
    }

    /// The feedback form lists an error the server reported by its code, others as they are.
    #[test]
    fn errors_are_noted_by_their_code() {
        let reported = crate::model::ApiError::Database.fail();
        assert_eq!(error_note(&reported), "E:database");
        let other = ServerFnError::Request("connection refused".into());
        assert!(error_note(&other).contains("connection refused"));
    }

    /// Three toasts at most, the newest ones, each with its own id.
    #[test]
    fn the_last_three_toasts_stay() {
        let mut list = Vec::new();
        for i in 0..5 {
            assert_eq!(add(&mut list, ToastKind::Info, None, text("hi")), Some(i));
        }
        assert_eq!(list.iter().map(|t| t.id).collect::<Vec<_>>(), [2, 3, 4]);
    }

    /// An outdated tab fails every call of a page: one toast asks for a reload, not three.
    #[test]
    fn one_reload_toast_at_a_time() {
        let reload = Some(ToastAction::Reload);
        let mut list = Vec::new();
        assert_eq!(
            add(&mut list, ToastKind::Error, reload, text("reload")),
            Some(0)
        );
        assert_eq!(
            add(&mut list, ToastKind::Error, reload, text("reload")),
            None
        );
        assert_eq!(
            add(&mut list, ToastKind::Error, None, text("other")),
            Some(1)
        );
        assert_eq!(list.iter().filter(|t| t.action == reload).count(), 1);
    }

    /// An app below the server's minimum fails every call: one toast asks for the update,
    /// however many there are.
    #[test]
    fn one_update_toast_at_a_time() {
        use leptos::server_fn::Bytes;
        use leptos::server_fn::error::FromServerFnError as _;
        let refused = ServerFnError::de(Bytes::from("ServerError|E:app_outdated"));
        let mut list = Vec::new();
        let mut shown = 0;
        for _ in 0..5 {
            // As in the app, where `api::needs_update` says so
            let action = error_action(true, crate::api::is_outdated(&refused));
            assert_eq!(action, Some(ToastAction::Update));
            shown +=
                usize::from(add(&mut list, ToastKind::Error, action, text("update")).is_some());
        }
        assert_eq!(shown, 1);
        assert_eq!(list.len(), 1);
        // Other errors still have their toasts
        assert_eq!(
            add(&mut list, ToastKind::Error, None, text("other")),
            Some(1)
        );
    }

    /// Without a network every call of the app fails at once (the ranks page makes three): one
    /// notice that it's offline, until the network is back. The server's own errors still show,
    /// and the website shows every error as before.
    #[test]
    fn the_app_says_once_that_it_is_offline() {
        let offline = ServerFnError::Request("TypeError: Failed to fetch".into());
        assert_eq!(shown(true, &offline, false), Shown::Offline);
        assert_eq!(shown(true, &offline, true), Shown::Nothing);
        let locked = ServerFnError::ServerError("E:daily_locked".into());
        assert_eq!(shown(true, &locked, false), Shown::Error);
        assert_eq!(shown(true, &locked, true), Shown::Error);
        assert_eq!(shown(false, &offline, false), Shown::Error);
        assert_eq!(shown(false, &offline, true), Shown::Error);
    }

    /// The window's `online` event (`state::provide_state`) brings the notice back: the next
    /// call that can't reach the server says once more that the app is offline.
    #[test]
    fn the_offline_notice_comes_again_once_the_network_is_back() {
        Owner::new().with(|| {
            let toasts = Toasts::new();
            let offline = ServerFnError::Request("TypeError: Failed to fetch".into());
            assert_eq!(toasts.noted(true, &offline), Shown::Offline);
            assert_eq!(toasts.noted(true, &offline), Shown::Nothing);
            toasts.back_online();
            assert_eq!(toasts.noted(true, &offline), Shown::Offline);
            assert_eq!(toasts.noted(true, &offline), Shown::Nothing);
            // The website has no notice, every error shows
            toasts.back_online();
            assert_eq!(toasts.noted(false, &offline), Shown::Error);
            assert_eq!(toasts.noted(true, &offline), Shown::Offline);
        });
    }

    #[test]
    fn outdated_apps_update_and_outdated_tabs_reload() {
        assert_eq!(error_action(true, true), Some(ToastAction::Update));
        assert_eq!(error_action(true, false), Some(ToastAction::Update));
        assert_eq!(error_action(false, true), Some(ToastAction::Reload));
        assert_eq!(error_action(false, false), None);
    }
}
