//! The river 川 of a group: a wide pixel-art scene of the bridge the members build together,
//! plank by plank, from the bridges of their solved dailies (see `model::river`).
//!
//! The planks laid since the viewer looked last are laid before their eyes, a level completed in
//! between is celebrated with them once. Below the scene: who laid the most this week, and the
//! waters the group crossed, a hanko each.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::load_group_river;
use crate::components::celebration::{Petals, Stamp as StampMark};
use crate::components::sprites::{
    BoardDefs, CELL, LANTERN, PENNANT, PINE, PixelArt, SAKURA, TORII,
};
use crate::components::ui::button::Button;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{Locale, t, td_string, use_i18n};
use crate::lang::{date_short, player_name, stamp_name};
use crate::model::{
    GroupMember, GroupRiver, LEVEL_DAYS, RiverLevel, Stamp, Waters, colour_var, day_of,
    ocean_number, planks_laid,
};
use crate::state::error_toast;

use super::GroupContext;
use super::archipelago::initial;
use super::today::ColourPip;

/// The scene's size in art pixels: a banner, as wide as the card.
const WIDTH: i32 = 256;
const HEIGHT: i32 = 112;
/// Where the near bank ends and the water starts.
const NEAR: i32 = 44;
/// The deck's top and height: the bridge crosses the middle of the scene.
const DECK_Y: i32 = 50;
const DECK_H: i32 = 12;
/// A plank and the gap after it.
const PLANK: i32 = 3;
/// How far the bridge reaches onto each bank.
const LANDING: i32 = 6;
/// The island at the end of the ocean's bridge, two cells wide.
const ISLAND_X: i32 = 222;

// The strait leaves a far bank, the ocean's island fits the scene
const _: () = assert!(NEAR + water_width(Waters::Strait) < WIDTH);
const _: () = assert!(ISLAND_X + 2 * CELL <= WIDTH);

/// Members on the near bank, the most planks first.
const CREW_SHOWN: usize = 3;

/// The water's width of each level's scene: wider and wider.
pub const fn water_width(waters: Waters) -> i32 {
    match waters {
        Waters::Stream => 56,
        Waters::River => 92,
        Waters::WideRiver => 128,
        Waters::Strait => 164,
        Waters::Ocean => WIDTH - NEAR,
    }
}

/// Where the bridge starts and ends, in art pixels.
pub const fn span(waters: Waters) -> (i32, i32) {
    let end = match waters {
        // The beach of the island
        Waters::Ocean => ISLAND_X + LANDING,
        _ => NEAR + water_width(waters) + LANDING,
    };
    (NEAR - LANDING, end)
}

/// The planks of a level's bridge.
pub fn plank_count(waters: Waters) -> usize {
    let (start, end) = span(waters);
    usize::try_from((end - start) / PLANK).unwrap_or(0)
}

/// A number with the thousands grouped the way the language writes them: 1,240 · 1.240 · 1 240.
pub fn thousands(n: i64, locale: Locale) -> String {
    let separator = match locale {
        Locale::de
        | Locale::it
        | Locale::es
        | Locale::pt
        | Locale::nl
        | Locale::tr
        | Locale::id
        | Locale::vi => ".",
        Locale::fr | Locale::pl | Locale::ru | Locale::uk => "\u{202f}",
        _ => ",",
    };
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(separator);
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// The name of a level's waters: "The river", "Ocean 3".
pub fn waters_name(locale: Locale, level: u32) -> String {
    match Waters::of(level) {
        Waters::Stream => td_string!(locale, river.waters.stream).to_string(),
        Waters::River => td_string!(locale, river.waters.river).to_string(),
        Waters::WideRiver => td_string!(locale, river.waters.wide_river).to_string(),
        Waters::Strait => td_string!(locale, river.waters.strait).to_string(),
        Waters::Ocean if ocean_number(level) == 1 => {
            td_string!(locale, river.waters.ocean).to_string()
        }
        Waters::Ocean => td_string!(locale, river.waters.ocean_n, n = ocean_number(level)),
    }
}

fn celebrate_title(locale: Locale, level: u32) -> &'static str {
    match Waters::of(level) {
        Waters::Stream => td_string!(locale, river.celebrate.stream),
        Waters::River => td_string!(locale, river.celebrate.river),
        Waters::WideRiver => td_string!(locale, river.celebrate.wide_river),
        Waters::Strait => td_string!(locale, river.celebrate.strait),
        Waters::Ocean => td_string!(locale, river.celebrate.ocean),
    }
}

/// A small house, a lit window. 12×9.
const HOUSE: [&str; 9] = [
    ".....oo.....",
    "...ooRRoo...",
    "..oRRRRRRo..",
    ".oRRRRRRRRo.",
    "oRRRRRRRRRRo",
    ".oPPPPPPPPo.",
    ".oPbbPPPyPo.",
    ".oPbbPPPPPo.",
    ".oooooooooo.",
];

