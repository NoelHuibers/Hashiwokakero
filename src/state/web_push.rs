//! Web Push in this browser: whether it can get notifications, its subscription, and its
//! registration with the server (`server::notify::web_push` sends to it).
//!
//! The settings are the account's, but delivery is per device: a browser gets notifications
//! once the player turned them on here (`subscribe`: the browser's permission, the service worker
//! at `/sw.js`, a push subscription for the site's key, `register_notification_device`). Its
//! device id and secret stay in local storage, for turning it off again (also after signing out)
//! and for registering again now and then, which keeps the device from being forgotten
//! (`refresh`).
//!
//! Only on the website: the apps' `WebView`s get their own ways (APNs, the Android worker).

use serde::{Deserialize, Serialize};

use crate::i18n::Locale;

/// Where this browser's registration is kept.
const KEY: &str = "hashi.push.v1";
/// How often a browser registers again: well within the 90 days after which the server forgets
/// a device it hasn't seen (`notify::DEVICE_IDLE_MS`).
const REFRESH_MS: f64 = 7.0 * 86_400_000.0;

/// This browser's registration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stored {
    pub id: i64,
    pub secret: String,
    pub endpoint: String,
    /// The language of its texts.
    pub locale: String,
    /// When it registered, ms since the epoch.
    pub at: f64,
}

/// What this browser can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// It can get notifications.
    Ready,
    /// An iPhone or iPad in Safari: notifications only come to the site added to the home
    /// screen and opened from there (iOS 16.4 and later).
    HomeScreen,
    /// The site doesn't send notifications to browsers (no key), or this browser can't take them.
    No,
}

/// What tells whether a browser can get notifications.
#[derive(Clone, Copy, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "what the page and the browser say, each yes or no"
)]
pub struct Traits {
    /// The page names the site's key: the site sends notifications to browsers.
    pub key: bool,
    /// A service worker, `PushManager` and `Notification`.
    pub push_api: bool,
    /// An iPhone or iPad.
    pub apple_mobile: bool,
    /// The site runs as an app from the home screen.
    pub standalone: bool,
}

impl Traits {
    /// What a browser with these traits can do.
    pub const fn support(self) -> Support {
        if !self.key {
            Support::No
        } else if self.push_api {
            Support::Ready
        } else if self.apple_mobile && !self.standalone {
            Support::HomeScreen
        } else {
            Support::No
        }
    }
}

/// What this browser can do. Never in the apps; not known on the server, so pages ask after
/// hydrating.
pub fn here() -> Support {
    #[cfg(feature = "hydrate")]
    {
        browser::support_here()
    }
    #[cfg(not(feature = "hydrate"))]
    {
        Support::No
    }
}

/// This browser's registration, if it has one.
pub fn stored() -> Option<Stored> {
    super::load(KEY)
}

/// Whether a registration is due again: a week old, or for another language than the site's now.
pub fn due(stored: &Stored, locale: &str, now: f64) -> bool {
    stored.locale != locale || now - stored.at >= REFRESH_MS
}

/// How turning notifications on in this browser went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    On,
    /// The player blocked them: the browser won't ask again, only its site settings change it.
    Denied,
    /// The prompt closed without an answer.
    Dismissed,
    /// Something else failed: no service worker, no push service, no server.
    Failed(String),
}

/// Whether this browser gets notifications: registered and still allowed to.
pub fn is_on() -> bool {
    #[cfg(feature = "hydrate")]
    {
        stored().is_some() && browser::permission().as_deref() == Some("granted")
    }
    #[cfg(not(feature = "hydrate"))]
    {
        false
    }
}

/// Whether the player blocked notifications for the site in this browser.
pub fn blocked() -> bool {
    #[cfg(feature = "hydrate")]
    {
        browser::permission().as_deref() == Some("denied")
    }
    #[cfg(not(feature = "hydrate"))]
    {
        false
    }
}

