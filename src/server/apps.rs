//! Requests of the app (docs/mobile.md): the version it says it is, the oldest version the server
//! still answers, and the origins its `WebView` calls from.
//!
//! The app sends `x-hashi-app: <platform>/<major>.<minor>.<patch>` (`api::APP_HEADER`, e.g.
//! `android/1.0.0`) with every call. Installed apps keep their version for months, so a server
//! function they rely on can't simply change; when one has to, `HASHI_APP_MIN_VERSION` turns the
//! older apps away with `ApiError::AppOutdated`, and they ask for an update from the store.
//!
//! The `WebView` runs on its own origin (`https://tauri.localhost` on Android, `tauri://localhost`
//! on iOS), so its calls are cross-origin: [`AppSettings::cors`] allows them, for those origins
//! only (`HASHI_APP_ORIGINS`). No credentials: the app sends its session as a bearer token.
//!
//! Links to some pages of `https://hashi.rs` open in the Android app instead of the browser (App
//! Links, `seo::app_link`). Android believes the app only when the site names the app's
//! certificates: [`AppSettings::asset_links`], served at `/.well-known/assetlinks.json`
//! (`HASHI_ANDROID_CERT_SHA256`).

use std::str::FromStr;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::Response;
use leptos::server_fn::ContentType as _;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorEncoding};
use leptos::server_fn::response::Res as _;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::api::{APP_HEADER, BUILD_HEADER, SESSION_HEADER};
use crate::model::ApiError;

/// The app's origins when `HASHI_APP_ORIGINS` isn't set: Android's `WebView` (with
/// `useHttpsScheme`) and iOS's.
const DEFAULT_ORIGINS: &str = "https://tauri.localhost,tauri://localhost";

/// The Android app's application ID (`identifier` in app/src-tauri/tauri.conf.json). Debug
/// builds (`rs.hashi.debug`) aren't in the asset links: their links are never verified.
pub const ANDROID_PACKAGE: &str = "rs.hashi";

/// A version of the app, `major.minor.patch`. Ordered as numbers, so 1.10.0 comes after 1.9.0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

impl FromStr for Version {
    type Err = ();

    /// Exactly three numbers of decimal digits: no `v`, no signs, no pre-release or build suffix.
    fn from_str(s: &str) -> Result<Self, ()> {
        let mut parts = s.split('.').map(|part| {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(());
            }
            part.parse::<u32>().map_err(drop)
        });
        let version = Self(
            parts.next().ok_or(())??,
            parts.next().ok_or(())??,
            parts.next().ok_or(())??,
        );
        if parts.next().is_some() {
            return Err(());
        }
        Ok(version)
    }
}

/// The platform and version in an `x-hashi-app` header, `<platform>/<version>`.
///
/// Platforms other than `android` and `ios` are fine too: an older server shouldn't turn a new
/// one away. `None` when it doesn't have that form.
pub fn parse_app_header(value: &str) -> Option<(&str, Version)> {
    let (platform, version) = value.split_once('/')?;
    let platform_ok = !platform.is_empty()
        && platform
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if !platform_ok {
        return None;
    }
    Some((platform, version.parse().ok()?))
}

/// The app settings of the server, from the environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppSettings {
    /// `HASHI_APP_MIN_VERSION`: older apps get `AppOutdated`. `None`: every version is answered.
    pub min_version: Option<Version>,
    /// `HASHI_APP_ORIGINS`: where the app's `WebView` calls from.
    pub origins: Vec<HeaderValue>,
    /// `HASHI_ANDROID_CERT_SHA256`: the SHA-256 fingerprints of the certificates the Android app
    /// is signed with, `AB:CD:…`. None: the site claims no app, `assetlinks.json` is a 404.
    pub android_certs: Vec<String>,
}