/// Reeds by the water. 5×5.
const REEDS: [&str; 5] = ["b...b", "b.b.b", "G.b.G", "G.G.G", "GGGGG"];

/// A stone in the water, foam around it. 6×4.
const ROCK: [&str; 4] = [".www..", "wnnnw.", "wnNNnw", ".wwww."];

/// A lily pad with a lotus. 5×4.
const LILY: [&str; 4] = [".gg..", "gggkg", "gglgg", ".ggg."];

/// A koi, swimming down the river. 3×6.
const KOI: [&str; 6] = [".r.", "rwr", "rrr", ".w.", ".r.", "r.r"];

/// A boat seen from above, a member at the oar. 5×8.
const SAMPAN: [&str; 8] = [
    "..o..", ".obo.", "obbbo", "obmbo", "obbbo", "obbbo", ".obo.", "..o..",
];

/// A gull. 5×2.
const GULL: [&str; 2] = ["w...w", ".w.w."];

/// A crest of foam. 4×1.
const CREST: [&str; 2] = [".ww.", "w..w"];

/// One plank of the bridge, its rails and every fourth one a post. New ones are laid one after
/// another (`delay` ms).
#[component]
fn Plank(i: usize, x: i32, new: Option<u32>) -> impl IntoView {
    let post = i.is_multiple_of(4);
    let style = new.map(|delay| format!("animation-delay: {delay}ms"));
    view! {
        <g class="plank" class:plank-new=new.is_some() style=style>
            <rect x=x y=DECK_Y - 2 width=PLANK height="1" class="px-wood-dark" />
            <rect x=x y=DECK_Y - 1 width=PLANK height="1" class="px-wood" />
            <rect x=x y=DECK_Y width="2" height=DECK_H class="px-wood" />
            <rect x=x + 2 y=DECK_Y width="1" height=DECK_H class="px-wood-dark" />
            <rect x=x y=DECK_Y + 3 width="2" height="1" class="px-wood-light" />
            <rect x=x y=DECK_Y + DECK_H width=PLANK height="1" class="px-wood" />
            <rect x=x y=DECK_Y + DECK_H + 1 width=PLANK height="1" class="px-wood-dark" />
            {post
                .then(|| {
                    view! {
                        <rect x=x y=DECK_Y - 4 width="1" height="2" class="px-wood-dark" />
                        <rect x=x y=DECK_Y + DECK_H + 2 width="1" height="2" class="px-wood-dark" />
                    }
                })}
        </g>
    }
}

/// A member of this week's crew on the near bank: a small island in their colour.
#[component]
fn CrewIsland(member: GroupMember, bridges: i64, x: i32, y: i32, me: bool) -> impl IntoView {
    let i18n = use_i18n();
    let name = player_name(i18n.get_locale_untracked(), &member.name);
    let tooltip = move || {
        let l = i18n.get_locale();
        td_string!(
            l,
            river.laid,
            name = name.clone(),
            count = thousands(bridges, l)
        )
    };
    let count = move || thousands(bridges, i18n.get_locale());
    view! {
        <g
            class="river-crew"
            style=format!("--member: {}", colour_var(member.colour))
            transform=format!("translate({x},{y})")
            data-tooltip=tooltip
            data-crew=member.user_id
        >
            <g transform="scale(0.75)">
                <use href="#isl" />
                <text class="num" x="8" y="8.4" text-anchor="middle" dominant-baseline="central">
                    {initial(&member.name)}
                </text>
            </g>
            <g class="nakama-tag" class:me=me>
                <rect x="-1" y="12.5" width="14" height="7" rx="1.5" />
                <text x="6" y="16.2" text-anchor="middle" dominant-baseline="central">
                    {count}
                </text>
            </g>
        </g>
    }
}

