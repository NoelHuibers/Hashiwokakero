//! Pixel art for the board, drawn as ASCII and rendered to SVG rects.
//!
//! One board cell is 16×16 art pixels. Every character maps to a `px-<char>` CSS class (see
//! `style/tailwind.css`), so the palette follows the day/night theme and island states can
//! recolour sprites through inherited CSS variables.

use leptos::prelude::*;

pub const CELL: i32 = 16;

/// Island, number plate in the middle (P/Q), grass ring, sandy beach at the bottom.
const ISLAND: [&str; 16] = [
    "................",
    ".....oooooo.....",
    "...oollllggoo...",
    "..olllggggggGo..",
    ".ollgPPPPPPgGGo.",
    ".olgPPPPPPPPgGo.",
    ".olgPPPPPPPPgGo.",
    ".oggPPPPPPPPgGo.",
    ".oggPPPPPPPPGGo.",
    ".ogsPPPPPPPPgGo.",
    ".ossPPPPPPPPsSo.",
    ".osssQQQQQQsSSo.",
    "..ossssssssSSo..",
    "...oosssssSoo...",
    ".....oooooo.....",
    "...ww......ww...",
];

/// Decorations sit on the top right rim of an island and may stick out of the cell.
const PINE: [&str; 7] = [
    "..T..", ".TtT.", "TTtTT", ".TTT.", "TTTTT", "..b..", "..b..",
];
const SAKURA: [&str; 6] = [".kkk.", "kKkkk", "kkkKk", ".kKk.", "..b..", "..b.."];
const TORII: [&str; 5] = ["rrrrrr", ".R..R.", ".rrrr.", ".r..r.", ".r..r."];
const LANTERN: [&str; 5] = [".NN.", "NNNN", ".yn.", ".nn.", "nNNn"];

/// A pennant in a group member's colour (`--member`) on a pole, raised on their island once
/// they finished today's daily. 6×7.
pub const PENNANT: [&str; 7] = [
    "bmmmm.", "bmmmmM", "bmmmM.", "b.....", "b.....", "b.....", "b.....",
];

/// Sakura petal for the celebration, 6×6.
pub const PETAL: [&str; 6] = ["..kk..", ".kkkk.", "kkKkkk", "kkkKkk", ".kkkk.", "..kk.."];

/// A small boat for the hero banner, 16×8.
pub const BOAT: [&str; 9] = [
    ".......o........",
    ".......or.......",
    ".......orr......",
    ".......orrr.....",
    ".......o........",
    "oooooooooooooo..",
    ".obbbbbbbbbbbo..",
    "..obbbbbbbbbo...",
    "...ooooooooo....",
];

/// Crossed katanas, the mark of a rival, 12×12.
pub const SWORDS: [&str; 12] = [
    "wN........Nw",
    "NwN......NwN",
    ".NwN....NwN.",
    "..NwN..NwN..",
    "...NwNNwN...",
    "....NwwN....",
    "....NwwN....",
    "..yNwNNwNy..",
    "..yy....yy..",
    "..rRy..yRr..",
    ".rR......Rr.",
    "RR........RR",
];

/// A flame for a day won against a rival, 8×11.
pub const FLAME: [&str; 11] = [
    "....r...", "....rr..", "...rrr..", "..rryr.r", ".rryyrrr", ".ryyyyrr", "rryyyyyr", "ryyyyyyr",
    "Ryyyyyyr", ".Ryyyyr.", "..RRRR..",
];

/// Horizontal runs of equal pixels: (x, y, width, char).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "pixel art is a few dozen pixels wide and tall"
)]
pub fn runs(art: &[&str]) -> Vec<(i32, i32, i32, char)> {
    let mut out = Vec::new();
    for (y, row) in art.iter().enumerate() {
        let chars: Vec<char> = row.chars().collect();
        let mut x = 0;
        while x < chars.len() {
            let c = chars[x];
            let start = x;
            while x < chars.len() && chars[x] == c {
                x += 1;
            }
            if c != '.' {
                out.push((start as i32, y as i32, (x - start) as i32, c));
            }
        }
    }
    out
}

/// Renders pixel art at an offset (in art pixels).
#[component]
pub fn PixelArt(
    art: &'static [&'static str],
    #[prop(optional)] x: i32,
    #[prop(optional)] y: i32,
) -> impl IntoView {
    runs(art)
        .into_iter()
        .map(|(rx, ry, w, c)| {
            view! { <rect x=x + rx y=y + ry width=w height="1" class=format!("px-{c}") /> }
        })
        .collect_view()
}

