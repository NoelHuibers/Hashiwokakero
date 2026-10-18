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
                Err(e) => leptos::logging::warn!("could not load the user: {e}"),
            }
            auth.loaded.set(true);
        });
    }
}
