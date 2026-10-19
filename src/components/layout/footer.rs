//! The footer with the drifting boat, credits and the legal links.

use leptos::prelude::*;
use leptos_router::components::A;

use super::{Hanko, use_localized};
use crate::components::sprites::{BOAT, PixelSprite};
use crate::i18n::{t, use_i18n};

#[component]
pub fn Footer() -> impl IntoView {
    let i18n = use_i18n();
    let rules = use_localized("/rules");
    let impressum = use_localized("/impressum");
    let dojo = use_localized("/dojo");
    let privacy = use_localized("/datenschutz");
    let contact = use_localized("/kontakt");
    let author = move || {
        view! {
            <a
                class="font-semibold text-foreground underline decoration-primary/50 underline-offset-4 hover:text-primary"
                href=crate::seo::AUTHOR_URL
                rel="author noopener"
                target="_blank"
            >
                {crate::seo::AUTHOR}
            </a>
        }
    };
    view! {
        <footer class="relative mt-16 overflow-hidden border-t">
            <div class="seigaiha absolute inset-0 opacity-80" aria-hidden="true"></div>
            <div class="@container pointer-events-none absolute inset-x-0 top-3 h-8" aria-hidden="true">
                <div class="drift absolute left-0 w-12">
                    <PixelSprite art=&BOAT class="bob w-12" />
                </div>
            </div>
            <div class="relative mx-auto flex max-w-7xl flex-col gap-3 px-6 pb-8 pt-14 text-sm text-muted-foreground sm:flex-row sm:items-end sm:justify-between">
                <div class="flex items-center gap-3">
                    <Hanko class="size-8 text-base" />
                    <p>
                        "Hashiwokakero 橋をかけろ — "
                        {t!(i18n, common.footer.tagline, <em> = <span class="text-foreground" />)}
                    </p>
                </div>
                <p class="flex flex-wrap items-center gap-x-1.5 sm:justify-end">
                    <span>{t!(i18n, common.footer.made_by, author)}</span>
                    <span aria-hidden="true">"·"</span>
                    <A href=rules attr:class="underline decoration-primary/50 underline-offset-4 hover:text-foreground">
                        {t!(i18n, common.footer.how_to_play)} " 遊び方"
                    </A>
                    <span aria-hidden="true">"·"</span>
                    <A href=dojo attr:class="underline decoration-primary/50 underline-offset-4 hover:text-foreground">
                        {t!(i18n, dojo.title)} " 道場"
                    </A>
                    <span aria-hidden="true">"·"</span>
                    <a
                        class="underline decoration-primary/50 underline-offset-4 hover:text-foreground"
                        href=crate::seo::REPO_URL
                        target="_blank"
                        rel="noopener"
                    >
                        {t!(i18n, common.footer.open_source)}
                    </a>
                    <span aria-hidden="true">"·"</span>
                    <span>{t!(i18n, common.footer.tech)}</span>
                    <span aria-hidden="true">"·"</span>
                    <A href=impressum attr:class="underline decoration-primary/50 underline-offset-4 hover:text-foreground">
                        {t!(i18n, common.legal.imprint)}
                    </A>
                    <span aria-hidden="true">"·"</span>
                    <A href=privacy attr:class="underline decoration-primary/50 underline-offset-4 hover:text-foreground">
                        {t!(i18n, common.legal.privacy)}
                    </A>
                    <span aria-hidden="true">"·"</span>
                    <A href=contact attr:class="underline decoration-primary/50 underline-offset-4 hover:text-foreground">
                        {t!(i18n, common.legal.contact)}
                    </A>
                </p>
            </div>
        </footer>
    }
}
