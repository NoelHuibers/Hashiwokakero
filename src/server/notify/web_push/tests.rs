//! Web Push against RFC 8291's example, a VAPID signature checked with our public key, and a
//! push service on localhost that decrypts what it gets as a browser would.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post as route_post;
use p256::ecdsa::VerifyingKey;
use p256::ecdsa::signature::Verifier as _;

use super::*;
use crate::i18n::Locale;

fn b64(value: &str) -> Vec<u8> {
    decode(value).unwrap()
}

// RFC 8291 section 5 and appendix A
const PLAINTEXT: &str = "V2hlbiBJIGdyb3cgdXAsIEkgd2FudCB0byBiZSBhIHdhdGVybWVsb24";
const AS_PUBLIC: &str =
    "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
const AS_PRIVATE: &str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw";
const UA_PUBLIC: &str =
    "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
const UA_PRIVATE: &str = "q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94";
const SALT: &str = "DGv6ra1nlYgDCS1FRnbzlw";
const AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";
const BODY: &str = "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27ml\
mlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPT\
pK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN";

fn browser_keys() -> String {
    serde_json::json!({ "p256dh": UA_PUBLIC, "auth": AUTH }).to_string()
}

fn salt() -> [u8; 16] {
    b64(SALT).try_into().unwrap()
}

fn as_private() -> SecretKey {
    SecretKey::from_slice(&b64(AS_PRIVATE)).unwrap()
}

/// A VAPID pair of our own, not RFC 8291's (that one is the one-time key of a message).
fn vapid() -> Vapid {
    let key = SecretKey::from_slice(&[7; 32]).unwrap();
    let public = encode(&key.public_key().to_sec1_bytes());
    Vapid::new(&public, &encode(&key.to_bytes()), DEFAULT_SUBJECT).unwrap()
}

/// What a browser does with a message (RFC 8291 section 3.4, RFC 8188), for the round trips.
fn decrypt(body: &[u8], ua_private: &SecretKey, auth: &[u8]) -> Vec<u8> {
    let (salt, rest) = body.split_at(16);
    assert_eq!(rest[..4], RECORD_SIZE.to_be_bytes());
    let key_len = usize::from(rest[4]);
    let (as_public, ciphertext) = rest[5..].split_at(key_len);
    let as_public = PublicKey::from_sec1_bytes(as_public).unwrap();
    let ecdh = p256::ecdh::diffie_hellman(ua_private.to_nonzero_scalar(), as_public.as_affine());
    let key_info = [
        b"WebPush: info\0".as_slice(),
        &ua_private.public_key().to_sec1_bytes(),
        &as_public.to_sec1_bytes(),
    ]
    .concat();
    let ikm: [u8; 32] = hkdf(auth, ecdh.raw_secret_bytes(), &key_info);
    let cek: [u8; 16] = hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0");
    let nonce: [u8; 12] = hkdf(salt, &ikm, b"Content-Encoding: nonce\0");
    let mut record = Aes128Gcm::new(&cek.into())
        .decrypt(&Nonce::from(nonce), ciphertext)
        .unwrap();
    assert_eq!(record.pop(), Some(2), "the last record's delimiter");
    record
}

#[test]
fn encryption_matches_rfc_8291s_example() {
    let keys = Keys::parse(&browser_keys()).unwrap();
    let body = encrypt(&b64(PLAINTEXT), &keys, &as_private(), &salt());
    assert_eq!(encode(&body), BODY);
    // The example's Content-Length says 145, but its body is 144 bytes: 86 of header, 41 of
    // text, the delimiter and the 16 of the tag
    assert_eq!(body.len(), 86 + 41 + 1 + 16);
    assert_eq!(
        encode(&as_private().public_key().to_sec1_bytes()),
        AS_PUBLIC
    );
    // And the browser reads it
    let ua = SecretKey::from_slice(&b64(UA_PRIVATE)).unwrap();
    assert_eq!(
        decrypt(&body, &ua, &b64(AUTH)),
        b"When I grow up, I want to be a watermelon"
    );
}

