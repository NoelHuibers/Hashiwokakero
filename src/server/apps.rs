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
//! Links to some pages of `https://hashi.rs` open in the app instead of the browser (App Links on
//! Android, Universal Links on iOS, `seo::app_link`). The phone believes the app only when the
//! site names it: Android its certificates, [`AppSettings::asset_links`] at
//! `/.well-known/assetlinks.json` (`HASHI_ANDROID_CERT_SHA256`), iOS its app ID,
//! [`AppSettings::apple_app_site_association`] at `/.well-known/apple-app-site-association`
//! (`HASHI_IOS_APP_IDS`).

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
use crate::seo::AppStores;

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
    /// `HASHI_IOS_APP_IDS`: the app IDs of the iOS app, `<team ID>.<bundle ID>`. None: the
    /// site claims no app, `apple-app-site-association` is a 404.
    pub ios_app_ids: Vec<String>,
    /// `HASHI_APP_STORE_ID` and `HASHI_PLAY_PACKAGE`: the apps' pages in the stores, which the
    /// site advertises once they are set (`seo::AppStores`). Unset until the apps are live.
    pub stores: AppStores,
}

impl AppSettings {
    /// Reads and checks the settings; a malformed one is an error, never ignored (the server
    /// refuses to start, see `validate_configuration`).
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let setting = |name: &str| {
            var(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
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
        let ios_app_ids = match var("HASHI_IOS_APP_IDS")
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            None => Vec::new(),
            Some(ids) => ids
                .split(',')
                .map(|id| {
                    let id = id.trim();
                    app_id(id).then(|| id.to_string()).ok_or_else(|| {
                        format!(
                            "HASHI_IOS_APP_IDS: {id:?} is no app ID like ABCDE12345.rs.hashi, the \
                             team ID (10 uppercase letters and digits) and the bundle ID"
                        )
                    })
                })
                .collect::<Result<_, _>>()?,
        };
        let app_store = match setting("HASHI_APP_STORE_ID") {
            None => None,
            Some(id) if store_id(&id) => Some(id),
            Some(id) => {
                return Err(format!(
                    "HASHI_APP_STORE_ID: {id:?} is no App Store ID like 6826916346, the digits of \
                     the app's apps.apple.com/app/id… link (App Store Connect, App Information, \
                     Apple ID)"
                ));
            }
        };
        let google_play = match setting("HASHI_PLAY_PACKAGE") {
            None => None,
            Some(package) if package_name(&package) => Some(package),
            Some(package) => {
                return Err(format!(
                    "HASHI_PLAY_PACKAGE: {package:?} is no Android package name like rs.hashi"
                ));
            }
        };
        Ok(Self {
            min_version,
            origins,
            android_certs,
            ios_app_ids,
            stores: AppStores {
                app_store,
                google_play,
            },
        })
    }

