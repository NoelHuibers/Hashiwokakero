//! Web Push: notifications to browsers, through the push service of the browser's maker (the
//! address a subscription gives us is that service's: Google's for Chrome, Mozilla's for
//! Firefox, Apple's for Safari).
//!
//! Standard Web Push, with our own key pair and no third party's SDK or project: the message is
//! encrypted for the browser (RFC 8291, `aes128gcm` of RFC 8188), so the push service sees only
//! the endpoint, the size and the timing, and every request is signed with our key (VAPID, RFC
//! 8292), which the browser bound the subscription to.
//!
//! The keys come from the environment: `VAPID_PUBLIC_KEY` (the uncompressed P-256 point, 65
//! bytes) and `VAPID_PRIVATE_KEY` (the scalar, 32 bytes), both base64url, and `VAPID_SUBJECT`
//! (a contact URL for the push services, `https://hashi.rs` by default). `pnpm run vapid-keys`
//! makes a pair. Without them the channel is off, and the pages don't offer it
//! (`app::shell` writes the public key into the page only when it is on).

use std::time::Duration;

use aes_gcm::aead::{Aead as _, KeyInit as _};
use aes_gcm::{Aes128Gcm, Nonce};
use base64ct::{Base64UrlUnpadded, Encoding as _};
use futures::future::BoxFuture;
use hkdf::Hkdf;
use p256::ecdsa::signature::Signer as _;
use p256::ecdsa::{Signature, SigningKey};
use p256::{PublicKey, SecretKey};
use sha2::{Digest as _, Sha256};

use super::{Channel, Delivery, Device};
use crate::model::{DeviceKind, NotifyKind, PushMessage};

/// The subject of the VAPID signature without `VAPID_SUBJECT`: the site, no mail address.
pub const DEFAULT_SUBJECT: &str = "https://hashi.rs";
/// RFC 8188's record size: the whole message is one record.
const RECORD_SIZE: u32 = 4096;
/// The most a push service has to take (RFC 8030 section 7.2), the header included.
const MAX_BODY: usize = 4096;
/// How long a push service keeps a message for a browser that's offline, at most: four weeks,
/// what Chrome's takes.
const MAX_TTL_S: i64 = 28 * 24 * 3600;
/// How long a VAPID signature is good for (RFC 8292 allows up to 24 hours).
const JWT_VALID_S: i64 = 12 * 3600;

fn decode(value: &str) -> Option<Vec<u8>> {
    Base64UrlUnpadded::decode_vec(value.trim().trim_end_matches('=')).ok()
}

fn encode(bytes: &[u8]) -> String {
    Base64UrlUnpadded::encode_string(bytes)
}

/// Our key pair and subject (RFC 8292).
#[derive(Clone)]
pub struct Vapid {
    key: SigningKey,
    /// The public key as the browsers get it: base64url of the uncompressed point.
    public: String,
    subject: String,
}

/// Without the private key.
impl std::fmt::Debug for Vapid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vapid")
            .field("public", &self.public)
            .field("subject", &self.subject)
            .finish_non_exhaustive()
    }
}

impl Vapid {
    /// From base64url keys: the private key's scalar and the public key's uncompressed point,
    /// which has to be the private key's.
    pub fn new(public: &str, private: &str, subject: &str) -> Result<Self, String> {
        let key = decode(private)
            .and_then(|bytes| SigningKey::from_slice(&bytes).ok())
            .ok_or("VAPID_PRIVATE_KEY is not a base64url P-256 private key (32 bytes)")?;
        let point = key.verifying_key().to_sec1_point(false);
        if decode(public).as_deref() != Some(point.as_bytes()) {
            return Err("VAPID_PUBLIC_KEY is not the public key of VAPID_PRIVATE_KEY".into());
        }
        let subject = subject.trim();
        if !(subject.starts_with("https://") || subject.starts_with("mailto:")) {
            return Err("VAPID_SUBJECT has to be an https: or mailto: URL".into());
        }
        Ok(Self {
            key,
            public: encode(point.as_bytes()),
            subject: subject.to_string(),
        })
    }

