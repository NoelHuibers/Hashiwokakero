//! Browser helpers: local storage, the device id, random seeds, downloads and the game clock.

use leptos::prelude::*;
use serde::{Serialize, de::DeserializeOwned};

/// The browser's local storage.
#[cfg(not(test))]
mod storage {
    use leptos::prelude::window;

    fn storage() -> Option<web_sys::Storage> {
        window().local_storage().ok().flatten()
    }

    pub(super) fn get(key: &str) -> Option<String> {
        storage()?.get_item(key).ok()?
    }

    pub(super) fn set(key: &str, value: &str) {
        if let Some(s) = storage() {
            let _ = s.set_item(key, value);
        }
    }

    #[cfg(feature = "csr")]
    pub(super) fn remove(key: &str) {
        if let Some(s) = storage() {
            let _ = s.remove_item(key);
        }
    }
}

/// The unit tests run natively, without a browser: each test thread gets a local storage of its
/// own, so what the client saves and restores can be tested.
#[cfg(test)]
pub(crate) mod storage {
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        static ITEMS: RefCell<HashMap<String, String>> = RefCell::default();
    }

    pub(crate) fn get(key: &str) -> Option<String> {
        ITEMS.with_borrow(|items| items.get(key).cloned())
    }

    pub(crate) fn set(key: &str, value: &str) {
        ITEMS.with_borrow_mut(|items| items.insert(key.to_string(), value.to_string()));
    }

    pub(crate) fn remove(key: &str) {
        ITEMS.with_borrow_mut(|items| items.remove(key));
    }
}

pub fn load<T: DeserializeOwned>(key: &str) -> Option<T> {
    serde_json::from_str(&storage::get(key)?).ok()
}

pub fn store<T: Serialize>(key: &str, value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        storage::set(key, &json);
    }
}

/// A value kept as it is, not as JSON: the theme (read by the page's script before the WASM
/// runs), the app's session token.
pub(crate) fn store_raw(key: &str, value: &str) {
    storage::set(key, value);
}

#[cfg(any(feature = "csr", test))]
pub(crate) fn load_raw(key: &str) -> Option<String> {
    storage::get(key)
}

#[cfg(any(feature = "csr", test))]
pub(crate) fn remove_item(key: &str) {
    storage::remove(key);
}

/// A random id this browser keeps, sent when a daily starts: a second account on the same device
/// isn't ranked (the server only stores it hashed per day).
pub fn device_id() -> String {
    const KEY: &str = "hashi.device";
    if let Some(id) = load::<String>(KEY).filter(|id| id.len() >= 16) {
        return id;
    }
    let id = format!("{:016x}{:016x}", random_seed(), random_seed());
    store(KEY, &id);
    id
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`Math.random()` is in [0, 1), so the first value fits in 32 bits, and `Date.now()` \
              is a positive whole number of ms"
)]
pub fn random_seed() -> u64 {
    let a = (js_sys::Math::random() * 4_294_967_296.0) as u64;
    let b = js_sys::Date::now() as u64;
    (a << 32) ^ b
}

/// Offers `content` as a file download.
///
/// TODO(step 10 of docs/mobile.md): the app's `WebView` ignores downloads of Blob links, it
/// needs a download handler (or Tauri's dialog and fs plugins).
pub fn download(filename: &str, content: &str, mime: &str) {
    use wasm_bindgen::JsCast;
    let parts = js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(content));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    if let Ok(link) = document().create_element("a")
        && let Ok(link) = link.dyn_into::<web_sys::HtmlAnchorElement>()
    {
        link.set_href(&url);
        link.set_download(filename);
        link.click();
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

pub fn format_time(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_show_hours_only_when_there_are_some() {
        assert_eq!(format_time(0), "00:00");
        assert_eq!(format_time(61), "01:01");
        assert_eq!(format_time(3599), "59:59");
        assert_eq!(format_time(3600), "1:00:00");
        assert_eq!(format_time(36_000 + 62), "10:01:02");
    }

    #[test]
    fn stored_values_load_back() {
        store("test.value", &vec![1, 2, 3]);
        assert_eq!(load::<Vec<u8>>("test.value"), Some(vec![1, 2, 3]));
        // Of another type, missing or not JSON: nothing
        assert_eq!(load::<String>("test.value"), None);
        assert_eq!(load::<Vec<u8>>("test.missing"), None);
        store_raw("test.raw", "dark");
        assert_eq!(load::<String>("test.raw"), None);
        assert_eq!(load_raw("test.raw").as_deref(), Some("dark"));
        remove_item("test.raw");
        assert_eq!(load_raw("test.raw"), None);
    }

    /// The device id is made once, then the kept one is sent with every daily.
    #[test]
    fn a_kept_device_id_is_reused() {
        store("hashi.device", &"0123456789abcdef0123");
        assert_eq!(device_id(), "0123456789abcdef0123");
    }
}
