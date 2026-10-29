//! The week's awards 賞, next to the banzuke.
//!
//! Six ways to win a week, so the ones who aren't the quickest have something to aim for too.
//! While the week runs they show who's leading, once it's over the winners, pressed with a hanko.

use leptos::prelude::*;

use crate::components::sprites::PixelSprite;
use crate::i18n::{Locale, t, td_string, use_i18n};
use crate::lang::player_name;
use crate::model::{AwardKind, Banzuke, BanzukeRow, Feat, MS_PER_DAY, format_duration};

use super::today::ColourPip;

/// A lightning bolt, for the fastest. 12×12, like the other award icons.
const BOLT: [&str; 12] = [
    "......oooo..",
    ".....oyyyo..",
    "....oyyyo...",
    "...oyyyo....",
    "..oyyyyoooo.",
    ".oyyyyyyyyo.",
    ".ooooyyyyo..",
    "....oyyyo...",
    "...oyyyo....",
    "...oyyo.....",
    "..oyo.......",
    "..oo........",
];

/// A drop of clear water, for the cleanest.
const DROP: [&str; 12] = [
    ".....oo.....",
    "....oaao....",
    "....oaao....",
    "...oaaaao...",
    "..oaaaaaao..",
    "..ocaaaaao..",
    ".ocaaaaaaao.",
    ".ocaaaaaaao.",
    ".oacaaaaaAo.",
    "..oaaaaaAo..",
    "...oAAAAo...",
    "....oooo....",
];

/// A metronome, for the steadiest.
const METRONOME: [&str; 12] = [
    ".........oo.",
    "........oyo.",
    ".......ono..",
    "....ooono...",
    "...oSSnSo...",
    "...oSSnSo...",
    "..oSSnSSSo..",
    "..oSSnSSSo..",
    "..oSSSSSSo..",
    ".oSSSSSSSSo.",
    ".obbbbbbbbo.",
    ".oooooooooo.",
];

/// The sun rising over the sea, for the early bird.
const SUNRISE: [&str; 12] = [
    "............",
    ".y...yy...y.",
    "..y......y..",
    "....oooo....",
    "...orrrro...",
    "..orrrrrro..",
    ".orrrrrrrro.",
    "aaaaaaaaaaaa",
    "ccaaaccaaacc",
    "aaaaaaaaaaaa",
    "AaaAAAaaAAAa",
    "AAAAAAAAAAAA",
];

/// A sprout growing, for the most improved.
const SPROUT: [&str; 12] = [
    "........oo..",
    "..oo...oggo.",
    ".olloooggGo.",
    ".ollllbgGGo.",
    "..olllbGGo..",
    "...oo.boo...",
    "......b.....",
    "..oooooooo..",
    "..osssssso..",
    "..oSssssSo..",
    "...oSSSSo...",
    "....oooo....",
];

/// A red paper lantern, the lanterne rouge.
const LANTERN: [&str; 12] = [
    ".....oo.....",
    "....oNNo....",
    "...orrrro...",
    "..orRrrRro..",
    "..orrrrrro..",
    "..orRrrRro..",
    "..orrrrrro..",
    "..orRrrRro..",
    "...orrrro...",
    "....oNNo....",
    ".....yy.....",
    "....y..y....",
];

const fn icon(kind: AwardKind) -> &'static [&'static str] {
    match kind {
        AwardKind::Fastest => &BOLT,
        AwardKind::Cleanest => &DROP,
        AwardKind::Steadiest => &METRONOME,
        AwardKind::EarlyBird => &SUNRISE,
        AwardKind::Improved => &SPROUT,
        AwardKind::Lantern => &LANTERN,
    }
}

