//! The HTTP routes of the site: the pages and server functions (Leptos), the plain axum
//! endpoints of the Dojo and the admin tools, static files, caching and compression, and the
//! app's version gate and cross-origin calls (see `server::apps`).

use axum::Router;
use axum::extract::{DefaultBodyLimit, Request};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{MethodRouter, get, post};
use leptos::config::Env;
use leptos::prelude::{IntoView, LeptosOptions};
use leptos_axum::{LeptosRoutes, generate_route_list};
use tower_http::compression::predicate::{NotForContentType, Predicate};
use tower_http::compression::{CompressionLayer, DefaultPredicate};

use super::apps::AppSettings;
use super::search::{SearchFile, SearchSettings};
use super::{admin, dojo, og};
use crate::api::{BUILD_HEADER, SESSION_HEADER};
use crate::app::App;

#[cfg(test)]
pub(super) mod tests;

/// Everything the server answers, around `shell` (`app::shell`). The database has to be set up
/// first (`db::init`), and the settings checked (`validate_configuration`).
///
/// Generic over the shell, so the code that renders the pages is compiled where the router is
/// made (the server binary, the tests) and not into the library: it is by far the heaviest
/// part of the build, and every build of the library would pay for it.
pub fn router<V: IntoView + 'static>(
    leptos_options: LeptosOptions,
    shell: fn(LeptosOptions) -> V,
) -> Router {
    let apps = AppSettings::from_env(super::env_var)
        .unwrap_or_else(|e| panic!("{e} (validate_configuration checks it at startup)"));
    let search = SearchSettings::from_env(super::env_var)
        .unwrap_or_else(|e| panic!("{e} (validate_configuration checks it at startup)"));
    router_with_client_source(
        leptos_options,
        shell,
        super::env_var("FLY_APP_NAME").is_some(),
        &apps,
        &search,
    )
}

fn router_with_client_source<V: IntoView + 'static>(
    leptos_options: LeptosOptions,
    shell: fn(LeptosOptions) -> V,
    fly: bool,
    apps: &AppSettings,
    search: &SearchSettings,
) -> Router {
    let routes = generate_route_list(App);
    let hash_files = leptos_options.hash_files;
    let build = build_id(&leptos_options).and_then(|b| HeaderValue::from_str(&b).ok());
    let min_version = apps.min_version;
    // The app's dev server runs on localhost, see `AppSettings::cors`
    let cors = apps.cors(leptos_options.env != Env::PROD);
    search_files(Router::new(), search.files())
        // Which Android app may open the site's links (see `AppSettings::asset_links`), and which
        // iOS app (`AppSettings::apple_app_site_association`). Google's verifier and Apple's CDN
        // fetch them: no redirect, and admission only budgets expensive calls
        .route(
            "/.well-known/assetlinks.json",
            app_association(apps.asset_links()),
        )
        .route(
            "/.well-known/apple-app-site-association",
            app_association(apps.apple_app_site_association()),
        )
        .route(
            "/sitemap.xml",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "application/xml")],
                    crate::seo::sitemap(),
                )
            }),
        )
        // Social preview images, per language and of the day's daily (see server/og)
        .route("/og.png", get(og::legacy_image))
        .route("/og/{file}", get(og::site_image))
        .route("/og/daily/{date}/{file}", get(og::daily_image))
        // The SAT Dojo: uploads, reports from the worker, house entries (see server/dojo)
        .route(
            "/dojo/submit",
            post(dojo::submit_handler).layer(DefaultBodyLimit::max(
                ::dojo::MAX_WASM_BYTES + 64 * 1024,
            )),
        )
        .route("/dojo/report", post(dojo::report_handler))
        // Community benchmarks: uploads and the worker's verdicts (see server/dojo/benchmarks.rs)
        .route(
            "/dojo/benchmarks/submit",
            post(dojo::benchmark_submit_handler).layer(DefaultBodyLimit::max(
                ::dojo::community::MAX_BYTES + 64 * 1024,
            )),
        )
        .route("/dojo/benchmark-report", post(dojo::benchmark_report_handler))
        // Reviewing daily results (see server/admin.rs)
        .route("/admin/attempt", get(admin::attempt))
        .route("/admin/flag", post(admin::flag))
        .route(
            "/admin/house",
            post(admin::house_form).layer(DefaultBodyLimit::max(
                ::dojo::MAX_WASM_BYTES + 64 * 1024,
            )),
        )
        .route("/dojo/board.json", get(dojo::board_json))
        // Seasons: the sets the worker evaluated on, the archive (see server/dojo/seasons.rs)
        .route(
            "/dojo/set",
            post(dojo::set_handler).layer(DefaultBodyLimit::max(dojo::MAX_SET_BYTES)),
        )
        .route("/dojo/seasons.json", get(dojo::seasons_json))
        .route("/dojo/seasons/{season}/set.tar.gz", get(dojo::set_download))
        .route("/dojo/seasons/{season}/results.csv", get(dojo::results_csv))
        .route("/dojo/seasons/{season}/results.json", get(dojo::results_json))
        .route(
            "/dojo/house",
            post(dojo::house_handler).layer(DefaultBodyLimit::max(::dojo::MAX_WASM_BYTES)),
        )
        // The web app manifest, with the apps once they are in the stores
        .route("/manifest.webmanifest", {
            let manifest = apps.manifest();
            get(move || {
                let manifest = manifest.clone();
                async move { ([(header::CONTENT_TYPE, "application/manifest+json")], manifest) }
            })
        })
        // The apps' stores, for the shell's tags and the footer's badges (`seo::AppStores`)
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let stores = apps.stores.clone();
                move || leptos::prelude::provide_context(stores.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .layer(middleware::from_fn(super::limits::admission))
        // Before admission too: a redirect costs nothing
        .layer(middleware::from_fn(canonical_redirect))
        // Before admission: an app too old to be answered costs nothing
        .layer(middleware::from_fn(move |req, next| {
            super::apps::gate(min_version, req, next)
        }))
        .layer(middleware::from_fn(move |req, next| {
            super::identify_client(req, next, fly)
        }))
        .layer(middleware::from_fn(move |req: Request, next: Next| async move {
            cache_control(hash_files, req, next).await
        }))
        .layer(middleware::from_fn(move |req: Request, next: Next| {
            let build = build.clone();
            async move {
                let mut res = next.run(req).await;
                if let Some(build) = build {
                    res.headers_mut().insert(BUILD_HEADER, build);
                }
                res
            }
        }))
        // woff2 is compressed already
        .layer(CompressionLayer::new().compress_when(
            DefaultPredicate::new().and(NotForContentType::const_new("font/")),
        ))
        // Outermost: preflights are answered here, before admission or the 404 page
        .layer(cors)
        .with_state(leptos_options)
}

