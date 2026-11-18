//! APNs with a key made for the test: the provider token checked with its public key, the
//! requests, Apple's answers, and a push service on localhost that speaks HTTP/2 as Apple's does.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, Request as HttpRequest};
use axum::http::{HeaderMap, StatusCode, Version};
use axum::routing::post as route_post;
use p256::SecretKey;
use p256::ecdsa::VerifyingKey;
use p256::ecdsa::signature::Verifier as _;
use p256::pkcs8::{EncodePrivateKey as _, LineEnding};

use super::*;
use crate::i18n::Locale;

const KEY_ID: &str = "ABC123DEFG";
const TEAM_ID: &str = "DEF123GHIJ";
/// A device token: 32 bytes in hex.
const TOKEN: &str = "5fa3c2d4e6f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192";

/// A P-256 key of this test run, never Apple's: the text of a `.p8` file.
fn p8() -> (String, VerifyingKey) {
    let key = loop {
        let mut scalar = [0; 32];
        getrandom::fill(&mut scalar).unwrap();
        if let Ok(key) = SecretKey::from_slice(&scalar) {
            break key;
        }
    };
    let pem = key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
    (pem, *SigningKey::from(&key).verifying_key())
}

fn credentials() -> (Credentials, VerifyingKey) {
    let (pem, public) = p8();
    (
        Credentials::new(&pem, KEY_ID, TEAM_ID, DEFAULT_TOPIC).unwrap(),
        public,
    )
}

fn b64(part: &str) -> Vec<u8> {
    Base64UrlUnpadded::decode_vec(part).unwrap()
}

/// A provider token's header and claims, after checking its signature with `public`.
fn verified(token: &str, public: &VerifyingKey) -> (serde_json::Value, serde_json::Value) {
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3, "{token}");
    // r and s, not DER
    let signature = Signature::from_slice(&b64(parts[2])).unwrap();
    let signed = format!("{}.{}", parts[0], parts[1]);
    assert!(public.verify(signed.as_bytes(), &signature).is_ok());
    assert!(public.verify(b"something else", &signature).is_err());
    let json = |part: &str| serde_json::from_slice::<serde_json::Value>(&b64(part)).unwrap();
    (json(parts[0]), json(parts[1]))
}

#[test]
fn the_provider_token_is_signed_with_our_key() {
    let (credentials, public) = credentials();
    let (header, claims) = verified(&credentials.jwt(1_800_000_000), &public);
    assert_eq!(header, serde_json::json!({ "alg": "ES256", "kid": KEY_ID }));
    assert_eq!(
        claims,
        serde_json::json!({ "iss": TEAM_ID, "iat": 1_800_000_000 })
    );
    // Another key doesn't verify it
    let (_, other) = p8();
    let token = credentials.jwt(1);
    let parts: Vec<&str> = token.split('.').collect();
    let signature = Signature::from_slice(&b64(parts[2])).unwrap();
    assert!(
        other
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .is_err()
    );
    // Nothing of the key in its Debug
    let debug = format!("{credentials:?}");
    assert!(debug.contains(KEY_ID) && !debug.contains("key:"), "{debug}");
}

