//! The win card over the daily board right after this tab's submit: the time, the rank, the
//! stamps the result earned and where to go next.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::celebration::Stamp;
use crate::components::layout::use_localized;
use crate::components::share::ShareBar;
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::Card;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::stamp_name;
use crate::model::{self, format_duration};

use super::result::share_text;
use super::state::Daily;

/// The most stamps the card names, the others are counted: it has to fit over a phone's board.
const NAMED_STAMPS: usize = 6;

/// The stamps the card names, and how many more there are.
fn named_stamps(stamps: &[model::Stamp]) -> (&[model::Stamp], usize) {
    let named = stamps.len().min(NAMED_STAMPS);
    (&stamps[..named], stamps.len() - named)
}

/// The stamps earned with the result, instead of toasts on top of the card.
#[component]
fn NewStamps(stamps: Vec<model::Stamp>) -> impl IntoView {
    let i18n = use_i18n();
    let account = use_localized("/account");
    let (named, more) = named_stamps(&stamps);
    let list = named
        .iter()
        .map(|&stamp| {
            view! {
                <li class="flex items-center gap-1.5 text-sm font-medium">
                    <span
                        class="hanko grid size-8 shrink-0 rotate-[-6deg] place-items-center rounded-md font-pixel text-base"
                        aria-hidden="true"
                    >
                        {stamp.kanji()}
                    </span>
                    {move || stamp_name(i18n.get_locale(), stamp)}
                </li>
            }
        })
        .collect_view();
    view! {
        <div class="flex w-full flex-col items-center gap-2 rounded-lg border border-dashed bg-muted/50 p-3">
            <p class="text-xs font-bold uppercase tracking-[0.16em] text-muted-foreground">
                {t!(i18n, stamps.earned)} " · 判子"
            </p>
            <ul class="flex flex-wrap justify-center gap-x-3 gap-y-2">{list}</ul>
            <A href=account attr:class="text-sm text-primary underline-offset-4 hover:underline">
                {move || {
                    if more > 0 {
                        td_string!(i18n.get_locale(), stamps.more, count = more)
                    } else {
                        format!("{} →", td_string!(i18n.get_locale(), stamps.title))
                    }
                }}
            </A>
        </div>
    }
}

/// Covers the board once this tab's submit went through, until the player puts it away to
/// admire the board (or presses Escape). The result card next to the board stays.
#[component]
pub(super) fn WinCard(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let practice = use_localized("/practice");
    let dialog = NodeRef::<leptos::html::Div>::new();
    Effect::new(move |_| {
        let Some(dialog) = dialog.get() else { return };
        let _ = dialog.focus();
        let escape = window_event_listener(leptos::ev::keydown, move |ev| {
            if ev.key() == "Escape" {
                daily.admired.set(true);
            }
        });
        on_cleanup(move || escape.remove());
    });
    let place = move || {
        let (rank, total) = daily
            .result
            .with(|r| r.as_ref().map_or((0, 0), |r| (r.rank, r.finishers)));
        match daily.flag() {
            Some(_) => t_string!(i18n, daily.flag.title).to_string(),
            None if rank > 0 => t_string!(i18n, daily.result.rank, rank, total),
            None => String::new(),
        }
    };
    let stamps = move || {
        daily
            .result
            .with(|r| r.as_ref().map(|r| r.stamps.clone()).unwrap_or_default())
    };
    let share = move || share_text(&daily, i18n.get_locale_untracked());
    view! {
        <div class="absolute inset-0 z-10 grid place-items-center p-3 sm:p-4">
            <div
                node_ref=dialog
                role="dialog"
                aria-modal="true"
                aria-labelledby="daily-win-title"
                tabindex="-1"
                data-testid="daily-win"
                class="max-h-full w-full max-w-sm overflow-y-auto overscroll-contain rounded-xl outline-none"
            >
                <Card class="items-center gap-2 px-5 text-center shadow-2xl animate-in zoom-in-95 fade-in sm:gap-3 sm:px-6">
                    <Stamp kanji="完" class="size-16 text-4xl sm:size-20 sm:text-5xl" />
                    <h2 id="daily-win-title" class="text-2xl font-black">
                        "見事！ " {t!(i18n, common.game.well_done)}
                    </h2>
                    <p class="font-pixel text-3xl tabular-nums">{move || format_duration(daily.elapsed_ms())}</p>
                    <p class="text-sm text-muted-foreground">{place}</p>
                    {move || {
                        let stamps = stamps();
                        (!stamps.is_empty()).then(|| view! { <NewStamps stamps /> })
                    }}
                    <div class="flex w-full flex-wrap justify-center gap-2 pt-1">
                        <Button href=untrack(practice)>{t!(i18n, common.nav.practice)} " 練習"</Button>
                        <Button variant=ButtonVariant::Ghost on:click=move |_| daily.admired.set(true)>
                            {t!(i18n, practice.admire)}
                        </Button>
                        <ShareBar text=share outline=true />
                    </div>
                </Card>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Stamp as S;

    #[test]
    fn a_few_stamps_are_all_named() {
        assert_eq!(named_stamps(&[]), (&[][..], 0));
        let few = [S::First, S::EarlyBird];
        assert_eq!(named_stamps(&few), (&few[..], 0));
        let six = &S::ALL[..NAMED_STAMPS];
        assert_eq!(named_stamps(six), (six, 0));
    }

    /// A big day (the first solve after an update that added stamps, say) still fits a phone.
    #[test]
    fn many_stamps_are_named_up_to_six_and_counted() {
        let many = &S::ALL[..NAMED_STAMPS + 3];
        assert_eq!(named_stamps(many), (&many[..NAMED_STAMPS], 3));
    }
}