/// The build of the site the server runs: the hash of its WASM.
///
/// From the hash.txt that cargo-leptos writes next to the server binary (read there, as leptos
/// reads it for the file names). Every response names it (`x-hashi-build`) and so does every page
/// (`<meta name="hashi-build">`, `app::shell`): a tab whose page names another build than a
/// failed call's answer runs an outdated version (see `api::client`).
///
/// The content hash, not a commit: CI builds the server binary and the WASM in separate jobs,
/// and the server reads the hash.txt of the browser's job, so the two always agree; a deploy
/// that doesn't change the WASM keeps the build, and its open tabs are still up to date.
/// Compiled into nothing, so no commit makes cargo rebuild the crate. `None` without hashed
/// file names (`cargo leptos watch` has them too) or without hash.txt: then nothing is compared.
pub fn build_id(options: &LeptosOptions) -> Option<String> {
    if !options.hash_files {
        return None;
    }
    let path = std::env::current_exe()
        .ok()?
        .parent()?
        .join(&*options.hash_file);
    wasm_hash(&std::fs::read_to_string(path).ok()?).map(str::to_owned)
}

/// The `wasm: <hash>` line of hash.txt.
fn wasm_hash(hashes: &str) -> Option<&str> {
    hashes
        .lines()
        .find_map(|line| line.trim().strip_prefix("wasm:"))
        .map(str::trim)
        .filter(|hash| !hash.is_empty())
}

/// The files of the search engines' consoles and of `IndexNow` at the root (see `server::search`),
/// each under its exact name. Their crawlers fetch them like any page, and come back to check.
fn search_files<S: Clone + Send + Sync + 'static>(
    router: Router<S>,
    files: Vec<SearchFile>,
) -> Router<S> {
    files.into_iter().fold(router, |router, file| {
        let SearchFile {
            path,
            content_type,
            body,
        } = file;
        router.route(
            &path,
            get(move || {
                let body = body.clone();
                async move { ([(header::CONTENT_TYPE, content_type)], body) }
            }),
        )
    })
}

