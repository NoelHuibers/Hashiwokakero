//! APNs: notifications to the iOS app, through Apple's push service.
//!
//! Straight from our server to Apple, no third party in between: an HTTP/2 request per message
//! to the service the device's token is for (`api.push.apple.com` for the App Store and
//! TestFlight, `api.sandbox.push.apple.com` for builds signed for development; the app registers
//! which, [`ApnsEnvironment`]), signed with a provider token (an ES256 JWT of our key, renewed
//! every [`RENEW_AFTER_S`]).
//!
//! The key comes from the environment: `APNS_KEY_P8` (the text of the key's `.p8` file, a P-256
//! key in PKCS #8, PEM), `APNS_KEY_ID` (its id, 10 characters) and `APNS_TEAM_ID` (the team's, 10
//! characters), and `APNS_TOPIC` (the app's bundle id, `rs.hashi` by default). Without them the
//! channel is off; half of them or a key that doesn't parse stops the server at startup
//! (`validate_configuration`).

use std::sync::Mutex;
use std::time::Duration;

use base64ct::{Base64UrlUnpadded, Encoding as _};
use futures::future::BoxFuture;
use p256::ecdsa::signature::Signer as _;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::DecodePrivateKey as _;
use sha2::{Digest as _, Sha256};

use super::{Channel, Delivery, Device};
use crate::model::{ApnsEnvironment, DeviceKind, NotifyKind, PushMessage};

/// The topic without `APNS_TOPIC`: the app's bundle id.
pub const DEFAULT_TOPIC: &str = "rs.hashi";
/// Apple's push service for the App Store's and TestFlight's apps.
pub const PRODUCTION: &str = "https://api.push.apple.com";
/// Apple's push service for apps signed for development.
pub const SANDBOX: &str = "https://api.sandbox.push.apple.com";
/// The most a notification's payload may have.
const MAX_PAYLOAD: usize = 4096;
/// The most an `apns-collapse-id` may have.
const MAX_COLLAPSE_ID: usize = 64;
/// How old a provider token gets before we make a new one. Apple takes one for an hour and
/// refuses new ones more often than every 20 minutes (`TooManyProviderTokenUpdates`).
pub const RENEW_AFTER_S: i64 = 40 * 60;
/// No new provider token before this, also when Apple refused the one we have: a fresh one it
/// refuses has a wrong key, team or key id, which a new one wouldn't change.
pub const RENEW_NOT_BEFORE_S: i64 = 20 * 60;
/// How long a message stays with Apple for a phone that's offline, at most.
const MAX_STORED_S: i64 = 28 * 24 * 3600;

fn encode(bytes: &[u8]) -> String {
    Base64UrlUnpadded::encode_string(bytes)
}

/// Our key with its id, the team and the app's topic.
pub struct Credentials {
    key: SigningKey,
    key_id: String,
    team_id: String,
    topic: String,
}

/// Without the key.
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("key_id", &self.key_id)
            .field("team_id", &self.team_id)
            .field("topic", &self.topic)
            .finish_non_exhaustive()
    }
}

/// Apple's ids: 10 letters and digits.
fn apple_id(value: &str) -> bool {
    value.len() == 10 && value.bytes().all(|b| b.is_ascii_alphanumeric())
}

impl Credentials {
    /// From the `.p8` file's text (newlines as `\n` work too: some secret stores keep one line),
    /// its key id, the team id and the topic.
    pub fn new(p8: &str, key_id: &str, team_id: &str, topic: &str) -> Result<Self, String> {
        let pem = p8.trim().replace("\\n", "\n");
        let key = SigningKey::from_pkcs8_pem(&pem)
            .map_err(|_| "APNS_KEY_P8 is not the text of a .p8 file (a P-256 key in PKCS #8)")?;
        let (key_id, team_id, topic) = (key_id.trim(), team_id.trim(), topic.trim());
        if !apple_id(key_id) {
            return Err("APNS_KEY_ID is not a key id (10 letters and digits)".into());
        }
        if !apple_id(team_id) {
            return Err("APNS_TEAM_ID is not a team id (10 letters and digits)".into());
        }
        if topic.is_empty() || topic.contains(char::is_whitespace) {
            return Err("APNS_TOPIC is not a bundle id".into());
        }
        Ok(Self {
            key,
            key_id: key_id.to_string(),
            team_id: team_id.to_string(),
            topic: topic.to_string(),
        })
    }