/// A standalone pixel art image (petals, boat, logo bits).
#[component]
pub fn PixelSprite(
    art: &'static [&'static str],
    #[prop(into, optional)] class: String,
) -> impl IntoView {
    let w = art.iter().map(|r| r.len()).max().unwrap_or(0);
    let h = art.len();
    view! {
        <svg
            class=class
            viewBox=format!("0 0 {w} {h}")
            xmlns="http://www.w3.org/2000/svg"
            shape-rendering="crispEdges"
            aria-hidden="true"
        >
            <PixelArt art=art />
        </svg>
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decor {
    None,
    Pine,
    Sakura,
    Torii,
    Lantern,
}

impl Decor {
    /// A stable, scattered choice per position, so islands keep their decoration.
    pub fn for_pos(x: u8, y: u8) -> Self {
        let h = u32::from(x)
            .wrapping_mul(73_856_093)
            .wrapping_add(u32::from(y).wrapping_mul(19_349_663))
            .rotate_left(7)
            .wrapping_mul(2_654_435_761);
        match (h >> 24) % 9 {
            0 | 1 => Self::Pine,
            2 | 3 => Self::Sakura,
            4 => Self::Torii,
            5 => Self::Lantern,
            _ => Self::None,
        }
    }

    pub fn id(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Pine => Some("#d-pine"),
            Self::Sakura => Some("#d-sakura"),
            Self::Torii => Some("#d-torii"),
            Self::Lantern => Some("#d-lantern"),
        }
    }
}

/// Shared symbols and patterns, rendered once inside every board SVG.
#[component]
pub fn BoardDefs() -> impl IntoView {
    view! {
        <defs>
            // No viewBox: symbols with one get scaled to the <use> size (100% by default)
            <symbol id="isl" overflow="visible">
                <PixelArt art=&ISLAND />
            </symbol>
            <symbol id="d-pine" overflow="visible">
                <PixelArt art=&PINE x=10 y=-4 />
            </symbol>
            <symbol id="d-sakura" overflow="visible">
                <PixelArt art=&SAKURA x=10 y=-3 />
            </symbol>
            <symbol id="d-torii" overflow="visible">
                <PixelArt art=&TORII x=10 y=-2 />
            </symbol>
            <symbol id="d-lantern" overflow="visible">
                <PixelArt art=&LANTERN x=11 y=-2 />
            </symbol>

            // The sea, drifting slowly in 4px steps
            <pattern id="sea" width="32" height="32" patternUnits="userSpaceOnUse">
                <rect width="32" height="32" class="px-sea" />
                <rect x="4" y="6" width="2" height="1" class="px-sea-light" />
                <rect x="3" y="7" width="1" height="1" class="px-sea-light" />
                <rect x="6" y="7" width="1" height="1" class="px-sea-light" />
                <rect x="20" y="22" width="2" height="1" class="px-sea-light" />
                <rect x="19" y="23" width="1" height="1" class="px-sea-light" />
                <rect x="22" y="23" width="1" height="1" class="px-sea-light" />
                <rect x="12" y="15" width="3" height="1" class="px-sea-deep" />
                <rect x="27" y="4" width="2" height="1" class="px-sea-deep" />
                <rect x="6" y="27" width="2" height="1" class="px-sea-deep" />
                <animate
                    attributeName="x"
                    values="0;4;8;12;16;20;24;28"
                    dur="12s"
                    calcMode="discrete"
                    repeatCount="indefinite"
                />
            </pattern>

            // Wooden planks of a single bridge
            <pattern id="plank-h" width="3" height="16" patternUnits="userSpaceOnUse">
                <rect width="2" height="16" class="px-wood" />
                <rect x="2" width="1" height="16" class="px-wood-dark" />
                <rect y="6" width="2" height="1" class="px-wood-light" />
            </pattern>
            <pattern id="plank-v" width="16" height="3" patternUnits="userSpaceOnUse">
                <rect width="16" height="2" class="px-wood" />
                <rect y="2" width="16" height="1" class="px-wood-dark" />
                <rect x="6" width="1" height="2" class="px-wood-light" />
            </pattern>
            // Deck of the vermilion double bridge
            <pattern id="deck-h" width="4" height="16" patternUnits="userSpaceOnUse">
                <rect width="3" height="16" class="px-wood" />
                <rect x="3" width="1" height="16" class="px-wood-dark" />
                <rect y="5" width="3" height="1" class="px-wood-light" />
            </pattern>
            <pattern id="deck-v" width="16" height="4" patternUnits="userSpaceOnUse">
                <rect width="16" height="3" class="px-wood" />
                <rect y="3" width="16" height="1" class="px-wood-dark" />
                <rect x="5" width="1" height="3" class="px-wood-light" />
            </pattern>
            // Railing posts above the top rail / left of the left rail
            <pattern id="posts-h" width="4" height="16" patternUnits="userSpaceOnUse">
                <rect x="1" y="2" width="1" height="1" class="px-shu" />
            </pattern>
            <pattern id="posts-v" width="16" height="4" patternUnits="userSpaceOnUse">
                <rect x="2" y="1" width="1" height="1" class="px-shu" />
            </pattern>
        </defs>
    }
}

/// Geometry of a bridge between two island cells (cell coordinates).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub horizontal: bool,
    /// Cell coordinates of the two islands, `a` left of / above `b`.
    pub a: (i32, i32),
    pub b: (i32, i32),
}

