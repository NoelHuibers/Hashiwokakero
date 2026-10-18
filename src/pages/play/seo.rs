//! The text about Hashiwokakero below the game, for visitors from a search engine.

use leptos::prelude::*;

use crate::components::layout::use_localized;
use crate::i18n::{t, use_i18n};

/// Text about the puzzle below the game, for people who arrive from a search engine (and for
/// the search engine itself, the board is just an SVG to it).
#[component]
pub fn SeoIntro() -> impl IntoView {
    const LINK: &str = "font-medium text-primary underline decoration-primary/40 underline-offset-4 hover:decoration-primary";
    let i18n = use_i18n();
    let rules = use_localized("/rules");
    let create = use_localized("/create");
    let lab = use_localized("/lab");
    view! {
        <article class="mx-auto max-w-7xl px-4 pb-4 pt-10 sm:px-6">
            <div class="mb-6 flex items-end gap-3">
                <span class="vertical-jp hidden font-pixel text-sm leading-none text-primary sm:block">"橋をかけろ"</span>
                <div>
                    <p class="font-pixel text-sm text-primary">{t!(i18n, practice.about.kicker)}</p>
                    <h2 class="text-2xl font-black sm:text-3xl">{t!(i18n, practice.about.title)}</h2>
                </div>
            </div>
            <div class="grid grid-cols-1 gap-x-10 gap-y-8 text-[15px] leading-relaxed text-muted-foreground md:grid-cols-2">
                <section class="flex flex-col gap-2">
                    <h3 class="text-lg font-bold text-foreground">{t!(i18n, practice.about.what_title)}</h3>
                    <p>{t!(i18n, practice.about.what, <b> = <b class="font-semibold text-foreground" />)}</p>
                </section>
                <section class="flex flex-col gap-2">
                    <h3 class="text-lg font-bold text-foreground">{t!(i18n, practice.about.how_title)}</h3>
                    <p>
                        {t!(
                            i18n,
                            practice.about.how,
                            <a> = move |children: ChildrenFn| view! { <a class=LINK href=rules>{children()}</a> }
                        )}
                    </p>
                </section>
                <section class="flex flex-col gap-2">
                    <h3 class="text-lg font-bold text-foreground">{t!(i18n, practice.about.techniques_title)}</h3>
                    <ul class="list-disc space-y-1 ps-5">
                        <li>{t!(i18n, practice.about.technique_double)}</li>
                        <li>{t!(i18n, practice.about.technique_isolation)}</li>
                        <li>{t!(i18n, practice.about.technique_count)}</li>
                        <li>{t!(i18n, practice.about.technique_block)}</li>
                        <li>{t!(i18n, practice.about.technique_connect)}</li>
                    </ul>
                </section>
                <section class="flex flex-col gap-2">
                    <h3 class="text-lg font-bold text-foreground">{t!(i18n, practice.about.why_title)}</h3>
                    <ul class="list-disc space-y-1 ps-5">
                        <li>{t!(i18n, practice.about.why_unique, <b> = <b class="font-semibold text-foreground" />)}</li>
                        <li>{t!(i18n, practice.about.why_unlimited)}</li>
                        <li>{t!(i18n, practice.about.why_art)}</li>
                        <li>
                            {t!(
                                i18n,
                                practice.about.why_workshop,
                                <workshop> = move |children: ChildrenFn| view! { <a class=LINK href=create>{children()}</a> },
                                <lab> = move |children: ChildrenFn| view! { <a class=LINK href=lab>{children()}</a> }
                            )}
                        </li>
                    </ul>
                </section>
            </div>
        </article>
    }
}