    /// The keys of the environment: `None` without them (Web Push is off), an error for half a
    /// pair or a broken one (`validate_configuration` stops the server).
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
        let set = |name| var(name).filter(|v| !v.trim().is_empty());
        match (set("VAPID_PUBLIC_KEY"), set("VAPID_PRIVATE_KEY")) {
            (None, None) => Ok(None),
            (Some(public), Some(private)) => Self::new(
                &public,
                &private,
                &set("VAPID_SUBJECT").unwrap_or_else(|| DEFAULT_SUBJECT.to_string()),
            )
            .map(Some),
            _ => Err("VAPID_PUBLIC_KEY and VAPID_PRIVATE_KEY go together".into()),
        }
    }

    pub fn public_key(&self) -> &str {
        &self.public
    }

    /// The signed JWT for the push service at `audience` (its origin), good until `exp` (s).
    fn jwt(&self, audience: &str, exp: i64) -> String {
        let header = encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = encode(
            serde_json::json!({ "aud": audience, "exp": exp, "sub": self.subject })
                .to_string()
                .as_bytes(),
        );
        let signed = format!("{header}.{claims}");
        // ES256 in a JWS: r and s, 32 bytes each (RFC 7518 section 3.4)
        let signature: Signature = self.key.sign(signed.as_bytes());
        format!("{signed}.{}", encode(&signature.to_bytes()))
    }

    /// The `Authorization` header for a request to `endpoint` (RFC 8292 section 3).
    fn authorization(&self, endpoint: &reqwest::Url, now_s: i64) -> String {
        let audience = endpoint.origin().ascii_serialization();
        format!(
            "vapid t={}, k={}",
            self.jwt(&audience, now_s + JWT_VALID_S),
            self.public
        )
    }
}

/// The public key for the pages, `None` when Web Push is off. `validate_configuration` made sure
/// at startup that the pair is whole and fits.
pub fn public_key() -> Option<String> {
    let set = |name| super::super::env_var(name).filter(|v| !v.trim().is_empty());
    set("VAPID_PRIVATE_KEY")?;
    set("VAPID_PUBLIC_KEY").map(|k| k.trim().to_string())
}

/// A browser's subscription keys (`PushSubscription.toJSON().keys`): its P-256 public key and
/// the authentication secret, base64url.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keys {
    p256dh: PublicKey,
    auth: Vec<u8>,
}

impl Keys {
    pub fn parse(json: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let p256dh = decode(value["p256dh"].as_str()?)?;
        let auth = decode(value["auth"].as_str()?)?;
        (auth.len() == 16).then_some(())?;
        Some(Self {
            p256dh: PublicKey::from_sec1_bytes(&p256dh).ok()?,
            auth,
        })
    }
}

/// HKDF-SHA-256, extract and expand to `N` bytes.
fn hkdf<const N: usize>(salt: &[u8], ikm: &[u8], info: &[u8]) -> [u8; N] {
    let mut out = [0; N];
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, &mut out)
        .expect("N is far below HKDF's limit");
    out
}

/// Encrypts `plaintext` for a browser (RFC 8291): an ECDH secret of our one-time key (`ours`)
/// and the browser's key, combined with its authentication secret, keys AES-128-GCM over one
/// record. The result is the request's body: the header (salt, record size, our one-time public
/// key) and the ciphertext.
pub fn encrypt(plaintext: &[u8], to: &Keys, ours: &SecretKey, salt: &[u8; 16]) -> Vec<u8> {
    let ua_public = to.p256dh.to_sec1_bytes();
    let as_public = ours.public_key().to_sec1_bytes();
    let ecdh_secret = p256::ecdh::diffie_hellman(ours.to_nonzero_scalar(), to.p256dh.as_affine());
    // key_info = "WebPush: info" || 0x00 || ua_public || as_public
    let key_info = [b"WebPush: info\0".as_slice(), &ua_public, &as_public].concat();
    let ikm: [u8; 32] = hkdf(&to.auth, ecdh_secret.raw_secret_bytes(), &key_info);
    let cek: [u8; 16] = hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0");
    let nonce: [u8; 12] = hkdf(salt, &ikm, b"Content-Encoding: nonce\0");
    // One record, the last: its padding delimiter is 2, no padding after it
    let record = [plaintext, &[2]].concat();
    let ciphertext = Aes128Gcm::new(&cek.into())
        .encrypt(&Nonce::from(nonce), record.as_slice())
        .expect("AES-GCM encrypts anything this short");
    let key_len = u8::try_from(as_public.len()).expect("an uncompressed point is 65 bytes");
    [
        salt.as_slice(),
        &RECORD_SIZE.to_be_bytes(),
        &[key_len],
        &as_public,
        &ciphertext,
    ]
    .concat()
}

/// What the service worker gets (`public/sw.js`).
fn payload(message: &PushMessage) -> String {
    serde_json::json!({
        "title": message.title,
        "body": message.body,
        "url": message.url,
        "tag": message.tag,
    })
    .to_string()
}

