//! The replay theatre (再生 saisei): a day's daily played back from the move logs.
//!
//! `/groups/:id/replay/:day` is a group's, every member at once on one board (`?with=<id>`: the
//! viewer against one of them), `/replay/:day` the player's own solve.
//!
//! The page loads the logs with `load_replay`, which only sends them to members, and for today
//! only once the viewer finished it. Everything else, the clock, the boards at any moment and
//! the finish order, is worked out here from the logs ([`state::Theatre`]).

pub mod controls;
pub mod grid;
pub mod legend;
pub mod stage;
pub mod state;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::api::load_replay;
use crate::components::celebration::Stamp;
use crate::components::layout::use_localized;
use crate::components::ui::card::Card;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{date_short, localized, prefix};
use crate::model::{LAUNCH_DAY, ReplayData, daily_number};
use crate::seo::title_text;
use crate::state::{Auth, error_toast};

use controls::Controls;
use grid::Apart;
use legend::{FinishLine, Legend};
use stage::Stage;
use state::{Layout, Theatre};

/// Whether the visitor asked for less motion: the replay starts paused then, for stepping.
fn reduced_motion() -> bool {
    #[cfg(feature = "browser")]
    {
        window()
            .match_media("(prefers-reduced-motion: reduce)")
            .ok()
            .flatten()
            .is_some_and(|m| m.matches())
    }
    #[cfg(not(feature = "browser"))]
    {
        false
    }
}

/// Runs the clock while the theatre plays, a frame at a time.
#[cfg(feature = "browser")]
fn run_frames(theatre: Theatre) {
    fn frame(theatre: Theatre, run: u32, last: f64) {
        request_animation_frame(move || {
            let current = theatre.playing.try_get_untracked() == Some(true)
                && theatre.run.try_with_value(|r| *r) == Some(run);
            if !current {
                return;
            }
            let now = js_sys::Date::now();
            // A tab in the background gets no frames: carry on from where it was
            theatre.tick((now - last).min(250.0));
            frame(theatre, run, now);
        });
    }
    let run = theatre.run.get_value();
    frame(theatre, run, js_sys::Date::now());
}

/// The keys of the theatre anywhere on the page, except while typing, and space on a button
/// (which presses it).
#[cfg(feature = "browser")]
fn listen_to_keys(theatre: Theatre) {
    let handle = window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.ctrl_key() || ev.meta_key() || ev.alt_key() || ev.default_prevented() {
            return;
        }
        let tag = ev
            .target()
            .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok())
            .map(|el| (el.tag_name(), el.has_attribute("contenteditable")));
        let skip = match &tag {
            Some((_, true)) => true,
            Some((tag, _)) => {
                matches!(tag.as_str(), "INPUT" | "TEXTAREA" | "SELECT")
                    || (ev.key() == " " && matches!(tag.as_str(), "BUTTON" | "A"))
            }
            None => false,
        };
        if !skip && controls::on_key(theatre, &ev.key(), ev.shift_key()) {
            ev.prevent_default();
        }
    });
    on_cleanup(move || handle.remove());
}

/// The theatre of a loaded day: the board (or the little boards), the controls, the finish line
/// and the members.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos components take their props by value"
)]
pub fn TheatreView(data: ReplayData, only: Option<Vec<i64>>) -> impl IntoView {
    let still = reduced_motion();
    let theatre = Theatre::new(data, only.as_deref());
    #[cfg(feature = "browser")]
    {
        Effect::new(move |_| {
            if theatre.playing.get() {
                run_frames(theatre);
            }
        });
        listen_to_keys(theatre);
        if !still {
            // A moment to see the empty board first
            set_timeout(
                move || {
                    if theatre.time.try_get_untracked() == Some(0.0) {
                        theatre.play();
                    }
                },
                std::time::Duration::from_millis(700),
            );
        }
    }
    let (w, h) = theatre.data.with_value(|d| {
        d.puzzle
            .as_ref()
            .map_or((1, 1), |p| (i32::from(p.width), i32::from(p.height)))
    });
    view! {
        <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_360px]" data-testid="replay-theatre">
            <div class="flex min-w-0 flex-col gap-4">
                {move || match theatre.layout.get() {
                    Layout::Together => view! {
                        <div
                            class="pixel-frame mx-auto w-full rounded-2xl bg-[var(--px-wood)] p-[7px]"
                            style=format!("max-width: calc(58vh * {w} / {h} + 14px)")
                        >
                            <div class="overflow-hidden rounded-xl">
                                <Stage theatre still />
                            </div>
                        </div>
                    }
                        .into_any(),
                    Layout::Apart => view! { <Apart theatre /> }.into_any(),
                }}
                <Card class="px-4 py-3 sm:px-5">
                    <Controls theatre still />
                </Card>
            </div>
            <aside class="flex min-w-0 flex-col gap-6">
                <FinishLine theatre />
                <Legend theatre />
            </aside>
        </div>
    }
}

