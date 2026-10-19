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
mod contact;
mod daily;
mod dojo;
mod puzzle;

pub use account::*;
pub use admin::*;
pub use contact::*;
pub use daily::*;
pub use dojo::*;
pub use puzzle::*;

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
}
