//! Notifications: the player's settings, the one-time offer, and the devices that get them (see
//! `server::notify`).

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use super::HashiClient;
use crate::model::{DeviceKind, DeviceRegistration, NotificationPrefs, PushMessage};
#[cfg(feature = "ssr")]
use crate::server::{auth, db, notify, now_ms};

#[cfg(feature = "ssr")]
async fn signed_in() -> Result<(libsql::Connection, auth::User), ServerFnError> {
    let conn = db::conn()?;
    let user = auth::current_user(&conn)
        .await?
        .ok_or_else(|| crate::model::ApiError::SessionExpired.fail())?;
    Ok((conn, user))
}

/// The current player's notification settings, `None` without a session.
#[server(input = Json, client = HashiClient)]
pub async fn load_notification_prefs() -> Result<Option<NotificationPrefs>, ServerFnError> {
    let conn = db::conn()?;
    match auth::current_user(&conn).await? {
        Some(user) => notify::prefs(&conn, user.id).await.map(Some),
        None => Ok(None),
    }
}

/// Saves the current player's notification settings, with the time zone of their browser or app
/// (IANA, `Europe/Berlin`) for the streak's last call. Returns them as stored.
#[server(input = Json, client = HashiClient)]
pub async fn save_notification_prefs(
    prefs: NotificationPrefs,
    time_zone: Option<String>,
) -> Result<NotificationPrefs, ServerFnError> {
    let (conn, user) = signed_in().await?;
    notify::save_prefs(&conn, user.id, prefs, time_zone.as_deref(), now_ms()).await
}

/// The answer to the offer after a finished daily: yes turns every notification on, either way
/// it doesn't come again.
#[server(input = Json, client = HashiClient)]
pub async fn answer_notification_offer(
    accept: bool,
    time_zone: Option<String>,
) -> Result<NotificationPrefs, ServerFnError> {
    let (conn, user) = signed_in().await?;
    notify::answer_offer(&conn, user.id, accept, time_zone.as_deref(), now_ms()).await
}

/// Registers this browser or app for the current player's notifications: `endpoint` and `keys`
/// of a Web Push subscription, the APNs token as `endpoint` on iOS, neither on Android. `locale`:
/// the language of the texts. The secret it returns is the device's to keep.
#[server(input = Json, client = HashiClient)]
pub async fn register_notification_device(
    kind: DeviceKind,
    endpoint: Option<String>,
    keys: Option<String>,
    locale: String,
    time_zone: Option<String>,
) -> Result<DeviceRegistration, ServerFnError> {
    let (conn, user) = signed_in().await?;
    notify::register_device(
        &conn,
        user.id,
        kind,
        endpoint.as_deref(),
        keys.as_deref(),
        notify::locale_of(&locale),
        time_zone.as_deref(),
        now_ms(),
    )
    .await
}

/// Forgets a device, by its id and secret (no session needed: also after signing out).
#[server(input = Json, client = HashiClient)]
pub async fn unregister_notification_device(id: i64, secret: String) -> Result<(), ServerFnError> {
    let conn = db::conn()?;
    notify::unregister_device(&conn, id, &secret).await?;
    Ok(())
}

/// The Android app asks for its new notifications, by its id and secret (in the background,
/// without a session). `None`: the device isn't registered (any more).
#[server(input = Json, client = HashiClient)]
pub async fn poll_notifications(
    device: i64,
    secret: String,
) -> Result<Option<Vec<PushMessage>>, ServerFnError> {
    let conn = db::conn()?;
    notify::poll(&conn, device, &secret, now_ms()).await
}