/// What the page has: still loading, nothing to see (not a member, or signed out), or the day.
#[derive(Clone, Debug, PartialEq)]
enum Loaded {
    Loading,
    Missing,
    Ready(ReplayData),
}

/// Today's daily, not finished by the viewer: no peeking.
#[component]
fn Locked() -> impl IntoView {
    let i18n = use_i18n();
    let daily = use_localized("/");
    view! {
        <div class="mx-auto flex max-w-xl flex-col items-center gap-4 py-12 text-center" data-testid="replay-locked">
            <Stamp kanji="待" class="size-20 text-5xl" />
            <h2 class="text-2xl font-black">{t!(i18n, replay.locked.title)}</h2>
            <p class="text-muted-foreground">{t!(i18n, replay.locked.text)}</p>
            <A href=daily exact=true attr:class="rounded-md bg-primary px-4 py-2 font-medium text-primary-foreground">
                {t!(i18n, replay.locked.play)} " →"
            </A>
        </div>
    }
}

/// A day nobody (shown) solved with a log.
#[component]
fn Empty() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="mx-auto flex max-w-xl flex-col items-center gap-3 py-12 text-center" data-testid="replay-empty">
            <span class="font-pixel text-5xl text-sea" aria-hidden="true">"〜"</span>
            <h2 class="text-xl font-black">{t!(i18n, replay.empty.title)}</h2>
            <p class="text-muted-foreground">{t!(i18n, replay.empty.text)}</p>
        </div>
    }
}

/// A group that doesn't exist or the visitor isn't in, or no user: like the group page.
#[component]
fn Missing(group: bool) -> impl IntoView {
    let i18n = use_i18n();
    let groups = use_localized("/groups");
    view! {
        <div class="mx-auto flex max-w-xl flex-col items-center gap-4 py-16 text-center" data-testid="replay-missing">
            <Stamp kanji="迷" class="size-20 text-5xl" />
            <h2 class="text-2xl font-black">
                {move || if group { t_string!(i18n, groups.page.not_found) } else { t_string!(i18n, replay.missing) }}
            </h2>
            {group.then(|| view! {
                <A href=groups attr:class="rounded-md bg-primary px-4 py-2 font-medium text-primary-foreground">
                    {t!(i18n, groups.page.back)}
                </A>
            })}
        </div>
    }
}