#[test]
fn the_provider_token_is_renewed_after_40_minutes_and_not_within_20() {
    let (credentials, public) = credentials();
    let mut tokens = Tokens::default();
    let start = 1_800_000_000;
    let first = tokens.get(&credentials, start);
    assert_eq!(verified(&first, &public).1["iat"], start);
    // The same for every request until it is 40 minutes old
    assert_eq!(tokens.get(&credentials, start + 60), first);
    assert_eq!(tokens.get(&credentials, start + RENEW_AFTER_S - 1), first);
    let second = tokens.get(&credentials, start + RENEW_AFTER_S);
    assert_ne!(second, first);
    assert_eq!(
        verified(&second, &public).1["iat"],
        start + RENEW_AFTER_S
    );
    const { assert!(RENEW_NOT_BEFORE_S >= 20 * 60 && RENEW_AFTER_S < 60 * 60) };

    // Refused when it's fresh: kept (a new one wouldn't help, Apple would refuse it for coming
    // too soon); refused once 20 minutes old: a new one with the next request
    let issued = start + RENEW_AFTER_S;
    tokens.refused(issued + RENEW_NOT_BEFORE_S - 1);
    assert_eq!(tokens.get(&credentials, issued + RENEW_NOT_BEFORE_S - 1), second);
    tokens.refused(issued + RENEW_NOT_BEFORE_S);
    let third = tokens.get(&credentials, issued + RENEW_NOT_BEFORE_S);
    assert_ne!(third, second);
    assert_eq!(
        verified(&third, &public).1["iat"],
        issued + RENEW_NOT_BEFORE_S
    );

    // A clock that went back makes a new one rather than one from the future
    let back = tokens.get(&credentials, issued);
    assert_eq!(verified(&back, &public).1["iat"], issued);
}

