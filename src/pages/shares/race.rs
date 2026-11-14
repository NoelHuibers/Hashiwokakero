//! "Race this with my group" (勝負) on a shared puzzle's page.
//!
//! A race on this puzzle in one of the player's groups (`server::groups::race::open_shared`),
//! opened through the link of the page. Only for players in a group, guests too; who has several
//! picks one.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;

use crate::api::{my_groups, open_shared_race};
use crate::components::ui::button::Button;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::label::Label;
use crate::components::ui::select::SelectNative;
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::prefix;
use crate::model::GroupSummary;
use crate::pages::race::race_path;
use crate::state::{Auth, error_toast};

/// The group a race opens in: the one picked, or the first.
fn chosen_group(groups: &[GroupSummary], picked: Option<i64>) -> Option<i64> {
    picked
        .filter(|id| groups.iter().any(|g| g.id == *id))
        .or_else(|| groups.first().map(|g| g.id))
}

/// The card, once the player's groups are known: nothing for a player without one.
#[component]
pub fn RaceWithGroup(code: Signal<String>) -> impl IntoView {
    let i18n = use_i18n();
    let auth = use_context::<Auth>();
    let navigate = use_navigate();
    let groups = RwSignal::new(Vec::<GroupSummary>::new());
    let picked = RwSignal::new(None::<i64>);
    let busy = RwSignal::new(false);
    let user = Memo::new(move |_| auth.and_then(|a| a.user.with(|u| u.as_ref().map(|u| u.id))));
    // Browser only: the groups of whoever is signed in (a guest made by a solve here, too)
    Effect::new(move |_| {
        if user.get().is_none() {
            groups.set(Vec::new());
            return;
        }
        spawn_local(async move {
            // Without them the card stays away, like for a player without a group
            if let Ok(list) = my_groups().await {
                groups.try_set(list);
            }
        });
    });
    let open = Callback::new(move |()| {
        let Some(group) = groups.with_untracked(|g| chosen_group(g, picked.get_untracked())) else {
            return;
        };
        busy.set(true);
        let navigate = navigate.clone();
        let code = code.get_untracked();
        spawn_local(async move {
            match open_shared_race(group, code).await {
                Ok(id) => navigate(
                    &format!(
                        "{}{}",
                        prefix(i18n.get_locale_untracked()),
                        race_path(group, id)
                    ),
                    NavigateOptions::default(),
                ),
                Err(e) => error_toast(e),
            }
            busy.try_set(false);
        });
    });
    let several = Memo::new(move |_| groups.with(|g| g.len() > 1));
    view! {
        <Show when=move || groups.with(|g| !g.is_empty())>
            <Card class="gap-3" attr:data-testid="shared-race">
                <CardHeader>
                    <CardTitle class="flex items-baseline gap-2 text-lg">
                        {t!(i18n, shared.race.title)} <span class="font-pixel whitespace-nowrap text-sm text-primary">"勝負"</span>
                    </CardTitle>
                    <CardDescription>{t!(i18n, shared.race.text)}</CardDescription>
                </CardHeader>
                <CardContent class="flex flex-col gap-2">
                    <Show when=move || several.get()>
                        <Label html_for="shared-race-group">{t!(i18n, shared.race.group)}</Label>
                        <SelectNative
                            id="shared-race-group"
                            value=Signal::derive(move || {
                                groups
                                    .with(|g| chosen_group(g, picked.get()))
                                    .map_or_default(|id| id.to_string())
                            })
                            on_change=Callback::new(move |v: String| picked.set(v.parse().ok()))
                        >
                            {move || groups
                                .get()
                                .into_iter()
                                .map(|g| view! { <option value=g.id.to_string()>{g.name}</option> })
                                .collect_view()}
                        </SelectNative>
                    </Show>
                    <Button class="h-11 w-full" attr:disabled=move || busy.get() attr:data-testid="shared-race-open" on:click=move |_| open.run(())>
                        {move || if busy.get() {
                            t_string!(i18n, shared.race.opening).to_string()
                        } else {
                            format!("{} 勝負", t_string!(i18n, shared.race.open))
                        }}
                    </Button>
                </CardContent>
            </Card>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: i64) -> GroupSummary {
        GroupSummary {
            id,
            name: format!("g{id}"),
            members: 2,
            done_today: 0,
            owner: false,
            colours: vec![],
        }
    }

    #[test]
    fn the_race_opens_in_the_group_picked_or_the_first() {
        let groups = [group(3), group(5)];
        assert_eq!(chosen_group(&groups, None), Some(3));
        assert_eq!(chosen_group(&groups, Some(5)), Some(5));
        // A group the player isn't in (any more): the first
        assert_eq!(chosen_group(&groups, Some(9)), Some(3));
        assert_eq!(chosen_group(&[], Some(5)), None);
    }
}