/// The replay page of a day, a group's (`group`) or the player's own.
#[component]
fn ReplayPage(group: Memo<Option<i64>>, day: Memo<Option<i64>>, own: bool) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let query = use_query_map();
    let loaded = RwSignal::new(Loaded::Loading);
    // `?with=<id>`: the viewer against one member
    let rival =
        Memo::new(move |_| query.with(|q| q.get("with").and_then(|w| w.parse::<i64>().ok())));

    Effect::new(move |_| {
        let (group, day) = (group.get(), day.get());
        let _ = auth.user.with(|u| u.as_ref().map(|u| u.id));
        if !auth.loaded.get() {
            return;
        }
        let Some(day) = day.filter(|_| own || group.is_some()) else {
            loaded.set(Loaded::Missing);
            return;
        };
        loaded.set(Loaded::Loading);
        spawn_local(async move {
            match load_replay(group, day).await {
                Ok(Some(data)) => loaded.set(Loaded::Ready(data)),
                Ok(None) => loaded.set(Loaded::Missing),
                Err(e) => error_toast(e),
            }
        });
    });

    let ready = move || match loaded.get() {
        Loaded::Ready(d) => Some(d),
        _ => None,
    };
    let title = move || {
        let page = if own {
            t_string!(i18n, replay.own_title)
        } else {
            t_string!(i18n, replay.title)
        };
        match ready().and_then(|d| d.group) {
            Some(g) => format!("{page} · {} · Hashi", g.name),
            None => format!("{page} · Hashi"),
        }
    };
    let back = move || {
        let p = prefix(i18n.get_locale());
        match group.get() {
            Some(id) if !own => format!("{p}/groups/{id}"),
            _ => localized(i18n.get_locale(), "/"),
        }
    };
    let back_text = move || match ready().and_then(|d| d.group) {
        Some(g) => g.name,
        None if own => t_string!(i18n, replay.page.back_daily).to_string(),
        None => t_string!(i18n, groups.page.back).to_string(),
    };
    // A day before or after, within what can be watched
    let day_link = move |delta: i64| {
        let d = ready()?;
        let to = d.day + delta;
        let first = d
            .group
            .as_ref()
            .map_or(LAUNCH_DAY, |g| g.first_day.max(LAUNCH_DAY));
        if to < first || to > d.today {
            return None;
        }
        let p = prefix(i18n.get_locale());
        Some(match &d.group {
            Some(g) => format!("{p}/groups/{}/replay/{to}", g.id),
            None => format!("{p}/replay/{to}"),
        })
    };
    let nav = move |delta: i64| {
        let label = move || {
            if delta < 0 {
                t_string!(i18n, replay.page.earlier)
            } else {
                t_string!(i18n, replay.page.later)
            }
        };
        let arrow = if delta < 0 { "←" } else { "→" };
        move || {
            match day_link(delta) {
            Some(href) => view! {
                <A
                    href=href
                    attr:class="inline-flex h-8 items-center gap-1 rounded-md border bg-background px-3 text-sm font-medium shadow-xs hover:bg-accent"
                    attr:aria-label=label
                    attr:data-tooltip=label
                >
                    {(delta < 0).then_some(arrow)}
                    <span class="hidden sm:inline">{label}</span>
                    {(delta > 0).then_some(arrow)}
                </A>
            }
                .into_any(),
            None => view! {
                <span class="inline-flex h-8 items-center rounded-md border px-3 text-sm opacity-40" aria-hidden="true">{arrow}</span>
            }
                .into_any(),
        }
        }
    };
    let subtitle = move || {
        let d = ready()?;
        let l = i18n.get_locale();
        Some(td_string!(
            l,
            replay.page.daily,
            number = daily_number(d.day),
            date = date_short(l, d.day)
        ))
    };
    view! {
        <Title text=title_text(title) />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto flex max-w-7xl flex-col gap-5 px-4 py-6 sm:px-6">
            <A href=back exact=true attr:class="w-fit text-sm text-primary hover:underline">
                "← " {back_text}
            </A>
            <div class="flex flex-wrap items-end justify-between gap-3">
                <div class="flex items-center gap-3">
                    <span class="hanko grid size-14 shrink-0 rotate-[-4deg] place-items-center rounded-lg font-pixel text-3xl">
                        "再"
                    </span>
                    <div class="min-w-0">
                        <p class="font-pixel text-sm text-primary">"再生 · saisei"</p>
                        <h1 class="text-3xl font-black">
                            {move || if own { t_string!(i18n, replay.own_title) } else { t_string!(i18n, replay.title) }}
                        </h1>
                        <p class="text-sm text-muted-foreground" data-testid="replay-day">{subtitle}</p>
                    </div>
                </div>
                <div class="flex gap-1.5">{nav(-1)} {nav(1)}</div>
            </div>
            {move || match loaded.get() {
                Loaded::Loading => view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                Loaded::Missing => view! { <Missing group=!own /> }.into_any(),
                Loaded::Ready(data) if data.locked => view! { <Locked /> }.into_any(),
                Loaded::Ready(data) if data.end() == 0 || data.puzzle.is_none() => view! { <Empty /> }.into_any(),
                Loaded::Ready(data) => {
                    let only = rival.get_untracked().map(|r| vec![data.me, r]);
                    view! { <TheatreView data only /> }.into_any()
                }
            }}
        </section>
    }
}

/// `/groups/:id/replay/:day`: every member's solve of the day at once.
#[component]
pub fn GroupReplayPage() -> impl IntoView {
    let params = use_params_map();
    let group =
        Memo::new(move |_| params.with(|p| p.get("id").and_then(|id| id.parse::<i64>().ok())));
    let day = Memo::new(move |_| params.with(|p| p.get("day").and_then(|d| d.parse::<i64>().ok())));
    view! { <ReplayPage group day own=false /> }
}

