//! The signed in user.

use leptos::prelude::*;
use leptos::task::spawn_local;

/// The signed in user (or guest), loaded after hydration.
#[derive(Clone, Copy, Debug)]
pub struct Auth {
    pub user: RwSignal<Option<crate::model::UserInfo>>,
    pub loaded: RwSignal<bool>,
}

impl Auth {
    pub fn refresh(&self) {
        let auth = *self;
        spawn_local(async move {
            match crate::api::current_user().await {
                Ok(user) => auth.user.set(user),
                // The first call of every start of the app: an outdated one asks for the update
                // here, also on pages whose own calls fail silently
                Err(e) if crate::api::needs_update(&e) => super::error_toast(e),
                Err(e) => leptos::logging::warn!("could not load the user: {e}"),
            }
            auth.loaded.set(true);
        });
    }
}
