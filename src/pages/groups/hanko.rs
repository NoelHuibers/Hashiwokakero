//! Hanko 判子 on a group's board of today (`model::hanko`).
//!
//! The seals on each result, the picker that presses one on someone else's, and the seals the
//! viewer got: a card on the group page, a list on the daily result.
//!
//! The board's seals live in a [`HankoBoard`] the rows share. Pressing shows at once and the
//! server's answer has the last word. The picker is a tray under the row, as wide as the board,
//! so its six seals fit on a phone too.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;

use crate::api::{open_hanko, set_hanko};
use crate::components::hanko::Seal;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{Locale, t, td_string, use_i18n};
use crate::model::{
    GroupToday, HankoKind, HankoMark, HankoReceived, TodayState, known_seals, tally, tilt, toggled,
};
use crate::state::{HankoInbox, error_toast};

use super::GroupContext;
use super::today::ColourPip;

/// The name of a seal: 速 is "Fast", ...
pub fn kind_name(locale: Locale, kind: HankoKind) -> String {
    match kind {
        HankoKind::Fast => td_string!(locale, hanko.kinds.fast),
        HankoKind::Wow => td_string!(locale, hanko.kinds.wow),
        HankoKind::Close => td_string!(locale, hanko.kinds.close),
        HankoKind::Funny => td_string!(locale, hanko.kinds.funny),
        HankoKind::Grit => td_string!(locale, hanko.kinds.grit),
        HankoKind::Cheers => td_string!(locale, hanko.kinds.cheers),
        // Left out wherever seals are shown
        HankoKind::Unknown => HankoKind::Unknown.kanji(),
    }
    .into()
}

/// The seals of one board, shared by its rows.
#[derive(Clone, Copy, Debug)]
pub struct HankoBoard {
    group: i64,
    day: i64,
    open: bool,
    viewer: Option<i64>,
    marks: RwSignal<Vec<HankoMark>>,
    /// The members' names, for who pressed a seal.
    names: StoredValue<Vec<(i64, String)>>,
    /// The result whose picker is open.
    picking: RwSignal<Option<i64>>,
    /// Seals pressed here, `(target, kind)`: they come down with a thud, the others are there.
    pressed: RwSignal<Vec<(i64, HankoKind)>>,
}

impl HankoBoard {
    /// The seals of `board` as `viewer` sees them, and the picker closing on a click elsewhere
    /// and on Escape.
    pub fn new(board: &GroupToday, viewer: Option<i64>) -> Self {
        let this = Self {
            group: board.group_id,
            day: board.day,
            open: board.open,
            viewer,
            marks: RwSignal::new(board.hanko.clone()),
            names: StoredValue::new(
                board
                    .entries
                    .iter()
                    .map(|e| (e.user_id, e.name.clone()))
                    .collect(),
            ),
            picking: RwSignal::new(None),
            pressed: RwSignal::new(Vec::new()),
        };
        // Only the browser has a window to listen on
        #[cfg(feature = "browser")]
        this.dismiss();
        this
    }

    /// Whether the viewer may press seals on `target`'s result: see `model::can_stamp`.
    pub fn can_stamp(self, target: i64, state: TodayState) -> bool {
        self.open
            && self.viewer.is_some_and(|v| v != target)
            && matches!(state, TodayState::Ranked { .. } | TodayState::Unranked)
    }

    /// Names the pick button of `target`'s row, to find it again.
    fn key(self, target: i64) -> String {
        format!("{}-{target}", self.group)
    }

    /// A member's name, "you" for the viewer.
    fn name(self, locale: Locale, id: i64) -> String {
        if Some(id) == self.viewer {
            return td_string!(locale, hanko.you).into();
        }
        self.names.with_value(|names| {
            names
                .iter()
                .find(|(user, _)| *user == id)
                .map(|(_, name)| name.clone())
                .unwrap_or_default()
        })
    }