/// The banks, the water and what's in it, for a level.
#[component]
fn Waterside(waters: Waters) -> impl IntoView {
    let water = water_width(waters);
    let far = NEAR + water;
    let sea = matches!(waters, Waters::Strait | Waters::Ocean);
    let water_fill = if sea { "url(#sea)" } else { "url(#rv-water)" };
    // Sand along the banks, stones along a strait's
    let shore = if waters == Waters::Strait {
        "px-n"
    } else {
        "px-s"
    };
    let far_bank = (waters != Waters::Ocean).then(|| {
        view! {
            <rect x=far y="0" width=WIDTH - far height=HEIGHT fill="url(#rv-grass)" />
            <rect x=far y="0" width="4" height=HEIGHT class=shore />
            <rect x=far - 1 y="0" width="1" height=HEIGHT fill="url(#rv-foam)" />
            // The path on from the bridge
            <rect x=far y=DECK_Y + 2 width=WIDTH - far height="8" class="px-s" />
            <rect x=far y=DECK_Y + 10 width=WIDTH - far height="1" class="px-S" />
        }
    });
    let flowing = (!sea).then(|| {
        view! {
            <g clip-path="url(#rv-clip)">
                <g class="river-flow">
                    <rect x=NEAR y="-16" width=water height=HEIGHT + 32 fill="url(#rv-ripples)" />
                </g>
            </g>
        }
    });
    let stream_bits = (waters == Waters::Stream).then(|| {
        view! {
            // Stepping stones across, reeds by the banks
            <PixelArt art=&ROCK x=NEAR + 6 y=18 />
            <PixelArt art=&ROCK x=NEAR + 20 y=26 />
            <PixelArt art=&ROCK x=NEAR + 34 y=34 />
            <PixelArt art=&REEDS x=NEAR + 1 y=84 />
            <PixelArt art=&REEDS x=far - 6 y=12 />
            <PixelArt art=&REEDS x=far - 6 y=92 />
        }
    });
    let river_bits = (waters == Waters::River).then(|| {
        view! {
            <PixelArt art=&ROCK x=NEAR + 14 y=80 />
            <PixelArt art=&ROCK x=far - 22 y=20 />
            <PixelArt art=&REEDS x=NEAR + 1 y=10 />
            <PixelArt art=&REEDS x=far - 6 y=86 />
            <g class="river-drift" style="animation-duration: 19s">
                <PixelArt art=&KOI x=NEAR + 40 y=0 />
            </g>
            <g class="river-drift" style="animation-duration: 27s; animation-delay: -11s">
                <PixelArt art=&KOI x=NEAR + 66 y=0 />
            </g>
        }
    });
    let wide_bits = (waters == Waters::WideRiver).then(|| {
        view! {
            <PixelArt art=&LILY x=NEAR + 6 y=16 />
            <PixelArt art=&LILY x=NEAR + 12 y=24 />
            <PixelArt art=&LILY x=far - 14 y=82 />
            <PixelArt art=&LILY x=far - 22 y=92 />
            <PixelArt art=&ROCK x=NEAR + 60 y=86 />
            <PixelArt art=&REEDS x=far - 6 y=10 />
            <g class="river-drift" style="--member: var(--nakama-1); animation-duration: 34s; animation-delay: -9s">
                <PixelArt art=&SAMPAN x=NEAR + 88 y=0 />
            </g>
            <g class="river-drift" style="animation-duration: 23s; animation-delay: -4s">
                <PixelArt art=&KOI x=NEAR + 40 y=0 />
            </g>
        }
    });
    let sea_bits = sea.then(|| {
        view! {
            <PixelArt art=&CREST x=NEAR + 20 y=20 />
            <PixelArt art=&CREST x=NEAR + 70 y=88 />
            <PixelArt art=&CREST x=NEAR + 120 y=14 />
            <PixelArt art=&CREST x=NEAR + 150 y=96 />
            <PixelArt art=&ROCK x=NEAR + 44 y=84 />
            <PixelArt art=&ROCK x=NEAR + 100 y=26 />
            <g class="bob">
                <PixelArt art=&GULL x=NEAR + 60 y=12 />
                <PixelArt art=&GULL x=NEAR + 70 y=18 />
            </g>
        }
    });
    view! {
        // The near bank, grass down to the sand by the water
        <rect x="0" y="0" width=NEAR height=HEIGHT fill="url(#rv-grass)" />
        <rect x=NEAR - 4 y="0" width="4" height=HEIGHT class=shore />
        <rect x=NEAR - 4 y="0" width="1" height=HEIGHT class="px-S" />
        // The path to the bridge
        <rect x="0" y=DECK_Y + 2 width=NEAR - 4 height="8" class="px-s" />
        <rect x="0" y=DECK_Y + 10 width=NEAR - 4 height="1" class="px-S" />
        <rect x=NEAR y="0" width=water height=HEIGHT fill=water_fill />
        <rect x=NEAR y="0" width="1" height=HEIGHT fill="url(#rv-foam)" />
        {flowing}
        {far_bank}
        {stream_bits}
        {river_bits}
        {wide_bits}
        {sea_bits}
        // A moon in the water at night, sun glints by day
        <g class="river-night">
            <rect x=NEAR + water / 2 - 3 y="18" width="6" height="1" class="px-w" opacity="0.7" />
            <rect x=NEAR + water / 2 - 2 y="21" width="4" height="1" class="px-w river-twinkle" opacity="0.6" />
            <rect x=NEAR + water / 2 - 4 y="24" width="8" height="1" class="px-w" opacity="0.5" />
            <rect x=NEAR + water / 2 - 1 y="27" width="2" height="1" class="px-w river-twinkle" opacity="0.5" />
            <rect x=NEAR + water / 3 y="86" width="1" height="1" class="px-w river-twinkle" />
            <rect x=NEAR + 2 * water / 3 y="74" width="1" height="1" class="px-w river-twinkle" style="animation-delay: -1.1s" />
        </g>
        <g class="river-day">
            <rect x=NEAR + water / 3 y="30" width="3" height="1" class="px-w river-twinkle" />
            <rect x=NEAR + 2 * water / 3 y="88" width="2" height="1" class="px-w river-twinkle" style="animation-delay: -0.8s" />
        </g>
    }
}