/// Draws a bridge. `count` 1 is a rustic wooden bridge, 2 a vermilion taiko-bashi.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos components take their props by value"
)]
pub fn BridgeSprite(span: Span, count: u8, #[prop(into, optional)] class: String) -> impl IntoView {
    let (ax, ay) = span.a;
    let (bx, by) = span.b;
    let class = format!("bridge {class}");
    if span.horizontal {
        let cy = ay * CELL + 8;
        let x0 = ax * CELL + 13;
        let x1 = bx * CELL + 3;
        let w = x1 - x0;
        if count >= 2 {
            view! {
                <g class=class>
                    <rect x=x0 y=cy + 5 width=w height="1" class="px-sea-deep" />
                    <rect x=x0 y=cy - 6 width=w height="1" fill="url(#posts-h)" />
                    <rect x=x0 y=cy - 5 width=w height="1" class="px-shu-light" />
                    <rect x=x0 y=cy - 4 width=w height="1" class="px-shu" />
                    <rect x=x0 y=cy - 3 width=w height="6" fill="url(#deck-h)" />
                    <rect x=x0 y=cy + 3 width=w height="1" class="px-shu" />
                    <rect x=x0 y=cy + 4 width=w height="1" class="px-shu-dark" />
                    <rect x=x0 + 2 y=cy - 7 width="2" height="2" class="px-gold" />
                    <rect x=x1 - 4 y=cy - 7 width="2" height="2" class="px-gold" />
                </g>
            }
            .into_any()
        } else {
            view! {
                <g class=class>
                    <rect x=x0 y=cy + 3 width=w height="1" class="px-sea-deep" />
                    <rect x=x0 y=cy - 3 width=w height="1" class="px-wood-dark" />
                    <rect x=x0 y=cy - 2 width=w height="4" fill="url(#plank-h)" />
                    <rect x=x0 y=cy + 2 width=w height="1" class="px-wood-dark" />
                    <rect x=x0 + 2 y=cy - 4 width="1" height="1" class="px-wood-dark" />
                    <rect x=x1 - 3 y=cy - 4 width="1" height="1" class="px-wood-dark" />
                </g>
            }
            .into_any()
        }
    } else {
        let cx = ax * CELL + 8;
        let y0 = ay * CELL + 13;
        let y1 = by * CELL + 3;
        let h = y1 - y0;
        if count >= 2 {
            view! {
                <g class=class>
                    <rect x=cx + 5 y=y0 width="1" height=h class="px-sea-deep" />
                    <rect x=cx - 6 y=y0 width="1" height=h fill="url(#posts-v)" />
                    <rect x=cx - 5 y=y0 width="1" height=h class="px-shu-light" />
                    <rect x=cx - 4 y=y0 width="1" height=h class="px-shu" />
                    <rect x=cx - 3 y=y0 width="6" height=h fill="url(#deck-v)" />
                    <rect x=cx + 3 y=y0 width="1" height=h class="px-shu" />
                    <rect x=cx + 4 y=y0 width="1" height=h class="px-shu-dark" />
                    <rect x=cx - 7 y=y0 + 2 width="2" height="2" class="px-gold" />
                    <rect x=cx - 7 y=y1 - 4 width="2" height="2" class="px-gold" />
                </g>
            }
            .into_any()
        } else {
            view! {
                <g class=class>
                    <rect x=cx + 3 y=y0 width="1" height=h class="px-sea-deep" />
                    <rect x=cx - 3 y=y0 width="1" height=h class="px-wood-dark" />
                    <rect x=cx - 2 y=y0 width="4" height=h fill="url(#plank-v)" />
                    <rect x=cx + 2 y=y0 width="1" height=h class="px-wood-dark" />
                    <rect x=cx - 4 y=y0 + 2 width="1" height="1" class="px-wood-dark" />
                    <rect x=cx - 4 y=y1 - 3 width="1" height="1" class="px-wood-dark" />
                </g>
            }
            .into_any()
        }
    }
}