/// An environment of these variables.
fn env(pairs: &[(&'static str, String)]) -> impl Fn(&str) -> Option<String> {
    let pairs = pairs.to_vec();
    move |name: &str| {
        pairs
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
    }
}

#[test]
fn the_key_comes_from_the_environment_whole_or_not_at_all() {
    let (pem, _) = p8();
    // Off without it, also when empty
    assert!(Credentials::from_env(env(&[])).unwrap().is_none());
    assert!(
        Credentials::from_env(env(&[
            ("APNS_KEY_P8", " ".into()),
            ("APNS_KEY_ID", String::new()),
            ("APNS_TEAM_ID", String::new()),
            ("APNS_TOPIC", "rs.hashi".into()),
        ]))
        .unwrap()
        .is_none()
    );
    let whole = [
        ("APNS_KEY_P8", pem.clone()),
        ("APNS_KEY_ID", KEY_ID.to_string()),
        ("APNS_TEAM_ID", TEAM_ID.to_string()),
    ];
    let on = Credentials::from_env(env(&whole)).unwrap().unwrap();
    assert_eq!(
        (on.key_id.as_str(), on.team_id.as_str(), on.topic.as_str()),
        (KEY_ID, TEAM_ID, "rs.hashi")
    );
    // Another topic, and the file's text in one line with \n (as some secret stores keep it)
    let one_line = [
        ("APNS_KEY_P8", pem.trim().replace('\n', "\\n")),
        ("APNS_KEY_ID", format!(" {KEY_ID}\n")),
        ("APNS_TEAM_ID", TEAM_ID.to_string()),
        ("APNS_TOPIC", "rs.hashi.debug".to_string()),
    ];
    let on = Credentials::from_env(env(&one_line)).unwrap().unwrap();
    assert_eq!((on.key_id.as_str(), on.topic.as_str()), (KEY_ID, "rs.hashi.debug"));

    // Part of it
    for part in [
        vec![whole[0].clone()],
        vec![whole[1].clone(), whole[2].clone()],
        vec![whole[0].clone(), whole[2].clone()],
        vec![whole[0].clone(), whole[1].clone()],
    ] {
        let error = Credentials::from_env(env(&part)).unwrap_err();
        assert!(error.contains("go together"), "{part:?}: {error}");
    }
    // Broken: not a key, a key cut short, ids that aren't Apple's, a topic with a space
    let body = pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect::<String>();
    for (name, value) in [
        ("APNS_KEY_P8", "not a key".to_string()),
        ("APNS_KEY_P8", body),
        ("APNS_KEY_P8", pem.replace("PRIVATE KEY", "PUBLIC KEY")),
        ("APNS_KEY_P8", pem[..pem.len() / 2].to_string()),
        ("APNS_KEY_ID", "ABC123".to_string()),
        ("APNS_KEY_ID", "ABC123DEF!".to_string()),
        ("APNS_TEAM_ID", "Q35TCGM5PW1".to_string()),
        ("APNS_TOPIC", "rs hashi".to_string()),
    ] {
        let mut broken = whole.to_vec();
        broken.retain(|(n, _)| *n != name);
        broken.push((name, value));
        let error = Credentials::from_env(env(&broken)).unwrap_err();
        assert!(error.contains(name), "{name}: {error}");
        // Never the key in the error
        assert!(!error.contains("BEGIN"), "{error}");
    }
}

#[test]
fn apples_answers_decide_the_device_s_fate() {
    assert_eq!(outcome(200, None), Delivery::Sent);
    for (status, reason) in [
        (410, Some("Unregistered")),
        (410, None),
        (400, Some("BadDeviceToken")),
        (400, Some("DeviceTokenNotForTopic")),
    ] {
        assert_eq!(outcome(status, reason), Delivery::Gone, "{status} {reason:?}");
    }
    for (status, reason) in [
        (400, Some("BadCollapseId")),
        (400, Some("PayloadEmpty")),
        (400, None),
        (403, Some("InvalidProviderToken")),
        (403, Some("ExpiredProviderToken")),
        (413, Some("PayloadTooLarge")),
        (429, Some("TooManyRequests")),
        (429, Some("TooManyProviderTokenUpdates")),
        (500, Some("InternalServerError")),
        (503, Some("ServiceUnavailable")),
    ] {
        assert_eq!(
            outcome(status, reason),
            Delivery::Failed,
            "{status} {reason:?}"
        );
    }
    for reason in [
        "InvalidProviderToken",
        "ExpiredProviderToken",
        "MissingProviderToken",
    ] {
        assert!(token_refused(403, Some(reason)), "{reason}");
    }
    assert!(!token_refused(403, Some("BadCertificate")));
    assert!(!token_refused(403, None));
    assert!(!token_refused(400, Some("InvalidProviderToken")));
}

fn device(token: &str, keys: Option<&str>) -> Device {
    Device {
        id: 7,
        user_id: 1,
        kind: DeviceKind::Apns,
        endpoint: Some(token.into()),
        keys: keys.map(str::to_string),
        locale: Locale::de,
        cursor: 0,
    }
}

fn production() -> Device {
    device(TOKEN, Some(&ApnsEnvironment::Production.keys()))
}

fn message(kind: NotifyKind, tag: &str, expires_at: i64) -> PushMessage {
    PushMessage {
        id: 3,
        kind: kind.id().into(),
        title: "Letzter Aufruf für deine Serie".into(),
        body: "Das Tägliche ist noch offen.".into(),
        url: "https://hashi.rs/de".into(),
        tag: tag.into(),
        expires_at,
    }
}

fn header<'a>(request: &'a Request, name: &str) -> &'a str {
    request
        .headers
        .iter()
        .find(|(n, _)| *n == name)
        .map_or_else(|| panic!("no {name}"), |(_, v)| v.as_str())
}

