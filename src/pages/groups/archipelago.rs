//! The archipelago 群島: every member of a group an island in their colour, bridges between
//! neighbours who solved the same dailies, a pennant on the islands of who's done today.

use leptos::prelude::*;

use crate::components::sprites::{
    BOAT, BoardDefs, BridgeSprite, CELL, PENNANT, PixelArt, PixelSprite, Span,
};
use crate::i18n::{td_string, use_i18n};
use crate::lang::player_name;

use super::banzuke::CROWN;
use crate::model::{GroupMember, SharedDays, archipelago, bridge_count, colour_var, shared_days};

/// Sea around the islands, in art pixels.
const MARGIN: i32 = 14;
/// Widest a name tag gets, in art pixels: islands' slots are three cells apart.
const TAG_MAX: f32 = 44.0;

/// The smallest sea shown, in art pixels: a group of one is an island in a bay, not a close-up.
const MIN_SEA: (i32, i32) = (144, 72);
/// The scene is at least this much wider than high, a banner across the page.
const WIDE: f32 = 1.8;

/// The viewBox `(x, y, width, height)` around islands that take `w` × `h` art pixels from
/// `(-MARGIN, -MARGIN)`: at least [`MIN_SEA`] and [`WIDE`], with the islands in the middle.
pub fn scene(w: i32, h: i32) -> (i32, i32, i32, i32) {
    let vh = h.max(MIN_SEA.1);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "a scene is a few hundred pixels"
    )]
    let wide = (vh as f32 * WIDE).round() as i32;
    let vw = w.max(MIN_SEA.0).max(wide);
    (-MARGIN - (vw - w) / 2, -MARGIN - (vh - h) / 2, vw, vh)
}

/// The first letter of a name, on the island's plate.
pub fn initial(name: &str) -> String {
    name.chars()
        .next()
        .map_or_else(|| "?".into(), |c| c.to_uppercase().to_string())
}

/// About how wide a character of the pixel font is at the tags' size: Latin half a square,
/// kanji and other wide scripts a whole one.
fn char_width(c: char) -> f32 {
    if c.is_ascii() { 3.1 } else { 5.6 }
}

/// A name as it fits on a tag under an island, cut with … if it's too long, and the tag's width.
pub fn tag(name: &str) -> (String, f32) {
    let full: f32 = name.chars().map(char_width).sum();
    if full + 4.0 <= TAG_MAX {
        return (name.to_string(), full + 4.0);
    }
    let mut out = String::new();
    let mut width = 4.0 + 3.1;
    for c in name.chars() {
        if width + char_width(c) > TAG_MAX {
            break;
        }
        width += char_width(c);
        out.push(c);
    }
    (format!("{}…", out.trim_end()), width)
}

/// One member's island, with their pennant if they're done, a crown if they were last week's
/// yokozuna (see the banzuke), and their name tag.
#[component]
fn Island(member: GroupMember, x: i32, y: i32, me: bool, crowned: Signal<bool>) -> impl IntoView {
    let i18n = use_i18n();
    let name = player_name(i18n.get_locale_untracked(), &member.name);
    let (label, width) = tag(&name);
    let done = member.done_today;
    let tooltip = move || {
        let l = i18n.get_locale();
        let state = if done {
            td_string!(l, groups.archipelago.done)
        } else {
            td_string!(l, groups.archipelago.waiting)
        };
        if crowned.get() {
            format!("{name} · {state} · {}", td_string!(l, banzuke.crown))
        } else {
            format!("{name} · {state}")
        }
    };
    let (px, py) = (x * CELL, y * CELL);
    view! {
        <g
            class="nakama-island"
            class:waiting=!done
            style=format!("--member: {}", colour_var(member.colour))
            transform=format!("translate({px},{py})")
            data-tooltip=tooltip
            data-member=member.user_id
        >
            {done.then(|| view! { <PixelArt art=&PENNANT x=10 y=-5 /> })}
            <use href="#isl" />
            {move || crowned.get().then(|| view! { <g class="nakama-crown" data-crown="true"><PixelArt art=&CROWN x=0 y=-4 /></g> })}
            <text class="num" x="8" y="8.4" text-anchor="middle" dominant-baseline="central">
                {initial(&member.name)}
            </text>
            <g class="nakama-tag" class:me=me>
                <rect x=8.0 - width / 2.0 y="16.5" width=width height="8" rx="1.5" />
                <text x="8" y="20.6" text-anchor="middle" dominant-baseline="central">
                    {label}
                </text>
            </g>
        </g>
    }
}

