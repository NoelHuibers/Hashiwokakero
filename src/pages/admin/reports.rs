//! Reports of players: rename the player, ban them, or dismiss the report. Every report is to be
//! looked at within 24 hours (the community rules promise it), the oldest is on top.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{admin_ban_player, admin_rename_player, admin_reports, admin_resolve_report};
use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::model::{AdminReport, BanOutcome, MS_PER_DAY, ReportOutcome, ReportReason, date_string};
use crate::state::{ToastKind, toast};

use super::{SECTION, error};

const fn reason(r: ReportReason) -> &'static str {
    match r {
        ReportReason::Name => "name",
        ReportReason::Cheating => "cheating",
        ReportReason::Other => "other",
    }
}

/// When, to the minute in UTC: "2026-10-29 14:05 UTC", for the 24 hours.
fn when(at: i64) -> String {
    let minute = at.rem_euclid(MS_PER_DAY) / 60_000;
    format!(
        "{} {:02}:{:02} UTC",
        date_string(at.div_euclid(MS_PER_DAY)),
        minute / 60,
        minute % 60
    )
}

fn player_account(r: &AdminReport) -> String {
    let id = r.player;
    r.player_username
        .as_ref()
        .map_or_else(|| format!("guest #{id}"), |u| format!("@{u} · #{id}"))
}