/// The glow of a lantern or a window at night, round in pixels, around `(x, y)`.
#[component]
fn Glow(x: i32, y: i32) -> impl IntoView {
    view! {
        <g class="river-glow">
            <rect x=x - 4 y=y - 2 width="8" height="4" class="px-y" />
            <rect x=x - 2 y=y - 4 width="4" height="8" class="px-y" />
            <rect x=x - 3 y=y - 3 width="6" height="6" class="px-y" />
        </g>
    }
}

/// Trees, houses and lanterns on the banks, more of them the further the group got; the
/// destination across the water, a torii (an island with one, across the ocean).
#[component]
fn Shores(waters: Waters, end: i32) -> impl IntoView {
    let far = NEAR + water_width(waters);
    let ocean = waters == Waters::Ocean;
    // Room on the far bank for a village
    let village = WIDTH - far >= 60;
    let far_side = (!ocean).then(|| {
        view! {
            <g transform=format!("translate({},{}) scale(2)", end + 3, DECK_Y - 13)>
                <PixelArt art=&TORII />
            </g>
            <PixelArt art=&PINE x=WIDTH - 10 y=4 />
            <PixelArt art=&SAKURA x=far + 10 y=90 />
            {village
                .then(|| {
                    view! {
                        <PixelArt art=&HOUSE x=WIDTH - 30 y=24 />
                        <PixelArt art=&HOUSE x=WIDTH - 46 y=72 />
                        <PixelArt art=&PINE x=far + 12 y=8 />
                        <PixelArt art=&SAKURA x=WIDTH - 14 y=96 />
                        <g class="river-night">
                            <Glow x=WIDTH - 23 y=30 />
                        </g>
                    }
                })}
        }
    });
    let island = ocean.then(|| {
        view! {
            <g transform=format!("translate({ISLAND_X},{}) scale(2)", DECK_Y + DECK_H / 2 - CELL)>
                <use href="#isl" />
                <use href="#d-torii" />
                <text class="river-goal" x="8" y="8.4" text-anchor="middle" dominant-baseline="central">
                    {Waters::Ocean.kanji()}
                </text>
            </g>
        }
    });
    view! {
        <PixelArt art=&PINE x=4 y=6 />
        <PixelArt art=&SAKURA x=30 y=4 />
        <PixelArt art=&PINE x=6 y=92 />
        <PixelArt art=&SAKURA x=28 y=98 />
        {far_side}
        {island}
        // Lanterns at both ends of the bridge, glowing at night
        <g class="river-night">
            <Glow x=NEAR - 11 y=DECK_Y - 5 />
            <Glow x=end + 8 y=DECK_Y + DECK_H + 5 />
        </g>
        <PixelArt art=&LANTERN x=NEAR - 13 y=DECK_Y - 7 />
        <PixelArt art=&LANTERN x=end + 6 y=DECK_Y + DECK_H + 3 />
        // Fireflies over the grass at night
        <g class="river-night">
            <rect x="14" y="36" width="1" height="1" class="px-y river-twinkle" />
            <rect x="34" y="80" width="1" height="1" class="px-y river-twinkle" style="animation-delay: -0.7s" />
            <rect x="8" y="64" width="1" height="1" class="px-y river-twinkle" style="animation-delay: -1.5s" />
        </g>
    }
}

