//! Rivals ライバル: the toggle on a player's profile, the crossed swords that mark one on the
//! daily and monthly boards, and the head-to-head record against them.

use leptos::prelude::*;

use super::sprites::{PixelSprite, SWORDS};
use super::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::i18n::{Locale, t, td_string, use_i18n};
use crate::model::{HeadToHead, Outcome, daily_number, format_duration, format_gap};
use crate::state::{Auth, Rivals};

/// Whether `user_id` is the signed in player, who can't be their own rival.
fn is_me(auth: Auth, user_id: i64) -> bool {
    auth.user
        .with(|u| u.as_ref().is_some_and(|u| u.id == user_id))
}

/// The small crossed swords after a rival's name on the daily and monthly boards. Only a
/// marker, players are marked and unmarked on their profile ([`RivalButton`]).
#[component]
pub fn RivalMark(user_id: i64) -> impl IntoView {
    let i18n = use_i18n();
    let rivals = expect_context::<Rivals>();
    let label = move || td_string!(i18n.get_locale(), rivals.marked_button);
    view! {
        <Show when=move || rivals.is(user_id)>
            <span class="rival-mark grid shrink-0 place-items-center" role="img" aria-label=label data-tooltip=label>
                <PixelSprite art=&SWORDS class="size-3.5" />
            </span>
        </Show>
    }
}

/// The toggle on a player's profile, the one place to mark a rival: a quiet "Mark as rival",
/// "Your rival" once marked.
#[component]
pub fn RivalButton(user_id: i64, name: String) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let rivals = expect_context::<Rivals>();
    let name = StoredValue::new(name);
    let toggle = move |_| rivals.toggle(user_id, name.get_value());
    let private = move || td_string!(i18n.get_locale(), rivals.private);
    let marked = move || rivals.is(user_id);
    view! {
        <Show when=move || !is_me(auth, user_id)>
            <Button
                variant=Signal::derive(move || if marked() { ButtonVariant::Outline } else { ButtonVariant::Ghost })
                size=ButtonSize::Sm
                class="text-muted-foreground aria-pressed:text-primary"
                attr:aria-pressed=move || marked().to_string()
                attr:data-tooltip=private
                on:click=toggle
            >
                <PixelSprite art=&SWORDS class="size-4" />
                {move || if marked() { t!(i18n, rivals.marked_button).into_any() } else { t!(i18n, rivals.mark_button).into_any() }}
            </Button>
        </Show>
    }
}

/// A gap between two times in words: "12 s", "1:05 min".
pub fn gap_text(locale: Locale, ms: i64) -> String {
    let (gap, minutes) = format_gap(ms);
    if minutes {
        td_string!(locale, rivals.gap_minutes, gap)
    } else {
        td_string!(locale, rivals.gap_seconds, gap)
    }
}

/// The colours of a win, a loss and a draw: matcha, shu and paper.
pub const fn outcome_class(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Win => "bg-success-light text-success-dark border-success/40",
        Outcome::Loss => "bg-primary/10 text-primary border-primary/40",
        Outcome::Draw => "bg-muted text-muted-foreground border-border",
    }
}

/// A head-to-head record as "5–3", green while ahead, red while behind.
#[component]
pub fn Score(wins: i64, losses: i64, draws: i64) -> impl IntoView {
    let i18n = use_i18n();
    let record = HeadToHead {
        wins,
        losses,
        draws,
        recent: Vec::new(),
    };
    let (score, lead) = (record.score(), record.lead());
    let summary = {
        move || {
            td_string!(
                i18n.get_locale(),
                rivals.record.summary,
                wins,
                losses,
                draws
            )
        }
    };
    view! {
        <span
            class=format!(
                "inline-flex h-6 min-w-10 items-center justify-center whitespace-nowrap rounded-md border px-1.5 font-pixel text-xs tabular-nums {}",
                outcome_class(lead),
            )
            data-tooltip=summary
            aria-label=summary
        >
            {score}
        </span>
    }
}

/// The last duels as little hanko: 勝 won, 負 lost, 分 drawn, oldest first.
#[component]
pub fn RecentDuels(record: HeadToHead, #[prop(optional)] large: bool) -> impl IntoView {
    let i18n = use_i18n();
    let size = if large {
        "size-8 text-base"
    } else {
        "size-5 text-[11px]"
    };
    view! {
        <ol class="flex items-center gap-1">
            {record
                .recent
                .into_iter()
                .map(|duel| {
                    let outcome = duel.outcome();
                    let text = move || {
                        td_string!(
                            i18n.get_locale(),
                            rivals.record.day,
                            number = daily_number(duel.day),
                            mine = format_duration(duel.mine),
                            theirs = format_duration(duel.theirs)
                        )
                    };
                    view! {
                        <li
                            class=format!(
                                "grid place-items-center rounded-[4px] border font-pixel leading-none {size} {}",
                                outcome_class(outcome),
                            )
                            class=("rotate-[-6deg]", outcome == Outcome::Win)
                            data-tooltip=text
                            aria-label=text
                        >
                            {outcome.kanji()}
                        </li>
                    }
                })
                .collect_view()}
        </ol>
    }
}
