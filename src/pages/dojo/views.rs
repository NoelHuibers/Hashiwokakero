//! A season's board in three views, sortable, with the cactus plot below.
//!
//! The views: the official ranking by PAR-2, SAT/UNSAT/ALL, and every solver's contribution to
//! the field. The leaderboard and the archive's final boards look the same.

use dojo::metrics::Split;
use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::prefix;
use crate::model::{DojoEntry, DojoPortfolio};

use super::cactus::Cactus;
use super::names::{family_kanji, family_name, par2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum View {
    Ranking,
    Answers,
    Contribution,
}

/// What a view can be sorted by. Every column sorts best first on the first click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Column {
    Rank,
    SatSolved,
    SatPar2,
    UnsatSolved,
    UnsatPar2,
    Unverified,
    Marginal,
    Relative,
    Sota,
    Shapley,
}

impl Column {
    /// The column's value of an entry, larger is better; `None` sorts last.
    fn key(self, e: &DojoEntry) -> Option<f64> {
        let split = |sat: bool| {
            e.breakdown
                .as_ref()
                .and_then(|b| if sat { b.total.sat } else { b.total.unsat })
        };
        let c = e.contribution;
        match self {
            Self::Rank => Some(-f64::from(e.rank)),
            Self::SatSolved => split(true).map(|s| f64::from(s.solved)),
            Self::SatPar2 => split(true).map(|s| -s.par2),
            Self::UnsatSolved => split(false).map(|s| f64::from(s.solved)),
            Self::UnsatPar2 => split(false).map(|s| -s.par2),
            Self::Unverified => Some(-f64::from(e.score.unverified)),
            Self::Marginal => c.map(|c| c.marginal),
            Self::Relative => c.map(|c| c.relative),
            Self::Sota => c.map(|c| c.sota),
            Self::Shapley => c.map(|c| c.shapley),
        }
    }
}

/// The entries sorted by `column`, best first (or worst first when `reversed`), ties by rank.
pub(super) fn sorted(
    mut entries: Vec<DojoEntry>,
    column: Column,
    reversed: bool,
) -> Vec<DojoEntry> {
    entries.sort_by(|a, b| {
        let (ka, kb) = (column.key(a), column.key(b));
        let order = match (ka, kb) {
            (Some(x), Some(y)) => {
                let o = y.total_cmp(&x);
                if reversed { o.reverse() } else { o }
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        order.then(a.rank.cmp(&b.rank))
    });
    entries
}

fn solved_of(split: Option<Split>) -> String {
    split
        .filter(|s| s.total > 0)
        .map_or_else(|| "—".into(), |s| format!("{}/{}", s.solved, s.total))
}

fn par2_of(split: Option<Split>) -> String {
    split
        .filter(|s| s.total > 0)
        .map_or_else(|| "—".into(), |s| format!("{:.3}", s.par2))
}

/// A contribution with three decimals, "—" for none.
fn number(value: Option<f64>, digits: usize) -> String {
    value.map_or_else(|| "—".into(), |v| format!("{v:.digits$}"))
}

/// One small square per family, filled by the share of solved formulas.
#[component]
fn FamilyChips(entry: DojoEntry) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="flex gap-1">
            {entry
                .score
                .families
                .into_iter()
                .map(|f| {
                    let share = if f.total == 0 { 0.0 } else { f64::from(f.solved) / f64::from(f.total) };
                    let title = move || format!("{}: {}/{}", family_name(i18n.get_locale(), f.family), f.solved, f.total);
                    view! {
                        <span
                            class="relative grid size-7 place-items-center overflow-hidden rounded-sm border bg-muted font-pixel text-xs"
                            data-tooltip=title
                        >
                            <span class="absolute inset-x-0 bottom-0 bg-primary/70" style=format!("height: {:.0}%", share * 100.0)></span>
                            <span class="relative">{family_kanji(f.family)}</span>
                        </span>
                    }
                })
                .collect_view()}
        </div>
    }
}

