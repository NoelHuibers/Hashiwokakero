//! The author's view of their puzzle (自作): they can't play it, they watch how everyone does,
//! where the help went, and can take it back while it's open. Once it's closed, the applause.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;

use crate::api::withdraw_group_puzzle;
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::dialog::ConfirmDialog;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::prefix;
use crate::model::{PuzzleStats, format_duration};
use crate::state::{ToastKind, error_toast, toast};

use super::replay::ReplayLink;
use super::state::Play;

/// A number of the author's view.
#[component]
fn Tile(label: String, value: String) -> impl IntoView {
    view! {
        <div class="flex flex-col items-center gap-0.5 rounded-lg border bg-card px-2 py-2 shadow-xs">
            <span class="font-pixel text-lg tabular-nums leading-tight">{value}</span>
            <span class="text-center text-[10px] font-bold uppercase tracking-[0.12em] text-muted-foreground">{label}</span>
        </div>
    }
}

/// How everyone did: solved, the fastest and the middle time, the helps taken.
#[component]
pub fn StatsGrid(stats: PuzzleStats, open: bool) -> impl IntoView {
    let i18n = use_i18n();
    let time = |ms: Option<i64>| ms.map_or_else(|| "–".to_string(), format_duration);
    move || {
        let l = i18n.get_locale();
        let s = &stats;
        let (last_label, last_value) = if open {
            (td_string!(l, kadai.stats.playing), s.playing)
        } else {
            (td_string!(l, kadai.stats.gave_up), s.gave_up)
        };
        view! {
            <div class="grid grid-cols-3 gap-2" data-testid="kadai-stats">
                <Tile label=td_string!(l, kadai.stats.solved).to_string() value=format!("{} / {}", s.solved, s.players) />
                <Tile label=td_string!(l, kadai.stats.fastest).to_string() value=time(s.fastest_ms) />
                <Tile label=td_string!(l, kadai.stats.median).to_string() value=time(s.median_ms) />
                <Tile label=td_string!(l, kadai.stats.hints).to_string() value=s.hints.to_string() />
                <Tile label=td_string!(l, kadai.stats.checks).to_string() value=s.checks.to_string() />
                <Tile label=last_label.to_string() value=last_value.to_string() />
            </div>
        }
    }
}

/// Where the help went: the bridge marked on the board.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos components take their props by value"
)]
pub fn Hardest(stats: PuzzleStats) -> impl IntoView {
    let i18n = use_i18n();
    move || {
        let l = i18n.get_locale();
        let text = match stats.hardest {
            Some(h) => td_string!(l, kadai.stats.hardest, hints = h.hints, wrong = h.wrong),
            None if stats.solved > 0 => td_string!(l, kadai.stats.no_help).to_string(),
            None => return None,
        };
        Some(view! {
            <p class="flex items-start gap-2 rounded-lg border border-dashed bg-muted/40 p-3 text-sm" data-testid="kadai-hardest">
                <span class="font-pixel text-primary" aria-hidden="true">"難"</span>
                <span>{text}</span>
            </p>
        })
    }
}

/// 拍手: the applause for an author when their puzzle closed.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos components take their props by value"
)]
pub fn Applause(stats: PuzzleStats) -> impl IntoView {
    let i18n = use_i18n();
    let claps = usize::try_from(stats.solved.clamp(0, 12)).unwrap_or(0);
    view! {
        <div class="flex items-center gap-3">
            <span class="hanko grid size-12 shrink-0 rotate-[-8deg] place-items-center rounded-lg font-pixel text-2xl">"拍"</span>
            <div class="min-w-0">
                <p class="font-black">
                    {move || {
                        if stats.solved > 0 {
                            t_string!(i18n, kadai.kudos.claps, count = stats.solved)
                        } else {
                            t_string!(i18n, kadai.kudos.none).to_string()
                        }
                    }}
                </p>
                <p class="clap-row font-pixel text-primary" aria-hidden="true">
                    {(0..claps).map(|i| view! { <span style=format!("animation-delay: {}ms", i * 90)>"拍"</span> }).collect_view()}
                </p>
            </div>
        </div>
    }
}