/// The scene: banks, water, and the bridge as far as it got. `new_from` is the first plank the
/// viewer hasn't seen yet.
#[component]
fn Scene(
    level: u32,
    laid: usize,
    new_from: usize,
    crew: Vec<(GroupMember, i64)>,
    me: i64,
    #[prop(into)] label: Signal<String>,
) -> impl IntoView {
    let waters = Waters::of(level);
    let planks = plank_count(waters);
    let (start, end) = span(waters);
    let fresh = laid.saturating_sub(new_from);
    // New planks one after another, all of them within two and a half seconds
    let step = if fresh == 0 {
        0
    } else {
        (2_500 / u32::try_from(fresh).unwrap_or(u32::MAX)).clamp(25, 90)
    };
    let x_of = |i: usize| start + i32::try_from(i).unwrap_or(0) * PLANK;
    let deck = (0..laid)
        .map(|i| {
            let new = (i >= new_from).then(|| {
                u32::try_from(i - new_from)
                    .unwrap_or(0)
                    .saturating_mul(step)
            });
            view! { <Plank i x=x_of(i) new /> }
        })
        .collect_view();
    // Piles where the planks will go
    let piles = (laid..planks)
        .filter(|i| i.is_multiple_of(4))
        .map(|i| {
            let x = x_of(i);
            view! {
                <rect x=x y=DECK_Y - 1 width="1" height="2" class="px-wood-dark" opacity="0.75" />
                <rect x=x y=DECK_Y + DECK_H - 1 width="1" height="2" class="px-wood-dark" opacity="0.75" />
            }
        })
        .collect_view();
    let tip = x_of(laid);
    let leader = crew.first().map(|(m, _)| colour_var(m.colour));
    let pennant = view! {
        <g style=format!("--member: {}", leader.unwrap_or_else(|| "var(--primary)".into()))>
            <PixelArt art=&PENNANT x=tip - 1 y=DECK_Y - 9 />
        </g>
    };
    // This week's crew by the bridge head, the most planks closest to it
    let spots = [(20, 22), (3, 30), (12, 72)];
    let islands = crew
        .into_iter()
        .take(CREW_SHOWN)
        .zip(spots)
        .map(|((member, bridges), (x, y))| {
            let mine = member.user_id == me;
            view! { <CrewIsland member bridges x y me=mine /> }
        })
        .collect_view();
    view! {
        <svg
            class="nakama-sea w-full"
            style=format!("aspect-ratio: {WIDTH} / {HEIGHT};")
            viewBox=format!("0 0 {WIDTH} {HEIGHT}")
            xmlns="http://www.w3.org/2000/svg"
            role="img"
            aria-label=label
            data-waters=waters.id()
            data-planks=planks
            data-laid=laid
        >
            <BoardDefs />
            <defs>
                <clipPath id="rv-clip">
                    <rect x=NEAR y="0" width=water_width(waters) height=HEIGHT />
                </clipPath>
                <pattern id="rv-grass" width="12" height="12" patternUnits="userSpaceOnUse">
                    <rect width="12" height="12" class="px-g" />
                    <rect x="2" y="3" width="1" height="1" class="px-l" />
                    <rect x="3" y="2" width="1" height="1" class="px-l" />
                    <rect x="8" y="8" width="1" height="1" class="px-G" />
                    <rect x="9" y="9" width="1" height="1" class="px-G" />
                    <rect x="7" y="1" width="1" height="1" class="px-G" />
                </pattern>
                <pattern id="rv-water" width="16" height="16" patternUnits="userSpaceOnUse">
                    <rect width="16" height="16" class="px-sea" />
                    <rect x="3" y="11" width="2" height="1" class="px-sea-deep" />
                    <rect x="12" y="4" width="1" height="2" class="px-sea-deep" />
                </pattern>
                <pattern id="rv-ripples" width="16" height="16" patternUnits="userSpaceOnUse">
                    <rect x="2" y="2" width="1" height="3" class="px-sea-light" />
                    <rect x="9" y="9" width="1" height="2" class="px-sea-light" />
                    <rect x="13" y="13" width="1" height="2" class="px-sea-light" />
                    <rect x="6" y="14" width="1" height="1" class="px-w" opacity="0.6" />
                </pattern>
                <pattern id="rv-foam" width="1" height="8" patternUnits="userSpaceOnUse">
                    <rect width="1" height="5" class="px-w" />
                </pattern>
            </defs>
            <Waterside waters />
            // The bridge's shadow on the water
            <rect x=start.max(NEAR) y=DECK_Y + DECK_H + 2 width=(tip - start.max(NEAR)).max(0) height="2" class="px-sea-deep" opacity="0.6" />
            // Stone abutments at both ends
            <rect x=start - 3 y=DECK_Y - 3 width="4" height=DECK_H + 6 class="px-n" />
            <rect x=start - 3 y=DECK_Y + DECK_H + 2 width="4" height="1" class="px-N" />
            <rect x=end - 1 y=DECK_Y - 3 width="4" height=DECK_H + 6 class="px-n" />
            <rect x=end - 1 y=DECK_Y + DECK_H + 2 width="4" height="1" class="px-N" />
            {piles}
            {deck}
            {pennant}
            <Shores waters end />
            {islands}
        </svg>
    }
}

/// A level completed since the viewer looked last: petals, the level's hanko, and on to the
/// next water.
#[component]
fn Celebration(level: RiverLevel, on_close: Callback<()>) -> impl IntoView {
    let i18n = use_i18n();
    let dialog = NodeRef::<leptos::html::Div>::new();
    Effect::new(move |_| {
        let Some(dialog) = dialog.get() else { return };
        let _ = dialog.focus();
        let escape = window_event_listener(leptos::ev::keydown, move |ev| {
            if ev.key() == "Escape" {
                on_close.run(());
            }
        });
        on_cleanup(move || escape.remove());
    });
    let RiverLevel { level, target, .. } = level;
    view! {
        <Petals />
        <div class="z-10 mt-3 flex justify-center sm:absolute sm:inset-0 sm:mt-0 sm:grid sm:place-items-center sm:bg-background/30 sm:p-4">
            <div
                node_ref=dialog
                role="dialog"
                aria-labelledby="river-crossed-title"
                tabindex="-1"
                data-testid="river-crossed"
                class="w-full max-w-sm rounded-xl outline-none"
            >
                <Card class="items-center gap-2 px-5 py-5 text-center shadow-2xl animate-in zoom-in-95 fade-in">
                    <StampMark kanji=Waters::of(level).kanji() class="size-14 text-4xl" />
                    <h3 id="river-crossed-title" class="text-xl font-black">
                        {move || celebrate_title(i18n.get_locale(), level)}
                    </h3>
                    <p class="text-sm text-muted-foreground">
                        {move || {
                            let l = i18n.get_locale();
                            td_string!(
                                l,
                                river.celebrate.text,
                                count = thousands(target, l),
                                waters = waters_name(l, level + 1)
                            )
                        }}
                    </p>
                    <p class="flex items-center gap-2 text-sm font-medium">
                        <span
                            class="hanko grid size-8 shrink-0 rotate-[-6deg] place-items-center rounded-md font-pixel text-base"
                            aria-hidden="true"
                        >
                            {Stamp::RiverCrew.kanji()}
                        </span>
                        {move || {
                            let l = i18n.get_locale();
                            td_string!(l, river.celebrate.stamp, stamp = stamp_name(l, Stamp::RiverCrew))
                        }}
                    </p>
                    <Button on:click=move |_| on_close.run(())>
                        {t!(i18n, river.celebrate.onward)} " 次へ"
                    </Button>
                </Card>
            </div>
        </div>
    }
}