pub fn award_name(l: Locale, kind: AwardKind) -> &'static str {
    match kind {
        AwardKind::Fastest => td_string!(l, banzuke.awards.names.fastest),
        AwardKind::Cleanest => td_string!(l, banzuke.awards.names.cleanest),
        AwardKind::Steadiest => td_string!(l, banzuke.awards.names.steadiest),
        AwardKind::EarlyBird => td_string!(l, banzuke.awards.names.early_bird),
        AwardKind::Improved => td_string!(l, banzuke.awards.names.improved),
        AwardKind::Lantern => td_string!(l, banzuke.awards.names.lantern),
    }
}

pub fn award_rule(l: Locale, kind: AwardKind) -> &'static str {
    match kind {
        AwardKind::Fastest => td_string!(l, banzuke.awards.rules.fastest),
        AwardKind::Cleanest => td_string!(l, banzuke.awards.rules.cleanest),
        AwardKind::Steadiest => td_string!(l, banzuke.awards.rules.steadiest),
        AwardKind::EarlyBird => td_string!(l, banzuke.awards.rules.early_bird),
        AwardKind::Improved => td_string!(l, banzuke.awards.rules.improved),
        AwardKind::Lantern => td_string!(l, banzuke.awards.rules.lantern),
    }
}

/// Minutes the browser's clock is behind UTC, 0 outside the browser.
fn utc_offset_ms() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "time zone offsets are a few hundred minutes"
        )]
        let minutes = js_sys::Date::new_0().get_timezone_offset() as i64;
        minutes * 60_000
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

/// A time of day as `07:40`, from ms after midnight UTC, on the clock of `offset_ms` behind UTC.
pub fn clock(after_midnight_utc: i64, offset_ms: i64) -> String {
    let local = (after_midnight_utc - offset_ms).rem_euclid(MS_PER_DAY);
    format!("{:02}:{:02}", local / 3_600_000, local / 60_000 % 60)
}

/// The one line of why an award was won.
pub fn reason(l: Locale, feat: Feat, offset_ms: i64) -> String {
    match feat {
        Feat::Fastest { solves, total_ms } => td_string!(
            l,
            banzuke.awards.reasons.fastest,
            time = format_duration(total_ms),
            days = solves
        ),
        Feat::Cleanest { clean, solves } => {
            td_string!(l, banzuke.awards.reasons.cleanest, clean, solves)
        }
        Feat::Steadiest { solves, spread } => {
            td_string!(l, banzuke.awards.reasons.steadiest, spread, days = solves)
        }
        Feat::EarlyBird { usual_ms } => td_string!(
            l,
            banzuke.awards.reasons.early_bird,
            time = clock(usual_ms, offset_ms)
        ),
        Feat::Improved { percent } => {
            td_string!(l, banzuke.awards.reasons.improved, percent)
        }
        Feat::Lantern { total_ms, of } => td_string!(
            l,
            banzuke.awards.reasons.lantern,
            time = format_duration(total_ms),
            of
        ),
    }
}