#[test]
fn broken_subscription_keys_are_refused() {
    assert!(Keys::parse(&browser_keys()).is_some());
    for broken in [
        "{}",
        "not json",
        r#"{"p256dh": "AAAA", "auth": "BTBZMqHH6r4Tts7J_aSIgg"}"#,
        &serde_json::json!({ "p256dh": UA_PUBLIC, "auth": "c2hvcnQ" }).to_string(),
        &serde_json::json!({ "p256dh": UA_PUBLIC }).to_string(),
    ] {
        assert_eq!(Keys::parse(broken), None, "{broken}");
    }
}

#[test]
fn the_vapid_jwt_is_signed_with_our_key() {
    let vapid = vapid();
    let endpoint = reqwest::Url::parse("https://fcm.googleapis.com/fcm/send/abc:def").unwrap();
    let header = vapid.authorization(&endpoint, 1_800_000_000);
    let (token, key) = header
        .strip_prefix("vapid t=")
        .and_then(|rest| rest.split_once(", k="))
        .unwrap();
    assert_eq!(key, vapid.public_key());
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    let json = |part: &str| serde_json::from_slice::<serde_json::Value>(&b64(part)).unwrap();
    assert_eq!(
        json(parts[0]),
        serde_json::json!({ "typ": "JWT", "alg": "ES256" })
    );
    assert_eq!(
        json(parts[1]),
        serde_json::json!({
            "aud": "https://fcm.googleapis.com",
            "exp": 1_800_000_000 + 12 * 3600,
            "sub": "https://hashi.rs",
        })
    );
    // r and s, not DER
    let signature = Signature::from_slice(&b64(parts[2])).unwrap();
    let verifying = VerifyingKey::from_sec1_bytes(&b64(key)).unwrap();
    let signed = format!("{}.{}", parts[0], parts[1]);
    assert!(verifying.verify(signed.as_bytes(), &signature).is_ok());
    assert!(verifying.verify(b"something else", &signature).is_err());
    // The audience is the push service's origin, a port included
    let other = reqwest::Url::parse("https://push.example:8443/x").unwrap();
    let token = vapid.authorization(&other, 0);
    let claims = token.split('.').nth(1).unwrap();
    assert_eq!(json(claims)["aud"], "https://push.example:8443");
}