    /// The key of the environment: `None` without it (APNs is off), an error for part of it or a
    /// broken one (`validate_configuration` stops the server).
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
        let set = |name| var(name).filter(|v| !v.trim().is_empty());
        match (set("APNS_KEY_P8"), set("APNS_KEY_ID"), set("APNS_TEAM_ID")) {
            (None, None, None) => Ok(None),
            (Some(p8), Some(key_id), Some(team_id)) => Self::new(
                &p8,
                &key_id,
                &team_id,
                &set("APNS_TOPIC").unwrap_or_else(|| DEFAULT_TOPIC.to_string()),
            )
            .map(Some),
            _ => Err("APNS_KEY_P8, APNS_KEY_ID and APNS_TEAM_ID go together".into()),
        }
    }

    /// The provider token issued at `iat` (s): `kid` our key's id, `iss` our team.
    fn jwt(&self, iat: i64) -> String {
        let header = encode(
            serde_json::json!({ "alg": "ES256", "kid": self.key_id })
                .to_string()
                .as_bytes(),
        );
        let claims = encode(
            serde_json::json!({ "iss": self.team_id, "iat": iat })
                .to_string()
                .as_bytes(),
        );
        let signed = format!("{header}.{claims}");
        // ES256 in a JWS: r and s, 32 bytes each (RFC 7518 section 3.4)
        let signature: Signature = self.key.sign(signed.as_bytes());
        format!("{signed}.{}", encode(&signature.to_bytes()))
    }
}

/// The provider token in use and when it was made: one for every request until it is
/// [`RENEW_AFTER_S`] old. No `Debug`: the token is a credential for an hour.
#[derive(Default)]
struct Tokens {
    current: Option<(String, i64)>,
}

impl Tokens {
    /// The token for a request at `now` (s), a new one when it's due.
    fn get(&mut self, credentials: &Credentials, now: i64) -> String {
        match &self.current {
            Some((token, issued)) if now - issued < RENEW_AFTER_S && now >= *issued => {
                token.clone()
            }
            _ => {
                let token = credentials.jwt(now);
                self.current = Some((token.clone(), now));
                token
            }
        }
    }

    /// Apple refused the token: the next request gets a new one, if this one is old enough.
    fn refused(&mut self, now: i64) {
        if self
            .current
            .as_ref()
            .is_some_and(|(_, issued)| now - issued >= RENEW_NOT_BEFORE_S)
        {
            self.current = None;
        }
    }
}

/// What the app gets (`aps` for iOS, our `url` for the tap). `thread-id` groups a kind's
/// notifications in the Notification Centre.
fn payload(message: &PushMessage) -> String {
    serde_json::json!({
        "aps": {
            "alert": { "title": message.title, "body": message.body },
            "sound": "default",
            "thread-id": message.kind,
        },
        "url": message.url,
    })
    .to_string()
}

/// The tag as an `apns-collapse-id`: as it is when short enough (ours are), else a hash of it.
fn collapse_id(tag: &str) -> String {
    if tag.len() <= MAX_COLLAPSE_ID {
        tag.to_string()
    } else {
        let hex: String = Sha256::digest(tag.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        hex[..MAX_COLLAPSE_ID].to_string()
    }
}

/// A request to Apple's push service, before it goes out (without the provider token).
#[derive(Debug)]
pub struct Request {
    pub environment: ApnsEnvironment,
    /// `/3/device/<token>`
    pub path: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

/// Builds the request that pushes `message` to an iPhone.
///
/// Not for a device that can't get any (a token that isn't hex, keys that don't say which
/// service it is for: nothing a later try would fix): [`Delivery::Gone`]; nor for a message too
/// long for APNs: [`Delivery::Failed`].
///
/// `apns-expiration`: until the notification expires (the daily's change, a race's lobby), so a
/// phone that comes online later doesn't get it after that. `apns-priority`: 10 (at once) for the
/// last call, 5 (when it suits the phone's battery) for the rest. `apns-collapse-id`: the tag, so
/// a newer one replaces the shown one (the count of a shared puzzle's solvers).
pub fn request(
    topic: &str,
    device: &Device,
    message: &PushMessage,
    now_ms: i64,
) -> Result<Request, Delivery> {
    let token = device
        .endpoint
        .as_deref()
        .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or(Delivery::Gone)?;
    let environment = device
        .keys
        .as_deref()
        .and_then(ApnsEnvironment::of_keys)
        .ok_or(Delivery::Gone)?;
    let body = payload(message);
    if body.len() > MAX_PAYLOAD {
        return Err(Delivery::Failed);
    }
    let now = now_ms.div_euclid(1000);
    let expiration = message
        .expires_at
        .div_euclid(1000)
        .clamp(now, now + MAX_STORED_S);
    let priority = if message.kind == NotifyKind::LastCall.id() {
        "10"
    } else {
        "5"
    };
    Ok(Request {
        environment,
        path: format!("/3/device/{}", token.to_ascii_lowercase()),
        headers: vec![
            ("apns-topic", topic.to_string()),
            ("apns-push-type", "alert".into()),
            ("apns-priority", priority.into()),
            ("apns-expiration", expiration.to_string()),
            ("apns-collapse-id", collapse_id(&message.tag)),
            ("content-type", "application/json".into()),
        ],
        body,
    })
}

/// What Apple's answer means: its status and the `reason` of its JSON body.
///
/// 200 it took it; 410 (`Unregistered`: the app was deleted, or turned notifications off for
/// good) and 400 `BadDeviceToken` or `DeviceTokenNotForTopic` (a token of another service or app)
/// the device is gone, so it is forgotten; anything else (our provider token refused, too many
/// requests, Apple down, a header it didn't like) didn't work this time.
pub fn outcome(status: u16, reason: Option<&str>) -> Delivery {
    match (status, reason) {
        (200, _) => Delivery::Sent,
        (410, _) | (400, Some("BadDeviceToken" | "DeviceTokenNotForTopic")) => Delivery::Gone,
        _ => Delivery::Failed,
    }
}

/// Whether Apple refused our provider token: expired, invalid or missing.
pub fn token_refused(status: u16, reason: Option<&str>) -> bool {
    status == 403
        && matches!(
            reason,
            Some("ExpiredProviderToken" | "InvalidProviderToken" | "MissingProviderToken")
        )
}

/// The APNs channel.
pub struct Apns {
    credentials: Credentials,
    http: reqwest::Client,
    production: reqwest::Url,
    sandbox: reqwest::Url,
    tokens: Mutex<Tokens>,
}

/// Without the provider token.
impl std::fmt::Debug for Apns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Apns")
            .field("credentials", &self.credentials)
            .field("production", &self.production.as_str())
            .field("sandbox", &self.sandbox.as_str())
            .finish_non_exhaustive()
    }
}