/// The ranked solvers of a season with the views, then its cactus plot.
#[component]
pub(super) fn BoardViews(
    entries: Vec<DojoEntry>,
    portfolio: Option<DojoPortfolio>,
    /// The leaderboard's family squares; the archive leaves them out.
    #[prop(optional)]
    families: bool,
) -> impl IntoView {
    let i18n = use_i18n();
    let view_mode = RwSignal::new(View::Ranking);
    let sort = RwSignal::new((Column::Rank, false));
    // The row under the pointer or focus, emphasised in the plot
    let hovered = RwSignal::new(None::<i64>);
    let entries = StoredValue::new(entries);
    let tab = move |v: View, label: AnyView| {
        view! {
            <button
                type="button"
                role="tab"
                aria-selected=move || (view_mode.get() == v).to_string()
                class=move || {
                    if view_mode.get() == v {
                        "rounded-md bg-background px-3 py-1 text-sm font-medium text-foreground shadow-sm"
                    } else {
                        "rounded-md px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground"
                    }
                }
                on:click=move |_| {
                    view_mode.set(v);
                    sort.set((Column::Rank, false));
                }
            >
                {label}
            </button>
        }
    };
    // A header that sorts by its column
    let header = move |column: Column, label: AnyView, tip: Option<String>| {
        let state = move || {
            let (by, reversed) = sort.get();
            match (by == column, reversed) {
                (false, _) => "none",
                // Best first: descending for solved and contributions, ascending for PAR-2 and rank
                (true, false) => match column {
                    Column::Rank | Column::SatPar2 | Column::UnsatPar2 | Column::Unverified => {
                        "ascending"
                    }
                    _ => "descending",
                },
                (true, true) => match column {
                    Column::Rank | Column::SatPar2 | Column::UnsatPar2 | Column::Unverified => {
                        "descending"
                    }
                    _ => "ascending",
                },
            }
        };
        view! {
            <th class="py-2 pe-3 text-end font-semibold" aria-sort=state>
                <button
                    type="button"
                    class="inline-flex items-center gap-1 uppercase tracking-wider hover:text-foreground"
                    data-tooltip=tip
                    on:click=move |_| sort.update(|(by, reversed)| {
                        if *by == column {
                            *reversed = !*reversed;
                        } else {
                            (*by, *reversed) = (column, false);
                        }
                    })
                >
                    {label}
                    <span aria-hidden="true" class="text-[10px]">
                        {move || match state() {
                            "ascending" => "▲",
                            "descending" => "▼",
                            _ => "",
                        }}
                    </span>
                </button>
            </th>
        }
    };
    let rows = move || {
        let (column, reversed) = sort.get();
        let mode = view_mode.get();
        sorted(entries.get_value(), column, reversed)
            .into_iter()
            .map(|e| {
                let chips = families.then(|| e.clone());
                let id = e.id;
                let href = move || format!("{}/dojo/{id}", prefix(i18n.get_locale()));
                let medal = match e.rank {
                    1 => "金",
                    2 => "銀",
                    3 => "銅",
                    _ => "",
                };
                // An empty name: the author deleted their account
                let name = if e.name.is_empty() {
                    view! { <span class="text-muted-foreground">"—"</span> }.into_any()
                } else {
                    view! {
                        <A href=href attr:class="font-medium hover:text-primary hover:underline">{e.name.clone()}</A>
                    }
                        .into_any()
                };
                let author = if e.author.is_empty() {
                    t_string!(i18n, dojo.seasons.deleted).to_string()
                } else {
                    e.author.clone()
                };
                let b = e.breakdown.clone();
                let c = e.contribution;
                let cells = match mode {
                    View::Ranking => view! {
                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{format!("{}/{}", e.score.solved, e.score.total)}</td>
                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{par2(&e.score)}</td>
                        {chips.map(|entry| view! { <td class="hidden py-2.5 sm:table-cell"><FamilyChips entry /></td> })}
                    }
                        .into_any(),
                    View::Answers => {
                        let sat = b.as_ref().and_then(|b| b.total.sat);
                        let unsat = b.as_ref().and_then(|b| b.total.unsat);
                        view! {
                            <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{solved_of(sat)}</td>
                            <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{par2_of(sat)}</td>
                            <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{solved_of(unsat)}</td>
                            <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{par2_of(unsat)}</td>
                            <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{format!("{}/{}", e.score.solved, e.score.total)}</td>
                            <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{par2(&e.score)}</td>
                            <td class="py-2.5 text-end font-pixel tabular-nums">{e.score.unverified}</td>
                        }
                            .into_any()
                    }
                    View::Contribution => view! {
                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{par2(&e.score)}</td>
                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{number(c.map(|c| c.marginal), 3)}</td>
                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">
                            {c.map_or_else(|| "—".to_string(), |c| format!("{:.1}%", c.relative * 100.0))}
                        </td>
                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{number(c.map(|c| c.sota), 2)}</td>
                        <td class="py-2.5 text-end font-pixel tabular-nums">{number(c.map(|c| c.shapley), 3)}</td>
                    }
                        .into_any(),
                };
                view! {
                    <tr
                        class="border-b last:border-0 hover:bg-muted/50"
                        on:pointerenter=move |_| hovered.set(Some(id))
                        on:pointerleave=move |_| hovered.set(None)
                        on:focusin=move |_| hovered.set(Some(id))
                        on:focusout=move |_| hovered.set(None)
                    >
                        <td class="py-2.5 pe-2 font-pixel tabular-nums">
                            {if medal.is_empty() { e.rank.to_string() } else { medal.to_string() }}
                        </td>
                        <td class="py-2.5 pe-3">
                            {name}
                            {e
                                .house
                                .then(|| {
                                    view! {
                                        <Badge variant=BadgeVariant::Secondary size=BadgeSize::Sm class="ms-2">
                                            {t!(i18n, dojo.board.house)}
                                        </Badge>
                                    }
                                })}
                            <span class="block text-xs text-muted-foreground">{author}</span>
                        </td>
                        {cells}
                    </tr>
                }
            })
            .collect_view()
    };
    let head = move || {
        match view_mode.get() {
        View::Ranking => view! {
            <th class="py-2 pe-3 text-end font-semibold">{t!(i18n, dojo.board.solved)}</th>
            <th class="py-2 pe-3 text-end font-semibold">{t!(i18n, dojo.board.par2)}</th>
            {families.then(|| view! { <th class="hidden py-2 text-start font-semibold sm:table-cell">{t!(i18n, dojo.board.families)}</th> })}
        }
            .into_any(),
        View::Answers => view! {
            {header(Column::SatSolved, view! { "SAT" }.into_any(), None)}
            {header(Column::SatPar2, view! { "SAT " {t!(i18n, dojo.board.par2)} }.into_any(), None)}
            {header(Column::UnsatSolved, view! { "UNSAT" }.into_any(), None)}
            {header(Column::UnsatPar2, view! { "UNSAT " {t!(i18n, dojo.board.par2)} }.into_any(), None)}
            <th class="py-2 pe-3 text-end font-semibold">{t!(i18n, dojo.board.all)}</th>
            {header(Column::Rank, view! { {t!(i18n, dojo.board.all)} " " {t!(i18n, dojo.board.par2)} }.into_any(), None)}
            {header(Column::Unverified, t!(i18n, dojo.detail.unverified_col).into_any(), None)}
        }
            .into_any(),
        View::Contribution => view! {
            {header(Column::Rank, t!(i18n, dojo.board.par2).into_any(), None)}
            {header(Column::Marginal, t!(i18n, dojo.board.marginal).into_any(), Some(t_string!(i18n, dojo.board.marginal_tip).to_string()))}
            {header(Column::Relative, t!(i18n, dojo.board.relative).into_any(), Some(t_string!(i18n, dojo.board.relative_tip).to_string()))}
            {header(Column::Sota, t!(i18n, dojo.board.sota).into_any(), Some(t_string!(i18n, dojo.board.sota_tip).to_string()))}
            {header(Column::Shapley, t!(i18n, dojo.board.shapley).into_any(), Some(t_string!(i18n, dojo.board.shapley_tip).to_string()))}
        }
            .into_any(),
    }
    };
    let hint = move || match view_mode.get() {
        View::Ranking => t!(i18n, dojo.board.par2_hint).into_any(),
        View::Answers => t!(i18n, dojo.board.answers_hint).into_any(),
        View::Contribution => t!(i18n, dojo.board.contribution_hint).into_any(),
    };
    let vbs = portfolio.as_ref().map(|p| {
        let line = td_string!(
            i18n.get_locale_untracked(),
            dojo.board.vbs_line,
            count = p.entries,
            solved = p.vbs.solved,
            total = p.vbs.total,
            par2 = format!("{:.3}", p.vbs.par2)
        );
        view! { <p class="text-sm text-muted-foreground" data-testid="vbs">{line}</p> }
    });
    let cactus = portfolio.map(|p| view! { <Cactus cactus=p.cactus highlight=hovered /> });

    view! {
        <div class="flex flex-col gap-4">
            <div role="tablist" aria-label=move || t_string!(i18n, dojo.board.views) class="inline-flex w-fit flex-wrap gap-1 rounded-lg bg-muted p-1">
                {tab(View::Ranking, t!(i18n, dojo.board.view_ranking).into_any())}
                {tab(View::Answers, view! { "SAT / UNSAT" }.into_any())}
                {tab(View::Contribution, t!(i18n, dojo.board.view_contribution).into_any())}
            </div>
            <p class="text-sm text-muted-foreground">{hint}</p>
            <div class="overflow-x-auto" role="tabpanel">
                <table class="w-full text-sm">
                    <thead>
                        <tr class="border-b text-start text-xs uppercase tracking-wider text-muted-foreground">
                            <th class="py-2 pe-2 text-start font-semibold">"#"</th>
                            <th class="py-2 pe-3 text-start font-semibold">{t!(i18n, dojo.board.solver)}</th>
                            {head}
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
            {vbs}
            {cactus}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use dojo::metrics::{Breakdown, Contribution, EntryBreakdown};

    use super::*;

    fn entry(rank: u32, sat: Option<(u32, f64)>, shapley: Option<f64>) -> DojoEntry {
        let split = |(solved, par2): (u32, f64)| Split {
            solved,
            total: 10,
            par2,
        };
        DojoEntry {
            rank,
            id: i64::from(rank),
            name: format!("#{rank}"),
            author: String::new(),
            house: false,
            score: serde_json::from_str(
                r#"{"par2":1.0,"solved":1,"total":1,"wrong":0,"budget":10,"fuel":0.5,"families":[]}"#,
            )
            .unwrap(),
            created_at: 0,
            breakdown: sat.map(|s| EntryBreakdown {
                total: Breakdown {
                    all: split(s),
                    sat: Some(split(s)),
                    unsat: None,
                    unverified: 0,
                },
                held_out: 0,
                families: Vec::new(),
            }),
            contribution: shapley.map(|shapley| Contribution {
                marginal: 0.0,
                log_ratio: 0.0,
                relative: 0.0,
                sota: 0.0,
                shapley,
            }),
        }
    }

    fn ranks(entries: &[DojoEntry]) -> Vec<u32> {
        entries.iter().map(|e| e.rank).collect()
    }

    #[test]
    fn sorting_puts_the_best_first() {
        let board = vec![
            entry(1, Some((5, 2.0)), Some(0.5)),
            entry(2, Some((7, 3.0)), Some(1.5)),
            entry(3, None, None),
            entry(4, Some((7, 1.0)), Some(1.5)),
        ];
        assert_eq!(
            ranks(&sorted(board.clone(), Column::Rank, false)),
            [1, 2, 3, 4]
        );
        assert_eq!(
            ranks(&sorted(board.clone(), Column::Rank, true)),
            [4, 3, 2, 1]
        );
        // More solved first, ties by rank, entries without a value last either way
        assert_eq!(
            ranks(&sorted(board.clone(), Column::SatSolved, false)),
            [2, 4, 1, 3]
        );
        assert_eq!(
            ranks(&sorted(board.clone(), Column::SatSolved, true)),
            [1, 2, 4, 3]
        );
        // Lower PAR-2 first
        assert_eq!(
            ranks(&sorted(board.clone(), Column::SatPar2, false)),
            [4, 1, 2, 3]
        );
        assert_eq!(
            ranks(&sorted(board.clone(), Column::UnsatSolved, false)),
            [1, 2, 3, 4]
        );
        assert_eq!(
            ranks(&sorted(board.clone(), Column::Shapley, false)),
            [2, 4, 1, 3]
        );
        assert_eq!(ranks(&sorted(board, Column::Marginal, false)), [1, 2, 4, 3]);
        assert_eq!(
            (solved_of(None), par2_of(None)),
            ("—".to_string(), "—".to_string())
        );
        assert_eq!(number(Some(1.23456), 3), "1.235");
    }

    /// The board rendered on the server: the tabs, a row per entry (a deleted author's without
    /// a link), the VBS line and the plot.
    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn renders_the_views() {
        use crate::i18n::{Locale, use_i18n};
        use crate::model::{DojoCactus, DojoCurve};
        _ = any_spawner::Executor::init_tokio();
        let mut deleted = entry(2, Some((7, 3.0)), Some(1.5));
        deleted.name = String::new();
        let board = vec![entry(1, Some((5, 2.0)), Some(0.5)), deleted];
        let portfolio = DojoPortfolio {
            entries: 2,
            vbs: Split {
                solved: 8,
                total: 10,
                par2: 1.5,
            },
            cactus: DojoCactus {
                budget: 7_000_000_000,
                formulas: 10,
                vbs: vec![1_000, 2_000],
                curves: vec![DojoCurve {
                    id: 1,
                    name: "#1".into(),
                    house: false,
                    fuel: vec![1_000],
                }],
            },
        };
        let owner = Owner::new_root(None);
        let html = owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::en);
            // Links need a router, on the server with the request's URL
            provide_context(leptos_router::location::RequestUrl::new("/dojo"));
            view! {
                <leptos_router::components::Router>
                    <BoardViews entries=board portfolio=Some(portfolio) families=true />
                </leptos_router::components::Router>
            }
            .to_html()
        });
        assert_eq!(html.matches("role=\"tab\"").count(), 3, "{html}");
        assert!(html.contains("aria-selected=\"true\""));
        assert!(html.contains("href=\"/dojo/1\""));
        assert!(!html.contains("href=\"/dojo/2\""), "no link without a name");
        assert!(html.contains("Virtual best solver of 2 solvers: 8/10 solved, PAR-2 1.500"));
        assert!(html.contains("data-testid=\"cactus\""));
    }
}
