//! Challenging a group (挑戦) with the puzzle in the workshop.
//!
//! Once the solver proved it has exactly one solution, it can go to one of the player's groups as
//! a group puzzle (課題), with an optional title. The server proves it again before it takes it.

use common::SolveOutcome;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_query_map};

use crate::api::{my_groups, post_group_puzzle};
use crate::components::layout::use_localized;
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::input::Input;
use crate::components::ui::label::Label;
use crate::components::ui::select::SelectNative;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{error_text, prefix};
use crate::model::{GroupSummary, MAX_SIDE, TITLE_MAX, check_size};
use crate::pages::kadai::puzzle_path;
use crate::state::{Auth, Editor, ToastKind, toast};

/// Where the puzzle in the workshop stands, for posting it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    Empty,
    TooBig,
    /// The solver hasn't said yet whether it's unique.
    Unchecked,
    NotUnique,
    Unsolvable,
    Ready,
}

/// What the workshop's last solver run says about posting its puzzle.
pub fn readiness(editor: &Editor) -> Readiness {
    let size = editor.puzzle.with(check_size);
    if editor.puzzle.with(|p| p.islands.is_empty()) {
        return Readiness::Empty;
    }
    if size == Err(crate::model::ApiError::PuzzleTooBig) {
        return Readiness::TooBig;
    }
    editor
        .report
        .with(|r| match r.as_ref().map(|r| &r.outcome) {
            Some(SolveOutcome::Solved {
                unique: Some(true), ..
            }) if size.is_ok() => Readiness::Ready,
            Some(SolveOutcome::Solved {
                unique: Some(false),
                ..
            }) => Readiness::NotUnique,
            Some(SolveOutcome::Unsatisfiable) => Readiness::Unsolvable,
            _ => Readiness::Unchecked,
        })
}

