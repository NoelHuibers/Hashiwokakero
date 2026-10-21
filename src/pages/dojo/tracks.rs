//! The Dojo's tracks: the Sprint, which is the running competition, and the Industrial track,
//! announced but not open yet.
//!
//! The Sprint gives a solver about half a second per generated formula. In that regime `MiniSat`
//! beats `CaDiCaL` and Kissat, whose inprocessing only pays off on large industrial instances,
//! so those get a track of their own with real instances and minutes per formula.

use leptos::prelude::*;

use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{t, use_i18n};

use super::benchmarks::GBD_URL;
use super::guide::cite;

/// The badges sit in headings too, which have their own font
const BADGE_FONT: &str = "font-sans";

/// Names the running competition, everywhere the Dojo talks about its leaderboard.
#[component]
pub(super) fn SprintBadge() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <Badge variant=BadgeVariant::Secondary class=BADGE_FONT attr:data-testid="sprint-track">
            {t!(i18n, dojo.track.sprint)}
        </Badge>
    }
}

/// What the Sprint measures, in one line under the page heading.
#[component]
pub(super) fn SprintLine() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <p class="flex flex-wrap items-center gap-2 text-sm">
            <SprintBadge />
            <span class="text-muted-foreground">{t!(i18n, dojo.track.sprint_text)}</span>
        </p>
    }
}

/// The Industrial track, announced as coming soon.
///
/// Deliberately nothing to click: no form, no button and no endpoint behind it until the track
/// opens. Entry will cost a fee for the compute, so a signup flow needs payment first.
#[component]
pub(super) fn IndustrialTrack() -> impl IntoView {
    let i18n = use_i18n();
    let item = |kanji: &'static str, title: AnyView, text: AnyView| {
        view! {
            <div class="flex gap-3">
                <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-secondary font-pixel text-lg">
                    {kanji}
                </span>
                <div>
                    <h3 class="font-bold">{title}</h3>
                    <p class="text-sm text-muted-foreground">{text}</p>
                </div>
            </div>
        }
    };
    view! {
        // Dashed like the empty leaderboard: there is nothing to enter yet
        <Card class="gap-4 border-dashed" attr:id="industrial" attr:data-testid="industrial-track">
            <CardHeader>
                <CardTitle class="flex flex-wrap items-center gap-2 text-xl">
                    {t!(i18n, dojo.track.industrial.title)}
                    <span class="font-pixel text-sm text-primary">"産業 · sangyō"</span>
                    <Badge variant=BadgeVariant::Warning class=BADGE_FONT attr:data-testid="industrial-soon">
                        {t!(i18n, dojo.track.soon)}
                    </Badge>
                </CardTitle>
            </CardHeader>
            <CardContent class="flex flex-col gap-5">
                <p class="text-sm text-muted-foreground">
                    {t!(i18n, dojo.track.industrial.intro, <a> = cite(GBD_URL))}
                </p>
                <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
                    {item(
                        "時",
                        t!(i18n, dojo.track.industrial.budget_title).into_any(),
                        t!(i18n, dojo.track.industrial.budget).into_any(),
                    )}
                    {item(
                        "証",
                        t!(i18n, dojo.track.industrial.proofs_title).into_any(),
                        t!(i18n, dojo.track.industrial.proofs).into_any(),
                    )}
                    {item(
                        "大",
                        t!(i18n, dojo.track.industrial.why_title).into_any(),
                        t!(i18n, dojo.track.industrial.why).into_any(),
                    )}
                    {item(
                        "費",
                        t!(i18n, dojo.track.industrial.fee_title).into_any(),
                        t!(i18n, dojo.track.industrial.fee).into_any(),
                    )}
                </div>
            </CardContent>
        </Card>
    }
}

/// A line under the leaderboard that points to the announcement further down the page.
#[component]
pub(super) fn IndustrialNote() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <p class="text-sm text-muted-foreground" data-testid="industrial-note">
            {t!(
                i18n,
                dojo.track.industrial.board_note,
                <a> = |children: ChildrenFn| view! {
                    <a href="#industrial" class="text-primary underline underline-offset-4">
                        {children()}
                    </a>
                }
            )}
        </p>
    }
}

// Rendering on the server needs its features
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::i18n::{Locale, td_string};

    /// Renders on the server in English, inside a `#[tokio::test]` for the tasks `leptos_i18n`
    /// starts.
    fn render(view: impl FnOnce() -> AnyView) -> String {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        let html = owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::en);
            view().to_html()
        });
        drop(owner);
        html
    }

    #[tokio::test]
    async fn the_industrial_track_is_announced_as_coming_soon() {
        let html = render(|| view! { <IndustrialTrack /> }.into_any());
        assert!(html.contains("id=\"industrial\""), "{html}");
        for text in [
            td_string!(Locale::en, dojo.track.industrial.title),
            td_string!(Locale::en, dojo.track.soon),
            td_string!(Locale::en, dojo.track.industrial.budget),
            td_string!(Locale::en, dojo.track.industrial.fee),
        ] {
            assert!(html.contains(text), "{text} missing in {html}");
        }
        assert!(html.contains(&format!("href=\"{GBD_URL}\"")), "{html}");
    }

    /// Entry isn't open yet: no form, no field, no button, and no link but the citation.
    #[tokio::test]
    async fn the_industrial_track_has_no_way_to_enter_yet() {
        let html = render(|| view! { <IndustrialTrack /> }.into_any());
        for tag in ["<form", "<input", "<button", "action="] {
            assert!(!html.contains(tag), "{tag} in {html}");
        }
        assert_eq!(html.matches("<a ").count(), 1, "{html}");
    }

    #[tokio::test]
    async fn the_sprint_line_says_what_the_sprint_measures() {
        let html = render(|| view! { <SprintLine /> }.into_any());
        assert!(
            html.contains(td_string!(Locale::en, dojo.track.sprint)),
            "{html}"
        );
        assert!(
            html.contains(td_string!(Locale::en, dojo.track.sprint_text)),
            "{html}"
        );
    }

    /// The leaderboard names its track and points to the one that's coming.
    #[tokio::test]
    async fn the_leaderboard_is_the_sprint_and_mentions_the_industrial_track() {
        use crate::model::DojoBoard;

        use super::super::leaderboard::Leaderboard;

        let html = render(|| {
            let board = RwSignal::new(None::<DojoBoard>);
            view! { <Leaderboard board /> }.into_any()
        });
        assert!(html.contains("data-testid=\"sprint-track\""), "{html}");
        assert!(
            html.contains(td_string!(Locale::en, dojo.track.sprint)),
            "{html}"
        );
        assert!(html.contains("href=\"#industrial\""), "{html}");
    }
}
