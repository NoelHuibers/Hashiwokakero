//! The iOS app's registration for notifications through Apple's push service
//! (`server::notify::apns` sends them).
//!
//! iOS gives the app a device token once the player allowed notifications (the app's plugin,
//! `HashiPlugin.swift`: `notify_permission`, `notify_token`), and says which of Apple's push
//! services it is for. The page registers it with the server (`register_notification_device`)
//! and keeps the device's id and secret in local storage, for unregistering (turned off, signed
//! out, also offline: no session needed) and for registering again when iOS hands out another
//! token (after a restore or a reinstall), the page's language changed, or a week went by, which
//! keeps the server from forgetting the phone (`refresh`, at every start).

use serde::{Deserialize, Serialize};

use crate::model::ApnsEnvironment;

/// Where this phone's registration is kept.
const KEY: &str = "hashi.apns.v1";
/// How often the phone registers again: well within the 90 days after which the server forgets
/// a device it hasn't seen (`notify::DEVICE_IDLE_MS`).
const REFRESH_MS: f64 = 7.0 * 86_400_000.0;

/// This phone's registration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stored {
    pub id: i64,
    pub secret: String,
    /// The device token it registered, hex.
    pub token: String,
    pub env: ApnsEnvironment,
    /// The language of its texts.
    pub locale: String,
    /// When it registered, ms since the epoch.
    pub at: f64,
}

/// This phone's registration, if it has one.
pub fn stored() -> Option<Stored> {
    super::load(KEY)
}

/// Whether the phone has to register (again) with this token, push service and language: not
/// registered yet, a token or a service other than the registered one, another language, or a
/// week since the last time.
pub fn due(
    stored: Option<&Stored>,
    token: &str,
    env: ApnsEnvironment,
    locale: &str,
    now: f64,
) -> bool {
    stored.is_none_or(|s| {
        !s.token.eq_ignore_ascii_case(token)
            || s.env != env
            || s.locale != locale
            || now - s.at >= REFRESH_MS
    })
}

#[cfg(feature = "csr")]
pub use app::{refresh, register, unregister};

#[cfg(feature = "csr")]
#[expect(
    clippy::future_not_send,
    reason = "the app's plugin, on the page's one thread"
)]
mod app {
    use leptos::prelude::ServerFnError;
    use leptos_i18n::Locale as _;

    use super::{KEY, Stored, due, stored};
    use crate::api::{native, register_notification_device, unregister_notification_device};
    use crate::i18n::Locale;
    use crate::model::DeviceKind;
    use crate::state::notify::time_zone;

    thread_local! {
        /// A registration under way: the start's refresh and the settings page's don't both
        /// register (the server would keep the later one, the page maybe the other's secret).
        static REGISTERING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// Registers this phone for the signed in player's notifications when that is due ([`due`]),
    /// with the token iOS has for the app now. Needs the player's leave (`notify_permission`).
    pub async fn register(locale: Locale) -> Result<(), ServerFnError> {
        if REGISTERING.replace(true) {
            return Ok(());
        }
        let done = register_now(locale).await;
        REGISTERING.set(false);
        done
    }

    async fn register_now(locale: Locale) -> Result<(), ServerFnError> {
        let token = native::notify_token().await?;
        let old = stored();
        if !due(
            old.as_ref(),
            &token.token,
            token.environment,
            locale.as_str(),
            js_sys::Date::now(),
        ) {
            return Ok(());
        }
        let registration = register_notification_device(
            DeviceKind::Apns,
            Some(token.token.clone()),
            Some(token.environment.keys()),
            locale.as_str().to_string(),
            time_zone(),
        )
        .await?;
        crate::state::store(
            KEY,
            &Stored {
                id: registration.id,
                secret: registration.secret,
                token: token.token,
                env: token.environment,
                locale: locale.as_str().to_string(),
                at: js_sys::Date::now(),
            },
        );
        // The registration of an earlier token: the server keeps one per token
        if let Some(old) = old
            && old.id != registration.id
            && let Err(e) = unregister_notification_device(old.id, old.secret).await
        {
            leptos::logging::warn!("could not unregister the phone's old token: {e}");
        }
        Ok(())
    }

    /// The server forgets this phone: no more notifications here. Needs no session, so it works
    /// after signing out too.
    pub async fn unregister() {
        if let Some(old) = stored() {
            crate::state::remove_item(KEY);
            if let Err(e) = unregister_notification_device(old.id, old.secret).await {
                leptos::logging::warn!("could not unregister this phone: {e}");
            }
        }
    }

    /// At the app's start: a registered phone registers again when that is due ([`register`]),
    /// and ends its registration when the player took iOS's leave back in its settings.
    pub async fn refresh(locale: Locale) {
        if stored().is_none() {
            return;
        }
        match native::notify_state().await {
            Ok(state) if state.granted => {
                if let Err(e) = register(locale).await {
                    leptos::logging::warn!("could not register this phone again: {e}");
                }
            }
            Ok(_) => unregister().await,
            Err(e) => leptos::logging::warn!("notify_state failed: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept() -> Stored {
        Stored {
            id: 4,
            secret: "secret".into(),
            token: "ab12".repeat(16),
            env: ApnsEnvironment::Production,
            locale: "de".into(),
            at: 1_000.0,
        }
    }

    #[test]
    fn a_phone_registers_again_for_a_new_token_language_or_week() {
        let s = kept();
        let token = "ab12".repeat(16);
        let prod = ApnsEnvironment::Production;
        assert!(due(None, &token, prod, "de", 1_000.0));
        assert!(!due(
            Some(&s),
            &token,
            prod,
            "de",
            1_000.0 + REFRESH_MS - 1.0
        ));
        // iOS's hex in either case is the same token
        assert!(!due(Some(&s), &token.to_uppercase(), prod, "de", 1_001.0));
        assert!(due(Some(&s), &"cd34".repeat(16), prod, "de", 1_001.0));
        assert!(due(
            Some(&s),
            &token,
            ApnsEnvironment::Development,
            "de",
            1_001.0
        ));
        assert!(due(Some(&s), &token, prod, "ja", 1_001.0));
        assert!(due(Some(&s), &token, prod, "de", 1_000.0 + REFRESH_MS));
    }

    /// What the app keeps between starts.
    #[test]
    fn the_registration_is_kept_in_local_storage() {
        assert_eq!(stored(), None);
        crate::state::store(KEY, &kept());
        assert_eq!(stored(), Some(kept()));
        crate::state::remove_item(KEY);
        assert_eq!(stored(), None);
    }
}
