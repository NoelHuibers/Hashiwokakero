//! Server functions. Solving and generating run on the server, in the backend crate; users and
//! daily results live in the database (see `server`).

// In the browser build `#[server]` replaces each body with a request to the server, and the
// trait impl the macro generates keeps an `async fn` that has nothing to await
#![cfg_attr(
    feature = "hydrate",
    expect(
        clippy::unused_async_trait_impl,
        reason = "`#[server]` generates an `async fn` without `.await` for the client side"
    )
)]

mod account;
mod admin;
mod banzuke;
mod client;
mod contact;
mod daily;
mod dojo;
mod groups;
mod hanko;
mod puzzle;
mod replay;
mod rivals;
mod river;

pub use account::*;
pub use admin::*;
pub use banzuke::*;
pub use client::{BUILD_HEADER, HashiClient, is_outdated};
pub use contact::*;
pub use daily::*;
pub use dojo::*;
pub use groups::*;
pub use hanko::*;
pub use puzzle::*;
pub use replay::*;
pub use rivals::*;
pub use river::*;

#[cfg(test)]
mod tests {
    use leptos::server_fn::axum_export::body::Body;
    use leptos::server_fn::http_export::{Request, Response};
    use leptos::server_fn::{ServerFn, ServerFnTraitObj, inventory};

    /// Server functions answer at `/api/<name>`, the same in every build: without the hash of
    /// the build directory and module path that leptos adds unless `DISABLE_SERVER_FN_HASH` is
    /// set (`.cargo/config.toml`). With the hash, a tab of an older build called paths the new
    /// server doesn't have whenever the build moved.
    #[test]
    fn server_functions_have_stable_paths() {
        assert_eq!(
            <super::DailyLeaderboard as ServerFn>::PATH,
            "/api/daily_leaderboard"
        );
        let mut paths: Vec<_> = inventory::iter::<ServerFnTraitObj<Request<Body>, Response<Body>>>
            .into_iter()
            .map(ServerFnTraitObj::path)
            .collect();
        for path in &paths {
            let name = path.strip_prefix("/api/").unwrap_or_default();
            assert!(
                !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{path}"
            );
        }
        // Without the hash, two functions of the same name would share a path, one of them
        // unreachable
        let count = paths.len();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), count, "server functions with the same name");
    }

    /// Every server function goes through `HashiClient`, which notes the server's build: with
    /// the default client a call of an outdated tab would show its raw error again.
    #[test]
    fn server_functions_use_the_hashi_client() {
        let sources = [
            include_str!("account.rs"),
            include_str!("admin.rs"),
            include_str!("banzuke.rs"),
            include_str!("contact.rs"),
            include_str!("daily.rs"),
            include_str!("dojo.rs"),
            include_str!("groups.rs"),
            include_str!("hanko.rs"),
            include_str!("puzzle.rs"),
            include_str!("replay.rs"),
            include_str!("rivals.rs"),
            include_str!("river.rs"),
        ];
        let attributes: Vec<_> = sources
            .iter()
            .flat_map(|s| s.lines())
            .filter(|line| line.starts_with("#[server"))
            .collect();
        for attribute in &attributes {
            assert!(attribute.contains("client = HashiClient"), "{attribute}");
        }
        // No server function in another file
        let registered = inventory::iter::<ServerFnTraitObj<Request<Body>, Response<Body>>>
            .into_iter()
            .count();
        assert_eq!(attributes.len(), registered);
    }
}