/// Turns notifications on in this browser, for the signed in player.
///
/// Asks for permission (call it from a tap: browsers only ask then), registers the service
/// worker, subscribes with the site's key and registers the subscription.
#[cfg_attr(
    feature = "hydrate",
    expect(clippy::future_not_send, reason = "the browser's promises")
)]
#[cfg_attr(
    not(feature = "hydrate"),
    expect(clippy::unused_async, reason = "only the browser awaits")
)]
pub async fn subscribe(locale: Locale) -> Outcome {
    #[cfg(feature = "hydrate")]
    {
        browser::subscribe(locale).await
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = locale;
        Outcome::Failed("no browser".into())
    }
}

/// Turns notifications off in this browser: the server forgets it, the subscription ends.
///
/// Needs no session, so it works after signing out too.
#[cfg_attr(
    feature = "hydrate",
    expect(clippy::future_not_send, reason = "the browser's promises")
)]
#[cfg_attr(
    not(feature = "hydrate"),
    expect(clippy::unused_async, reason = "only the browser awaits")
)]
pub async fn unsubscribe() {
    #[cfg(feature = "hydrate")]
    browser::unsubscribe().await;
}

/// Registers this browser again when that is due (see [`due`]), with the subscription it has.
///
/// A subscription that is gone (the player cleared the site's data, revoked the permission) ends
/// the registration.
#[cfg_attr(
    feature = "hydrate",
    expect(clippy::future_not_send, reason = "the browser's promises")
)]
#[cfg_attr(
    not(feature = "hydrate"),
    expect(clippy::unused_async, reason = "only the browser awaits")
)]
pub async fn refresh(locale: Locale) {
    #[cfg(feature = "hydrate")]
    browser::refresh(locale).await;
    #[cfg(not(feature = "hydrate"))]
    let _ = locale;
}

/// base64url without padding, as the page names the key, into bytes.
pub fn base64url(text: &str) -> Option<Vec<u8>> {
    let mut bits = 0u32;
    let mut count = 0;
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for c in text.trim_end_matches('=').bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push(u8::try_from((bits >> count) & 0xff).ok()?);
        }
    }
    Some(out)
}

#[cfg(feature = "hydrate")]
#[expect(
    clippy::future_not_send,
    reason = "the browser's promises, awaited in the page's task"
)]
mod browser {
    use js_sys::{Promise, Reflect};
    use leptos::prelude::{document, window};
    use leptos_i18n::Locale as _;
    use wasm_bindgen::{JsCast as _, JsValue};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{PushManager, PushSubscription, PushSubscriptionOptionsInit};

    use super::{KEY, Outcome, Stored, Support, Traits, base64url, due, stored};
    use crate::api::{register_notification_device, unregister_notification_device};
    use crate::i18n::Locale;
    use crate::model::DeviceKind;
    use crate::state::notify::time_zone;

    /// The site's key, as the page names it (`app::shell`).
    fn push_key() -> Option<String> {
        document()
            .query_selector("meta[name=hashi-push-key]")
            .ok()
            .flatten()
            .and_then(|meta| meta.get_attribute("content"))
            .filter(|key| !key.is_empty())
    }

    fn has(target: &JsValue, name: &str) -> bool {
        Reflect::has(target, &name.into()).unwrap_or(false)
    }

    pub(super) fn support_here() -> Support {
        let window = window();
        let navigator = window.navigator();
        let push_api = has(&navigator, "serviceWorker")
            && has(&window, "PushManager")
            && has(&window, "Notification");
        let agent = navigator.user_agent().unwrap_or_default();
        // iPadOS asks for the desktop site: a Mac with a touch screen
        let apple_mobile = ["iPhone", "iPad", "iPod"].iter().any(|d| agent.contains(d))
            || (agent.contains("Macintosh") && navigator.max_touch_points() > 1);
        let standalone = Reflect::get(&navigator, &"standalone".into())
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || window
                .match_media("(display-mode: standalone)")
                .ok()
                .flatten()
                .is_some_and(|m| m.matches());
        Traits {
            key: push_key().is_some(),
            push_api,
            apple_mobile,
            standalone,
        }
        .support()
    }

    /// `Notification.permission`: granted, denied or default.
    pub(super) fn permission() -> Option<String> {
        let notification = Reflect::get(&window(), &"Notification".into()).ok()?;
        Reflect::get(&notification, &"permission".into())
            .ok()?
            .as_string()
    }

