//! The footer with the drifting boat, credits, the legal links and, once they are in the
//! stores, the apps.

use leptos::prelude::*;
use leptos_router::components::A;

use super::{Hanko, use_localized};
use crate::components::sprites::{BOAT, PixelSprite};
use crate::i18n::{Locale, t, t_string, use_i18n};
use crate::seo::{AppStores, use_app_stores};

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
            <AppBadges stores=use_app_stores() />
        </footer>
    }
}

/// The languages with Apple's own "Download on the App Store" badge, in
/// `public/badges/app-store/<language>.svg`; the others get the English one.
const APP_STORE_BADGES: [Locale; 18] = [
    Locale::en,
    Locale::de,
    Locale::fr,
    Locale::it,
    Locale::es,
    Locale::pt,
    Locale::nl,
    Locale::pl,
    Locale::tr,
    Locale::ru,
    Locale::uk,
    Locale::id,
    Locale::vi,
    Locale::th,
    Locale::ja,
    Locale::ko,
    Locale::zh,
    Locale::zh_Hant,
];

/// The languages with Google's own "Get it on Google Play" badge, in
/// `public/badges/google-play/<language>.png`; the others get the English one.
const GOOGLE_PLAY_BADGES: [Locale; 19] = [
    Locale::en,
    Locale::de,
    Locale::fr,
    Locale::it,
    Locale::es,
    Locale::pt,
    Locale::nl,
    Locale::pl,
    Locale::tr,
    Locale::ru,
    Locale::ar,
    Locale::hi,
    Locale::id,
    Locale::vi,
    Locale::th,
    Locale::ja,
    Locale::ko,
    Locale::zh,
    Locale::zh_Hant,
];

/// The badge of a store in `locale`'s language, or the English one.
fn badge(badges: &[Locale], dir: &str, extension: &str, locale: Locale) -> String {
    use leptos_i18n::Locale as _;

    let locale = if badges.contains(&locale) {
        locale
    } else {
        Locale::en
    };
    format!("/badges/{dir}/{}.{extension}", locale.as_str())
}

/// The stores' official badges, as Apple and Google provide them (unchanged, in the visitor's
/// language where they have one), linking to the apps' pages. Apple's at least 40 px high,
/// Google's at least as high as Apple's (its image has transparent margins), with space around.
/// Nothing until the server names a store.
#[component]
pub fn AppBadges(stores: AppStores) -> impl IntoView {
    let i18n = use_i18n();
    if stores.is_empty() {
        return ().into_any();
    }
    let link = "rounded-lg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary";
    let app_store = stores.app_store_url().map(|url| {
        view! {
            <a href=url target="_blank" rel="noopener" class=link data-testid="app-store-badge">
                <img
                    src=move || badge(&APP_STORE_BADGES, "app-store", "svg", i18n.get_locale())
                    alt=move || t_string!(i18n, common.apps.app_store)
                    class="h-10 w-auto"
                    height="40"
                    loading="lazy"
                />
            </a>
        }
    });
    let google_play = stores.google_play.is_some().then(move || {
        view! {
            <a
                href=move || stores.google_play_url(Some(i18n.get_locale())).unwrap_or_default()
                target="_blank"
                rel="noopener"
                class=link
                data-testid="google-play-badge"
            >
                <img
                    src=move || badge(&GOOGLE_PLAY_BADGES, "google-play", "png", i18n.get_locale())
                    alt=move || t_string!(i18n, common.apps.google_play)
                    class="h-[60px] w-auto"
                    height="60"
                    loading="lazy"
                />
            </a>
        }
    });
    view! {
        <div
            role="group"
            aria-label=move || t_string!(i18n, common.apps.label)
            class="relative mx-auto flex max-w-7xl flex-wrap items-center justify-center gap-2.5 px-6 pb-8 sm:justify-end"
        >
            {app_store}
            {google_play}
        </div>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use leptos_i18n::Locale as _;

    use super::*;

    /// Every language's badge is a file of the site, its own or the English one, so no badge is
    /// ever a broken image.
    #[test]
    fn every_language_has_its_badges() {
        let public = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("public");
        for (badges, dir, extension) in [
            (&APP_STORE_BADGES[..], "app-store", "svg"),
            (&GOOGLE_PLAY_BADGES[..], "google-play", "png"),
        ] {
            for locale in Locale::get_all() {
                let path = badge(badges, dir, extension, *locale);
                assert!(public.join(&path[1..]).is_file(), "{path}");
                let own = badges.contains(locale);
                assert_eq!(
                    path.ends_with(&format!("/{}.{extension}", locale.as_str())),
                    own || *locale == Locale::en,
                    "{path}"
                );
            }
            // And nothing else in the directory: every file is one a language uses
            let files = std::fs::read_dir(public.join("badges").join(dir))
                .unwrap()
                .count();
            assert_eq!(files, badges.len(), "{dir}");
        }
        // Arabic and Hindi have no badge of Apple's, Ukrainian none of Google's
        assert_eq!(
            badge(&APP_STORE_BADGES, "app-store", "svg", Locale::ar),
            "/badges/app-store/en.svg"
        );
        assert_eq!(
            badge(&GOOGLE_PLAY_BADGES, "google-play", "png", Locale::uk),
            "/badges/google-play/en.png"
        );
        assert_eq!(
            badge(&GOOGLE_PLAY_BADGES, "google-play", "png", Locale::zh_Hant),
            "/badges/google-play/zh-Hant.png"
        );
    }
}