/// Browsers kept an old stylesheet after a deploy because its URL never changed and no cache
/// headers were sent. With hashed file names everything under /pkg/ can be cached forever, since
/// a new build gets new names; everything else is revalidated on every visit. The font files
/// always have a content hash in their names (scripts/fonts.mjs). Errors are never cached for
/// good: a hashed name that isn't there yet (a page from another deploy) would stay broken.
/// A response with the app's session token (see `auth`) is stored nowhere.
/// A file of `/.well-known` that names the site's apps, JSON, cached for an hour: a 404 without
/// one (`None`), which the phone takes as no app.
fn app_association<S: Clone + Send + Sync + 'static>(json: Option<String>) -> MethodRouter<S> {
    get(move || {
        let json = json.clone();
        async move {
            match json {
                Some(json) => (
                    [
                        (header::CONTENT_TYPE, "application/json"),
                        (header::CACHE_CONTROL, "public, max-age=3600"),
                    ],
                    json,
                )
                    .into_response(),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
    })
}

async fn cache_control(hash_files: bool, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    let immutable = (hash_files && path.starts_with("/pkg/"))
        || (path.starts_with("/fonts/")
            && std::path::Path::new(path)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("woff2")));
    let mut res = next.run(req).await;
    if res.headers().contains_key(SESSION_HEADER) {
        res.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        return res;
    }
    let value = if immutable && res.status().is_success() {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    res.headers_mut()
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static(value));
    res
}

/// The page a URL of another spelling is, for a permanent redirect: without a trailing slash
/// (`/rules/` is `/rules`, `/de/` is `/de`), and without `/en`, the default language, which has
/// no prefix (`/en/rules` is `/rules`). The query stays. These were 404s (the slash) or a second
/// URL of the same page (`/en`), which search engines and shared links may have.
///
/// Only pages: not the server's own paths, where a slash is another file (Apple's and Google's
/// verifiers want a 404 there, not a redirect). Never to another site (`//example.com`).
fn canonical_target(path: &str, query: Option<&str>) -> Option<String> {
    const NOT_PAGES: [&str; 5] = ["/.well-known/", "/api/", "/pkg/", "/fonts/", "/og/"];
    if NOT_PAGES.iter().any(|prefix| path.starts_with(prefix)) {
        return None;
    }
    let english = match path.strip_prefix("/en") {
        Some("") => "/",
        Some(rest) if rest.starts_with('/') => rest,
        _ => path,
    };
    let trimmed = english.trim_end_matches('/');
    let target = if trimmed.is_empty() { "/" } else { trimmed };
    if target == path || target.starts_with("//") {
        return None;
    }
    Some(match query {
        Some(query) => format!("{target}?{query}"),
        None => target.to_string(),
    })
}

/// Redirects requests for pages to their canonical URL, see [`canonical_target`].
async fn canonical_redirect(req: Request, next: Next) -> Response {
    let read = matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    );
    match canonical_target(req.uri().path(), req.uri().query()).filter(|_| read) {
        Some(target) => match HeaderValue::from_str(&target) {
            Ok(location) => (
                StatusCode::MOVED_PERMANENTLY,
                [(header::LOCATION, location)],
            )
                .into_response(),
            Err(_) => next.run(req).await,
        },
        None => next.run(req).await,
    }
}

#[cfg(test)]
mod redirect_tests {
    use super::canonical_target;

    #[test]
    fn pages_have_one_url() {
        for (path, query, target) in [
            ("/rules/", None, "/rules"),
            ("/de/", None, "/de"),
            ("/de/rules/", None, "/de/rules"),
            ("/de/rules///", None, "/de/rules"),
            ("/practice/", Some("size=9"), "/practice?size=9"),
            ("/en", None, "/"),
            ("/en/", None, "/"),
            ("/en/rules", None, "/rules"),
            ("/en/rules/", Some("a=1&b=2"), "/rules?a=1&b=2"),
            ("/en/g/k7m2xq_9", None, "/g/k7m2xq_9"),
            ("/zh-Hant/dojo/seasons/", None, "/zh-Hant/dojo/seasons"),
            ("///", None, "/"),
        ] {
            assert_eq!(
                canonical_target(path, query).as_deref(),
                Some(target),
                "{path}"
            );
        }
        for (path, query) in [
            ("/", None),
            ("/", Some("x=1")),
            ("/rules", None),
            ("/de", None),
            ("/ranks", Some("day=20747")),
            // Not English
            ("/english", None),
            ("/enx/rules", None),
            ("/de/en/rules", None),
            // Never to another site
            ("//example.com/", None),
            ("/en//example.com", None),
            ("/en//example.com/", None),
            // Not pages
            ("/.well-known/apple-app-site-association/", None),
            ("/api/daily_today/", None),
            ("/pkg/x/", None),
            ("/fonts/", None),
            ("/og/daily/", None),
        ] {
            assert_eq!(canonical_target(path, query), None, "{path}");
        }
    }
}
