pub mod App;

use crate::App::App;
use leptos::*;

fn main() {
    leptos::mount_to_body(|| view! { <App/> })
}