    /// The web app manifest (`/manifest.webmanifest`): `public/manifest.webmanifest`, and once
    /// the apps are in the stores their pages as `related_applications`, so browsers know the
    /// site's apps (`getInstalledRelatedApps`). `prefer_related_applications` is false: the
    /// site works on its own, browsers shouldn't offer the store instead of installing it.
    pub fn manifest(&self) -> String {
        let mut manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../public/manifest.webmanifest"))
                .expect("public/manifest.webmanifest is JSON");
        let mut related = Vec::new();
        if let (Some(package), Some(url)) =
            (&self.stores.google_play, self.stores.google_play_url(None))
        {
            related.push(serde_json::json!({ "platform": "play", "url": url, "id": package }));
        }
        if let Some(url) = self.stores.app_store_url() {
            related.push(serde_json::json!({ "platform": "itunes", "url": url }));
        }
        if !related.is_empty() {
            manifest["related_applications"] = related.into();
            manifest["prefer_related_applications"] = false.into();
        }
        manifest.to_string()
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

    /// The site's `apple-app-site-association` (`/.well-known/`): the iOS apps of `ios_app_ids`
    /// may open its App Links (`seo::app_link_components`, the pages Android's intent filter
    /// lists). Apple's CDN fetches it from the site, iOS from the CDN when the app is installed
    /// or updated, and opens the links in the app only if its app ID is listed and its
    /// entitlements name `applinks:hashi.rs`. `None` without app IDs.
    ///
    /// No `webcredentials`: the app's entitlements would have to name the site for those too,
    /// and the deep-link plugin writes its Associated Domains on every build.
    pub fn apple_app_site_association(&self) -> Option<String> {
        if self.ios_app_ids.is_empty() {
            return None;
        }
        let association = serde_json::json!({
            "applinks": {
                "details": [{
                    "appIDs": self.ios_app_ids,
                    "components": crate::seo::app_link_components(),
                }],
            },
        });
        Some(association.to_string())
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

/// An app ID of an iOS app as Apple's developer portal shows it: the team ID, ten uppercase
/// letters and digits, a dot and the bundle ID (letters, digits, `-`, parts separated by dots).
fn app_id(id: &str) -> bool {
    let Some((team, bundle)) = id.split_once('.') else {
        return false;
    };
    team.len() == 10
        && team
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && bundle.split('.').all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// The Apple ID of an app in the App Store: its number, the digits in `apps.apple.com/app/id…`.
fn store_id(id: &str) -> bool {
    (1..=12).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()) && !id.starts_with('0')
}

/// An Android package name (application ID), the `id` of a Google Play link: at least two
/// parts separated by dots, each a letter followed by letters, digits and `_`.
fn package_name(package: &str) -> bool {
    package.len() <= 150
        && package.split('.').count() >= 2
        && package.split('.').all(|part| {
            part.starts_with(|c: char| c.is_ascii_alphabetic())
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
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
        assert_eq!(defaults.ios_app_ids, Vec::<String>::new());
        assert_eq!(defaults.apple_app_site_association(), None);
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
        for ids in [
            // The team ID alone, or the bundle ID
            "Q35TCGM5PW",
            "rs.hashi",
            "Q35TCGM5PW.",
            ".rs.hashi",
            // Lowercase, too short, too long
            "q35tcgm5pw.rs.hashi",
            "Q35TCGM5P.rs.hashi",
            "Q35TCGM5PWX.rs.hashi",
            "Q35TCGM5PW.rs..hashi",
            "Q35TCGM5PW.rs.hashi.",
            "Q35TCGM5PW.rs hashi",
            "Q35TCGM5PW.rs.hashi,",
            "Q35TCGM5PW.rs.hashi;Q35TCGM5PW.rs.hashi.beta",
            "Q35TCGM5PW.*",
        ] {
            let error = settings(&[("HASHI_IOS_APP_IDS", ids)]).unwrap_err();
            assert!(error.contains("HASHI_IOS_APP_IDS"), "{ids:?}: {error}");
        }
    }

    /// The upload key's certificate and Google's app signing certificate, as in fly.toml.
    const UPLOAD_CERT: &str = "BB:86:D4:C6:60:8F:98:D8:FC:68:BA:3C:5F:23:CC:4C:08:80:D7:76:61:A8:68:56:10:24:C5:CB:1D:38:5A:4B";
    const PLAY_CERT: &str = "5B:25:46:4B:F1:0A:07:4B:0E:3B:29:3E:C4:07:7E:14:25:93:67:0D:06:75:63:C1:BC:7B:7C:9D:26:D6:A9:D9";

    /// fly.toml names both: the APK of the GitHub release and the app from Play may open the
    /// site's links.
    #[test]
    fn fly_toml_names_the_upload_and_the_play_certificate() {
        let line = include_str!("../../fly.toml")
            .lines()
            .find_map(|l| l.trim().strip_prefix("HASHI_ANDROID_CERT_SHA256 = "))
            .expect("HASHI_ANDROID_CERT_SHA256 in fly.toml");
        let value = line.trim_matches('"');
        let parsed = settings(&[("HASHI_ANDROID_CERT_SHA256", value)]).unwrap();
        assert_eq!(parsed.android_certs, [UPLOAD_CERT, PLAY_CERT]);
    }

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

    /// The app ID of fly.toml, or several: each app may open the site's links, all of them.
    #[test]
    fn the_association_names_the_ios_apps() {
        let one = settings(&[("HASHI_IOS_APP_IDS", "Q35TCGM5PW.rs.hashi")]).unwrap();
        assert_eq!(one.ios_app_ids, ["Q35TCGM5PW.rs.hashi"]);
        let association: serde_json::Value =
            serde_json::from_str(&one.apple_app_site_association().unwrap()).unwrap();
        assert_eq!(
            association,
            serde_json::json!({
                "applinks": {
                    "details": [{
                        "appIDs": ["Q35TCGM5PW.rs.hashi"],
                        "components": crate::seo::app_link_components(),
                    }],
                },
            })
        );
        let two = settings(&[(
            "HASHI_IOS_APP_IDS",
            " Q35TCGM5PW.rs.hashi , A1B2C3D4E5.rs.hashi-beta\n",
        )])
        .unwrap();
        assert_eq!(
            two.ios_app_ids,
            ["Q35TCGM5PW.rs.hashi", "A1B2C3D4E5.rs.hashi-beta"]
        );
        assert_eq!(
            settings(&[("HASHI_IOS_APP_IDS", " ")])
                .unwrap()
                .apple_app_site_association(),
            None
        );
        // One platform's setting claims nothing for the other
        let android = settings(&[("HASHI_ANDROID_CERT_SHA256", UPLOAD_CERT)]).unwrap();
        assert_eq!(android.apple_app_site_association(), None);
        assert_eq!(one.asset_links(), None);
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

    /// The stores' IDs, once the apps are live: unset (or empty, as in fly.toml) there are none.
    #[test]
    fn the_stores_are_set_when_the_apps_are_live() {
        assert!(settings(&[]).unwrap().stores.is_empty());
        let empty = settings(&[("HASHI_APP_STORE_ID", ""), ("HASHI_PLAY_PACKAGE", " ")]).unwrap();
        assert!(empty.stores.is_empty());
        let both = settings(&[
            ("HASHI_APP_STORE_ID", " 6826916346 "),
            ("HASHI_PLAY_PACKAGE", "rs.hashi"),
        ])
        .unwrap();
        assert_eq!(
            both.stores,
            AppStores {
                app_store: Some("6826916346".into()),
                google_play: Some("rs.hashi".into()),
            }
        );
        assert!(settings(&[("HASHI_PLAY_PACKAGE", "com.example.my_app2")]).is_ok());
        for (name, value) in [
            ("HASHI_APP_STORE_ID", "id6826916346"),
            (
                "HASHI_APP_STORE_ID",
                "https://apps.apple.com/app/id6826916346",
            ),
            ("HASHI_APP_STORE_ID", "0123"),
            ("HASHI_APP_STORE_ID", "1234567890123"),
            ("HASHI_APP_STORE_ID", "Q35TCGM5PW.rs.hashi"),
            ("HASHI_PLAY_PACKAGE", "hashi"),
            ("HASHI_PLAY_PACKAGE", "rs.hashi&hl=de"),
            ("HASHI_PLAY_PACKAGE", "rs..hashi"),
            ("HASHI_PLAY_PACKAGE", "rs.1hashi"),
            ("HASHI_PLAY_PACKAGE", "rs.hashi,rs.other"),
            (
                "HASHI_PLAY_PACKAGE",
                "https://play.google.com/store/apps/details?id=rs.hashi",
            ),
        ] {
            let error = settings(&[(name, value)]).unwrap_err();
            assert!(error.starts_with(name), "{value}: {error}");
        }
    }

    /// The manifest is the site's file, with the apps once they are in the stores. Browsers
    /// shouldn't prefer the stores to the site.
    #[test]
    fn the_manifest_names_the_apps_in_the_stores() {
        let file: serde_json::Value =
            serde_json::from_str(include_str!("../../public/manifest.webmanifest")).unwrap();
        let without: serde_json::Value =
            serde_json::from_str(&settings(&[]).unwrap().manifest()).unwrap();
        assert_eq!(without, file);
        assert_eq!(file["id"], "/");
        assert_eq!(file["start_url"], "/");
        assert!(file.get("related_applications").is_none());

        let both = settings(&[
            ("HASHI_APP_STORE_ID", "6826916346"),
            ("HASHI_PLAY_PACKAGE", "rs.hashi"),
        ])
        .unwrap();
        let with: serde_json::Value = serde_json::from_str(&both.manifest()).unwrap();
        assert_eq!(
            with["related_applications"],
            serde_json::json!([
                {
                    "platform": "play",
                    "url": "https://play.google.com/store/apps/details?id=rs.hashi",
                    "id": "rs.hashi",
                },
                { "platform": "itunes", "url": "https://apps.apple.com/app/id6826916346" },
            ])
        );
        assert_eq!(with["prefer_related_applications"], false);
        assert_eq!(with["name"], file["name"]);
        // One store alone
        let ios = settings(&[("HASHI_APP_STORE_ID", "6826916346")]).unwrap();
        let with: serde_json::Value = serde_json::from_str(&ios.manifest()).unwrap();
        assert_eq!(with["related_applications"].as_array().unwrap().len(), 1);
    }
}
