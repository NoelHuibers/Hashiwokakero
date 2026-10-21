//! Making a group: a name, then straight to the invite step with the link to share.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::create_group;
use crate::components::celebration::Stamp;
use crate::components::ui::button::Button;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::input::Input;
use crate::components::ui::label::Label;
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::{error_text, prefix};
use crate::model::{GROUP_NAME_MAX, GroupPage};
use crate::state::Auth;

use super::archipelago::Archipelago;
use super::invite::InviteLink;

/// The form for a new group. Whoever has no user yet becomes a guest, like with the daily.
#[component]
pub fn CreateCard(created: RwSignal<Option<GroupPage>>) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let name = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let wanted = name.get_untracked();
        spawn_local(async move {
            match create_group(wanted).await {
                Ok(page) => {
                    name.set(String::new());
                    created.set(Some(page));
                    // A new guest, or a new group in the list
                    auth.refresh();
                }
                Err(e) => error.set(Some(error_text(i18n.get_locale_untracked(), &e))),
            }
            busy.set(false);
        });
    };

    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, groups.create.title)} <span class="font-pixel text-sm text-primary">"作る"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, groups.create.text)}</CardDescription>
            </CardHeader>
            <CardContent>
                <form class="flex flex-col gap-3" on:submit=submit>
                    <div class="flex flex-col gap-2">
                        <Label html_for="group-name">{t!(i18n, groups.create.name)}</Label>
                        <Input
                            id="group-name"
                            bind_value=name
                            required=true
                            autocomplete="off"
                            placeholder=t_string!(i18n, groups.create.placeholder).to_string()
                            attr:maxlength=GROUP_NAME_MAX
                        />
                    </div>
                    {move || error.get().map(|e| view! { <p class="text-sm text-destructive">{e}</p> })}
                    <Button attr:r#type="submit" class="w-full" attr:disabled=move || busy.get() || name.with(|n| n.trim().is_empty())>
                        {move || if busy.get() { t_string!(i18n, groups.create.busy) } else { t_string!(i18n, groups.create.button) }}
                    </Button>
                    <p class="text-xs text-muted-foreground">{t!(i18n, groups.create.invite_hint)}</p>
                </form>
            </CardContent>
        </Card>
    }
}

/// The group just made: its first island, and the link to send around.
#[component]
pub fn ReadyCard(page: GroupPage) -> impl IntoView {
    let i18n = use_i18n();
    let id = page.id;
    let href = move || format!("{}/groups/{id}", prefix(i18n.get_locale()));
    let name = page.name.clone();
    let code = page.invite.clone();
    view! {
        <Card class="gap-4 border-primary/50" attr:data-testid="group-ready">
            <CardHeader>
                <div class="flex items-center gap-3">
                    <Stamp kanji="完" class="size-14 shrink-0 text-3xl" />
                    <div class="min-w-0">
                        <CardTitle class="text-xl">{t!(i18n, groups.ready.title)}</CardTitle>
                        <CardDescription class="break-words">{page.name.clone()}</CardDescription>
                    </div>
                </div>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                <Archipelago group=page.id members=page.members.clone() me=page.me />
                <p class="text-sm text-muted-foreground">{t!(i18n, groups.ready.text)}</p>
                <InviteLink name=Signal::stored(name) code=Signal::stored(code) />
                <A
                    href=href
                    attr:class="inline-flex min-h-10 items-center justify-center rounded-md bg-secondary px-4 text-sm font-medium hover:bg-secondary/80"
                >
                    {t!(i18n, groups.ready.open)} " →"
                </A>
            </CardContent>
        </Card>
    }
}