#[component]
pub fn AuthorCard(play: Play) -> impl IntoView {
    let i18n = use_i18n();
    let navigate = use_navigate();
    let open = move || {
        play.page
            .with(|p| p.as_ref().is_some_and(|p| p.summary.open))
    };
    let stats = Memo::new(move |_| play.page.with(|p| p.as_ref().and_then(|p| p.stats.clone())));
    let withdrawing = RwSignal::new(false);
    let withdraw = move || {
        let Some(id) = play.id() else { return };
        let group = play
            .page
            .with_untracked(|p| p.as_ref().map_or(0, |p| p.group_id));
        play.busy.set(true);
        let navigate = navigate.clone();
        spawn_local(async move {
            let reply = withdraw_group_puzzle(id).await;
            play.busy.try_set(false);
            match reply {
                Ok(()) => {
                    toast(ToastKind::Success, |l| {
                        td_string!(l, kadai.author.withdrawn).to_string()
                    });
                    let path = format!("{}/groups/{group}", prefix(i18n.get_locale_untracked()));
                    navigate(&path, NavigateOptions::default());
                }
                Err(e) => error_toast(e),
            }
        });
    };
    view! {
        <Card class="gap-4" attr:data-testid="kadai-author">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, kadai.author.title)} <span class="font-pixel text-sm text-primary">"自作"</span>
                </CardTitle>
                <CardDescription>
                    {move || if open() { t_string!(i18n, kadai.author.open) } else { t_string!(i18n, kadai.author.closed) }}
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                {move || stats.get().map(|s| {
                    let open = open();
                    let applause = (!open).then(|| view! { <Applause stats=s.clone() /> });
                    let ids = play.page.with(|p| p.as_ref().map(|p| (p.group_id, p.summary.id)));
                    let replay = ids
                        .filter(|_| s.solved > 0)
                        .map(|(group, puzzle)| view! { <ReplayLink group puzzle /> });
                    view! {
                        {applause}
                        <StatsGrid stats=s.clone() open />
                        <Hardest stats=s />
                        {replay}
                    }
                })}
                <Show when=open>
                    <Button
                        variant=ButtonVariant::Ghost
                        class="w-fit text-destructive"
                        attr:disabled=move || play.busy.get()
                        attr:data-testid="kadai-withdraw"
                        attr:aria-haspopup="dialog"
                        on:click=move |_| withdrawing.set(true)
                    >
                        {t!(i18n, kadai.author.withdraw)}
                    </Button>
                </Show>
                <ConfirmDialog
                    open=withdrawing
                    question=Signal::derive(move || t_string!(i18n, kadai.author.withdraw_confirm).to_string())
                    yes=Signal::derive(move || t_string!(i18n, kadai.author.withdraw).to_string())
                    on_yes=withdraw
                    destructive=true
                />
            </CardContent>
        </Card>
    }
}

/// How everyone did with a closed puzzle, for the members who played it (総評).
#[component]
pub fn SummaryCard(play: Play) -> impl IntoView {
    let i18n = use_i18n();
    let stats = Memo::new(move |_| play.page.with(|p| p.as_ref().and_then(|p| p.stats.clone())));
    view! {
        <Card class="gap-4" attr:data-testid="kadai-summary">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, kadai.summary.title)} <span class="font-pixel text-sm text-primary">"総評"</span>
                </CardTitle>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                {move || stats.get().map(|s| {
                    let ids = play.page.with(|p| p.as_ref().map(|p| (p.group_id, p.summary.id)));
                    // Those who solved it see it from their result card
                    let replay = ids
                        .filter(|_| s.solved > 0 && play.mode() == Some(super::state::Mode::Closed))
                        .map(|(group, puzzle)| view! { <ReplayLink group puzzle /> });
                    view! {
                        <StatsGrid stats=s.clone() open=false />
                        <Hardest stats=s />
                        {replay}
                    }
                })}
            </CardContent>
        </Card>
    }
}
