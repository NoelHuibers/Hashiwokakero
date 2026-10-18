//! Browser helpers: local storage, the device id, random seeds, downloads and the game clock.

use leptos::prelude::*;
use serde::{Serialize, de::DeserializeOwned};

fn storage() -> Option<web_sys::Storage> {
    window().local_storage().ok().flatten()
}

pub fn load<T: DeserializeOwned>(key: &str) -> Option<T> {
    let raw = storage()?.get_item(key).ok()??;
    serde_json::from_str(&raw).ok()
}

pub fn store<T: Serialize>(key: &str, value: &T) {
    if let (Some(s), Ok(json)) = (storage(), serde_json::to_string(value)) {
        let _ = s.set_item(key, &json);
    }
}

pub(super) fn store_raw(key: &str, value: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(key, value);
    }
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
