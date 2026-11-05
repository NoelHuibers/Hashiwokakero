//! The Lab's frame: one header for the whole section and its tabs, each a page of its own with
//! its URL (linkable, the back button returns to the tab before, one canonical URL per tab).

use leptos::prelude::*;

use crate::components::ui::tabs::{Tabs, TabsLink, TabsList};
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::localized;
use crate::seo::PageMeta;

/// The Lab's tabs: racing our solvers on a puzzle, and the SAT Dojo's leaderboard, how to submit,
/// the community benchmarks and the season archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabTab {
    Race,
    Leaderboard,
    Submit,
    Benchmarks,
    Seasons,
}

impl LabTab {
    pub const ALL: [Self; 5] = [
        Self::Race,
        Self::Leaderboard,
        Self::Submit,
        Self::Benchmarks,
        Self::Seasons,
    ];

    /// The tab's page, without the language prefix (`seo::SITEMAP` lists them all).
    pub const fn path(self) -> &'static str {
        match self {
            Self::Race => "/lab",
            Self::Leaderboard => "/lab/leaderboard",
            Self::Submit => "/lab/submit",
            Self::Benchmarks => "/lab/benchmarks",
            Self::Seasons => "/lab/seasons",
        }
    }

    /// The decoration in front of its name.
    const fn kanji(self) -> &'static str {
        match self {
            Self::Race => "競",
            Self::Leaderboard => "番",
            Self::Submit => "入",
            Self::Benchmarks => "持",
            Self::Seasons => "季",
        }
    }

    fn name(self, locale: Locale) -> &'static str {
        match self {
            Self::Race => td_string!(locale, lab.tabs.race),
            Self::Leaderboard => td_string!(locale, lab.tabs.leaderboard),
            Self::Submit => td_string!(locale, lab.tabs.submit),
            Self::Benchmarks => td_string!(locale, lab.tabs.benchmarks),
            Self::Seasons => td_string!(locale, lab.tabs.seasons),
        }
    }

    /// The title and description of the tab's page, for search results.
    fn meta(self, locale: Locale) -> (&'static str, &'static str) {
        match self {
            Self::Race => (
                td_string!(locale, lab.meta.title),
                td_string!(locale, lab.meta.description),
            ),
            Self::Leaderboard => (
                td_string!(locale, dojo.meta.title),
                td_string!(locale, dojo.meta.description),
            ),
            Self::Submit => (
                td_string!(locale, dojo.submit.meta_title),
                td_string!(locale, dojo.submit.meta_description),
            ),
            Self::Benchmarks => (
                td_string!(locale, dojo.community.meta_title),
                td_string!(locale, dojo.community.meta_description),
            ),
            Self::Seasons => (
                td_string!(locale, dojo.seasons.meta_title),
                td_string!(locale, dojo.seasons.meta_description),
            ),
        }
    }
}

/// A page of the Lab: the section's header, the tabs with `tab` selected, and the page.
///
/// With `meta`, the tab's title, description and canonical URL; a solver's page (under the
/// leaderboard's tab) has its own, and isn't indexed.
#[component]
pub fn LabSection(
    tab: LabTab,
    #[prop(default = true)] meta: bool,
    children: Children,
) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        {meta
            .then(|| {
                view! {
                    <PageMeta
                        title=Signal::derive(move || tab.meta(i18n.get_locale()).0.to_string())
                        description=Signal::derive(move || tab.meta(i18n.get_locale()).1.to_string())
                        path=tab.path()
                    />
                }
            })}
        <section class="mx-auto flex max-w-7xl flex-col gap-6 px-4 py-6 sm:px-6">
            <div class="flex flex-col gap-1">
                <p class="font-pixel text-sm text-primary">"研究室 · kenkyūshitsu"</p>
                <h1 class="text-3xl font-black">{t!(i18n, lab.title)}</h1>
                <p class="max-w-3xl text-muted-foreground">{t!(i18n, lab.section.intro)}</p>
            </div>
            <LabTabs active=tab />
            {children()}
        </section>
    }
}

/// The tabs, links to their pages. On a phone they scroll sideways, the selected one in view.
#[component]
fn LabTabs(active: LabTab) -> impl IntoView {
    let i18n = use_i18n();
    let nav = NodeRef::<leptos::html::Nav>::new();
    #[cfg(feature = "browser")]
    Effect::new(move |_| {
        if let Some(nav) = nav.get() {
            reveal_selected(&nav);
        }
    });
    view! {
        <nav
            node_ref=nav
            class="-mx-4 overflow-x-auto px-4 pb-1.5 sm:mx-0 sm:px-0"
            aria-label=move || t_string!(i18n, lab.tabs.label)
        >
            <Tabs>
                <TabsList class="group-data-[orientation=Horizontal]/tabs:h-10">
                    {LabTab::ALL
                        .into_iter()
                        .map(|tab| {
                            view! {
                                <TabsLink
                                    href=Signal::derive(move || localized(i18n.get_locale(), tab.path()))
                                    active=Signal::stored(tab == active)
                                    class="px-3"
                                >
                                    <span class="font-pixel text-primary" aria-hidden="true">{tab.kanji()}</span>
                                    {move || tab.name(i18n.get_locale())}
                                </TabsLink>
                            }
                        })
                        .collect_view()}
                </TabsList>
            </Tabs>
        </nav>
    }
}

/// Scrolls the tabs sideways so the selected one is whole on the screen (a link to the season
/// archive opens a phone's tabs at their end), without moving the page.
#[cfg(feature = "browser")]
fn reveal_selected(nav: &web_sys::HtmlElement) {
    let Ok(Some(tab)) = nav.query_selector("[data-state=Active]") else {
        return;
    };
    let (outer, inner) = (
        nav.get_bounding_client_rect(),
        tab.get_bounding_client_rect(),
    );
    let margin = 16.0;
    let by = if inner.right() > outer.right() {
        inner.right() - outer.right() + margin
    } else if inner.left() < outer.left() {
        inner.left() - outer.left() - margin
    } else {
        return;
    };
    nav.scroll_by_with_x_and_y(by, 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tab is a page of the router and of the sitemap, below `/lab`.
    #[test]
    fn tabs_are_pages_of_the_lab() {
        let app = include_str!("../../app.rs");
        for tab in LabTab::ALL {
            assert!(tab.path().starts_with("/lab"), "{tab:?}");
            assert!(
                app.contains(&format!("path=path!(\"{}\")", tab.path())),
                "{tab:?} is no route"
            );
        }
        let kanji: std::collections::BTreeSet<_> = LabTab::ALL.map(LabTab::kanji).into();
        assert_eq!(kanji.len(), LabTab::ALL.len());
    }
}