#[test]
fn the_keys_come_from_the_environment_whole_or_not_at_all() {
    let key = SecretKey::from_slice(&[9; 32]).unwrap();
    let public = encode(&key.public_key().to_sec1_bytes());
    let private = encode(&key.to_bytes());
    let env = |pairs: &[(&'static str, String)]| {
        let pairs = pairs.to_vec();
        move |name: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.clone())
        }
    };
    // Off without them, also when empty
    assert!(Vapid::from_env(env(&[])).unwrap().is_none());
    assert!(
        Vapid::from_env(env(&[
            ("VAPID_PUBLIC_KEY", String::new()),
            ("VAPID_PRIVATE_KEY", " ".into())
        ]))
        .unwrap()
        .is_none()
    );
    let on = Vapid::from_env(env(&[
        ("VAPID_PUBLIC_KEY", public.clone()),
        ("VAPID_PRIVATE_KEY", private.clone()),
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(on.public_key(), public);
    assert_eq!(on.subject, "https://hashi.rs");
    let subject = Vapid::from_env(env(&[
        ("VAPID_PUBLIC_KEY", public.clone()),
        ("VAPID_PRIVATE_KEY", private.clone()),
        ("VAPID_SUBJECT", "https://hashi.rs/kontakt".into()),
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(subject.subject, "https://hashi.rs/kontakt");
    // Half a pair, a pair that doesn't fit, a broken key, a subject that isn't a URL
    for broken in [
        vec![("VAPID_PUBLIC_KEY", public.clone())],
        vec![("VAPID_PRIVATE_KEY", private.clone())],
        vec![
            ("VAPID_PUBLIC_KEY", AS_PUBLIC.to_string()),
            ("VAPID_PRIVATE_KEY", private.clone()),
        ],
        vec![
            ("VAPID_PUBLIC_KEY", public.clone()),
            ("VAPID_PRIVATE_KEY", "not base64!".into()),
        ],
        vec![
            ("VAPID_PUBLIC_KEY", public.clone()),
            ("VAPID_PRIVATE_KEY", private.clone()),
            ("VAPID_SUBJECT", "hashi.rs".into()),
        ],
    ] {
        assert!(Vapid::from_env(env(&broken)).is_err(), "{broken:?}");
    }
}

#[test]
fn push_services_answers_decide_the_device_s_fate() {
    for status in [200, 201, 202] {
        assert_eq!(outcome(status), Delivery::Sent, "{status}");
    }
    for status in [404, 410] {
        assert_eq!(outcome(status), Delivery::Gone, "{status}");
    }
    for status in [400, 401, 403, 413, 429, 500, 502, 503] {
        assert_eq!(outcome(status), Delivery::Failed, "{status}");
    }
}

fn device(endpoint: &str, keys: &str) -> Device {
    Device {
        id: 7,
        user_id: 1,
        kind: DeviceKind::Web,
        endpoint: Some(endpoint.into()),
        keys: Some(keys.into()),
        locale: Locale::de,
        cursor: 0,
    }
}

fn message(kind: NotifyKind, expires_at: i64) -> PushMessage {
    PushMessage {
        id: 3,
        kind: kind.id().into(),
        title: "Letzter Aufruf für deine Serie".into(),
        body: "Das Tägliche ist noch offen.".into(),
        url: "https://hashi.rs/de".into(),
        tag: "last_call:20832".into(),
        expires_at,
    }
}

fn header<'a>(request: &'a Request, name: &str) -> &'a str {
    request
        .headers
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or_else(|| panic!("no {name}"))
}

#[test]
fn requests_carry_the_encrypted_message_until_it_expires() {
    let now = 1_800_000_000_000;
    let ua = SecretKey::from_slice(&b64(UA_PRIVATE)).unwrap();
    let browser = device("https://updates.push.services.mozilla.com/wpush/v2/x", &browser_keys());
    let last_call = message(NotifyKind::LastCall, now + 3 * 3_600_000);
    let request = request(&vapid(), &browser, &last_call, now, &as_private(), &salt()).unwrap();
    assert_eq!(request.endpoint.as_str(), browser.endpoint.as_deref().unwrap());
    assert_eq!(header(&request, "ttl"), "10800");
    assert_eq!(header(&request, "urgency"), "high");
    assert_eq!(header(&request, "content-encoding"), "aes128gcm");
    assert_eq!(header(&request, "topic").len(), 32);
    assert!(header(&request, "authorization").starts_with("vapid t="));
    let payload: serde_json::Value =
        serde_json::from_slice(&decrypt(&request.body, &ua, &b64(AUTH))).unwrap();
    assert_eq!(
        payload,
        serde_json::json!({
            "title": "Letzter Aufruf für deine Serie",
            "body": "Das Tägliche ist noch offen.",
            "url": "https://hashi.rs/de",
            "tag": "last_call:20832",
        })
    );

    // The rest is normal; an expired one has no time left, a far one four weeks at most
    let race = message(NotifyKind::Race, now - 5_000);
    let request = super::request(&vapid(), &browser, &race, now, &as_private(), &salt()).unwrap();
    assert_eq!(header(&request, "urgency"), "normal");
    assert_eq!(header(&request, "ttl"), "0");
    let far = message(NotifyKind::Shared, now + 400 * 86_400_000);
    let request = super::request(&vapid(), &browser, &far, now, &as_private(), &salt()).unwrap();
    assert_eq!(header(&request, "ttl"), "2419200");
}

#[test]
fn unusable_subscriptions_are_gone_and_long_messages_fail() {
    let now = 0;
    let note = message(NotifyKind::Rival, 60_000);
    let send = |device: &Device, message: &PushMessage| {
        request(&vapid(), device, message, now, &as_private(), &salt()).map(|_| ())
    };
    assert_eq!(
        send(&device("http://push.example/x", &browser_keys()), &note),
        Err(Delivery::Gone)
    );
    assert_eq!(
        send(&device("not a url", &browser_keys()), &note),
        Err(Delivery::Gone)
    );
    assert_eq!(
        send(&device("https://push.example/x", "{}"), &note),
        Err(Delivery::Gone)
    );
    let mut none = device("https://push.example/x", &browser_keys());
    none.keys = None;
    assert_eq!(send(&none, &note), Err(Delivery::Gone));
    let long = PushMessage {
        body: "x".repeat(4000),
        ..note.clone()
    };
    assert_eq!(
        send(&device("https://push.example/x", &browser_keys()), &long),
        Err(Delivery::Failed)
    );
}

/// What the push service on localhost got.
#[derive(Default)]
struct Got {
    headers: Option<HeaderMap>,
    body: Vec<u8>,
}

/// A push service on localhost that answers `status`.
async fn push_service(status: StatusCode) -> (reqwest::Url, Arc<Mutex<Got>>) {
    let got = Arc::new(Mutex::new(Got::default()));
    let app = Router::new().route(
        "/push/{id}",
        route_post({
            let got = Arc::clone(&got);
            move |headers: HeaderMap, body: Bytes| async move {
                *got.lock().unwrap() = Got {
                    headers: Some(headers),
                    body: body.to_vec(),
                };
                status
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (
        reqwest::Url::parse(&format!("http://{addr}/push/abc")).unwrap(),
        got,
    )
}

#[tokio::test]
async fn a_push_service_gets_the_message_and_its_answer_counts() {
    let ua = SecretKey::from_slice(&b64(UA_PRIVATE)).unwrap();
    let browser = device("https://push.example/push/abc", &browser_keys());
    let note = message(NotifyKind::LastCall, 3_600_000);
    let http = reqwest::Client::new();
    for (status, delivery) in [
        (StatusCode::CREATED, Delivery::Sent),
        (StatusCode::GONE, Delivery::Gone),
        (StatusCode::NOT_FOUND, Delivery::Gone),
        (StatusCode::TOO_MANY_REQUESTS, Delivery::Failed),
    ] {
        let (endpoint, got) = push_service(status).await;
        let mut request = request(&vapid(), &browser, &note, 0, &as_private(), &salt()).unwrap();
        // Plain http here: the real ones are https only
        request.endpoint = endpoint;
        assert_eq!(post(&http, request, browser.id).await, delivery, "{status}");
        let got = got.lock().unwrap();
        let headers = got.headers.as_ref().unwrap();
        assert_eq!(headers["ttl"], "3600");
        assert_eq!(headers["urgency"], "high");
        assert_eq!(headers["content-encoding"], "aes128gcm");
        assert!(
            headers["authorization"]
                .to_str()
                .unwrap()
                .ends_with(&format!(", k={}", vapid().public_key()))
        );
        let payload: serde_json::Value =
            serde_json::from_slice(&decrypt(&got.body, &ua, &b64(AUTH))).unwrap();
        assert_eq!(payload["url"], "https://hashi.rs/de");
    }
    // Nobody there
    let mut request = request(&vapid(), &browser, &note, 0, &as_private(), &salt()).unwrap();
    request.endpoint = reqwest::Url::parse("http://127.0.0.1:9/push/abc").unwrap();
    assert_eq!(post(&http, request, browser.id).await, Delivery::Failed);
}
