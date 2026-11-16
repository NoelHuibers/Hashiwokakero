//! Notifications on this device: whether they can reach it at all, and its time zone for the
//! streak's last call. The settings themselves are the account's (`server::notify`).

use crate::i18n::Locale;
use crate::model::DeviceKind;

/// How notifications would reach this device.
///
/// The website through Web Push, the iOS app through Apple's push service, the Android app by
/// asking the server itself. `app_platform`: the app's platform (`api::app_identity`), `None` on
/// the website. The desktop apps get none.
pub fn device_kind(app_platform: Option<&str>) -> Option<DeviceKind> {
    match app_platform {
        None => Some(DeviceKind::Web),
        Some("ios") => Some(DeviceKind::Apns),
        Some("android") => Some(DeviceKind::Android),
        Some(_) => None,
    }
}

/// Whether a way of delivery is built.
///
/// Each one flips with its own change: Web Push with the website's service worker, APNs with the
/// iOS app's push registration (docs/notifications.md). The Android app asks the server itself,
/// in the background (`Notifications.kt` in `app/src-tauri/gen/android`).
pub const fn delivers(kind: DeviceKind) -> bool {
    match kind {
        DeviceKind::Android => true,
        DeviceKind::Web | DeviceKind::Apns => false,
    }
}

/// Whether notifications can reach this device: only then does a finished daily offer them, and
/// the settings say so where they can't.
pub fn can_deliver() -> bool {
    device_kind(crate::api::app_identity().map(|(platform, _)| platform)).is_some_and(delivers)
}

/// The IANA time zone of this browser or phone (`Europe/Berlin`), `None` where it can't tell.
pub fn time_zone() -> Option<String> {
    #[cfg(feature = "browser")]
    {
        let format =
            js_sys::Intl::DateTimeFormat::new(&js_sys::Array::new(), &js_sys::Object::new());
        js_sys::Reflect::get(&format.resolved_options(), &"timeZone".into())
            .ok()
            .and_then(|zone| zone.as_string())
            .filter(|zone| !zone.is_empty())
    }
    #[cfg(not(feature = "browser"))]
    {
        None
    }
}

/// Whether this is the Android app, which asks for leave to show notifications and polls the
/// server itself.
#[cfg(any(feature = "csr", test))]
fn android() -> bool {
    device_kind(crate::api::app_identity().map(|(platform, _)| platform))
        == Some(DeviceKind::Android)
}

/// Asks for leave to show notifications on this device, where it needs it.
///
/// In the Android app (13 and newer ask the player, on a tap that turns them on). `false` when
/// refused: the page then says where to allow them.
#[cfg_attr(
    feature = "csr",
    expect(
        clippy::future_not_send,
        reason = "the app's plugin, on the page's one thread"
    )
)]
pub async fn allowed_here() -> bool {
    #[cfg(feature = "csr")]
    if android() {
        return crate::api::native::notify_permission().await;
    }
    true
}

/// Whether this device refuses notifications although they are on (`enabled`): the Android
/// app without Android's leave, which the player took back in its settings or never gave.
#[cfg_attr(
    feature = "csr",
    expect(clippy::future_not_send, reason = "as `allowed_here`")
)]
pub async fn refused_here(enabled: bool) -> bool {
    #[cfg(feature = "csr")]
    if enabled && android() {
        return crate::api::native::notify_state()
            .await
            .is_ok_and(|state| !state.granted);
    }
    let _ = enabled;
    false
}

/// Makes this device get the player's notifications while they are on, or no more.
///
/// `enabled`: the saved master switch. On, the Android app registers once with the server, in the
/// page's `locale`, and hands the phone's id and secret to its background poll; off, it stops the
/// poll and has the server forget the phone. Only with Android's leave: without it, nothing
/// changes.
#[cfg_attr(
    feature = "csr",
    expect(clippy::future_not_send, reason = "as `allowed_here`")
)]
pub async fn follow(enabled: bool, locale: Locale) -> Result<(), leptos::prelude::ServerFnError> {
    #[cfg(feature = "csr")]
    if android() {
        use leptos_i18n::Locale as _;

        use crate::api::native;
        let state = native::notify_state().await?;
        if !enabled {
            if state.device.is_some() {
                native::notify_stop().await?;
            }
        } else if state.device.is_none() && state.granted {
            let phone = crate::api::register_notification_device(
                DeviceKind::Android,
                None,
                None,
                locale.as_str().to_string(),
                time_zone(),
            )
            .await?;
            native::notify_start(&phone).await?;
        }
        return Ok(());
    }
    let _ = (enabled, locale);
    Ok(())
}

/// Signing out: this device gets no more notifications of the account (the Android app stops
/// its poll and has the server forget the phone).
pub fn forget_here() {
    #[cfg(feature = "csr")]
    if android() {
        leptos::task::spawn_local(async {
            if let Err(e) = crate::api::native::notify_stop().await {
                leptos::logging::warn!("could not stop the notifications: {e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_platform_has_its_way() {
        assert_eq!(device_kind(None), Some(DeviceKind::Web));
        assert_eq!(device_kind(Some("ios")), Some(DeviceKind::Apns));
        assert_eq!(device_kind(Some("android")), Some(DeviceKind::Android));
        assert_eq!(device_kind(Some("msstore")), None);
        assert_eq!(device_kind(Some("web")), None);
    }

    /// The Android app delivers; outside the app nothing does yet: no offer, and the settings
    /// say so.
    #[test]
    fn only_the_android_app_delivers_yet() {
        assert!(delivers(DeviceKind::Android));
        for kind in [DeviceKind::Web, DeviceKind::Apns] {
            assert!(!delivers(kind), "{kind:?}");
        }
        assert!(!can_deliver());
        assert!(!android());
    }
}
