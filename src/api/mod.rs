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