#[test]
fn requests_carry_the_alert_until_it_expires() {
    let now = 1_800_000_000_000;
    let last_call = message(NotifyKind::LastCall, "last_call:20832", now + 3 * 3_600_000);
    let request = request(DEFAULT_TOPIC, &production(), &last_call, now).unwrap();
    assert_eq!(request.environment, ApnsEnvironment::Production);
    assert_eq!(request.path, format!("/3/device/{TOKEN}"));
    assert_eq!(header(&request, "apns-topic"), "rs.hashi");
    assert_eq!(header(&request, "apns-push-type"), "alert");
    assert_eq!(header(&request, "apns-priority"), "10");
    assert_eq!(header(&request, "apns-expiration"), "1800010800");
    assert_eq!(header(&request, "apns-collapse-id"), "last_call:20832");
    assert_eq!(header(&request, "content-type"), "application/json");
    assert!(request.headers.iter().all(|(n, _)| *n != "authorization"));
    let payload: serde_json::Value = serde_json::from_str(&request.body).unwrap();
    assert_eq!(
        payload,
        serde_json::json!({
            "aps": {
                "alert": {
                    "title": "Letzter Aufruf für deine Serie",
                    "body": "Das Tägliche ist noch offen.",
                },
                "sound": "default",
                "thread-id": "last_call",
            },
            "url": "https://hashi.rs/de",
        })
    );

    // The rest when it suits the phone; the sandbox's token goes to the sandbox, upper case hex
    // as Apple's
    let sandbox = device(
        &TOKEN.to_ascii_uppercase(),
        Some(&ApnsEnvironment::Development.keys()),
    );
    let race = message(NotifyKind::Race, "race:group:12", now + 900_000);
    let request = super::request("rs.hashi", &sandbox, &race, now).unwrap();
    assert_eq!(request.environment, ApnsEnvironment::Development);
    assert_eq!(request.path, format!("/3/device/{TOKEN}"));
    assert_eq!(header(&request, "apns-priority"), "5");
    assert_eq!(header(&request, "apns-expiration"), "1800000900");
    assert_eq!(header(&request, "apns-collapse-id"), "race:group:12");
    let payload: serde_json::Value = serde_json::from_str(&request.body).unwrap();
    assert_eq!(payload["aps"]["thread-id"], "race");

    // Expired by now: once, not kept; far away: four weeks at most
    let late = message(NotifyKind::Rival, "rival:day:1:2", now - 5_000);
    let request = super::request(DEFAULT_TOPIC, &production(), &late, now).unwrap();
    assert_eq!(header(&request, "apns-expiration"), "1800000000");
    let far = message(NotifyKind::Shared, "shared:share:1", now + 400 * 86_400_000);
    let request = super::request(DEFAULT_TOPIC, &production(), &far, now).unwrap();
    assert_eq!(header(&request, "apns-expiration"), "1802419200");

    // A tag beyond 64 bytes collapses by its hash
    let long = message(NotifyKind::Group, &format!("group:{}", "x".repeat(80)), now);
    let request = super::request(DEFAULT_TOPIC, &production(), &long, now).unwrap();
    let id = header(&request, "apns-collapse-id");
    assert_eq!(id.len(), 64);
    assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_ne!(
        id,
        collapse_id(&format!("group:{}", "y".repeat(80))),
        "another tag, another id"
    );
}