#[component]
pub(super) fn ReportsAdmin(on_change: Callback<()>) -> impl IntoView {
    let list = RwSignal::new(None::<Vec<AdminReport>>);
    let load = move || {
        spawn_local(async move {
            match admin_reports().await {
                Ok(l) => list.set(Some(l)),
                Err(e) => error(&e),
            }
        });
    };
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            load();
        }
    });
    let done = move || {
        load();
        on_change.run(());
    };
    let resolve = move |id: i64, outcome: ReportOutcome| {
        spawn_local(async move {
            match admin_resolve_report(id, outcome).await {
                Ok(_) => done(),
                Err(e) => error(&e),
            }
        });
    };
    let rename = move |id: i64, player: i64| {
        if !window()
            .confirm_with_message(&format!(
                "Rename player #{player} to \"Player {player}\"? This resolves every open report of their name."
            ))
            .unwrap_or(false)
        {
            return;
        }
        spawn_local(async move {
            match admin_rename_player(player).await {
                Ok(Some(name)) => {
                    toast(ToastKind::Success, move |_| {
                        format!("Player #{player} is now {name}.")
                    });
                    resolve(id, ReportOutcome::Renamed);
                }
                Ok(None) => toast(ToastKind::Error, move |_| {
                    format!("There is no player #{player}.")
                }),
                Err(e) => error(&e),
            }
        });
    };
    let ban = move |id: i64, player: i64, why: String| {
        if !window()
            .confirm_with_message(&format!(
                "Ban player #{player}? They are signed out and can't sign in, and their results leave every board until the ban is lifted. This resolves every open report of them."
            ))
            .unwrap_or(false)
        {
            return;
        }
        spawn_local(async move {
            match admin_ban_player(player, why).await {
                Ok(BanOutcome::Banned(name)) => {
                    toast(ToastKind::Success, move |_| {
                        format!("{name} (#{player}) is banned.")
                    });
                    resolve(id, ReportOutcome::Banned);
                }
                Ok(BanOutcome::NoSuchPlayer) => toast(ToastKind::Error, move |_| {
                    format!("There is no player #{player}.")
                }),
                Ok(BanOutcome::Admin) => toast(ToastKind::Error, move |_| {
                    format!("Player #{player} is an admin, admins can't be banned.")
                }),
                Err(e) => error(&e),
            }
        });
    };
    let dismiss = move |id: i64| {
        if window()
            .confirm_with_message(&format!("Dismiss report #{id}?"))
            .unwrap_or(false)
        {
            resolve(id, ReportOutcome::Dismissed);
        }
    };

    view! {
        <Card class=SECTION>
            <CardHeader class="max-sm:px-0">
                <CardTitle class="text-xl">"Reports"</CardTitle>
                <CardDescription>
                    "Players reported from their profiles, the one waiting longest first. Look at each within 24 hours: rename the player (\"Player <number>\", for a name), ban them (cheating, abuse) or dismiss the report. Each new report is mailed too, if SMTP is set up. Resolved reports are deleted after half a year."
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3 max-sm:px-0">
                {move || match list.get() {
                    None => view! { <p class="text-sm text-muted-foreground">"…"</p> }.into_any(),
                    Some(l) if l.is_empty() => view! { <p class="text-sm text-muted-foreground">"No open reports."</p> }.into_any(),
                    Some(l) => {
                        l.into_iter()
                            .map(|r| {
                                let (id, player) = (r.id, r.player);
                                let href = format!("/player/{player}");
                                let renamed = r.player_name != r.reported_name;
                                let why = format!(
                                    "Report #{id} ({}){}",
                                    reason(r.reason),
                                    if r.note.is_empty() { String::new() } else { format!(": {}", r.note) }
                                );
                                view! {
                                    <article class="flex flex-col gap-2 rounded-lg border bg-card p-3" data-report=id>
                                        <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
                                            <span class="font-mono text-xs text-muted-foreground">{format!("#{id}")}</span>
                                            <a href=href class="font-medium underline-offset-4 hover:underline">{r.player_name.clone()}</a>
                                            <span class="font-mono text-xs text-muted-foreground">{player_account(&r)}</span>
                                            <Badge variant=BadgeVariant::Outline size=BadgeSize::Sm>{reason(r.reason)}</Badge>
                                            {r.banned.then(|| view! { <Badge variant=BadgeVariant::Destructive size=BadgeSize::Sm>"banned"</Badge> })}
                                            <span class="flex-1"></span>
                                            <span class="text-xs text-muted-foreground tabular-nums">{when(r.updated_at)}</span>
                                        </div>
                                        {renamed.then(|| view! {
                                            <p class="text-xs text-muted-foreground">"Reported as " <span class="font-medium text-foreground">{r.reported_name.clone()}</span></p>
                                        })}
                                        {(!r.note.is_empty()).then(|| view! {
                                            <p class="whitespace-pre-wrap break-words text-sm">{r.note.clone()}</p>
                                        })}
                                        <div class="flex flex-wrap items-center gap-2">
                                            <span class="text-xs text-muted-foreground">
                                                {format!("by {} (#{})", if r.reporter_name.is_empty() { "?" } else { &r.reporter_name }, r.reporter)}
                                            </span>
                                            <span class="flex-1"></span>
                                            <Button variant=ButtonVariant::Outline size=ButtonSize::Sm on:click=move |_| rename(id, player)>"Rename"</Button>
                                            <Button variant=ButtonVariant::Destructive size=ButtonSize::Sm on:click=move |_| ban(id, player, why.clone())>"Ban"</Button>
                                            <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on:click=move |_| dismiss(id)>"Dismiss"</Button>
                                        </div>
                                    </article>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }
                }}
            </CardContent>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_and_accounts() {
        assert_eq!(
            when(MS_PER_DAY * 20_755 + 14 * 3_600_000 + 5 * 60_000 + 59_999),
            "2026-10-29 14:05 UTC"
        );
        let report = AdminReport {
            id: 3,
            player: 9,
            player_name: "bob".into(),
            reported_name: "bob".into(),
            player_username: None,
            banned: false,
            reason: ReportReason::Name,
            note: String::new(),
            reporter: 7,
            reporter_name: "noel".into(),
            created_at: 0,
            updated_at: 0,
        };
        assert_eq!(player_account(&report), "guest #9");
        let named = AdminReport {
            player_username: Some("bob".into()),
            ..report
        };
        assert_eq!(player_account(&named), "@bob · #9");
    }
}