    async fn wait(promise: Result<Promise, JsValue>) -> Result<JsValue, JsValue> {
        JsFuture::from(promise?).await
    }

    fn text(error: &JsValue) -> String {
        error
            .dyn_ref::<js_sys::Error>()
            .map_or_else(|| format!("{error:?}"), |e| String::from(e.message()))
    }

    /// The push manager of the service worker, registering it first if `register`.
    async fn push_manager(register: bool) -> Result<Option<PushManager>, JsValue> {
        let container = window().navigator().service_worker();
        let registration = if register {
            JsFuture::from(container.register("/sw.js")).await?;
            // Subscribing needs an active worker
            wait(container.ready()).await?
        } else {
            JsFuture::from(container.get_registration()).await?
        };
        if registration.is_undefined() {
            return Ok(None);
        }
        registration
            .unchecked_into::<web_sys::ServiceWorkerRegistration>()
            .push_manager()
            .map(Some)
    }

    async fn subscription(manager: &PushManager) -> Result<Option<PushSubscription>, JsValue> {
        let existing = wait(manager.get_subscription()).await?;
        Ok((!existing.is_null() && !existing.is_undefined()).then(|| existing.unchecked_into()))
    }

    /// The endpoint and the keys (JSON) of a subscription.
    fn details(subscription: &PushSubscription) -> Result<(String, String), JsValue> {
        let json: serde_json::Value =
            serde_json::from_str(&String::from(js_sys::JSON::stringify(subscription)?))
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let keys = json["keys"].to_string();
        Ok((subscription.endpoint(), keys))
    }

    async fn register(locale: Locale, subscription: &PushSubscription) -> Result<(), String> {
        let (endpoint, keys) = details(subscription).map_err(|e| text(&e))?;
        let registration = register_notification_device(
            DeviceKind::Web,
            Some(endpoint.clone()),
            Some(keys),
            locale.as_str().to_string(),
            time_zone(),
        )
        .await
        .map_err(|e| e.to_string())?;
        // A registration of another subscription before (an endpoint the browser changed)
        if let Some(old) = stored()
            && old.id != registration.id
        {
            let _ = unregister_notification_device(old.id, old.secret).await;
        }
        crate::state::store(
            KEY,
            &Stored {
                id: registration.id,
                secret: registration.secret,
                endpoint,
                locale: locale.as_str().to_string(),
                at: js_sys::Date::now(),
            },
        );
        Ok(())
    }

    async fn subscribe_or_fail(locale: Locale) -> Result<Outcome, JsValue> {
        let key = push_key()
            .and_then(|k| base64url(&k))
            .ok_or_else(|| JsValue::from_str("the site has no push key"))?;
        let answer = wait(web_sys::Notification::request_permission()).await?;
        match answer.as_string().as_deref() {
            Some("granted") => {}
            Some("denied") => return Ok(Outcome::Denied),
            _ => return Ok(Outcome::Dismissed),
        }
        let manager = push_manager(true)
            .await?
            .ok_or_else(|| JsValue::from_str("no service worker"))?;
        let options = PushSubscriptionOptionsInit::new();
        options.set_user_visible_only(true);
        options.set_application_server_key_opt_u8_array(Some(&js_sys::Uint8Array::from(
            key.as_slice(),
        )));
        let subscription = match wait(manager.subscribe_with_options(&options)).await {
            Ok(subscription) => subscription,
            // A subscription for another key (the site's earlier one) is in the way: replaced
            Err(e) => match subscription(&manager).await? {
                Some(old) => {
                    wait(old.unsubscribe()).await?;
                    wait(manager.subscribe_with_options(&options)).await?
                }
                None => return Err(e),
            },
        };
        match register(locale, &subscription.unchecked_into()).await {
            Ok(()) => Ok(Outcome::On),
            Err(e) => Ok(Outcome::Failed(e)),
        }
    }

