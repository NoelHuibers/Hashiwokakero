//! The players one blocked, to unblock them without looking for their profiles.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, use_i18n};
use crate::lang::prefix;
use crate::state::Blocks;

#[component]
pub(super) fn BlockedPlayers() -> impl IntoView {
    let i18n = use_i18n();
    let blocks = expect_context::<Blocks>();
    let rows = move || {
        blocks
            .list
            .get()
            .into_iter()
            .map(|b| {
                let (id, name) = (b.id, b.name);
                let href = move || format!("{}/player/{id}", prefix(i18n.get_locale()));
                let unblock = {
                    let name = name.clone();
                    move |_| blocks.set(id, name.clone(), false)
                };
                view! {
                    <li class="flex items-center gap-3 py-2" data-blocked-player=id>
                        <A href=href attr:class="min-w-0 flex-1 truncate font-medium underline-offset-4 hover:text-primary hover:underline">
                            {name}
                        </A>
                        {b.guest.then(|| view! { <span class="text-xs text-muted-foreground">{t!(i18n, common.ranking.guest)}</span> })}
                        <Button variant=ButtonVariant::Outline size=ButtonSize::Sm class="shrink-0" on:click=unblock>
                            {t!(i18n, ranks.player.block.unblock)}
                        </Button>
                    </li>
                }
            })
            .collect_view()
    };
    view! {
        <Card class="gap-4 self-start" attr:data-blocked-players="">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, account.blocks.title)} <span class="font-pixel text-sm text-primary">"遮断"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, account.blocks.text)}</CardDescription>
            </CardHeader>
            <CardContent>
                <Show
                    when=move || blocks.list.with(|l| !l.is_empty())
                    fallback=move || view! { <p class="text-sm text-muted-foreground">{t!(i18n, account.blocks.none)}</p> }
                >
                    <ul class="divide-y">{rows}</ul>
                </Show>
            </CardContent>
        </Card>
    }
}
