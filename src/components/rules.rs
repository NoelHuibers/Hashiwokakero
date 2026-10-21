//! The four rules of Hashiwokakero, each with its number tile, on the rules page and under the
//! daily.

use leptos::prelude::*;

use crate::i18n::{t, use_i18n};

#[component]
fn Rule(number: &'static str, nested: bool, title: AnyView, children: Children) -> impl IntoView {
    let title = if nested {
        view! { <h4 class="text-lg font-bold">{title}</h4> }.into_any()
    } else {
        view! { <h3 class="text-lg font-bold">{title}</h3> }.into_any()
    };
    view! {
        <div class="flex gap-4">
            <span class="grid size-12 shrink-0 place-items-center rounded-lg bg-primary font-pixel text-2xl text-primary-foreground shadow-md">
                {number}
            </span>
            <div>
                {title}
                <p class="text-muted-foreground">{children()}</p>
            </div>
        </div>
    }
}

/// The rules, numbered 一 to 四.
#[component]
pub fn Rules(
    /// Classes of the list, which lays the rules out.
    #[prop(into)]
    class: String,
    /// The titles are `<h4>`s, below an `<h3>` of the page, instead of `<h3>`s.
    #[prop(optional)]
    nested: bool,
) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class=class>
            <Rule number="一" nested title=t!(i18n, rules.rule.count_title).into_any()>
                {t!(i18n, rules.rule.count)}
            </Rule>
            <Rule number="二" nested title=t!(i18n, rules.rule.double_title).into_any()>
                {t!(i18n, rules.rule.double)}
            </Rule>
            <Rule number="三" nested title=t!(i18n, rules.rule.straight_title).into_any()>
                {t!(i18n, rules.rule.straight)}
            </Rule>
            <Rule number="四" nested title=t!(i18n, rules.rule.connected_title).into_any()>
                {t!(i18n, rules.rule.connected)}
            </Rule>
        </div>
    }
}