/// One award: its icon and kanji, who won it (or leads) and why.
#[component]
fn AwardTile(
    kind: AwardKind,
    winners: Vec<BanzukeRow>,
    feat: Option<Feat>,
    over: bool,
    me: i64,
) -> impl IntoView {
    let i18n = use_i18n();
    let won = !winners.is_empty();
    let mine = winners.iter().any(|w| w.user_id == me);
    // Leaves room for the hanko in the corner
    let stamped = won && over;
    let rule = move || award_rule(i18n.get_locale(), kind);
    let who = if won {
        winners
            .into_iter()
            .map(|w| {
                view! {
                    <span class="inline-flex min-w-0 items-center gap-1">
                        <ColourPip colour=w.colour />
                        <span class="truncate" class:text-primary=w.user_id == me>{player_name(i18n.get_locale_untracked(), &w.name)}</span>
                    </span>
                }
            })
            .collect_view()
            .into_any()
    } else {
        view! {
            <span class="text-xs italic text-muted-foreground">
                {move || {
                    if over {
                        td_string!(i18n.get_locale(), banzuke.awards.nobody_final)
                    } else {
                        td_string!(i18n.get_locale(), banzuke.awards.nobody)
                    }
                }}
            </span>
        }
        .into_any()
    };
    let why = move || {
        let l = i18n.get_locale();
        feat.map_or_else(
            || award_rule(l, kind).to_string(),
            |f| reason(l, f, utc_offset_ms()),
        )
    };
    view! {
        <li
            class="award-tile relative flex min-w-0 flex-col gap-2 rounded-lg border bg-background p-2.5 sm:flex-row sm:gap-3 sm:p-3"
            data-award=kind.id()
            data-won=won.to_string()
            data-mine=mine.to_string()
            data-tooltip=rule
        >
            <span class="relative grid size-11 shrink-0 place-items-center rounded-md bg-muted/60">
                <PixelSprite art=icon(kind) class="size-8" />
                <span class="absolute -bottom-1.5 -end-1.5 grid size-5 place-items-center rounded-[4px] bg-card font-pixel text-[11px] leading-none shadow-sm">
                    {kind.kanji()}
                </span>
            </span>
            <span class="flex min-w-0 flex-col gap-0.5" class=(["sm:pe-6"], stamped)>
                <span class="text-sm font-bold leading-tight">{move || award_name(i18n.get_locale(), kind)}</span>
                <span class="flex min-w-0 flex-wrap gap-x-2 text-sm">{who}</span>
                <span class="text-[11px] leading-snug text-muted-foreground">{why}</span>
            </span>
            {stamped
                .then(|| {
                    view! {
                        <span
                            class="hanko-ink absolute end-2 top-2 grid size-6 rotate-[8deg] place-items-center rounded-[4px] font-pixel text-xs leading-none"
                            aria-hidden="true"
                        >
                            "賞"
                        </span>
                    }
                })}
        </li>
    }
}

/// The awards of a week: the winners once it's over, who's leading while it runs.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos components take their props by value"
)]
pub fn Awards(banzuke: Banzuke, me: i64) -> impl IntoView {
    let i18n = use_i18n();
    let over = banzuke.over();
    let tiles = AwardKind::ALL
        .into_iter()
        .map(|kind| {
            let award = banzuke.award(kind);
            let winners: Vec<BanzukeRow> = award
                .map(|a| {
                    banzuke
                        .rows
                        .iter()
                        .filter(|r| a.winners.contains(&r.user_id))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            let feat = award.map(|a| a.feat);
            view! { <AwardTile kind winners feat over me /> }
        })
        .collect_view();
    view! {
        <section class="flex flex-col gap-3" aria-labelledby="awards-title">
            <div class="flex flex-wrap items-baseline justify-between gap-2">
                <h3 id="awards-title" class="flex items-baseline gap-2 text-lg font-bold">
                    {t!(i18n, banzuke.awards.title)}
                    <span class="font-pixel text-sm text-primary">"賞"</span>
                </h3>
                <span class="text-xs text-muted-foreground" data-testid="awards-state">
                    {move || {
                        if over {
                            td_string!(i18n.get_locale(), banzuke.awards.final_state)
                        } else {
                            td_string!(i18n.get_locale(), banzuke.awards.leading)
                        }
                    }}
                </span>
            </div>
            <ul class="grid grid-cols-2 gap-2">{tiles}</ul>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_of_day_on_the_players_clock() {
        // 07:40 UTC
        let at = 7 * 3_600_000 + 40 * 60_000;
        assert_eq!(clock(at, 0), "07:40");
        // Berlin in summer, 120 minutes ahead of UTC
        assert_eq!(clock(at, -120 * 60_000), "09:40");
        // New York, the evening before
        assert_eq!(clock(at, 240 * 60_000), "03:40");
        assert_eq!(clock(60_000, 300 * 60_000), "19:01");
    }

    #[test]
    fn every_award_has_an_icon_of_twelve_by_twelve() {
        for kind in AwardKind::ALL {
            let art = icon(kind);
            assert_eq!(art.len(), 12, "{kind:?}");
            assert!(art.iter().all(|row| row.len() == 12), "{kind:?}");
        }
    }
}
