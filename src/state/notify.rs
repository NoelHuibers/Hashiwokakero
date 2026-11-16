//! Notifications on this device: whether they can reach it at all, and its time zone for the
//! streak's last call. The settings themselves are the account's (`server::notify`).

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

/// Whether a way of delivery is built, and works here.
///
/// Each one flips with its own change (docs/notifications.md): Web Push where the site has its
/// key and the browser can take them (`state::web_push`, never in the apps), APNs with the iOS
/// app's push registration, the Android app with its background worker.
pub fn delivers(kind: DeviceKind) -> bool {
    match kind {
        DeviceKind::Web => super::web_push::here() == super::web_push::Support::Ready,
        DeviceKind::Apns | DeviceKind::Android => false,
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

    /// Nothing delivers yet: no offer anywhere, and the settings say so.
    #[test]
    fn no_way_of_delivery_is_built_yet() {
        for kind in DeviceKind::ALL {
            assert!(!delivers(kind), "{kind:?}");
        }
        assert!(!can_deliver());
    }
}