    /// Whether the viewer pressed `kind` on `target`'s result (tracked).
    fn mine(self, target: i64, kind: HankoKind) -> bool {
        let Some(viewer) = self.viewer else {
            return false;
        };
        self.marks.with(|m| {
            m.iter()
                .any(|m| m.target == target && m.giver == viewer && m.kind == kind)
        })
    }

    /// Presses a seal (`on`) or takes it back: at once here, then on the server.
    fn press(self, target: i64, kind: HankoKind, on: bool) {
        let Some(viewer) = self.viewer else {
            return;
        };
        let before = self.marks.get_untracked();
        if on {
            self.pressed.update(|p| p.push((target, kind)));
            crate::audio::play(crate::audio::Sfx::Place);
        }
        self.marks
            .set(toggled(before.clone(), viewer, target, kind, on));
        let (group, day, marks) = (self.group, self.day, self.marks);
        spawn_local(async move {
            match set_hanko(group, target, day, kind, on).await {
                Ok(list) => marks.set(list),
                Err(e) => {
                    marks.set(before);
                    error_toast(e);
                }
            }
        });
    }

    /// Closes the picker, focus back on its button (`focus`).
    fn close(self, focus: bool) {
        let Some(target) = self.picking.get_untracked() else {
            return;
        };
        self.picking.set(None);
        if focus
            && let Ok(Some(button)) =
                document().query_selector(&format!("[data-hanko-pick=\"{}\"]", self.key(target)))
            && let Ok(button) = button.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = button.focus();
        }
    }

    /// The picker closes on a click outside of it (its own button toggles it) and on Escape.
    #[cfg(feature = "browser")]
    fn dismiss(self) {
        let outside = window_event_listener(leptos::ev::pointerdown, move |ev| {
            if self.picking.get_untracked().is_none() {
                return;
            }
            let inside = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                .and_then(|el| el.closest(".hanko-tray, [data-hanko-pick]").ok().flatten())
                .is_some();
            if !inside {
                self.close(false);
            }
        });
        let escape = window_event_listener(leptos::ev::keydown, move |ev| {
            if ev.key() == "Escape" && self.picking.get_untracked().is_some() {
                self.close(true);
            }
        });
        on_cleanup(move || {
            outside.remove();
            escape.remove();
        });
    }
}

/// The small ＋判 button after the seals of a result the viewer may stamp: opens the picker.
#[component]
fn PickButton(board: HankoBoard, target: i64, name: String) -> impl IntoView {
    let i18n = use_i18n();
    let label = Memo::new(move |_| td_string!(i18n.get_locale(), hanko.pick, name = name.clone()));
    let open = move || board.picking.get() == Some(target);
    view! {
        <button
            type="button"
            class="hanko-pick inline-flex h-[26px] shrink-0 items-center gap-0.5 rounded-full px-1.5 font-pixel text-[12px] leading-none"
            data-hanko-pick=board.key(target)
            aria-haspopup="true"
            aria-expanded=move || open().to_string()
            aria-label=move || label.get()
            data-tooltip=move || label.get()
            on:click=move |_| {
                board.picking.update(|p| *p = if *p == Some(target) { None } else { Some(target) });
            }
        >
            <span aria-hidden="true" class="text-[10px]">"＋"</span>
            <span aria-hidden="true">"判"</span>
        </button>
    }
}