    pub(super) async fn subscribe(locale: Locale) -> Outcome {
        subscribe_or_fail(locale)
            .await
            .unwrap_or_else(|e| Outcome::Failed(text(&e)))
    }

    pub(super) async fn unsubscribe() {
        if let Some(old) = stored() {
            crate::state::remove_item(KEY);
            if let Err(e) = unregister_notification_device(old.id, old.secret).await {
                leptos::logging::warn!("could not unregister this browser: {e}");
            }
        }
        let ended = async {
            if let Some(manager) = push_manager(false).await?
                && let Some(subscription) = subscription(&manager).await?
            {
                wait(subscription.unsubscribe()).await?;
            }
            Ok::<_, JsValue>(())
        };
        if let Err(e) = ended.await {
            leptos::logging::warn!("could not end the push subscription: {}", text(&e));
        }
    }

    pub(super) async fn refresh(locale: Locale) {
        let Some(old) = stored() else {
            return;
        };
        if !due(&old, locale.as_str(), js_sys::Date::now()) {
            return;
        }
        let current = async {
            match push_manager(false).await? {
                Some(manager) => subscription(&manager).await,
                None => Ok(None),
            }
        };
        match current.await {
            Ok(Some(subscription)) if permission().as_deref() == Some("granted") => {
                if let Err(e) = register(locale, &subscription).await {
                    leptos::logging::warn!("could not register this browser again: {e}");
                }
            }
            Ok(_) => unsubscribe().await,
            Err(e) => leptos::logging::warn!("no push subscription: {}", text(&e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browsers_get_notifications_where_they_can() {
        let traits = |key, push_api, apple_mobile, standalone| Traits {
            key,
            push_api,
            apple_mobile,
            standalone,
        };
        // No key: the site doesn't send them to browsers
        for (push, apple, standalone) in [(true, false, false), (false, true, false)] {
            assert_eq!(
                traits(false, push, apple, standalone).support(),
                Support::No
            );
        }
        assert_eq!(traits(true, true, false, false).support(), Support::Ready);
        // A home screen web app on iOS 16.4 has the push API
        assert_eq!(traits(true, true, true, true).support(), Support::Ready);
        // Safari's tabs on iOS don't: the home screen does
        assert_eq!(
            traits(true, false, true, false).support(),
            Support::HomeScreen
        );
        // An older iOS on the home screen, an old browser elsewhere
        assert_eq!(traits(true, false, true, true).support(), Support::No);
        assert_eq!(traits(true, false, false, false).support(), Support::No);
        // Never outside the website's browser code: the server, the apps
        assert_eq!(here(), Support::No);
        assert!(!is_on() && !blocked());
    }

    #[test]
    fn a_browser_registers_again_weekly_and_in_a_new_language() {
        let stored = Stored {
            id: 1,
            secret: "s".into(),
            endpoint: "https://push.example/1".into(),
            locale: "de".into(),
            at: 1_000.0,
        };
        assert!(!due(&stored, "de", 1_000.0 + REFRESH_MS - 1.0));
        assert!(due(&stored, "de", 1_000.0 + REFRESH_MS));
        assert!(due(&stored, "ja", 1_001.0));
    }

    #[test]
    fn the_key_decodes_from_base64url() {
        let key = "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
        let bytes = base64url(key).unwrap();
        assert_eq!(bytes.len(), 65);
        assert_eq!(bytes[0], 4);
        assert_eq!(bytes[64], 0x0f);
        assert_eq!(base64url("aGFzaGk").unwrap(), b"hashi");
        assert_eq!(base64url("aGFzaGk=").unwrap(), b"hashi");
        assert_eq!(base64url("a+b/").as_deref(), None);
    }

    /// What a browser keeps between visits.
    #[test]
    fn the_registration_is_kept_in_local_storage() {
        assert_eq!(stored(), None);
        let kept = Stored {
            id: 4,
            secret: "secret".into(),
            endpoint: "https://push.example/4".into(),
            locale: "en".into(),
            at: 5.0,
        };
        crate::state::store(KEY, &kept);
        assert_eq!(stored(), Some(kept));
        crate::state::remove_item(KEY);
        assert_eq!(stored(), None);
    }
}
