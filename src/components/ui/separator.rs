use leptos::prelude::*;
use tw_merge::{
    AsTailwindClass, IntoBuilder, IntoTailwindClass, TailwindCompose, TailwindMerge, TwClass,
    TwVariant,
};

#[component]
pub fn Separator(
    #[prop(into, optional)] orientation: Signal<SeparatorOrientation>,
    #[prop(into, optional)] class: String,
    // children: Children,
) -> impl IntoView {
    let merged_class = Memo::new(move |_| {
        let orientation = orientation.get();
        let separator = SeparatorClass { orientation };
        separator.with_class(class.clone())
    });

    view! { <div class=merged_class role="separator" /> }
}

/* ========================================================== */
/*                       🧬 STRUCT 🧬                         */
/* ========================================================== */

#[derive(TwClass, Default, Debug)]
#[tw(class = "shrink-0 bg-border")]
pub struct SeparatorClass {
    orientation: SeparatorOrientation,
}

#[derive(TwVariant, Debug)]
#[expect(
    clippy::expl_impl_clone_on_copy,
    reason = "the `TwVariant` derive writes out `Clone` by hand next to a derived `Copy`"
)]
pub enum SeparatorOrientation {
    #[tw(default, class = "w-full h-[1px]")]
    Default,
    #[tw(class = "h-full w-[1px]")]
    Vertical,
}
