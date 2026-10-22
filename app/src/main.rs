//! The mobile app's web bundle: mounts the UI into the `<body>` of `index.html`.

use hashi::api::DailyLeaderboard;
use leptos::server_fn::ServerFn;

// Server functions answer at `/api/<name>` only when the proc macros were compiled with
// `DISABLE_SERVER_FN_HASH` (`.cargo/config.toml`, so this crate has to be built inside the
// repository). With the hash, every call of the app would hit a path the server doesn't have.
// The test in `hashi::api` checks this for the server's build, this the app's.
const _: () = assert!(
    same(
        <DailyLeaderboard as ServerFn>::PATH,
        "/api/daily_leaderboard"
    ),
    "server function paths have a hash: build inside the repository, see .cargo/config.toml"
);

/// `a == b`, which isn't `const` for `str` yet.
const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn main() {
    // The app's version, the one of this crate: the stores' version name, which the release
    // (docs/mobile.md, step 11) keeps equal to it. The server answers versions below its
    // minimum with `AppOutdated`.
    hashi::mount(env!("CARGO_PKG_VERSION"));
}
