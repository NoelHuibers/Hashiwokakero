//! Sharing the puzzle of the workshop as a link (共有), once the solver proved it has exactly one
//! solution: anyone with the link plays it (`/p/<code>`, `server::shares`). The server proves it
//! again before it makes the link.

use common::SolveOutcome;
use leptos::prelude::*;

use crate::components::share_puzzle::{SharePuzzle, share_text};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, td_string, use_i18n};
use crate::state::Editor;

/// Whether the workshop's last solver run proved its puzzle unique.
pub fn proven_unique(editor: &Editor) -> bool {
    !editor.puzzle.with(|p| p.islands.is_empty())
        && editor.report.with(|r| {
            matches!(
                r.as_ref().map(|r| &r.outcome),
                Some(SolveOutcome::Solved {
                    unique: Some(true),
                    ..
                })
            )
        })
}

#[component]
pub fn ShareCard() -> impl IntoView {
    let i18n = use_i18n();
    let editor = expect_context::<Editor>();
    let proven = Memo::new(move |_| proven_unique(&editor));
    let text = move |locale, url: &str| {
        share_text(
            locale,
            td_string!(locale, common.source.custom),
            None,
            url,
        )
    };
    view! {
        <Card class="gap-4" attr:data-testid="share-card">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, shared.create.title)} <span class="font-pixel text-sm text-primary">"共有"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, shared.create.text)}</CardDescription>
            </CardHeader>
            <CardContent>
                // A new puzzle, a new link
                {move || {
                    editor.epoch.track();
                    if proven.get() {
                        view! {
                            <SharePuzzle
                                request=move || (editor.puzzle.get_untracked(), None)
                                text
                                outline=true
                            />
                        }
                            .into_any()
                    } else {
                        view! {
                            <p class="text-sm text-muted-foreground">{t!(i18n, shared.create.unchecked)}</p>
                        }
                            .into_any()
                    }
                }}
            </CardContent>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use common::{Pos, SolveReport, SolveStats};

    use super::*;

    #[test]
    fn only_a_puzzle_proven_unique_is_shared() {
        Owner::new().with(|| {
            let editor = Editor::default();
            assert!(!proven_unique(&editor));
            editor.edit(|p| {
                p.set(Pos::new(0, 0), 1);
                p.set(Pos::new(2, 0), 1);
            });
            let report = |unique| {
                Some(SolveReport {
                    outcome: SolveOutcome::Solved {
                        bridges: vec![1],
                        unique,
                    },
                    stats: SolveStats::default(),
                })
            };
            editor.report.set(report(Some(true)));
            assert!(proven_unique(&editor));
            editor.report.set(report(Some(false)));
            assert!(!proven_unique(&editor));
            editor.report.set(report(None));
            assert!(!proven_unique(&editor));
            editor.report.set(report(Some(true)));
            editor.edit(|p| p.set(Pos::new(2, 0), 2));
            assert!(!proven_unique(&editor));
        });
    }
}
