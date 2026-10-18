#![recursion_limit = "256"]

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use axum::extract::Request;
    use axum::middleware::{self, Next};
    use hashi::app::{App, shell};
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list};
    use tower_http::compression::CompressionLayer;

    if let Err(e) = hashi::server::db::init().await {
        eprintln!("{e}");
        std::process::exit(1);
    }
    hashi::server::dojo::start_dispatcher();

    // Reads the cargo-leptos settings (Cargo.toml) or the LEPTOS_* environment variables
    let conf = get_configuration(None).expect("invalid Leptos configuration");
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);
    let hash_files = leptos_options.hash_files;

    let app =
        Router::new()
            .route(
                "/sitemap.xml",
                axum::routing::get(|| async {
                    (
                        [(axum::http::header::CONTENT_TYPE, "application/xml")],
                        hashi::seo::sitemap(),
                    )
                }),
            )
            // The SAT Dojo: uploads, reports from the worker, house entries (see server/dojo.rs)
            .route(
                "/dojo/submit",
                axum::routing::post(hashi::server::dojo::submit_handler).layer(
                    axum::extract::DefaultBodyLimit::max(dojo::MAX_WASM_BYTES + 64 * 1024),
                ),
            )
            .route(
                "/dojo/report",
                axum::routing::post(hashi::server::dojo::report_handler),
            )
            // Reviewing daily results (see server/admin.rs)
            .route(
                "/admin/attempt",
                axum::routing::get(hashi::server::admin::attempt),
            )
            .route(
                "/admin/flag",
                axum::routing::post(hashi::server::admin::flag),
            )
            .route(
                "/admin/house",
                axum::routing::post(hashi::server::admin::house_form).layer(
                    axum::extract::DefaultBodyLimit::max(dojo::MAX_WASM_BYTES + 64 * 1024),
                ),
            )
            .route(
                "/dojo/board.json",
                axum::routing::get(hashi::server::dojo::board_json),
            )
            .route(
                "/dojo/house",
                axum::routing::post(hashi::server::dojo::house_handler)
                    .layer(axum::extract::DefaultBodyLimit::max(dojo::MAX_WASM_BYTES)),
            )
            .leptos_routes(&leptos_options, routes, {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            })
            .fallback(leptos_axum::file_and_error_handler(shell))
            .layer(middleware::from_fn(
                move |req: Request, next: Next| async move {
                    cache_control(hash_files, req, next).await
                },
            ))
            .layer(CompressionLayer::new())
            .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("could not bind the server address");
    println!("listening on http://{addr}");
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

/// Browsers kept an old stylesheet after a deploy because its URL never changed and no cache
/// headers were sent. With hashed file names everything under /pkg/ can be cached forever, since
/// a new build gets new names; everything else is revalidated on every visit.
#[cfg(feature = "ssr")]
async fn cache_control(
    hash_files: bool,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{HeaderValue, header::CACHE_CONTROL};
    let immutable = hash_files && req.uri().path().starts_with("/pkg/");
    let mut res = next.run(req).await;
    let value = if immutable {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    res.headers_mut()
        .entry(CACHE_CONTROL)
        .or_insert(HeaderValue::from_static(value));
    res
}

/// fly.io stops machines with SIGINT/SIGTERM, which a container's PID 1 ignores by default.
#[cfg(feature = "ssr")]
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut terminate = signal(SignalKind::terminate()).expect("could not listen for SIGTERM");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // No client-side main function, see lib.rs for the hydration entry point
}