#[test]
fn unusable_devices_are_gone_and_long_messages_fail() {
    let note = message(NotifyKind::Rival, "rival:1", 60_000);
    let send = |device: &Device, message: &PushMessage| {
        request(DEFAULT_TOPIC, device, message, 0).map(|_| ())
    };
    let production = ApnsEnvironment::Production.keys();
    for broken in [
        device("not hex", Some(&production)),
        device("", Some(&production)),
        device("../../3/device/abc", Some(&production)),
        device(TOKEN, None),
        device(TOKEN, Some("{}")),
        device(TOKEN, Some(r#"{"env":"staging"}"#)),
    ] {
        assert_eq!(send(&broken, &note), Err(Delivery::Gone), "{broken:?}");
    }
    let mut tokenless = super::tests::production();
    tokenless.endpoint = None;
    assert_eq!(send(&tokenless, &note), Err(Delivery::Gone));
    assert_eq!(send(&super::tests::production(), &note), Ok(()));
    // 4 KB at most
    let long = PushMessage {
        body: "x".repeat(4000),
        ..note.clone()
    };
    assert_eq!(
        send(&super::tests::production(), &long),
        Err(Delivery::Failed)
    );
    let fits = PushMessage {
        body: "x".repeat(3800),
        ..note
    };
    assert_eq!(send(&super::tests::production(), &fits), Ok(()));
}

/// What the push service on localhost got.
#[derive(Clone, Debug)]
struct Got {
    service: &'static str,
    token: String,
    version: Version,
    headers: HeaderMap,
    body: serde_json::Value,
}

/// A push service on localhost, HTTP/2 without TLS (h2c), that answers each request with the
/// next of `answers` (status and reason) and notes what it got. `service` names it in [`Got`].
async fn push_service(
    service: &'static str,
    answers: Vec<(StatusCode, Option<&'static str>)>,
    got: Arc<Mutex<Vec<Got>>>,
) -> reqwest::Url {
    let answers = Arc::new(Mutex::new(answers.into_iter()));
    let app = Router::new().route(
        "/3/device/{token}",
        route_post(move |Path(token): Path<String>, request: HttpRequest| {
            let (answers, got) = (Arc::clone(&answers), Arc::clone(&got));
            async move {
                let version = request.version();
                let headers = request.headers().clone();
                let body = axum::body::to_bytes(request.into_body(), 8192)
                    .await
                    .unwrap();
                got.lock().unwrap().push(Got {
                    service,
                    token,
                    version,
                    headers,
                    body: serde_json::from_slice(&body).unwrap(),
                });
                let (status, reason) = answers.lock().unwrap().next().expect("an answer");
                let body = reason
                    .map(|r| serde_json::json!({ "reason": r }).to_string())
                    .unwrap_or_default();
                (status, body)
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    reqwest::Url::parse(&format!("http://{addr}")).unwrap()
}

/// The provider token of a request, checked: its header, its claims.
fn bearer(got: &Got, public: &VerifyingKey) -> (String, i64) {
    let token = got.headers["authorization"]
        .to_str()
        .unwrap()
        .strip_prefix("bearer ")
        .unwrap()
        .to_string();
    let (header, claims) = verified(&token, public);
    assert_eq!(header["kid"], KEY_ID);
    assert_eq!(claims["iss"], TEAM_ID);
    let iat = claims["iat"].as_i64().unwrap();
    (token, iat)
}

#[tokio::test]
async fn a_push_service_gets_the_message_over_http2_and_its_answer_counts() {
    let (credentials, public) = credentials();
    let got = Arc::new(Mutex::new(Vec::new()));
    let production = push_service(
        "production",
        vec![
            (StatusCode::OK, None),
            (StatusCode::GONE, Some("Unregistered")),
            (StatusCode::BAD_REQUEST, Some("BadDeviceToken")),
            (StatusCode::BAD_REQUEST, Some("BadCollapseId")),
            (StatusCode::TOO_MANY_REQUESTS, Some("TooManyRequests")),
            (StatusCode::SERVICE_UNAVAILABLE, None),
            // Our token, too fresh to renew, then old enough
            (StatusCode::FORBIDDEN, Some("InvalidProviderToken")),
            (StatusCode::OK, None),
            (StatusCode::FORBIDDEN, Some("ExpiredProviderToken")),
            (StatusCode::OK, None),
        ],
        Arc::clone(&got),
    )
    .await;
    let sandbox = push_service(
        "sandbox",
        vec![(StatusCode::OK, None)],
        Arc::clone(&got),
    )
    .await;
    let apns = Apns::new(credentials, production, sandbox).unwrap();
    let start = 1_800_000_000_000;
    let note = message(NotifyKind::LastCall, "last_call:20832", start + 3_600_000);
    let phone = super::tests::production();
    for delivery in [
        Delivery::Sent,
        Delivery::Gone,
        Delivery::Gone,
        Delivery::Failed,
        Delivery::Failed,
        Delivery::Failed,
        Delivery::Failed,
        Delivery::Sent,
    ] {
        assert_eq!(apns.push(&phone, &note, start).await, delivery);
    }
    // The sandbox's device to the sandbox
    let development = device(TOKEN, Some(&ApnsEnvironment::Development.keys()));
    assert_eq!(apns.push(&development, &note, start).await, Delivery::Sent);

    let requests = got.lock().unwrap().clone();
    assert_eq!(requests.len(), 9);
    let first = &requests[0];
    assert_eq!(first.service, "production");
    assert_eq!(first.version, Version::HTTP_2);
    assert_eq!(first.token, TOKEN);
    assert_eq!(first.headers["apns-topic"], "rs.hashi");
    assert_eq!(first.headers["apns-push-type"], "alert");
    assert_eq!(first.headers["apns-priority"], "10");
    assert_eq!(first.headers["apns-expiration"], "1800003600");
    assert_eq!(first.headers["apns-collapse-id"], "last_call:20832");
    assert_eq!(first.body["aps"]["alert"]["title"], "Letzter Aufruf für deine Serie");
    assert_eq!(first.body["url"], "https://hashi.rs/de");
    let (token, iat) = bearer(first, &public);
    assert_eq!(iat, start / 1000);
    // One token for all of them: refused while fresh, it stays
    for request in &requests[..8] {
        assert_eq!(bearer(request, &public).0, token);
    }
    assert_eq!(requests[8].service, "sandbox");
    assert_eq!(requests[8].version, Version::HTTP_2);

    // Refused once it's 20 minutes old: the next request has a new one
    let later = start + RENEW_NOT_BEFORE_S * 1000;
    assert_eq!(apns.push(&phone, &note, later).await, Delivery::Failed);
    assert_eq!(apns.push(&phone, &note, later + 1000).await, Delivery::Sent);
    let requests = got.lock().unwrap().clone();
    assert_eq!(bearer(&requests[9], &public).0, token);
    let (renewed, iat) = bearer(&requests[10], &public);
    assert_ne!(renewed, token);
    assert_eq!(iat, later / 1000 + 1);

    // Nobody there
    let nobody = reqwest::Url::parse("http://127.0.0.1:9").unwrap();
    let (credentials, _) = super::tests::credentials();
    let apns = Apns::new(credentials, nobody.clone(), nobody).unwrap();
    assert_eq!(apns.push(&phone, &note, start).await, Delivery::Failed);
}

/// Against Apple's sandbox with the real key, by hand (it needs the key and the network):
/// `APNS_KEY_P8="$(cat AuthKey_….p8)" APNS_KEY_ID=… APNS_TEAM_ID=… cargo test --features ssr
/// --lib apple_s_sandbox -- --ignored`. A made-up token is `BadDeviceToken`, so Gone: Apple
/// took the connection (TLS, HTTP/2) and our provider token, which it would have answered with
/// 403 otherwise. `APNS_TEST_TOKEN` (and `APNS_TEST_ENV=production`) sends to a real phone.
#[tokio::test]
#[ignore = "needs Apple's push service and our key"]
async fn apple_s_sandbox_takes_our_provider_token() {
    let credentials = Credentials::from_env(|name| std::env::var(name).ok())
        .unwrap()
        .expect("APNS_KEY_P8, APNS_KEY_ID and APNS_TEAM_ID");
    let apns = Apns::new(
        credentials,
        reqwest::Url::parse(PRODUCTION).unwrap(),
        reqwest::Url::parse(SANDBOX).unwrap(),
    )
    .unwrap();
    let real = std::env::var("APNS_TEST_TOKEN").ok();
    let environment = match std::env::var("APNS_TEST_ENV").as_deref() {
        Ok("production") => ApnsEnvironment::Production,
        _ => ApnsEnvironment::Development,
    };
    let phone = device(real.as_deref().unwrap_or(TOKEN), Some(&environment.keys()));
    let now = crate::server::now_ms();
    let note = PushMessage {
        title: "Hashi".into(),
        body: "APNs test".into(),
        url: "https://hashi.rs/groups".into(),
        ..message(NotifyKind::Race, "race:test", now + 600_000)
    };
    let delivery = apns.push(&phone, &note, now).await;
    let expected = if real.is_some() {
        Delivery::Sent
    } else {
        Delivery::Gone
    };
    assert_eq!(delivery, expected);
}
