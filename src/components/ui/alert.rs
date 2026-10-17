use leptos::prelude::*;
use leptos_ui::clx;

mod components {
    use super::{
        Children, ClassAttribute, CustomAttribute, ElementChild, IntoView, clx, component, view,
    };
    clx! {Alert, div, "relative w-full rounded-lg border px-4 py-3 text-sm [&>svg+div]:translate-y-[-3px] [&>svg]:absolute [&>svg]:start-4 [&>svg]:top-4 [&>svg]:text-foreground [&>svg~*]:ps-7"}
    clx! {AlertTitle, h4, "mb-1 font-medium tracking-tight leading-none"}
    clx! {AlertDescription, p, "text-sm [&_p]:leading-relaxed"}
}

pub use components::*;