/// The waters crossed, a hanko each, and the one being built.
#[component]
fn Crossed(completed: Vec<RiverLevel>, level: u32) -> impl IntoView {
    let i18n = use_i18n();
    let stamps = completed
        .into_iter()
        .map(|l| {
            let label = move || {
                let loc = i18n.get_locale();
                td_string!(
                    loc,
                    river.crossed_level,
                    waters = waters_name(loc, l.level),
                    date = date_short(loc, day_of(l.completed_at)),
                    count = thousands(l.target, loc)
                )
            };
            view! {
                <li>
                    <span
                        class="hanko grid size-9 rotate-[-6deg] place-items-center rounded-md font-pixel text-lg"
                        data-tooltip=label
                        data-level=l.level
                        role="img"
                        aria-label=label
                    >
                        {Waters::of(l.level).kanji()}
                    </span>
                </li>
            }
        })
        .collect_view();
    let building = move || {
        let loc = i18n.get_locale();
        td_string!(loc, river.building, waters = waters_name(loc, level))
    };
    view! {
        <ul class="flex flex-wrap items-center gap-2" data-testid="river-crossed-row">
            {stamps}
            <li>
                <span
                    class="grid size-9 place-items-center rounded-md border-2 border-dashed border-primary/50 font-pixel text-lg text-primary/50"
                    data-tooltip=building
                    role="img"
                    aria-label=building
                >
                    {Waters::of(level).kanji()}
                </span>
            </li>
        </ul>
    }
}

/// How far the current level got, and what's new since the viewer looked last.
#[component]
fn Progress(river: GroupRiver) -> impl IntoView {
    let i18n = use_i18n();
    let r = river;
    let level = r.level;
    let fresh = r.total - r.seen.min(r.total);
    view! {
        <div class="flex flex-col items-start gap-0.5 sm:items-end">
            <p class="font-pixel text-lg tabular-nums" data-testid="river-progress">
                {move || {
                    let l = i18n.get_locale();
                    td_string!(
                        l,
                        river.progress,
                        done = thousands(r.progress, l),
                        target = thousands(r.target, l)
                    )
                }}
            </p>
            {(fresh > 0)
                .then(|| {
                    view! {
                        <span class="rounded-full bg-success-light px-2 py-0.5 text-xs font-medium text-success-dark">
                            {move || td_string!(i18n.get_locale(), river.new, count = thousands(fresh, i18n.get_locale()))}
                        </span>
                    }
                })}
        </div>
        <span class="sr-only">{move || waters_name(i18n.get_locale(), level)}</span>
    }
}

