//! The row of game actions under the practice board: undo and redo, check and hint, and the
//! rarer ones (restart, a new puzzle) behind a "more" button, so the row stays one line on phones.

use leptos::prelude::*;

use crate::components::layout::settings::use_dismiss;
use crate::components::ui::button::{Button, ButtonVariant};
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::difficulty_name;
use crate::state::{self, Game, Settings, Status};

use super::new_game::new_random;

/// Lucide's "undo-2"; "redo-2" is the same arrow mirrored.
const UNDO: &str = "M9 14 4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5 5.5 5.5 0 0 1-5.5 5.5H11";
const REDO: &str = "m15 14 5-5-5-5M20 9H9.5A5.5 5.5 0 0 0 4 14.5 5.5 5.5 0 0 0 9.5 20H13";

/// Undo and redo are arrows on phones, with their names next to them from `sm` on. Check and
/// hint keep their names everywhere: below 360 px they get smaller type, so the longest ones
/// (Russian, Ukrainian, Polish) still fit next to three square buttons.
#[component]
pub(super) fn GameActions(
    #[prop(into)] busy: Signal<bool>,
    #[prop(into)] not_playing: Signal<bool>,
) -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();
    let arrow = |path: &'static str| {
        view! {
            <svg viewBox="0 0 24 24" class="size-5 rtl:-scale-x-100" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path d=path />
            </svg>
        }
    };
    let square = "h-11 min-w-11 px-0 sm:px-3";
    let named =
        "h-11 min-w-0 flex-auto px-2 max-[359px]:px-1 max-[359px]:text-xs sm:flex-none sm:px-4";

    view! {
        <div class="flex items-center gap-1.5 max-[359px]:gap-1 sm:gap-2">
            <Button
                variant=ButtonVariant::Outline
                class=square
                attr:aria-label=move || t_string!(i18n, common.game.undo)
                attr:data-tooltip=move || format!("{} · Ctrl Z", t_string!(i18n, common.game.undo))
                attr:disabled=move || not_playing.get() || game.undo_stack.with(Vec::is_empty)
                on:click=move |_| game.undo()
            >
                {arrow(UNDO)}
                <span class="hidden sm:inline">{t!(i18n, common.game.undo)}</span>
            </Button>
            <Button
                variant=ButtonVariant::Outline
                class=square
                attr:aria-label=move || t_string!(i18n, common.game.redo)
                attr:data-tooltip=move || format!("{} · Ctrl Y", t_string!(i18n, common.game.redo))
                attr:disabled=move || not_playing.get() || game.redo_stack.with(Vec::is_empty)
                on:click=move |_| game.redo()
            >
                {arrow(REDO)}
                <span class="hidden sm:inline">{t!(i18n, common.game.redo)}</span>
            </Button>
            <span class="hidden flex-1 sm:block"></span>
            <Button
                variant=ButtonVariant::Secondary
                class=named
                attr:disabled=move || not_playing.get()
                on:click=move |_| state::check(game, settings)
            >
                {t!(i18n, common.game.check)} <span class="hidden sm:inline">" 確認"</span>
            </Button>
            <Button
                variant=ButtonVariant::Secondary
                class=named
                attr:disabled=move || not_playing.get()
                on:click=move |_| state::hint(game, settings)
            >
                {t!(i18n, common.game.hint)} <span class="hidden sm:inline">" 助言"</span>
            </Button>
            <MoreActions busy=busy />
        </div>
    }
}

