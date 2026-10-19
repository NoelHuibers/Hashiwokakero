//! Toasts: short messages in a corner of the screen that follow the language picker.

use std::time::Duration;

use leptos::prelude::*;

use crate::i18n::Locale;
use crate::model::is_outdated_client;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

/// The text of a toast in a given language, so it follows the language picker.
pub type ToastText = std::sync::Arc<dyn Fn(Locale) -> String + Send + Sync>;

#[derive(Clone)]
pub struct Toast {
    pub id: u32,
    pub kind: ToastKind,
    pub text: ToastText,
    /// With a button that reloads the page: the tab runs an outdated version of the site.
    pub reload: bool,
}

impl std::fmt::Debug for Toast {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Toast")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("reload", &self.reload)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Toasts(pub RwSignal<Vec<Toast>>);

impl Toasts {
    pub fn push(&self, kind: ToastKind, text: impl Fn(Locale) -> String + Send + Sync + 'static) {
        self.show(kind, false, std::sync::Arc::new(text));
    }

    /// A server function call that failed, see `lang::error_text`. When the server didn't know
    /// the function, the tab runs an outdated version of the site: the toast offers a reload
    /// and stays a while.
    pub fn push_error(&self, error: ServerFnError) {
        let reload = is_outdated_client(&error.to_string());
        let text = move |l: Locale| crate::lang::error_text(l, &error);
        self.show(ToastKind::Error, reload, std::sync::Arc::new(text));
    }

    fn show(&self, kind: ToastKind, reload: bool, text: ToastText) {
        let list = self.0;
        let Some(Some(id)) = list.try_update(|l| add(l, kind, reload, text)) else {
            return;
        };
        let shown = if reload { 12_000 } else { 3600 };
        set_timeout(
            move || list.update(|l| l.retain(|t| t.id != id)),
            Duration::from_millis(shown),
        );
    }
}

/// Adds a toast to the list, which keeps the last three. Its id, or `None` when it asks for a
/// reload that is already asked for: an outdated tab fails every call at once.
fn add(list: &mut Vec<Toast>, kind: ToastKind, reload: bool, text: ToastText) -> Option<u32> {
    if reload && list.iter().any(|t| t.reload) {
        return None;
    }
    let id = list.iter().map(|t| t.id + 1).max().unwrap_or(0);
    list.push(Toast {
        id,
        kind,
        text,
        reload,
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
            assert_eq!(add(&mut list, ToastKind::Info, false, text("hi")), Some(i));
        }
        assert_eq!(list.iter().map(|t| t.id).collect::<Vec<_>>(), [2, 3, 4]);
    }

    /// An outdated tab fails every call of a page: one toast asks for a reload, not three.
    #[test]
    fn one_reload_toast_at_a_time() {
        let mut list = Vec::new();
        assert_eq!(
            add(&mut list, ToastKind::Error, true, text("reload")),
            Some(0)
        );
        assert_eq!(add(&mut list, ToastKind::Error, true, text("reload")), None);
        assert_eq!(
            add(&mut list, ToastKind::Error, false, text("other")),
            Some(1)
        );
        assert_eq!(list.iter().filter(|t| t.reload).count(), 1);
    }
}