impl AppSettings {
    /// Reads and checks the settings; a malformed one is an error, never ignored (the server
    /// refuses to start, see `validate_configuration`).
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let min_version = match var("HASHI_APP_MIN_VERSION")
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            None => None,
            Some(v) => Some(v.parse().map_err(|()| {
                format!("HASHI_APP_MIN_VERSION must be a version like 1.2.3, not {v:?}")
            })?),
        };
        let origins = var("HASHI_APP_ORIGINS");
        let origins = origins
            .as_deref()
            .unwrap_or(DEFAULT_ORIGINS)
            .split(',')
            .map(|origin| parse_origin(origin.trim()))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("HASHI_APP_ORIGINS: {e}"))?;
        let android_certs = match var("HASHI_ANDROID_CERT_SHA256")
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            None => Vec::new(),
            Some(certs) => certs
                .split(',')
                .map(|cert| {
                    let cert = cert.trim();
                    fingerprint(cert).then(|| cert.to_string()).ok_or_else(|| {
                        format!(
                            "HASHI_ANDROID_CERT_SHA256: {cert:?} is no SHA-256 fingerprint like \
                             AB:CD:…, 32 bytes in uppercase hex"
                        )
                    })
                })
                .collect::<Result<_, _>>()?,
        };
        Ok(Self {
            min_version,
            origins,
            android_certs,
        })
    }

    /// The Digital Asset Links of the site (`/.well-known/assetlinks.json`): the Android app
    /// signed with one of `android_certs` may open the site's links. Android fetches it when the
    /// app is installed, and opens links of `https://hashi.rs` in the app only if the
    /// certificate of the installed app is listed (`autoVerify` in the app's manifest). `None`
    /// without certificates.
    ///
    /// One certificate is the upload key's, which signs the APK of the GitHub release, the
    /// other Google's, which signs the app installed from Play (docs/deployment.md).
    pub fn asset_links(&self) -> Option<String> {
        if self.android_certs.is_empty() {
            return None;
        }
        let links = serde_json::json!([{
            "relation": ["delegate_permission/common.handle_all_urls"],
            "target": {
                "namespace": "android_app",
                "package_name": ANDROID_PACKAGE,
                "sha256_cert_fingerprints": self.android_certs,
            },
        }]);
        Some(links.to_string())
    }

    /// Cross-origin calls from the app: its origins, and in development (`dev`) any port of
    /// `http://localhost` and `http://127.0.0.1`, where the app's dev server runs. Preflights
    /// are answered here, before anything else runs. Other origins get no `Allow-Origin`, so the
    /// few endpoints for scripts keep their own `*`.
    pub fn cors(&self, dev: bool) -> CorsLayer {
        let origins = self.origins.clone();
        CorsLayer::new()
            .allow_origin(AllowOrigin::predicate(move |origin, _| {
                origins.contains(origin) || (dev && local_origin(origin.as_bytes()))
            }))
            .allow_methods([Method::GET, Method::POST])
            .allow_headers([
                header::CONTENT_TYPE,
                header::ACCEPT,
                header::AUTHORIZATION,
                HeaderName::from_static(APP_HEADER),
            ])
            .expose_headers([
                HeaderName::from_static(BUILD_HEADER),
                HeaderName::from_static(SESSION_HEADER),
            ])
            .max_age(Duration::from_hours(2))
    }
}

/// Whether a request is from an app older than the minimum. Only the app sends its header, so a
/// value it can't have sent counts as outdated too. Requests without it (the web) never are, and
/// nothing is without a minimum.
fn outdated(min_version: Option<Version>, headers: &HeaderMap) -> bool {
    let (Some(min), Some(value)) = (min_version, headers.get(APP_HEADER)) else {
        return false;
    };
    !value.is_empty()
        && value
            .to_str()
            .ok()
            .and_then(parse_app_header)
            .is_none_or(|(_, version)| version < min)
}

/// An origin as the browser sends it: `scheme://host[:port]`, lowercase, nothing after.
fn parse_origin(origin: &str) -> Result<HeaderValue, String> {
    let shaped = origin.split_once("://").is_some_and(|(scheme, host)| {
        scheme.starts_with(|c: char| c.is_ascii_lowercase())
            && scheme
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+-.".contains(&b))
            && !host.is_empty()
            && host
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-.:[]".contains(&b))
    });
    if !shaped {
        return Err(format!(
            "{origin:?} is no origin like https://tauri.localhost (lowercase, no path)"
        ));
    }
    HeaderValue::from_str(origin).map_err(|e| e.to_string())
}

/// A certificate's SHA-256 fingerprint as keytool and the Play Console show it: 32 bytes in
/// uppercase hex, separated by colons, the form `assetlinks.json` takes.
fn fingerprint(cert: &str) -> bool {
    cert.len() == 32 * 3 - 1
        && cert.split(':').all(|byte| {
            byte.len() == 2
                && byte
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
        })
}

/// `http://localhost` or `http://127.0.0.1`, with any port or none.
fn local_origin(origin: &[u8]) -> bool {
    let Some(rest) = origin
        .strip_prefix(b"http://localhost")
        .or_else(|| origin.strip_prefix(b"http://127.0.0.1"))
    else {
        return false;
    };
    match rest {
        [] => true,
        [b':', port @ ..] => std::str::from_utf8(port)
            .is_ok_and(|p| p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u16>().is_ok()),
        _ => false,
    }
}