/// One kind of seal on a result: the seal, inked if the viewer pressed it, and how many did.
/// A toggle on others' results, on one's own just the seal (focusable, for who pressed it).
#[component]
fn Pill(board: HankoBoard, target: i64, kind: HankoKind, can: bool) -> impl IntoView {
    let i18n = use_i18n();
    let givers = Memo::new(move |_| {
        board.marks.with(|m| {
            m.iter()
                .filter(|m| m.target == target && m.kind == kind)
                .map(|m| m.giver)
                .collect::<Vec<_>>()
        })
    });
    let mine = Memo::new(move |_| board.mine(target, kind));
    let label = Memo::new(move |_| {
        let locale = i18n.get_locale();
        let names = givers
            .get()
            .iter()
            .map(|&g| board.name(locale, g))
            .collect::<Vec<_>>()
            .join(", ");
        td_string!(locale, hanko.who, kind = kind_name(locale, kind), names)
    });
    let seal = move || {
        let mine = mine.get();
        // Only the seal just pressed comes down, not the ones that were there
        let thud = mine
            && board
                .pressed
                .with_untracked(|p| p.contains(&(target, kind)));
        view! {
            <Seal
                kind
                solid=mine
                tilt=tilt(target, kind)
                class=format!("size-6 text-[12px] {}", if thud { "hanko-press" } else { "" })
            />
        }
    };
    let count = move || givers.with(Vec::len);
    let class =
        "hanko-pill inline-flex items-center gap-0.5 rounded-full py-px ps-px pe-1.5 text-xs";
    if can {
        view! {
            <button
                type="button"
                class=class
                aria-pressed=move || mine.get().to_string()
                aria-label=move || label.get()
                data-tooltip=move || label.get()
                data-hanko=kind.id()
                on:click=move |_| board.press(target, kind, !mine.get_untracked())
            >
                {seal}
                <span class="font-pixel tabular-nums" aria-hidden="true">
                    {count}
                </span>
            </button>
        }
        .into_any()
    } else {
        view! {
            <span
                class=class
                tabindex="0"
                role="img"
                aria-label=move || label.get()
                data-tooltip=move || label.get()
                data-hanko=kind.id()
            >
                {seal}
                <span class="font-pixel tabular-nums" aria-hidden="true">
                    {count}
                </span>
            </span>
        }
        .into_any()
    }
}