/// The scene of a river, who builds most this week and the waters crossed, and the celebration
/// of a level when there is one.
#[component]
fn RiverView(
    river: GroupRiver,
    members: Vec<GroupMember>,
    me: i64,
    celebrating: RwSignal<Option<RiverLevel>>,
) -> impl IntoView {
    let i18n = use_i18n();
    let r = river;
    let crew: Vec<(GroupMember, i64)> = r
        .week
        .iter()
        .filter_map(|c| {
            members
                .iter()
                .find(|m| m.user_id == c.user_id)
                .map(|m| (m.clone(), c.bridges))
        })
        .collect();
    let planks = plank_count(Waters::of(r.level));
    let laid = planks_laid(r.progress, r.target, planks);
    let new_from = planks_laid(r.seen_progress(), r.target, planks);
    let (level, progress, target) = (r.level, r.progress, r.target);
    let label = Signal::derive(move || {
        let l = i18n.get_locale();
        td_string!(
            l,
            river.scene,
            waters = waters_name(l, level),
            done = thousands(progress, l),
            target = thousands(target, l)
        )
    });
    let to_go = move || {
        let l = i18n.get_locale();
        td_string!(l, river.to_go, count = thousands(target - progress, l))
    };
    let days = LEVEL_DAYS[usize::try_from(level)
        .unwrap_or(usize::MAX)
        .min(LEVEL_DAYS.len() - 1)];
    let week = crew
        .iter()
        .take(CREW_SHOWN)
        .map(|(m, bridges)| {
            let bridges = *bridges;
            let mine = m.user_id == me;
            view! {
                <li class="flex min-w-0 items-center gap-2 text-sm" data-member=m.user_id>
                    <ColourPip colour=m.colour />
                    <span class="truncate" class:font-bold=mine>{player_name(i18n.get_locale_untracked(), &m.name)}</span>
                    <span class="ms-auto font-pixel tabular-nums">
                        {move || thousands(bridges, i18n.get_locale())}
                    </span>
                </li>
            }
        })
        .collect_view();
    let nobody = crew.is_empty();
    view! {
        // The scene, and next to it on wide screens who builds and what was crossed
        <div class="grid grid-cols-1 gap-5 lg:grid-cols-[minmax(0,44rem)_minmax(0,1fr)]">
            <div class="flex min-w-0 flex-col gap-2">
                <div class="relative rounded-xl bg-[var(--px-sea)] shadow-inner">
                    <div class="overflow-hidden rounded-xl">
                        <Scene level laid new_from crew=crew.clone() me label />
                    </div>
                    {move || {
                        celebrating
                            .get()
                            .map(|level| {
                                view! {
                                    <Celebration level on_close=Callback::new(move |()| celebrating.set(None)) />
                                }
                            })
                    }}
                </div>
                <p class="text-xs font-medium">{to_go}</p>
                <p class="text-xs text-muted-foreground">{move || td_string!(i18n.get_locale(), river.how, days)}</p>
            </div>
            <div class="grid min-w-0 grid-cols-1 content-start gap-5 sm:grid-cols-2 lg:grid-cols-1">
                <div class="flex min-w-0 flex-col gap-2">
                    <h3 class="text-sm font-bold">{t!(i18n, river.week)}</h3>
                    {if nobody {
                        view! { <p class="text-sm text-muted-foreground">{t!(i18n, river.week_none)}</p> }.into_any()
                    } else {
                        view! { <ol class="flex max-w-xs flex-col gap-1.5" data-testid="river-week">{week}</ol> }.into_any()
                    }}
                </div>
                <div class="flex min-w-0 flex-col gap-2">
                    <h3 class="text-sm font-bold">{t!(i18n, river.crossed)}</h3>
                    <Crossed completed=r.completed level />
                </div>
            </div>
        </div>
    }
}