/// A request to a push service, before it goes out.
#[derive(Debug)]
pub struct Request {
    pub endpoint: reqwest::Url,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

/// Builds the request that pushes `message` to a browser. Not for a subscription that can't be
/// used (no https endpoint or broken keys, nothing a later try would fix): [`Delivery::Gone`];
/// nor for a message too long for a push service: [`Delivery::Failed`].
///
/// `TTL`: until the notification expires (the daily's change, a race's lobby), so a browser that
/// comes online later doesn't get it after that. `Urgency`: high for the last call, which wakes
/// a phone that saves its battery, normal for the rest. `Topic`: a hash of the tag, so the push
/// service keeps only the latest of one (the count of a shared puzzle's solvers).
pub fn request(
    vapid: &Vapid,
    device: &Device,
    message: &PushMessage,
    now_ms: i64,
    ours: &SecretKey,
    salt: &[u8; 16],
) -> Result<Request, Delivery> {
    let endpoint = device
        .endpoint
        .as_deref()
        .and_then(|e| reqwest::Url::parse(e).ok())
        .filter(|e| e.scheme() == "https")
        .ok_or(Delivery::Gone)?;
    let keys = device
        .keys
        .as_deref()
        .and_then(Keys::parse)
        .ok_or(Delivery::Gone)?;
    let body = encrypt(payload(message).as_bytes(), &keys, ours, salt);
    if body.len() > MAX_BODY {
        return Err(Delivery::Failed);
    }
    let ttl = ((message.expires_at - now_ms) / 1000).clamp(0, MAX_TTL_S);
    let urgency = if message.kind == NotifyKind::LastCall.id() {
        "high"
    } else {
        "normal"
    };
    let topic = encode(&Sha256::digest(message.tag.as_bytes())[..24]);
    Ok(Request {
        headers: vec![
            ("authorization", vapid.authorization(&endpoint, now_ms / 1000)),
            ("ttl", ttl.to_string()),
            ("urgency", urgency.into()),
            ("topic", topic),
            ("content-encoding", "aes128gcm".into()),
            ("content-type", "application/octet-stream".into()),
        ],
        endpoint,
        body,
    })
}

/// What a push service's answer means (RFC 8030 section 5): 2xx it took it; 404 and 410 the
/// subscription is gone (unsubscribed, expired), so the device is forgotten; anything else (too
/// many requests, the service down, a key the browser doesn't know) didn't work this time.
pub const fn outcome(status: u16) -> Delivery {
    match status {
        200..=299 => Delivery::Sent,
        404 | 410 => Delivery::Gone,
        _ => Delivery::Failed,
    }
}

/// A one-time key pair and salt for a message.
fn one_time() -> (SecretKey, [u8; 16]) {
    let mut salt = [0; 16];
    getrandom::fill(&mut salt).expect("no system randomness");
    loop {
        let mut scalar = [0; 32];
        getrandom::fill(&mut scalar).expect("no system randomness");
        // Zero or beyond the group's order: about one in 2^32
        if let Ok(key) = SecretKey::from_slice(&scalar) {
            return (key, salt);
        }
    }
}

/// The Web Push channel.
#[derive(Debug)]
pub struct WebPush {
    vapid: Vapid,
    http: reqwest::Client,
}

impl WebPush {
    /// The channel, if the keys are configured.
    pub fn from_env() -> Option<Self> {
        let vapid = Vapid::from_env(super::super::env_var).ok().flatten()?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .ok()?;
        Some(Self { vapid, http })
    }
}

impl Channel for WebPush {
    fn kind(&self) -> DeviceKind {
        DeviceKind::Web
    }

    fn send<'a>(&'a self, device: &'a Device, message: &'a PushMessage) -> BoxFuture<'a, Delivery> {
        Box::pin(async move {
            let (ours, salt) = one_time();
            let request = match request(
                &self.vapid,
                device,
                message,
                super::now_ms(),
                &ours,
                &salt,
            ) {
                Ok(request) => request,
                Err(delivery) => return delivery,
            };
            post(&self.http, request, device.id).await
        })
    }
}

/// Sends a request to the push service and tells what its answer means.
pub async fn post(http: &reqwest::Client, request: Request, device: i64) -> Delivery {
    let mut builder = http.post(request.endpoint).body(request.body);
    for (name, value) in request.headers {
        builder = builder.header(name, value);
    }
    match builder.send().await {
        Ok(res) => {
            let delivery = outcome(res.status().as_u16());
            if delivery == Delivery::Failed {
                eprintln!(
                    "notify: the push service answered {} for device #{device}",
                    res.status()
                );
            }
            delivery
        }
        Err(e) => {
            eprintln!("notify: push to device #{device} failed: {e}");
            Delivery::Failed
        }
    }
}

#[cfg(test)]
mod tests;