/// The picker: the six seals in a tray under the row, each a toggle. Picking one presses it
/// and closes the tray. Arrow keys go along the seals.
#[component]
fn Tray(board: HankoBoard, target: i64, name: String) -> impl IntoView {
    let i18n = use_i18n();
    let tray = NodeRef::<leptos::html::Div>::new();
    // Into the tray when it opens
    Effect::new(move |_| {
        if let Some(tray) = tray.get()
            && let Ok(Some(first)) = tray.query_selector("button")
            && let Ok(first) = first.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = first.focus();
        }
    });
    let count = HankoKind::ALL.len();
    let keydown = move |ev: leptos::ev::KeyboardEvent| {
        let Some(tray) = tray.get_untracked() else {
            return;
        };
        let current = document()
            .active_element()
            .and_then(|el| el.get_attribute("data-i"))
            .and_then(|i| i.parse::<usize>().ok())
            .unwrap_or(0);
        // In the reading direction: right is back in Arabic
        let rtl = tray.closest("[dir=rtl]").ok().flatten().is_some();
        let (forward, back) = if rtl {
            ("ArrowLeft", "ArrowRight")
        } else {
            ("ArrowRight", "ArrowLeft")
        };
        let next = match ev.key().as_str() {
            k if k == forward => (current + 1) % count,
            k if k == back => (current + count - 1) % count,
            "Home" => 0,
            "End" => count - 1,
            _ => return,
        };
        ev.prevent_default();
        if let Ok(Some(el)) = tray.query_selector(&format!("[data-i=\"{next}\"]"))
            && let Ok(el) = el.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = el.focus();
        }
    };
    let label = td_string!(i18n.get_locale_untracked(), hanko.picker, name = name);
    let aria = label.clone();
    view! {
        <div
            node_ref=tray
            role="group"
            aria-label=aria
            class="hanko-tray absolute inset-x-0 z-30 mt-1.5 rounded-xl border bg-card p-2 shadow-xl animate-in fade-in zoom-in-95 motion-reduce:animate-none"
            data-testid="hanko-tray"
            on:keydown=keydown
        >
            <p class="flex items-baseline gap-1.5 px-1 pb-1.5 text-xs text-muted-foreground">
                <span class="font-pixel text-primary" aria-hidden="true">
                    "判子"
                </span>
                <span class="truncate">{label}</span>
            </p>
            <div class="grid grid-cols-6 gap-1">
                {HankoKind::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(i, kind)| {
                        let mine = Memo::new(move |_| board.mine(target, kind));
                        let kind_label = move || kind_name(i18n.get_locale(), kind);
                        view! {
                            <button
                                type="button"
                                class="hanko-choice flex min-w-0 flex-col items-center gap-1 rounded-lg px-0.5 py-1.5 transition-colors hover:bg-secondary focus-visible:outline-2 focus-visible:outline-ring"
                                aria-pressed=move || mine.get().to_string()
                                aria-label=kind_label
                                data-tooltip=kind_label
                                data-i=i
                                data-hanko=kind.id()
                                on:click=move |_| {
                                    board.press(target, kind, !mine.get_untracked());
                                    board.close(true);
                                }
                            >
                                <Seal
                                    kind
                                    solid=mine.get_untracked()
                                    tilt=tilt(target, kind)
                                    class="size-9 text-lg"
                                />
                                <span
                                    class="w-full truncate text-center text-[10px] leading-tight text-muted-foreground"
                                    aria-hidden="true"
                                >
                                    {kind_label}
                                </span>
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}

/// The seals on `target`'s result, under the name, with the button to press one (`can`) and
/// the picker when it's open. Nothing on a result without seals that can't take any.
#[component]
pub fn HankoStrip(board: HankoBoard, target: i64, name: String, can: bool) -> impl IntoView {
    let kinds = Memo::new(move |_| {
        board.marks.with(|m| {
            tally(m, target)
                .into_iter()
                .map(|(k, _)| k)
                .collect::<Vec<_>>()
        })
    });
    let picking = move || board.picking.get() == Some(target);
    view! {
        <div
            class="hanko-strip flex flex-wrap items-center gap-1 pt-1"
            class:hidden=move || !can && kinds.with(Vec::is_empty)
            data-hanko-of=target
        >
            <For each=move || kinds.get() key=|k| *k let:kind>
                <Pill board target kind can />
            </For>
            {can.then(|| view! { <PickButton board target name=name.clone() /> })}
        </div>
        {move || picking().then(|| view! { <Tray board target name=name.clone() /> })}
    }
}

/// One seal got: who pressed which on the viewer's result, new ones marked.
#[component]
fn ReceivedItem(item: HankoReceived, today: i64, show_group: bool, index: usize) -> impl IntoView {
    let i18n = use_i18n();
    let HankoReceived {
        group_name,
        day,
        giver,
        giver_name,
        giver_colour,
        kind,
        fresh,
        ..
    } = item;
    let kind_label = move || kind_name(i18n.get_locale(), kind);
    let yesterday = day < today;
    let meta = show_group || yesterday;
    view! {
        <li
            class="hanko-received flex items-center gap-3 rounded-lg px-2 py-1.5"
            data-fresh=fresh.to_string()
            data-testid="hanko-received"
        >
            <Seal
                kind
                solid=true
                tilt=tilt(giver, kind)
                class=format!("size-9 text-lg {}", if fresh { "hanko-press" } else { "" })
                delay=u32::try_from(index.min(8) * 90).unwrap_or(0)
            />
            <div class="min-w-0 flex-1 text-start">
                <p class="text-sm leading-snug">
                    {t!(
                        i18n, hanko.received.item, name = giver_name, kind = kind_label, <b> = <b class="font-semibold" />
                    )}
                </p>
                {meta
                    .then(|| {
                        view! {
                            <p class="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                                {show_group
                                    .then(|| {
                                        view! {
                                            <ColourPip colour=giver_colour />
                                            <span class="truncate">{group_name}</span>
                                        }
                                    })} {(show_group && yesterday).then_some("·")}
                                {yesterday
                                    .then(|| {
                                        view! {
                                            <span class="shrink-0">
                                                {t!(i18n, hanko.received.yesterday)}
                                            </span>
                                        }
                                    })}
                            </p>
                        }
                    })}
            </div>
            {fresh
                .then(|| {
                    view! {
                        <span class="shrink-0 rounded-full bg-primary px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wider text-primary-foreground">
                            <span class="font-pixel" aria-hidden="true">
                                "新 "
                            </span>
                            {t!(i18n, hanko.received.new)}
                        </span>
                    }
                })}
        </li>
    }
}

/// A list of seals got, newest first. In two columns where there's room (`wide`).
#[component]
fn ReceivedList(
    items: Vec<HankoReceived>,
    today: i64,
    show_group: bool,
    #[prop(optional)] wide: bool,
) -> impl IntoView {
    view! {
        <ul class=if wide { "grid grid-cols-1 gap-0.5 sm:grid-cols-2" } else { "grid grid-cols-1 gap-0.5" }>
            {items
                .into_iter()
                .enumerate()
                .map(|(index, item)| view! { <ReceivedItem item today show_group index /> })
                .collect_view()}
        </ul>
    }
}

/// Opens the seals got, in a group or all (`None`): loads them into `items`, and they're seen.
fn load_received(group: Option<i64>, items: RwSignal<Option<Vec<HankoReceived>>>) {
    let inbox = use_context::<HankoInbox>();
    spawn_local(async move {
        match open_hanko(group).await {
            Ok(list) => {
                if let Some(inbox) = inbox {
                    inbox.seen(group);
                }
                items.set(Some(known_seals(list)));
            }
            Err(e) => error_toast(e),
        }
    });
}

/// The group page's card of the seals the viewer got here lately. Shown once there are some,
/// or once the viewer finished today's daily (the others can stamp their result then).
#[component]
pub fn HankoCard(group: GroupContext) -> impl IntoView {
    let i18n = use_i18n();
    let items = RwSignal::new(None::<Vec<HankoReceived>>);
    // The group and whether the board is open: again when either changes
    let key = Memo::new(move |_| {
        group
            .page
            .with(|p| p.as_ref().map(|p| (p.id, p.today.open, p.today.day)))
    });
    Effect::new(move |_| {
        if let Some((id, _, _)) = key.get() {
            load_received(Some(id), items);
        }
    });
    move || {
        let (_, open, today) = key.get()?;
        let list = items.get()?;
        if list.is_empty() && !open {
            return None;
        }
        Some(view! {
            <Card class="gap-3" attr:data-testid="hanko-card">
                <CardHeader>
                    <CardTitle class="flex items-baseline gap-2 text-xl">
                        {t!(i18n, hanko.received.title)}
                        <span class="font-pixel text-sm text-primary">"判子"</span>
                    </CardTitle>
                    <CardDescription>{t!(i18n, hanko.received.text)}</CardDescription>
                </CardHeader>
                <CardContent class="flex flex-col gap-3">
                    {if list.is_empty() {
                        view! {
                            <div class="flex items-center gap-3 rounded-lg border border-dashed bg-muted/40 p-3">
                                <Seal
                                    kind=HankoKind::Cheers
                                    tilt=-6
                                    class="size-9 text-lg opacity-40"
                                />
                                <p class="text-sm text-muted-foreground">
                                    {t!(i18n, hanko.received.empty)}
                                </p>
                            </div>
                        }
                            .into_any()
                    } else {
                        view! { <ReceivedList items=list today show_group=false wide=true /> }.into_any()
                    }}
                    <p class="text-xs text-muted-foreground">
                        {t!(
                            i18n, hanko.received.how, <k> = <span class="font-pixel text-primary" />
                        )}
                    </p>
                </CardContent>
            </Card>
        })
    }
}

/// The seals got today, on the daily's result card: in all the player's groups, nothing
/// without any.
#[component]
pub fn ReceivedHanko(today: i64) -> impl IntoView {
    let i18n = use_i18n();
    let items = RwSignal::new(None::<Vec<HankoReceived>>);
    Effect::new(move |_| load_received(None, items));
    move || {
        let list = items.get().filter(|l| !l.is_empty())?;
        Some(view! {
            <div
                class="flex w-full flex-col gap-1.5 border-t pt-3 text-start"
                data-testid="hanko-on-result"
            >
                <p class="flex items-baseline gap-2 px-2 text-sm font-semibold">
                    {t!(i18n, hanko.received.title)}
                    <span class="font-pixel text-xs text-primary">"判子"</span>
                </p>
                <ReceivedList items=list today show_group=true />
            </div>
        })
    }
}

/// How many seals of a group wait to be seen, a badge on the group list (nothing without any).
#[component]
pub fn UnreadBadge(group: i64) -> impl IntoView {
    let i18n = use_i18n();
    let inbox = expect_context::<HankoInbox>();
    move || {
        let count = inbox.of(group);
        (count > 0).then(|| {
            let label = move || td_string!(i18n.get_locale(), hanko.unread, count);
            view! {
                <span
                    class="hanko-badge inline-flex shrink-0 items-center gap-1 rounded-full bg-primary px-2 py-0.5 text-xs font-bold text-primary-foreground"
                    data-tooltip=label
                    data-testid="hanko-badge"
                >
                    <span class="font-pixel" aria-hidden="true">
                        "判"
                    </span>
                    <span aria-hidden="true">{count}</span>
                    <span class="sr-only">{label}</span>
                </span>
            }
        })
    }
}

/// The group list asks again for the unread seals when it opens.
pub fn refresh_unread() {
    if let Some(inbox) = use_context::<HankoInbox>() {
        Effect::new(move |_| inbox.load(true));
    }
}

// Rendering on the server needs its features
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::model::{TodayEntry, UnreadHanko};

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

    const RANKED: TodayState = TodayState::Ranked {
        rank: 1,
        time_ms: 60_000,
        hints: 0,
        checks: 0,
    };

    /// aiko (1, the viewer), bram (2) and chen (3) finished; bram's result has 凄 from aiko and
    /// chen and 速 from chen.
    fn board() -> GroupToday {
        let entry = |user_id, name: &str| TodayEntry {
            user_id,
            name: name.into(),
            guest: false,
            colour: 0,
            stamp: None,
            state: RANKED,
        };
        let mark = |giver, kind| HankoMark {
            target: 2,
            giver,
            kind,
        };
        GroupToday {
            group_id: 9,
            group_name: "Team".into(),
            day: 20_747,
            open: true,
            entries: vec![entry(1, "aiko"), entry(2, "bram"), entry(3, "chen")],
            hanko: vec![
                mark(3, HankoKind::Fast),
                mark(1, HankoKind::Wow),
                mark(3, HankoKind::Wow),
            ],
        }
    }

    #[tokio::test]
    async fn others_results_take_seals_and_say_who_pressed_them() {
        let html = render(|| {
            let hanko = HankoBoard::new(&board(), Some(1));
            assert!(hanko.can_stamp(2, RANKED));
            assert!(hanko.can_stamp(2, TodayState::Unranked));
            assert!(!hanko.can_stamp(1, RANKED), "one's own");
            assert!(!hanko.can_stamp(3, TodayState::Waiting));
            view! { <HankoStrip board=hanko target=2 name="bram".to_string() can=true /> }
                .into_any()
        });
        assert!(
            html.contains("Stamp bram&#x27;s result with a hanko")
                || html.contains("Stamp bram's result with a hanko"),
            "{html}"
        );
        assert!(html.contains(r#"data-hanko-pick="9-2""#), "{html}");
        // In the picker's order: 速 first, then 凄, the viewer's own inked and pressed
        let fast = html.find(r#"data-hanko="fast""#).unwrap();
        let wow = html.find(r#"data-hanko="wow""#).unwrap();
        assert!(fast < wow, "{html}");
        assert!(html.contains("Amazing: you, chen"), "{html}");
        assert!(html.contains("Fast: chen"), "{html}");
        assert_eq!(html.matches(r#"aria-pressed="true""#).count(), 1, "{html}");
        assert_eq!(html.matches("<button").count(), 3, "{html}");
        // No picker until it's opened
        assert!(!html.contains("hanko-tray"), "{html}");
    }

    #[tokio::test]
    async fn ones_own_seals_are_shown_not_pressed() {
        let html = render(|| {
            let hanko = HankoBoard::new(&board(), Some(2));
            view! { <HankoStrip board=hanko target=2 name="bram".to_string() can=false /> }
                .into_any()
        });
        assert!(!html.contains("<button"), "{html}");
        assert!(html.contains("Amazing: aiko, chen"), "{html}");
        assert_eq!(html.matches(r#"tabindex="0""#).count(), 2, "{html}");
    }

    #[tokio::test]
    async fn the_picker_has_the_six_seals() {
        let html = render(|| {
            let hanko = HankoBoard::new(&board(), Some(1));
            hanko.picking.set(Some(2));
            view! { <HankoStrip board=hanko target=2 name="bram".to_string() can=true /> }
                .into_any()
        });
        assert!(html.contains("hanko-tray"), "{html}");
        assert!(html.contains("Hanko for bram"), "{html}");
        for kind in HankoKind::ALL {
            assert!(html.contains(kind.kanji()), "{kind:?} in {html}");
        }
        for i in 0..HankoKind::ALL.len() {
            assert!(html.contains(&format!(r#"data-i="{i}""#)), "{html}");
        }
    }

    #[tokio::test]
    async fn seals_got_say_who_and_which_and_what_is_new() {
        let item = |giver_name: &str, kind, day, fresh| HankoReceived {
            group_id: 9,
            group_name: "Lunch Bridges".into(),
            day,
            giver: 3,
            giver_name: giver_name.into(),
            giver_colour: 2,
            kind,
            at: 0,
            fresh,
        };
        let html = render(|| {
            view! {
                <ReceivedList
                    items=vec![
                        item("Keiko", HankoKind::Wow, 20_747, true),
                        item("Ren", HankoKind::Grit, 20_746, false),
                    ]
                    today=20_747
                    show_group=true
                />
            }
            .into_any()
        });
        assert!(html.contains("Keiko"), "{html}");
        assert!(html.contains("stamped your result: "), "{html}");
        assert!(html.contains("Amazing"), "{html}");
        assert!(html.contains("Lunch Bridges"), "{html}");
        assert_eq!(html.matches(r#"data-fresh="true""#).count(), 1, "{html}");
        assert_eq!(html.matches("hanko-press").count(), 1, "{html}");
        // Only Ren's is of yesterday's daily
        assert_eq!(
            html.matches("yesterday&#x27;s daily").count()
                + html.matches("yesterday's daily").count(),
            1,
            "{html}"
        );
    }

    #[tokio::test]
    async fn the_badge_counts_a_groups_unread_seals() {
        let html = render(|| {
            let inbox = HankoInbox::new();
            inbox.unread.set(vec![UnreadHanko {
                group_id: 9,
                count: 3,
            }]);
            provide_context(inbox);
            view! {
                <UnreadBadge group=9 />
                <UnreadBadge group=4 />
                <crate::components::hanko::HankoDot />
            }
            .into_any()
        });
        assert_eq!(
            html.matches(r#"data-testid="hanko-badge""#).count(),
            1,
            "{html}"
        );
        assert!(html.contains("New hanko: 3"), "{html}");
        assert!(html.contains(r#"data-testid="hanko-dot""#), "{html}");
    }
}