impl Apns {
    /// The channel, if the key is configured.
    pub fn from_env() -> Option<Self> {
        let credentials = Credentials::from_env(super::super::env_var)
            .ok()
            .flatten()?;
        Self::new(
            credentials,
            reqwest::Url::parse(PRODUCTION).ok()?,
            reqwest::Url::parse(SANDBOX).ok()?,
        )
    }

    /// The channel with the push services at these addresses: Apple's, a fake one in the tests.
    /// HTTP/2 only (Apple takes nothing else), over TLS with ALPN for Apple's; the one
    /// connection to each is kept and reused, as Apple asks.
    fn new(
        credentials: Credentials,
        production: reqwest::Url,
        sandbox: reqwest::Url,
    ) -> Option<Self> {
        let http = reqwest::Client::builder()
            .http2_prior_knowledge()
            .timeout(Duration::from_secs(15))
            .build()
            .ok()?;
        Some(Self {
            credentials,
            http,
            production,
            sandbox,
            tokens: Mutex::default(),
        })
    }

    fn provider_token(&self, now: i64) -> String {
        self.tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&self.credentials, now)
    }

    /// Sends `message` to `device` at `now_ms` and tells what Apple's answer means.
    async fn push(&self, device: &Device, message: &PushMessage, now_ms: i64) -> Delivery {
        let request = match request(&self.credentials.topic, device, message, now_ms) {
            Ok(request) => request,
            Err(delivery) => return delivery,
        };
        let base = match request.environment {
            ApnsEnvironment::Production => &self.production,
            ApnsEnvironment::Development => &self.sandbox,
        };
        let Ok(url) = base.join(&request.path) else {
            return Delivery::Gone;
        };
        let now = now_ms.div_euclid(1000);
        let mut builder = self
            .http
            .post(url)
            .header(
                "authorization",
                format!("bearer {}", self.provider_token(now)),
            )
            .body(request.body);
        for (name, value) in request.headers {
            builder = builder.header(name, value);
        }
        let response = match builder.send().await {
            Ok(response) => response,
            Err(e) => {
                eprintln!("notify: APNs push to device #{} failed: {e}", device.id);
                return Delivery::Failed;
            }
        };
        let status = response.status().as_u16();
        let body = response.bytes().await.unwrap_or_default();
        let reason = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v["reason"].as_str().map(str::to_string));
        if token_refused(status, reason.as_deref()) {
            self.tokens
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .refused(now);
        }
        let delivery = outcome(status, reason.as_deref());
        if delivery == Delivery::Failed {
            eprintln!(
                "notify: APNs answered {status} {} for device #{}",
                reason.as_deref().unwrap_or("-"),
                device.id
            );
        }
        delivery
    }
}

impl Channel for Apns {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Apns
    }

    fn send<'a>(&'a self, device: &'a Device, message: &'a PushMessage) -> BoxFuture<'a, Delivery> {
        Box::pin(self.push(device, message, super::now_ms()))
    }
}

#[cfg(test)]
mod tests;
