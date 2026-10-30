//! Reporting a player to us and blocking them, on their profile (not on one's own).
//!
//! "Report" opens a small dialog with the reason and a note, "Block" asks first. A blocked
//! player's name isn't shown to the viewer anywhere else any more (`server::blocks`), their
//! profile is where to unblock them (and the account page).

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::report_player;
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::dialog::Dialog;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::error_text;
use crate::model::{MAX_REPORT_NOTE, ReportReason};
use crate::state::{Auth, Blocks, ToastKind, toast};

fn reason_label(reason: ReportReason) -> impl Fn() -> &'static str + Send + Sync + Copy {
    let i18n = use_i18n();
    move || match reason {
        ReportReason::Name => td_string!(i18n.get_locale(), ranks.player.report.name),
        ReportReason::Cheating => td_string!(i18n.get_locale(), ranks.player.report.cheating),
        ReportReason::Other => td_string!(i18n.get_locale(), ranks.player.report.other),
    }
}

/// The report: the reason (a name by default, what reports are mostly about) and a note.
#[component]
fn ReportDialog(player: i64, name: StoredValue<String>, open: RwSignal<bool>) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let reason = RwSignal::new(ReportReason::Name);
    let note = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let (why, text) = (reason.get_untracked(), note.get_untracked());
        spawn_local(async move {
            match report_player(player, why, text).await {
                Ok(()) => {
                    open.set(false);
                    toast(ToastKind::Success, |l| {
                        td_string!(l, ranks.player.report.sent).to_string()
                    });
                    // A visitor is a guest now
                    if auth.user.with_untracked(Option::is_none) {
                        auth.refresh();
                    }
                }
                Err(e) => error.set(Some(error_text(i18n.get_locale_untracked(), &e))),
            }
            busy.set(false);
        });
    };
    let radio = move |r: ReportReason| {
        let label = reason_label(r);
        view! {
            <label class="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-accent/50">
                <input
                    type="radio"
                    name="report-reason"
                    class="size-4 accent-primary"
                    value=r.id()
                    prop:checked=move || reason.get() == r
                    on:change=move |_| reason.set(r)
                />
                {label}
            </label>
        }
    };
    view! {
        <Dialog open labelled_by="report-title">
            <form class="flex flex-col gap-4" on:submit=submit data-testid="report-dialog">
                <div class="flex flex-col gap-1">
                    <h2 id="report-title" class="text-xl font-black">
                        {move || td_string!(i18n.get_locale(), ranks.player.report.title, name = name.get_value())}
                    </h2>
                    <p class="text-sm text-muted-foreground">
                        {move || td_string!(i18n.get_locale(), ranks.player.report.intro, name = name.get_value())}
                    </p>
                </div>
                <fieldset class="flex flex-col gap-1">
                    <legend class="mb-1 text-sm font-medium">{t!(i18n, ranks.player.report.reason)}</legend>
                    {ReportReason::ALL.into_iter().map(radio).collect_view()}
                </fieldset>
                <div class="flex flex-col gap-2">
                    <label for="report-note" class="text-sm font-medium">{t!(i18n, ranks.player.report.note)}</label>
                    // 16px on phones, or iOS zooms in on focus
                    <textarea
                        id="report-note"
                        class="min-h-24 w-full rounded-md border border-input bg-transparent px-3 py-2 text-base shadow-xs outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 dark:bg-input/30 md:text-sm"
                        bind:value=note
                        maxlength=MAX_REPORT_NOTE
                    ></textarea>
                    <p class="flex justify-between gap-2 text-xs text-muted-foreground">
                        <span>{t!(i18n, ranks.player.report.note_hint)}</span>
                        <span class="tabular-nums">{move || format!("{} / {MAX_REPORT_NOTE}", note.with(|n| n.chars().count()))}</span>
                    </p>
                </div>
                {move || error.get().map(|e| view! { <p class="text-sm text-destructive">{e}</p> })}
                <div class="flex flex-wrap justify-end gap-2">
                    <Button variant=ButtonVariant::Ghost attr:r#type="button" on:click=move |_| open.set(false)>
                        {t!(i18n, ranks.player.report.cancel)}
                    </Button>
                    <Button attr:r#type="submit" attr:disabled=move || busy.get()>
                        {move || if busy.get() { t_string!(i18n, account.busy).to_string() } else { t_string!(i18n, ranks.player.report.send).to_string() }}
                    </Button>
                </div>
            </form>
        </Dialog>
    }
}

/// Asks before blocking: what it does, and that the player isn't told.
#[component]
fn BlockDialog(player: i64, name: StoredValue<String>, open: RwSignal<bool>) -> impl IntoView {
    let i18n = use_i18n();
    let blocks = expect_context::<Blocks>();
    let block = move |_| {
        open.set(false);
        blocks.set(player, name.get_value(), true);
    };
    view! {
        <Dialog open labelled_by="block-question">
            <p id="block-question" class="font-medium leading-relaxed">
                {move || td_string!(i18n.get_locale(), ranks.player.block.confirm, name = name.get_value())}
            </p>
            <div class="flex flex-wrap justify-end gap-2">
                <Button variant=ButtonVariant::Ghost on:click=move |_| open.set(false)>
                    {t!(i18n, ranks.player.report.cancel)}
                </Button>
                <Button variant=ButtonVariant::Destructive on:click=block attr:data-testid="block-confirm">
                    {t!(i18n, ranks.player.block.yes)}
                </Button>
            </div>
        </Dialog>
    }
}

/// "Report" and "Block" (or "Unblock") under the rival toggle of a profile, not on one's own.
#[component]
pub(super) fn ProfileActions(player: i64, name: String) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let blocks = expect_context::<Blocks>();
    let name = StoredValue::new(name);
    let reporting = RwSignal::new(false);
    let confirming = RwSignal::new(false);
    let mine = move || {
        auth.user
            .with(|u| u.as_ref().is_some_and(|u| u.id == player))
    };
    view! {
        <Show when=move || !mine()>
            <div class="flex flex-wrap items-center justify-end gap-1" data-profile-actions="">
                <Button
                    variant=ButtonVariant::Ghost
                    size=ButtonSize::Sm
                    class="text-muted-foreground"
                    attr:aria-haspopup="dialog"
                    on:click=move |_| reporting.set(true)
                >
                    {t!(i18n, ranks.player.report.button)}
                </Button>
                {move || {
                    if blocks.is(player) {
                        view! {
                            <Button
                                variant=ButtonVariant::Outline
                                size=ButtonSize::Sm
                                on:click=move |_| blocks.set(player, name.get_value(), false)
                            >
                                {t!(i18n, ranks.player.block.unblock)}
                            </Button>
                        }
                            .into_any()
                    } else {
                        view! {
                            <Button
                                variant=ButtonVariant::Ghost
                                size=ButtonSize::Sm
                                class="text-muted-foreground"
                                attr:aria-haspopup="dialog"
                                on:click=move |_| confirming.set(true)
                            >
                                {t!(i18n, ranks.player.block.button)}
                            </Button>
                        }
                            .into_any()
                    }
                }}
            </div>
            <Show when=move || reporting.get()>
                <ReportDialog player name open=reporting />
            </Show>
            <Show when=move || confirming.get()>
                <BlockDialog player name open=confirming />
            </Show>
        </Show>
    }
}
