//! Names, kanji and badges shared by the Dojo's pages: families, states and scores.

use dojo::{Family, Score};
use leptos::prelude::*;

use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::i18n::{Locale, t_string, td_string, use_i18n};
use crate::model::DojoState;

pub fn family_name(locale: Locale, family: Family) -> String {
    match family {
        Family::Hashi => td_string!(locale, dojo.family.hashi),
        Family::HashiParity => td_string!(locale, dojo.family.hashi_parity),
        Family::Random3Sat => td_string!(locale, dojo.family.random_3sat),
        Family::Pigeonhole => td_string!(locale, dojo.family.pigeonhole),
        Family::Queens => td_string!(locale, dojo.family.queens),
        Family::Factoring => td_string!(locale, dojo.family.factoring),
        Family::Miter => td_string!(locale, dojo.family.miter),
        Family::Coloring => td_string!(locale, dojo.family.coloring),
        Family::Tseitin => td_string!(locale, dojo.family.tseitin),
        Family::Quasigroup => td_string!(locale, dojo.family.quasigroup),
        Family::Hamiltonian => td_string!(locale, dojo.family.hamiltonian),
        Family::Mystery => td_string!(locale, dojo.family.mystery),
        Family::Community => td_string!(locale, dojo.family.community),
    }
    .to_string()
}

pub(super) fn family_text(locale: Locale, family: Family) -> String {
    match family {
        Family::Hashi => td_string!(locale, dojo.family.hashi_text),
        Family::HashiParity => td_string!(locale, dojo.family.hashi_parity_text),
        Family::Random3Sat => td_string!(locale, dojo.family.random_3sat_text),
        Family::Pigeonhole => td_string!(locale, dojo.family.pigeonhole_text),
        Family::Queens => td_string!(locale, dojo.family.queens_text),
        Family::Factoring => td_string!(locale, dojo.family.factoring_text),
        Family::Miter => td_string!(locale, dojo.family.miter_text),
        Family::Coloring => td_string!(locale, dojo.family.coloring_text),
        Family::Tseitin => td_string!(locale, dojo.family.tseitin_text),
        Family::Quasigroup => td_string!(locale, dojo.family.quasigroup_text),
        Family::Hamiltonian => td_string!(locale, dojo.family.hamiltonian_text),
        Family::Mystery => td_string!(locale, dojo.family.mystery_text),
        Family::Community => td_string!(locale, dojo.family.community_text),
    }
    .to_string()
}

pub(super) fn family_kanji(family: Family) -> &'static str {
    match family {
        Family::Hashi => "橋",
        Family::HashiParity => "偶",
        Family::Random3Sat => "乱",
        Family::Pigeonhole => "鳩",
        Family::Queens => "女",
        Family::Factoring => "因",
        Family::Miter => "回",
        Family::Coloring => "色",
        Family::Tseitin => "奇",
        Family::Quasigroup => "方",
        Family::Hamiltonian => "巡",
        Family::Mystery => "謎",
        Family::Community => "衆",
    }
}

fn state_name(locale: Locale, state: DojoState) -> String {
    match state {
        DojoState::Queued => td_string!(locale, dojo.state.queued),
        DojoState::Running => td_string!(locale, dojo.state.running),
        DojoState::Done => td_string!(locale, dojo.state.done),
        DojoState::Rejected => td_string!(locale, dojo.state.rejected),
        DojoState::Failed => td_string!(locale, dojo.state.failed),
    }
    .to_string()
}

#[component]
pub(super) fn StateBadge(state: DojoState, #[prop(optional)] disqualified: bool) -> impl IntoView {
    let i18n = use_i18n();
    let variant = match state {
        _ if disqualified => BadgeVariant::Destructive,
        DojoState::Queued => BadgeVariant::Muted,
        DojoState::Running => BadgeVariant::Warning,
        DojoState::Done => BadgeVariant::Success,
        DojoState::Rejected | DojoState::Failed => BadgeVariant::Destructive,
    };
    view! {
        <Badge variant class=if state == DojoState::Running { "animate-pulse" } else { "" }>
            {move || {
                if disqualified {
                    t_string!(i18n, dojo.state.disqualified).to_string()
                } else {
                    state_name(i18n.get_locale(), state)
                }
            }}
        </Badge>
    }
}

pub(super) fn par2(score: &Score) -> String {
    format!("{:.3}", score.par2)
}
