//! Toasts: short messages in a corner of the screen that follow the language picker.

use std::time::Duration;

use leptos::prelude::*;

use crate::i18n::Locale;

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
pub struct Toasts(pub RwSignal<Vec<Toast>>);

impl Toasts {
    pub fn push(&self, kind: ToastKind, text: impl Fn(Locale) -> String + Send + Sync + 'static) {
        self.show(kind, None, std::sync::Arc::new(text));
    }

    /// A server function call that failed, see `lang::error_text`. When the tab runs an outdated
    /// version of the site (`api::is_outdated`: the server didn't know the function, or runs
    /// another build that didn't understand the call), the toast offers a reload and stays a
    /// while; in an outdated app (`api::needs_update`) an update.
    pub fn push_error(&self, error: ServerFnError) {
        let action = error_action(
            crate::api::needs_update(&error),
            crate::api::is_outdated(&error),
        );
        let text = move |l: Locale| crate::lang::error_text(l, &error);
        self.show(ToastKind::Error, action, std::sync::Arc::new(text));
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

    #[test]
    fn outdated_apps_update_and_outdated_tabs_reload() {
        assert_eq!(error_action(true, true), Some(ToastAction::Update));
        assert_eq!(error_action(true, false), Some(ToastAction::Update));
        assert_eq!(error_action(false, true), Some(ToastAction::Reload));
        assert_eq!(error_action(false, false), None);
    }
}