/// The group page's river card.
#[component]
pub fn RiverCard(group: GroupContext) -> impl IntoView {
    let i18n = use_i18n();
    let river = RwSignal::new(None::<GroupRiver>);
    // Kept apart from the river: a reload of the page doesn't take the celebration away
    let celebrating = RwSignal::new(None::<RiverLevel>);
    let id = Memo::new(move |_| group.page.with(|p| p.as_ref().map(|p| p.id)));

    // Again when the page reloads (a member more or less)
    Effect::new(move |_| {
        let Some(id) = id.get() else { return };
        group.page.track();
        spawn_local(async move {
            match load_group_river(id).await {
                Ok(r) => {
                    if let Some(level) = r.celebrate {
                        celebrating.set(Some(level));
                    }
                    river.set(Some(r));
                }
                Err(e) => error_toast(e),
            }
        });
    });

    let me = move || group.page.with(|p| p.as_ref().map_or(0, |p| p.me));
    let header = move || river.get().map(|r| view! { <Progress river=r /> });
    let body = move || {
        let Some(r) = river.get() else {
            return view! { <p class="text-sm text-muted-foreground">{t!(i18n, common.game.loading)}</p> }
                .into_any();
        };
        let members = group
            .page
            .with(|p| p.as_ref().map(|p| p.members.clone()).unwrap_or_default());
        view! { <RiverView river=r members me=me() celebrating /> }.into_any()
    };
    view! {
        <Card class="gap-4" attr:data-testid="river-card">
            <CardHeader>
                <div class="flex w-full flex-wrap items-start justify-between gap-3">
                    <div class="min-w-0">
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            {t!(i18n, river.title)} <span class="font-pixel text-sm text-primary">"川"</span>
                        </CardTitle>
                        <CardDescription>
                            {move || {
                                river
                                    .with(|r| r.as_ref().map(|r| r.level))
                                    .map(|level| {
                                        let l = i18n.get_locale();
                                        format!(
                                            "{} · {}",
                                            td_string!(l, river.level, level = level + 1),
                                            waters_name(l, level),
                                        )
                                    })
                            }}
                        </CardDescription>
                    </div>
                    {header}
                </div>
                <p class="text-sm text-muted-foreground">{t!(i18n, river.intro)}</p>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">{body}</CardContent>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_grouped_like_the_language_writes_them() {
        assert_eq!(thousands(0, Locale::en), "0");
        assert_eq!(thousands(999, Locale::en), "999");
        assert_eq!(thousands(1_240, Locale::en), "1,240");
        assert_eq!(thousands(1_240, Locale::de), "1.240");
        assert_eq!(thousands(1_234_567, Locale::fr), "1\u{202f}234\u{202f}567");
        assert_eq!(thousands(-2_000, Locale::ja), "-2,000");
    }

    /// Every level's bridge fits the scene, from the near bank to the far one, and a wider water
    /// takes more planks.
    #[test]
    fn bridges_reach_across_and_grow_with_the_water() {
        let mut last = 0;
        for waters in Waters::ALL {
            let (start, end) = span(waters);
            assert!(start > 0 && start < NEAR, "{waters:?}");
            assert!(end <= WIDTH, "{waters:?}");
            let planks = plank_count(waters);
            assert!(planks > last, "{waters:?}: {planks}");
            // The last plank ends on the far bank
            let reach = start + i32::try_from(planks).unwrap() * PLANK;
            assert!(
                reach >= end - PLANK && reach <= end,
                "{waters:?}: {reach} {end}"
            );
            last = planks;
        }
    }

    fn member(user_id: i64, name: &str, colour: u8) -> GroupMember {
        GroupMember {
            user_id,
            name: name.into(),
            guest: false,
            colour,
            joined_at: 0,
            owner: false,
            stamp: None,
            done_today: true,
        }
    }

    fn river(level: u32, progress: i64, seen: i64) -> GroupRiver {
        let completed: Vec<RiverLevel> = (0..level)
            .map(|l| RiverLevel {
                level: l,
                target: 400,
                completed_at: 20_000 * 86_400_000 + i64::from(l),
            })
            .collect();
        let base = 400 * i64::from(level);
        GroupRiver {
            group_id: 1,
            level,
            base,
            progress,
            target: 850,
            total: base + progress,
            celebrate: completed.last().copied(),
            completed,
            week: vec![
                crate::model::Contribution {
                    user_id: 2,
                    bridges: 1_240,
                },
                crate::model::Contribution {
                    user_id: 9,
                    bridges: 30,
                },
                crate::model::Contribution {
                    user_id: 1,
                    bridges: 12,
                },
            ],
            seen,
        }
    }

    /// Every water renders on the server: the planks laid (the new ones animated), the crew on
    /// the bank, the crossed hanko, and the celebration when a level is new to the viewer.
    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn every_water_renders_its_bridge_crew_and_hanko() {
        use crate::i18n::use_i18n;
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::en);
        });
        let members = vec![member(1, "aiko", 0), member(2, "bram", 1)];
        for level in 0..6 {
            let waters = Waters::of(level);
            let r = river(level, 425, 400 * i64::from(level) + 200);
            let laid = planks_laid(425, 850, plank_count(waters));
            let new = laid - planks_laid(200, 850, plank_count(waters));
            let html = owner.with(|| {
                let celebrating = RwSignal::new(r.celebrate);
                let members = members.clone();
                view! {
                    <Progress river=r.clone() />
                    <RiverView river=r members me=1 celebrating />
                }
                .to_html()
            });
            assert!(
                html.contains(&format!("data-waters=\"{}\"", waters.id())),
                "{level}"
            );
            assert_eq!(html.matches("class=\"plank").count(), laid, "{level}");
            assert_eq!(html.matches("plank-new").count(), new, "{level}");
            assert!(html.contains("425 / 850 bridges"), "{level}");
            assert!(html.contains("+225 since you last looked"), "{level}");
            assert!(html.contains("425 to the other side"), "{level}");
            // bram leads the week, a member who left (9) isn't shown
            assert!(
                html.contains("data-crew=\"2\"") && html.contains("1,240"),
                "{level}"
            );
            assert!(!html.contains("data-crew=\"9\""), "{level}");
            assert_eq!(
                html.matches("data-level=").count(),
                usize::try_from(level).unwrap()
            );
            if level > 0 {
                assert!(html.contains("river-crossed-title"), "{level}");
                assert!(html.contains("Bridge builder"), "{level}");
            } else {
                assert!(!html.contains("river-crossed-title"));
            }
        }
        // Nobody laid a plank this week
        let html = owner.with(|| {
            let quiet = GroupRiver {
                week: vec![],
                ..river(0, 0, 0)
            };
            view! { <RiverView river=quiet members=vec![] me=1 celebrating=RwSignal::new(None) /> }
                .to_html()
        });
        assert!(html.contains("Nobody has laid a plank this week yet."));
        assert_eq!(html.matches("class=\"plank").count(), 0);
    }

    #[test]
    fn oceans_after_the_first_are_counted() {
        assert_eq!(waters_name(Locale::en, 0), "The stream");
        assert_eq!(waters_name(Locale::en, 4), "The ocean");
        assert_eq!(waters_name(Locale::en, 6), "Ocean 3");
        assert_eq!(celebrate_title(Locale::en, 1), "The river is bridged!");
    }
}