/// Answers server function calls of an app below the minimum version with `AppOutdated`.
///
/// Encoded the way a server function answers its errors, without calling it. Runs before
/// admission, so they cost nothing of the network's budget.
pub async fn gate(min_version: Option<Version>, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if !path.starts_with("/api/") || !outdated(min_version, req.headers()) {
        return next.run(req).await;
    }
    let error = ApiError::AppOutdated.fail().ser();
    let mut res = Response::<Body>::error_response(path, error);
    res.content_type(ServerFnErrorEncoding::CONTENT_TYPE);
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_three_numbers() {
        assert_eq!("1.0.0".parse(), Ok(Version(1, 0, 0)));
        assert_eq!("0.12.345".parse(), Ok(Version(0, 12, 345)));
        for bad in [
            "",
            "1",
            "1.0",
            "1.0.0.0",
            "v1.0.0",
            "1.0.0-beta",
            "1.0.0+7",
            "1..0",
            "1.0.",
            " 1.0.0",
            "+1.0.0",
            "-1.0.0",
            "1.a.0",
            "99999999999.0.0",
        ] {
            assert_eq!(bad.parse::<Version>(), Err(()), "{bad:?}");
        }
        // Numbers, not text
        assert!(Version(1, 10, 0) > Version(1, 9, 0));
        assert!(Version(2, 0, 0) > Version(1, 99, 99));
    }

    #[test]
    fn the_app_header_names_platform_and_version() {
        assert_eq!(
            parse_app_header("android/1.0.0"),
            Some(("android", Version(1, 0, 0)))
        );
        assert_eq!(
            parse_app_header("ios/2.3.4"),
            Some(("ios", Version(2, 3, 4)))
        );
        // A platform this server doesn't know yet
        assert_eq!(
            parse_app_header("harmony-os/1.2.3"),
            Some(("harmony-os", Version(1, 2, 3)))
        );
        for bad in [
            "",
            "1.0.0",
            "/1.0.0",
            "android",
            "android/",
            "android/1.0",
            "android/1.0.0/x",
            "Android/1.0.0",
            "android 1.0.0",
            "and roid/1.0.0",
        ] {
            assert_eq!(parse_app_header(bad), None, "{bad:?}");
        }
    }

    fn settings(pairs: &[(&str, &str)]) -> Result<AppSettings, String> {
        AppSettings::from_env(|name| {
            pairs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v).to_string())
        })
    }

    #[test]
    fn settings_have_defaults() {
        let defaults = settings(&[]).unwrap();
        assert_eq!(defaults.min_version, None);
        assert_eq!(defaults.android_certs, Vec::<String>::new());
        assert_eq!(defaults.asset_links(), None);
        assert_eq!(
            defaults.origins,
            ["https://tauri.localhost", "tauri://localhost"]
        );
        assert_eq!(
            settings(&[("HASHI_APP_MIN_VERSION", "")])
                .unwrap()
                .min_version,
            None
        );
        let set = settings(&[
            ("HASHI_APP_MIN_VERSION", " 1.4.0 "),
            (
                "HASHI_APP_ORIGINS",
                "https://tauri.localhost, http://tauri.localhost",
            ),
        ])
        .unwrap();
        assert_eq!(set.min_version, Some(Version(1, 4, 0)));
        assert_eq!(
            set.origins,
            ["https://tauri.localhost", "http://tauri.localhost"]
        );
    }

    /// A typo in a setting stops the server instead of quietly letting every app in, or none.
    #[test]
    fn malformed_settings_are_errors() {
        for version in ["1.4", "latest", "v1.4.0", "1.4.0-rc1"] {
            let error = settings(&[("HASHI_APP_MIN_VERSION", version)]).unwrap_err();
            assert!(error.contains("HASHI_APP_MIN_VERSION"), "{error}");
        }
        for origins in [
            "",
            "https://tauri.localhost,",
            "*",
            "tauri.localhost",
            "https://tauri.localhost/",
            "https://tauri.localhost/app",
            "https://Tauri.localhost",
            "https://tauri.localhost;tauri://localhost",
        ] {
            let error = settings(&[("HASHI_APP_ORIGINS", origins)]).unwrap_err();
            assert!(error.contains("HASHI_APP_ORIGINS"), "{origins:?}: {error}");
        }
        for certs in [
            // Lowercase, as some tools print it
            UPLOAD_CERT.to_lowercase(),
            // Without colons
            UPLOAD_CERT.replace(':', ""),
            // SHA-1, the other fingerprint the Play Console shows
            "5E:8F:16:06:2E:A3:CD:2C:4A:0D:54:78:76:BA:A6:F3:8C:AB:F6:25".into(),
            format!("{UPLOAD_CERT}:00"),
            UPLOAD_CERT[3..].into(),
            UPLOAD_CERT.replacen("BB", "BG", 1),
            UPLOAD_CERT.replacen("BB:", "BB::", 1),
            format!("{UPLOAD_CERT},"),
            format!("{UPLOAD_CERT};{PLAY_CERT}"),
            "SHA256: BB:86".into(),
        ] {
            let error = settings(&[("HASHI_ANDROID_CERT_SHA256", &certs)]).unwrap_err();
            assert!(
                error.contains("HASHI_ANDROID_CERT_SHA256"),
                "{certs:?}: {error}"
            );
        }
    }

    /// The upload key's certificate (fly.toml) and another one, as Play's would be.
    const UPLOAD_CERT: &str = "BB:86:D4:C6:60:8F:98:D8:FC:68:BA:3C:5F:23:CC:4C:08:80:D7:76:61:A8:68:56:10:24:C5:CB:1D:38:5A:4B";
    const PLAY_CERT: &str = "0F:1E:2D:3C:4B:5A:69:78:87:96:A5:B4:C3:D2:E1:F0:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF";

    /// One certificate or several, the upload key's and Google's: the app of either may open
    /// the site's links.
    #[test]
    fn asset_links_name_the_app_and_its_certificates() {
        let one = settings(&[("HASHI_ANDROID_CERT_SHA256", UPLOAD_CERT)]).unwrap();
        assert_eq!(one.android_certs, [UPLOAD_CERT]);
        let links: serde_json::Value = serde_json::from_str(&one.asset_links().unwrap()).unwrap();
        assert_eq!(
            links,
            serde_json::json!([{
                "relation": ["delegate_permission/common.handle_all_urls"],
                "target": {
                    "namespace": "android_app",
                    "package_name": "rs.hashi",
                    "sha256_cert_fingerprints": [UPLOAD_CERT],
                },
            }])
        );
        let two = settings(&[(
            "HASHI_ANDROID_CERT_SHA256",
            &format!(" {UPLOAD_CERT} , {PLAY_CERT}\n"),
        )])
        .unwrap();
        assert_eq!(two.android_certs, [UPLOAD_CERT, PLAY_CERT]);
        assert_eq!(
            settings(&[("HASHI_ANDROID_CERT_SHA256", " ")])
                .unwrap()
                .asset_links(),
            None
        );
    }

    #[test]
    fn local_origins_are_localhost_on_any_port() {
        for origin in [
            "http://localhost",
            "http://localhost:1420",
            "http://127.0.0.1:8080",
            "http://127.0.0.1:65535",
        ] {
            assert!(local_origin(origin.as_bytes()), "{origin}");
        }
        for origin in [
            "https://localhost:1420",
            "http://localhost.evil.example",
            "http://localhost:1420.evil.example",
            "http://localhost:",
            "http://localhost:65536",
            "http://localhost:-1",
            "http://127.0.0.1.evil.example",
            "http://127.0.0.2:8080",
            "http://example.com",
        ] {
            assert!(!local_origin(origin.as_bytes()), "{origin}");
        }
    }

    #[test]
    fn apps_below_the_minimum_are_outdated() {
        let headers = |value: Option<&str>| {
            let mut headers = HeaderMap::new();
            if let Some(value) = value {
                headers.insert(APP_HEADER, value.parse().unwrap());
            }
            headers
        };
        let min = Some(Version(1, 2, 0));
        for (value, expected) in [
            (None, false),
            (Some(""), false),
            (Some("android/1.1.9"), true),
            (Some("android/0.9.0"), true),
            (Some("android/1.2.0"), false),
            (Some("ios/1.2.1"), false),
            (Some("android/1.10.0"), false),
            (Some("plan9/2.0.0"), false),
            (Some("plan9/1.0.0"), true),
            // Only the app sends the header: one it can't have sent is no app worth answering
            (Some("android/1.2"), true),
            (Some("garbage"), true),
        ] {
            assert_eq!(outdated(min, &headers(value)), expected, "{value:?}");
            assert!(!outdated(None, &headers(value)), "{value:?}");
        }
    }
}
