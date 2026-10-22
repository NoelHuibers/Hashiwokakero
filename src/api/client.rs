//! The client every server function call goes through (`#[server(client = HashiClient)]`): the
//! browser's `fetch`, which also notes the build of the server that answered. With it a call
//! that fails because the tab runs an older build than the server asks for a reload instead of
//! showing the error, see [`is_outdated`].
//!
//! A build is the hash of the site's WASM (`wasm:` in hash.txt, read by the server next to its
//! binary): the server sends it with every response (`x-hashi-build`, see `server/router`) and
//! in every page (`<meta name="hashi-build">`, see `app::shell`), so a tab knows the build of
//! the WASM it runs from the page it loaded.
//!
//! In the app the client also says who it is and carries the session, see `api::app`. The app
//! can't reload into a newer build, it needs an update from the store ([`needs_update`]).

use std::sync::Mutex;

use futures::{Sink, Stream};
use leptos::prelude::ServerFnError;
use leptos::server_fn::Bytes;
use leptos::server_fn::client::Client;
use leptos::server_fn::client::browser::BrowserClient;
use leptos::server_fn::error::FromServerFnError;
use leptos::server_fn::request::browser::BrowserRequest;
use leptos::server_fn::response::browser::BrowserResponse;

use crate::model::{ApiError, is_outdated_error};

/// The response header with the build of the server, see the module docs.
pub const BUILD_HEADER: &str = "x-hashi-build";

/// The request header that marks a request of the app (any value but an empty one).
///
/// The app's `WebView` runs on its own origin, where the session cookie of the site is a
/// third-party cookie it blocks, so the app gets its session token in [`SESSION_HEADER`]
/// instead, and sends it back as `Authorization: Bearer hs1.<token>` ([`SESSION_BEARER`], see
/// `server::auth`).
pub const APP_HEADER: &str = "x-hashi-app";

/// The response header with the app's new session token; empty when the session ended.
pub const SESSION_HEADER: &str = "x-hashi-session";

/// What comes before the session token in the app's `Authorization: Bearer` header.
///
/// The admin and the Dojo worker send bearer tokens too: the prefix keeps a session token from
/// ever passing for one of theirs, and theirs for a session.
pub const SESSION_BEARER: &str = "hs1.";

/// The build named by the last response of a server function call, if it named one.
static SERVER_BUILD: Mutex<Option<String>> = Mutex::new(None);

/// The browser's `fetch` client of `server_fn`, noting the server's build of every response.
#[derive(Debug)]
pub struct HashiClient;

impl<E, InputStreamError, OutputStreamError> Client<E, InputStreamError, OutputStreamError>
    for HashiClient
where
    E: FromServerFnError,
    InputStreamError: FromServerFnError,
    OutputStreamError: FromServerFnError,
{
    type Request = BrowserRequest;
    type Response = BrowserResponse;

    async fn send(req: Self::Request) -> Result<Self::Response, E> {
        // The app says who it is and sends its session, see `api::app`
        #[cfg(feature = "csr")]
        let (_admitted, sent) = {
            let admitted = super::app::admit(
                &super::app::SESSION_GATE,
                super::app::may_make_session(&req.url()),
            )
            .await;
            let token = super::app::session_token();
            if let Some(app) = super::app::app_header() {
                let headers = req.headers();
                for (name, value) in super::app::request_headers(app, token.as_deref()) {
                    headers.set(name, &value);
                }
            }
            (admitted, token)
        };
        let res =
            <BrowserClient as Client<E, InputStreamError, OutputStreamError>>::send(req).await?;
        let headers = res.generate_headers();
        let build = headers
            .get(BUILD_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if let Ok(mut server) = SERVER_BUILD.lock() {
            *server = build;
        }
        #[cfg(feature = "csr")]
        super::app::keep_session(
            sent.as_deref(),
            headers.get(SESSION_HEADER).and_then(|v| v.to_str().ok()),
        );
        Ok(res)
    }

    fn open_websocket(
        path: &str,
    ) -> impl Future<
        Output = Result<
            (
                impl Stream<Item = Result<Bytes, Bytes>> + Send + 'static,
                impl Sink<Bytes> + Send + 'static,
            ),
            E,
        >,
    > + Send {
        <BrowserClient as Client<E, InputStreamError, OutputStreamError>>::open_websocket(path)
    }

    fn spawn(future: impl Future<Output = ()> + Send + 'static) {
        <BrowserClient as Client<E, InputStreamError, OutputStreamError>>::spawn(future);
    }
}

/// The build of the WASM this tab runs: the one named by the page it loaded, `None` for pages
/// without it.
#[cfg(feature = "browser")]
fn page_build() -> Option<&'static str> {
    static PAGE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    PAGE.get_or_init(|| {
        leptos::prelude::document()
            .query_selector("meta[name=hashi-build]")
            .ok()
            .flatten()
            .and_then(|meta| meta.get_attribute("content"))
            .filter(|build| !build.is_empty())
    })
    .as_deref()
}

/// The server has no page.
#[cfg(not(feature = "browser"))]
fn page_build() -> Option<&'static str> {
    None
}

/// Whether a failed server function call means the tab runs an outdated version of the site,
/// see `model::is_outdated_error`. Reloading the page fixes it.
pub fn is_outdated(error: &ServerFnError) -> bool {
    let server = SERVER_BUILD.lock().ok().and_then(|b| b.clone());
    is_outdated_error(error, page_build(), server.as_deref())
}

/// Whether a failed server function call means the app needs an update. Never on the website.
///
/// The server turned the app's version away (`ApiError::AppOutdated`), or the call failed like
/// an outdated tab's, which a reload can't fix in the app: it loads the same bundle again.
pub fn needs_update(error: &ServerFnError) -> bool {
    update_needed(cfg!(feature = "csr"), error)
}

/// [`needs_update`] in the app (`app`) or on the website.
fn update_needed(app: bool, error: &ServerFnError) -> bool {
    app && (is_outdated(error)
        || ApiError::from_message(&error.to_string()) == Some(ApiError::AppOutdated))
}

#[cfg(test)]
mod tests {
    use leptos::server_fn::Bytes;

    use super::*;

    #[test]
    fn the_app_needs_an_update_where_the_site_needs_a_reload() {
        let refused = ServerFnError::de(Bytes::from("ServerError|E:app_outdated"));
        let page = ServerFnError::de(Bytes::from("<!DOCTYPE html><html><body></body></html>"));
        for error in [&refused, &page] {
            assert!(update_needed(true, error), "{error}");
            assert!(!update_needed(false, error), "{error}");
        }
        assert!(is_outdated(&page) && !is_outdated(&refused));
        for error in [
            ServerFnError::new("E:session_expired"),
            ServerFnError::Request("TypeError: Failed to fetch".into()),
        ] {
            assert!(!update_needed(true, &error), "{error}");
        }
        // The website's build
        assert!(!needs_update(&refused));
    }
}