#[component]
pub fn ChallengeCard(
    /// Runs the uniqueness check of the solver panel.
    on_check: Callback<()>,
    #[prop(into)] busy: Signal<bool>,
) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let editor = expect_context::<Editor>();
    let query = use_query_map();
    let navigate = use_navigate();
    let groups_path = use_localized("/groups");
    let groups = RwSignal::new(None::<Vec<GroupSummary>>);
    let chosen = RwSignal::new(String::new());
    let title = RwSignal::new(String::new());
    let posting = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        let _ = auth.user.with(|u| u.as_ref().map(|u| u.id));
        if !auth.loaded.get() {
            return;
        }
        spawn_local(async move {
            if let Ok(list) = my_groups().await {
                // The group the player came from (`?group=`), or the first
                let wanted = query.with_untracked(|q| q.get("group"));
                let pick = wanted
                    .filter(|w| list.iter().any(|g| g.id.to_string() == *w))
                    .or_else(|| list.first().map(|g| g.id.to_string()))
                    .unwrap_or_default();
                chosen.try_set(pick);
                groups.try_set(Some(list));
            }
        });
    });

    let state = Memo::new(move |_| readiness(&editor));
    // A new puzzle, a new chance
    Effect::new(move |_| {
        editor.epoch.track();
        error.set(None);
    });

    let post = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let Ok(group) = chosen.get_untracked().parse::<i64>() else {
            return;
        };
        if posting.get_untracked() || state.get_untracked() != Readiness::Ready {
            return;
        }
        posting.set(true);
        error.set(None);
        let puzzle = editor.puzzle.get_untracked();
        let name = groups.with_untracked(|g| {
            g.as_ref()
                .and_then(|g| g.iter().find(|g| g.id == group))
                .map(|g| g.name.clone())
                .unwrap_or_default()
        });
        let navigate = navigate.clone();
        spawn_local(async move {
            match post_group_puzzle(group, puzzle, title.get_untracked()).await {
                Ok(id) => {
                    title.try_set(String::new());
                    toast(ToastKind::Success, move |l| {
                        td_string!(l, kadai.challenge.posted, group = name.clone())
                    });
                    let path = format!(
                        "{}{}",
                        prefix(i18n.get_locale_untracked()),
                        puzzle_path(group, id)
                    );
                    navigate(&path, NavigateOptions::default());
                }
                Err(e) => {
                    error.try_set(Some(error_text(i18n.get_locale_untracked(), &e)));
                }
            }
            posting.try_set(false);
        });
    };

    let note = move |text: String| {
        view! { <p class="text-sm text-muted-foreground" data-testid="challenge-note">{text}</p> }
    };

    view! {
        <Card class="gap-4" attr:data-testid="challenge-card">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, kadai.challenge.title)} <span class="font-pixel text-sm text-primary">"挑戦"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, kadai.challenge.text)}</CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                {move || match groups.get() {
                    None => note(t_string!(i18n, common.game.loading).to_string()).into_any(),
                    Some(list) if list.is_empty() => view! {
                        <div class="flex flex-col gap-1">
                            {note(t_string!(i18n, kadai.challenge.no_group).to_string())}
                            <A href=groups_path attr:class="w-fit text-sm font-medium text-primary underline-offset-4 hover:underline">
                                {t!(i18n, kadai.challenge.to_groups)} " →"
                            </A>
                        </div>
                    }
                        .into_any(),
                    Some(list) => match state.get() {
                        Readiness::Empty => note(t_string!(i18n, kadai.challenge.empty).to_string()).into_any(),
                        Readiness::TooBig => note(t_string!(i18n, kadai.challenge.too_big, max = MAX_SIDE)).into_any(),
                        Readiness::NotUnique => note(t_string!(i18n, kadai.challenge.not_unique).to_string()).into_any(),
                        Readiness::Unsolvable => note(t_string!(i18n, kadai.challenge.unsolvable).to_string()).into_any(),
                        Readiness::Unchecked => view! {
                            <div class="flex flex-col gap-2">
                                {note(t_string!(i18n, kadai.challenge.unchecked).to_string())}
                                <Button
                                    variant=ButtonVariant::Outline
                                    class="w-fit"
                                    attr:disabled=move || busy.get()
                                    on:click=move |_| on_check.run(())
                                >
                                    {t!(i18n, kadai.challenge.check)}
                                </Button>
                            </div>
                        }
                            .into_any(),
                        Readiness::Ready => view! {
                            <form class="flex flex-col gap-3" on:submit=post.clone()>
                                <p class="flex items-center gap-2 text-sm font-medium text-success-dark">
                                    <span class="font-pixel" aria-hidden="true">"唯"</span>
                                    {t!(i18n, kadai.challenge.ready)}
                                </p>
                                <div class="flex flex-col gap-2">
                                    <Label html_for="challenge-group">{t!(i18n, kadai.challenge.group)}</Label>
                                    <SelectNative
                                        id="challenge-group"
                                        value=chosen
                                        on_change=Callback::new(move |v| chosen.set(v))
                                    >
                                        {list
                                            .iter()
                                            .map(|g| view! { <option value=g.id.to_string()>{g.name.clone()}</option> })
                                            .collect_view()}
                                    </SelectNative>
                                </div>
                                <div class="flex flex-col gap-2">
                                    <Label html_for="challenge-title">{t!(i18n, kadai.challenge.name)}</Label>
                                    <Input
                                        id="challenge-title"
                                        bind_value=title
                                        autocomplete="off"
                                        placeholder=t_string!(i18n, kadai.challenge.placeholder).to_string()
                                        attr:maxlength=TITLE_MAX
                                    />
                                </div>
                                {move || error.get().map(|e| view! { <p class="text-sm text-destructive" role="alert">{e}</p> })}
                                <Button attr:r#type="submit" class="w-full" attr:disabled=move || posting.get() attr:data-testid="challenge-post">
                                    {move || if posting.get() {
                                        t_string!(i18n, kadai.challenge.posting).to_string()
                                    } else {
                                        format!("{} 挑戦 →", t_string!(i18n, kadai.challenge.post))
                                    }}
                                </Button>
                                <p class="text-xs text-muted-foreground">{t!(i18n, kadai.challenge.rules)}</p>
                            </form>
                        }
                            .into_any(),
                    },
                }}
            </CardContent>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use common::{Pos, Puzzle, SolveReport, SolveStats};

    use super::*;

    fn report(outcome: SolveOutcome) -> SolveReport {
        SolveReport {
            outcome,
            stats: SolveStats::default(),
        }
    }

    /// Only a puzzle the solver proved unique can be posted, and a new edit asks again.
    #[test]
    fn a_puzzle_is_ready_once_proven_unique() {
        Owner::new().with(|| {
            let editor = Editor::default();
            assert_eq!(readiness(&editor), Readiness::Empty);
            editor.edit(|p| {
                p.set(Pos::new(0, 0), 1);
                p.set(Pos::new(2, 0), 1);
            });
            assert_eq!(readiness(&editor), Readiness::Unchecked);
            editor.report.set(Some(report(SolveOutcome::Solved {
                bridges: vec![1],
                unique: Some(true),
            })));
            assert_eq!(readiness(&editor), Readiness::Ready);
            editor.report.set(Some(report(SolveOutcome::Solved {
                bridges: vec![1],
                unique: Some(false),
            })));
            assert_eq!(readiness(&editor), Readiness::NotUnique);
            editor.report.set(Some(report(SolveOutcome::Unsatisfiable)));
            assert_eq!(readiness(&editor), Readiness::Unsolvable);
            // Solved without the uniqueness check, or out of time: not proven
            editor.report.set(Some(report(SolveOutcome::Solved {
                bridges: vec![1],
                unique: None,
            })));
            assert_eq!(readiness(&editor), Readiness::Unchecked);
            editor.report.set(Some(report(SolveOutcome::Timeout)));
            assert_eq!(readiness(&editor), Readiness::Unchecked);
            editor.edit(|p| p.set(Pos::new(2, 0), 2));
            assert_eq!(readiness(&editor), Readiness::Unchecked);
            // Too big, however unique
            editor.edit(|p| {
                *p = Puzzle::empty(MAX_SIDE + 1, 4);
                p.set(Pos::new(0, 0), 1);
                p.set(Pos::new(2, 0), 1);
            });
            editor.report.set(Some(report(SolveOutcome::Solved {
                bridges: vec![1],
                unique: Some(true),
            })));
            assert_eq!(readiness(&editor), Readiness::TooBig);
        });
    }
}
