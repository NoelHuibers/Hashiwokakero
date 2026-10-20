//! The client every server function call goes through (`#[server(client = HashiClient)]`): the
//! browser's `fetch`, which also notes the build of the server that answered. With it a call
//! that fails because the tab runs an older build than the server asks for a reload instead of
//! showing the error, see [`is_outdated`].
//!
//! A build is the hash of the site's WASM (`wasm:` in hash.txt, read by the server next to its
//! binary): the server sends it with every response (`x-hashi-build`, see `server/router`) and
//! in every page (`<meta name="hashi-build">`, see `app::shell`), so a tab knows the build of
//! the WASM it runs from the page it loaded.

use std::sync::Mutex;

use futures::{Sink, Stream};
use leptos::prelude::ServerFnError;
use leptos::server_fn::Bytes;
use leptos::server_fn::client::Client;
use leptos::server_fn::client::browser::BrowserClient;
use leptos::server_fn::error::FromServerFnError;
use leptos::server_fn::request::browser::BrowserRequest;
use leptos::server_fn::response::browser::BrowserResponse;

use crate::model::is_outdated_error;

/// The response header with the build of the server, see the module docs.
pub const BUILD_HEADER: &str = "x-hashi-build";

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
        let res =
            <BrowserClient as Client<E, InputStreamError, OutputStreamError>>::send(req).await?;
        let build = res
            .generate_headers()
            .get(BUILD_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if let Ok(mut server) = SERVER_BUILD.lock() {
            *server = build;
        }
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
#[cfg(feature = "hydrate")]
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
#[cfg(not(feature = "hydrate"))]
fn page_build() -> Option<&'static str> {
    None
}

/// Whether a failed server function call means the tab runs an outdated version of the site,
/// see `model::is_outdated_error`. Reloading the page fixes it.
pub fn is_outdated(error: &ServerFnError) -> bool {
    let server = SERVER_BUILD.lock().ok().and_then(|b| b.clone());
    is_outdated_error(error, page_build(), server.as_deref())
}