/// The "more" button and what it opens: restart, and a new puzzle of the difficulty picked in
/// the card (which on phones is further down, below the board). A disclosure, not a menu: the
/// button says whether it is open, Tab walks on into the buttons, Escape or a click elsewhere
/// closes it.
#[component]
fn MoreActions(#[prop(into)] busy: Signal<bool>) -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();
    let open = RwSignal::new(false);
    let root = NodeRef::<leptos::html::Div>::new();
    let button = NodeRef::<leptos::html::Button>::new();
    // In an effect: in the browser only, server rendering has no window to listen on
    Effect::new(move |_| use_dismiss(open, root, button));
    let item = "flex min-h-11 w-full items-center gap-3 rounded-lg px-3 py-2 text-start text-sm font-medium hover:bg-accent focus-visible:outline-2 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-50";

    view! {
        <div node_ref=root class="relative shrink-0">
            <button
                node_ref=button
                type="button"
                class="grid size-11 place-items-center rounded-md border bg-background shadow-xs transition-all hover:bg-accent focus-visible:ring-[3px] focus-visible:ring-ring/50 dark:border-input dark:bg-input/30"
                class:ring-2=move || open.get()
                class:ring-primary=move || open.get()
                aria-expanded=move || open.get().to_string()
                aria-controls="practice-more"
                aria-label=move || t_string!(i18n, practice.more)
                data-tooltip=move || t_string!(i18n, practice.more)
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <svg viewBox="0 0 24 24" class="size-5" fill="currentColor" aria-hidden="true">
                    <circle cx="5" cy="12" r="1.75" />
                    <circle cx="12" cy="12" r="1.75" />
                    <circle cx="19" cy="12" r="1.75" />
                </svg>
            </button>
            // Above the button: on phones the tab bar is right below the row
            <div
                id="practice-more"
                class="absolute end-0 bottom-full z-50 mb-2 w-60 max-w-[calc(100vw-2rem)] flex-col gap-1 rounded-xl border bg-card p-2 shadow-2xl animate-in fade-in zoom-in-95"
                class:flex=move || open.get()
                class:hidden=move || !open.get()
            >
                <p class="px-3 pb-1 pt-1 font-pixel text-xs text-muted-foreground" aria-hidden="true">"他 · " {t!(i18n, practice.more)}</p>
                <button
                    type="button"
                    class=item
                    disabled=move || game.loading.get()
                    on:click=move |_| {
                        open.set(false);
                        new_random(&game, settings.difficulty.get_untracked());
                    }
                >
                    <span class="w-6 shrink-0 text-center font-pixel text-primary" aria-hidden="true">"新"</span>
                    <span class="flex min-w-0 flex-col">
                        {t!(i18n, practice.new_puzzle)}
                        <span class="text-xs font-normal text-muted-foreground">
                            {move || {
                                let d = settings.difficulty.get();
                                let (w, h) = d.size();
                                format!("{} {} · {w}×{h}", d.kanji(), difficulty_name(i18n.get_locale(), d))
                            }}
                        </span>
                    </span>
                </button>
                <button
                    type="button"
                    class=item
                    disabled=move || busy.get() || game.status.get() == Status::Empty
                    on:click=move |_| {
                        open.set(false);
                        game.restart();
                    }
                >
                    <span class="w-6 shrink-0 text-center font-pixel text-primary" aria-hidden="true">"初"</span>
                    {t!(i18n, common.game.restart)}
                </button>
            </div>
        </div>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::i18n::{Locale, td_string};

    /// The opening tag of the first element with `attribute`.
    fn tag_with<'a>(html: &'a str, attribute: &str) -> &'a str {
        let at = html.find(attribute).unwrap();
        let start = html[..at].rfind('<').unwrap();
        &html[start..at + html[at..].find('>').unwrap()]
    }

    #[tokio::test]
    async fn icon_buttons_are_named_and_the_more_panel_is_a_closed_disclosure() {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::ar);
            provide_context(Game::new("test.practice.actions"));
            provide_context(Settings::new());
            let html = view! { <GameActions busy=false not_playing=false /> }.to_html();
            for name in [
                td_string!(Locale::ar, common.game.undo),
                td_string!(Locale::ar, common.game.redo),
                td_string!(Locale::ar, practice.more),
            ] {
                let button = tag_with(&html, &format!("aria-label=\"{name}\""));
                assert!(button.starts_with("<button"), "{button}");
                assert!(button.contains("data-tooltip=\""), "{button}");
                assert!(!button.contains(" title="), "{button}");
            }
            let more = tag_with(&html, "aria-expanded");
            assert!(more.contains("aria-expanded=\"false\""), "{more}");
            assert!(more.contains("aria-controls=\"practice-more\""), "{more}");
            let panel = tag_with(&html, "id=\"practice-more\"");
            assert!(panel.contains("hidden"), "{panel}");
            // Restart and a new puzzle are in the panel, after the button
            let after = &html[html.find("id=\"practice-more\"").unwrap()..];
            assert!(after.contains(td_string!(Locale::ar, common.game.restart)));
            assert!(after.contains(td_string!(Locale::ar, practice.new_puzzle)));
            assert!(html.contains(td_string!(Locale::ar, common.game.check)));
            assert!(html.contains(td_string!(Locale::ar, common.game.hint)));
        });
    }
}