/// The archipelago of a group's `members` (longest standing first): laid out from the ids, so
/// the islands don't move between visits. `shared` makes bridges (none on the join page), `me`
/// gets a tag in the primary colour.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos components take their props by value"
)]
pub fn Archipelago(
    group: i64,
    members: Vec<GroupMember>,
    #[prop(optional)] shared: Vec<SharedDays>,
    #[prop(optional)] me: Option<i64>,
    /// Last week's yokozuna, with a crown.
    #[prop(optional)]
    crowned: Option<RwSignal<Vec<i64>>>,
) -> impl IntoView {
    let i18n = use_i18n();
    let ids: Vec<i64> = members.iter().map(|m| m.user_id).collect();
    let layout = archipelago(group, &ids);
    let (w, h) = (
        (layout.width - 1) * CELL + 2 * MARGIN,
        layout.height * CELL + 2 * MARGIN,
    );
    let (vx, vy, vw, vh) = scene(w, h);
    let done = members.iter().filter(|m| m.done_today).count();
    let total = members.len();
    let name_of = |id: i64| {
        members
            .iter()
            .find(|m| m.user_id == id)
            .map(|m| player_name(i18n.get_locale_untracked(), &m.name))
            .unwrap_or_default()
    };
    let bridges: Vec<_> = layout
        .straits()
        .into_iter()
        .filter_map(|s| {
            let days = shared_days(&shared, s.a.user_id, s.b.user_id);
            let count = bridge_count(days);
            (count > 0).then(|| {
                let (a, b) = (name_of(s.a.user_id), name_of(s.b.user_id));
                let tooltip = move || {
                    td_string!(
                        i18n.get_locale(),
                        groups.archipelago.bridge,
                        a = a.clone(),
                        b = b.clone(),
                        days
                    )
                };
                let span = Span {
                    horizontal: s.horizontal,
                    a: (s.a.x, s.a.y),
                    b: (s.b.x, s.b.y),
                };
                view! {
                    <g data-tooltip=tooltip>
                        <BridgeSprite span count />
                    </g>
                }
            })
        })
        .collect();
    let islands: Vec<_> = layout
        .spots
        .iter()
        .filter_map(|spot| {
            let member = members.iter().find(|m| m.user_id == spot.user_id)?.clone();
            let mine = me == Some(member.user_id);
            let id = member.user_id;
            let crowned =
                Signal::derive(move || crowned.is_some_and(|c| c.with(|c| c.contains(&id))));
            Some(view! { <Island member x=spot.x y=spot.y me=mine crowned /> })
        })
        .collect();
    view! {
        <div class="@container relative overflow-hidden rounded-xl bg-[var(--px-sea)] shadow-inner">
            // As wide as the card, not taller than a header should be: what the viewBox doesn't
            // fill is more sea
            <svg
                class="nakama-sea max-h-[24rem] w-full"
                style=format!("aspect-ratio: {vw} / {vh};")
                viewBox=format!("{vx} {vy} {vw} {vh}")
                xmlns="http://www.w3.org/2000/svg"
                role="img"
                aria-label=move || td_string!(i18n.get_locale(), groups.archipelago.label, done, total)
            >
                <BoardDefs />
                <rect x=vx - 4 * vw y=vy - 4 * vh width=9 * vw height=9 * vh fill="url(#sea)" />
                {bridges}
                {islands}
            </svg>
            <div class="pointer-events-none absolute inset-x-0 bottom-1 h-5" aria-hidden="true">
                <div class="drift absolute left-0 w-9">
                    <PixelSprite art=&BOAT class="bob w-9" />
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_groups_lie_in_a_wide_bay() {
        // One island: the smallest sea, the island in the middle
        let (x, y, w, h) = scene(28, 44);
        assert_eq!((w, h), MIN_SEA);
        assert_eq!((x + w / 2, y + h / 2), (-MARGIN + 14, -MARGIN + 22));
        // Two rows: wider than high
        let (_, _, w, h) = scene(156, 140);
        assert_eq!((w, h), (252, 140));
        // A wide group keeps its width
        let (x, _, w, _) = scene(396, 60);
        assert_eq!((x, w), (-MARGIN, 396));
    }

    #[test]
    fn names_fit_on_their_tags() {
        assert_eq!(initial("aiko"), "A");
        assert_eq!(initial("ünal"), "Ü");
        assert_eq!(initial(""), "?");
        let (short, width) = tag("aiko");
        assert_eq!(short, "aiko");
        assert!(width < TAG_MAX);
        let (cut, width) = tag("Sleepy Tanuki 27");
        assert!(cut.ends_with('…') && cut.starts_with("Sleepy"), "{cut}");
        assert!(width <= TAG_MAX);
        let (cut, width) = tag("橋橋橋橋橋橋橋橋橋");
        assert!(cut.ends_with('…'), "{cut}");
        assert!(width <= TAG_MAX);
    }
}
