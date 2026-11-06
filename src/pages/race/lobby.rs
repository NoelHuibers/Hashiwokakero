//! The lobby of a race: who joined, the host's start, and the link to share.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;

use crate::api::{join_group_race, leave_group_race, start_group_race};
use crate::components::share::ShareBar;
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::dialog::ConfirmDialog;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{player_name, prefix};
use crate::model::{MIN_RACERS, RaceView};
use crate::pages::groups::today::ColourPip;
use crate::state::error_toast;

use super::state::RaceState;
use super::{SolvedBefore, browser_now, race_path};

/// The link to a race's page, on this site.
pub fn race_url(group: i64, race: i64) -> String {
    format!("{}{}", crate::seo::public_origin(), race_path(group, race))
}

/// Minutes until the lobby closes, at least one.
fn minutes_left(view: &RaceView, now: i64) -> i64 {
    ((view.expires_at() - now + 59_999) / 60_000).max(1)
}

#[component]
pub fn Lobby(race: RaceState) -> impl IntoView {
    let i18n = use_i18n();
    let navigate = use_navigate();
    let view_of = Memo::new(move |_| race.view.get());
    let joined = Memo::new(move |_| view_of.with(|v| v.as_ref().is_some_and(RaceView::joined)));
    let host = Memo::new(move |_| view_of.with(|v| v.as_ref().is_some_and(RaceView::is_host)));
    let shared = Memo::new(move |_| view_of.with(|v| v.as_ref().is_some_and(|v| v.shared.is_some())));
    let can_start =
        Memo::new(move |_| view_of.with(|v| v.as_ref().is_some_and(RaceView::can_start)));
    let left = Memo::new(move |_| {
        let now = race.server_now();
        view_of.with(|v| v.as_ref().map(|v| minutes_left(v, now)))
    });
    let join = move |_| {
        let Some(id) = race.id() else { return };
        race.busy.set(true);
        spawn_local(async move {
            let sent = browser_now();
            let reply = join_group_race(id).await;
            if race.busy.try_set(false).is_some() {
                return;
            }
            match reply {
                Ok(view) => race.apply(view, sent, browser_now()),
                Err(e) => error_toast(e),
            }
        });
    };
    let start = move |_| {
        let Some(id) = race.id() else { return };
        race.busy.set(true);
        spawn_local(async move {
            let sent = browser_now();
            let reply = start_group_race(id).await;
            if race.busy.try_set(false).is_some() {
                return;
            }
            match reply {
                Ok(view) => race.apply(view, sent, browser_now()),
                Err(e) => error_toast(e),
            }
        });
    };
    let leave = move || {
        let Some((id, group)) = race
            .view
            .with_untracked(|v| v.as_ref().map(|v| (v.id, v.group_id)))
        else {
            return;
        };
        race.busy.set(true);
        let navigate = navigate.clone();
        spawn_local(async move {
            let reply = leave_group_race(id).await;
            if race.busy.try_set(false).is_some() {
                return;
            }
            match reply {
                Ok(()) => navigate(
                    &format!("{}/groups/{group}", prefix(i18n.get_locale_untracked())),
                    NavigateOptions::default(),
                ),
                Err(e) => error_toast(e),
            }
        });
    };
    let leaving = RwSignal::new(false);
    let leave_now = leave.clone();
    let share_text = move || {
        race.view.with_untracked(|v| {
            v.as_ref().map_or_else(String::new, |v| {
                format!(
                    "{} 勝負\n{}",
                    td_string!(
                        i18n.get_locale_untracked(),
                        race.lobby.share_text,
                        group = v.group_name.clone()
                    ),
                    race_url(v.group_id, v.id)
                )
            })
        })
    };
    let players = move || {
        view_of.get().map(|v| {
            let me = v.me;
            let host = v.host;
            v.players
                .into_iter()
                .map(|p| {
                    let name = p.name.clone();
                    view! {
                        <li class="flex items-center gap-2 rounded-lg border bg-card px-3 py-2 text-sm" data-user=p.user_id class:opacity-60=p.away>
                            <ColourPip colour=p.colour />
                            <span class="min-w-0 flex-1 truncate font-medium">
                                {move || player_name(i18n.get_locale(), &name)}
                                {(p.user_id == me).then(|| view! { <span class="text-muted-foreground">" (" {t!(i18n, race.track.you)} ")"</span> })}
                            </span>
                            {p.solved_before.then(|| view! { <SolvedBefore /> })}
                            {(Some(p.user_id) == host).then(|| view! {
                                <Badge variant=BadgeVariant::Secondary>
                                    <span class="font-pixel" aria-hidden="true">"主 "</span>{t!(i18n, race.lobby.host)}
                                </Badge>
                            })}
                            {p.away.then(|| view! { <span class="text-xs italic text-muted-foreground">{t!(i18n, race.lobby.away)}</span> })}
                        </li>
                    }
                })
                .collect_view()
        })
    };
    view! {
        <Card class="gap-4" attr:data-testid="race-lobby">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, race.lobby.title)} <span class="font-pixel text-sm text-primary">"控室"</span>
                </CardTitle>
                <CardDescription>
                    {move || if shared.get() {
                        t_string!(i18n, race.shared.lobby)
                    } else {
                        t_string!(i18n, race.lobby.text)
                    }}
                </CardDescription>
                // Before anyone joins: the times go on a public board
                <Show when=move || shared.get()>
                    <p class="text-sm font-medium" data-testid="race-public">
                        <span class="font-pixel text-primary" aria-hidden="true">"共 "</span>
                        {t!(i18n, race.shared.public)}
                    </p>
                </Show>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                <ul class="flex flex-col gap-2" data-testid="race-players">{players}</ul>
                <div class="flex flex-wrap items-center gap-2">
                    <Show when=move || !joined.get()>
                        <Button class="h-11 flex-1" attr:disabled=move || race.busy.get() attr:data-testid="race-join" on:click=join>
                            {t!(i18n, race.lobby.join)} " 参加"
                        </Button>
                    </Show>
                    <Show when=move || host.get()>
                        <Button class="h-11 flex-1" attr:disabled=move || !can_start.get() || race.busy.get() attr:data-testid="race-start" on:click=start>
                            {move || if race.busy.get() {
                                t_string!(i18n, race.lobby.starting).to_string()
                            } else {
                                format!("{} 開始", t_string!(i18n, race.lobby.start))
                            }}
                        </Button>
                    </Show>
                    <Show when=move || joined.get()>
                        <Button
                            variant=ButtonVariant::Ghost
                            attr:disabled=move || race.busy.get()
                            attr:aria-haspopup=move || host.get().then_some("dialog")
                            attr:data-testid="race-leave"
                            on:click={
                                let leave = leave_now.clone();
                                move |_| if host.get_untracked() { leaving.set(true) } else { leave() }
                            }
                        >
                            {move || if host.get() { t_string!(i18n, race.lobby.cancel) } else { t_string!(i18n, race.lobby.leave) }}
                        </Button>
                    </Show>
                    <ConfirmDialog
                        open=leaving
                        question=Signal::derive(move || t_string!(i18n, race.lobby.cancel_confirm).to_string())
                        yes=Signal::derive(move || t_string!(i18n, race.lobby.cancel).to_string())
                        on_yes=leave
                        destructive=true
                    />
                </div>
                <p class="text-sm text-muted-foreground" role="status">
                    {move || {
                        let count = view_of.with(|v| v.as_ref().map_or(0, |v| v.players.len()));
                        if count < MIN_RACERS {
                            t_string!(i18n, race.lobby.need_more).to_string()
                        } else if host.get() {
                            String::new()
                        } else {
                            t_string!(i18n, race.lobby.waiting_host).to_string()
                        }
                    }}
                </p>
                <p class="text-xs text-muted-foreground">
                    {move || left.get().map(|minutes| t_string!(i18n, race.lobby.expires, minutes))}
                </p>
                <ShareBar
                    text=share_text
                    outline=true
                    label=Signal::derive(move || t_string!(i18n, race.lobby.share).to_string())
                />
            </CardContent>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use common::Difficulty;

    use super::*;
    use crate::model::{HelpPenalties, LOBBY_MS};

    #[test]
    fn the_lobby_counts_down_whole_minutes() {
        let view = RaceView {
            id: 1,
            group_id: 1,
            group_name: String::new(),
            me: 1,
            host: Some(1),
            difficulty: Difficulty::Easy,
            penalties: HelpPenalties::default(),
            created_at: 0,
            starts_at: None,
            ended_at: None,
            players: vec![],
            puzzle: None,
            solution: None,
            rematch: None,
            rematch_open: false,
            server_now: 0,
            shared: None,
        };
        assert_eq!(minutes_left(&view, 0), 15);
        assert_eq!(minutes_left(&view, 1), 15);
        assert_eq!(minutes_left(&view, 60_001), 14);
        assert_eq!(minutes_left(&view, LOBBY_MS - 1), 1);
        assert_eq!(minutes_left(&view, LOBBY_MS + 5), 1);
    }
}
