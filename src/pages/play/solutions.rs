//! The badge next to the title of Play about the number of solutions.

use leptos::prelude::*;

use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::i18n::{t, t_string, use_i18n};

/// Whether the header warns about several solutions. Generated, daily and group puzzles always
/// have a single one, so a badge for it would never change; only a puzzle from the workshop can
/// have more (`Some(false)`) or be unchecked (`None`). With several, Check and hints compare the
/// board with one stored solution, and a bridge of another one counts as wrong.
pub(super) const fn warns(unique: Option<bool>) -> bool {
    matches!(unique, Some(false))
}

#[component]
pub(super) fn SolutionsBadge(unique: RwSignal<Option<bool>>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <Show when=move || warns(unique.get())>
            <Badge
                variant=BadgeVariant::Warning
                attr:tabindex="0"
                attr:data-tooltip=move || t_string!(i18n, common.game.several_solutions_tooltip)
            >
                {t!(i18n, common.game.several_solutions)}
            </Badge>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_several_solutions_get_a_badge() {
        assert!(!warns(Some(true)));
        assert!(!warns(None));
        assert!(warns(Some(false)));
    }

    /// A single solution used to show a "unique solution ✓" badge on every generated puzzle.
    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn the_badge_shows_for_several_solutions_only_with_its_tooltip() {
        use crate::i18n::{Locale, td_string};
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        owner.with(|| {
            crate::app::provide_server_i18n();
            let several = td_string!(Locale::en, common.game.several_solutions);
            let tooltip = td_string!(Locale::en, common.game.several_solutions_tooltip);
            for unique in [Some(true), None] {
                let unique = RwSignal::new(unique);
                let html = view! { <SolutionsBadge unique /> }.to_html();
                assert!(!html.contains("<span"), "{html}");
            }
            let unique = RwSignal::new(Some(false));
            let html = view! { <SolutionsBadge unique /> }.to_html();
            assert!(html.contains(several), "{html}");
            assert!(
                html.contains(&format!("data-tooltip=\"{tooltip}\"")),
                "{html}"
            );
            assert!(html.contains("tabindex=\"0\""), "{html}");
        });
    }
}