/// `/replay/:day`: the player's own solve of the day.
#[component]
pub fn OwnReplayPage() -> impl IntoView {
    let params = use_params_map();
    let group = Memo::new(|_| None);
    let day = Memo::new(move |_| params.with(|p| p.get("day").and_then(|d| d.parse::<i64>().ok())));
    view! { <ReplayPage group day own=true /> }
}

/// The path of a group's replay of `day` (without the language), against `rival` if given.
pub fn replay_path(group: i64, day: i64, rival: Option<i64>) -> String {
    match rival {
        Some(r) => format!("/groups/{group}/replay/{day}?with={r}"),
        None => format!("/groups/{group}/replay/{day}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Lane, LaneOutcome, Mark, MarkKind, MoveEvent, ReplayGroup};

    fn lane(user_id: i64, name: &str, log: Vec<MoveEvent>, outcome: LaneOutcome) -> Lane {
        Lane {
            user_id,
            name: name.into(),
            guest: false,
            colour: u8::try_from(user_id).unwrap(),
            outcome,
            log,
            marks: vec![Mark {
                t: 500,
                kind: MarkKind::Hint,
            }],
        }
    }

    fn data() -> ReplayData {
        let mut puzzle = common::Puzzle::empty(5, 5);
        puzzle.set(common::Pos::new(0, 0), 2);
        puzzle.set(common::Pos::new(4, 0), 3);
        puzzle.set(common::Pos::new(4, 4), 1);
        let mv = |t, edge, count| MoveEvent {
            t,
            edge,
            count,
            tap: true,
        };
        let solved = LaneOutcome::Solved {
            time_ms: 40_000,
            hints: 1,
            checks: 0,
            ranked: false,
        };
        ReplayData {
            day: 20_746,
            today: 20_747,
            group: Some(ReplayGroup {
                id: 1,
                name: "Bridge Club".into(),
                first_day: 20_700,
            }),
            me: 1,
            locked: false,
            puzzle: Some(puzzle),
            lanes: vec![
                lane(
                    1,
                    "aiko",
                    vec![mv(1000, 0, 1), mv(2000, 0, 2), mv(9000, 1, 1)],
                    solved,
                ),
                lane(2, "bram", vec![mv(1500, 0, 2), mv(4000, 1, 1)], solved),
                lane(3, "chen", Vec::new(), LaneOutcome::GaveUp),
                lane(4, "emre", Vec::new(), LaneOutcome::Missing),
            ],
            group_puzzle: None,
        }
    }

    /// The theatre as the server would render it: the board, the controls, the members and
    /// the finish line, together and apart.
    #[tokio::test]
    async fn the_theatre_renders() {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        owner.with(|| {
            crate::app::provide_server_i18n();
            let html = view! { <TheatreView data=data() only=None /> }.to_html();
            for text in [
                "replay-stage",
                "replay-timeline",
                "aiko",
                "bram",
                "chen",
                "emre",
                "1h 0c",
                "data-flag",
                "data-mark=\"hint\"",
            ] {
                assert!(html.contains(text), "{text}");
            }
            // Nobody across at the start: the finish line is empty
            assert!(!html.contains("data-place"));
            let duel = view! { <TheatreView data=data() only=Some(vec![1, 2]) /> }.to_html();
            assert!(duel.contains("replay-stage"));
            let theatre = Theatre::new(data(), None);
            theatre.layout.set(Layout::Apart);
            theatre.seek(9000.0);
            let apart = view! { <Apart theatre /> }.to_html();
            assert!(apart.contains("replay-apart"), "{apart}");
            let finish = view! { <FinishLine theatre /> }.to_html();
            assert!(finish.contains("data-place=\"2\""), "{finish}");
        });
    }

    /// A newer server's kind of help (an app that wasn't updated yet) gets no marker.
    #[tokio::test]
    async fn unknown_marks_are_left_out() {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        owner.with(|| {
            crate::app::provide_server_i18n();
            let markers = |data| {
                view! { <TheatreView data only=None /> }
                    .to_html()
                    .matches("data-mark=")
                    .count()
            };
            let mut newer = data();
            newer.lanes[0].marks.push(Mark {
                t: 700,
                kind: MarkKind::Unknown,
            });
            assert!(markers(data()) > 0);
            assert_eq!(markers(newer), markers(data()));
        });
    }

    #[test]
    fn replay_paths() {
        assert_eq!(replay_path(3, 20_747, None), "/groups/3/replay/20747");
        assert_eq!(
            replay_path(3, 20_747, Some(9)),
            "/groups/3/replay/20747?with=9"
        );
    }
}
