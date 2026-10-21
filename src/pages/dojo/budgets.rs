//! The budget view: the ranking with a tenth of the budget, the official one and with as much
//! fuel as the runs had, PAR-2 against the budget as a line chart, and where solvers overtake
//! each other.
//!
//! The runs go on to `dojo::RUN_BUDGET`; fuel is deterministic, so every smaller budget gives
//! exactly what a run with it would have (see `dojo::Outcome::at`). The budget to rank by is
//! the query parameter `budget` (a factor of the ranking budget: `0.1`, `1`, `10`), so a view
//! can be linked. The chart is an emphasis chart like the cactus plot: every solver as quiet
//! context, the hovered one (in the table or a sentence) in the accent colour.

use dojo::metrics::{BudgetScore, NATIVE_MS_PER_GFUEL};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_location, use_query_map};
use wasm_bindgen::JsCast;
use web_sys::{Element, KeyboardEvent, PointerEvent};

use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::prefix;
use crate::model::{DojoBudgetCurve, DojoBudgets, DojoEntry};

use super::cactus::{CONTEXT, LineKey, MARK, compact};

/// The SVG's coordinate space: both axes from 0 to this.
const SPACE: f64 = 1000.0;

/// The budgets to rank by: a tenth of the ranking budget, the budget, and the run budget when
/// the runs went further.
pub(super) fn choices(budgets: &DojoBudgets) -> Vec<u64> {
    let mut choices = vec![budgets.budget / 10, budgets.budget, budgets.run_budget];
    choices.sort_unstable();
    choices.dedup();
    choices
}

/// A budget as a factor of the ranking budget: `⅒×`, `1×`, `10×`.
pub(super) fn factor(budget: u64, at: u64) -> String {
    if at.saturating_mul(10) == budget {
        return "⅒×".into();
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "budgets are far below 2^53, only shown"
    )]
    let times = at as f64 / budget.max(1) as f64;
    format!("{}×", (times * 100.0).round() / 100.0)
}

/// The value of the query parameter for a budget: its factor of the ranking budget.
fn query_value(budget: u64, at: u64) -> String {
    factor(budget, at).trim_end_matches('×').replace('⅒', "0.1")
}

/// The budget the query parameter asks for, the official one if it asks for none of the
/// choices.
pub(super) fn chosen(budgets: &DojoBudgets, query: Option<&str>) -> u64 {
    choices(budgets)
        .into_iter()
        .find(|&at| query == Some(query_value(budgets.budget, at).as_str()))
        .unwrap_or(budgets.budget)
}

/// A curve's score at a budget of the grid, the nearest one below if it isn't on it.
fn point_at(curve: &DojoBudgetCurve, at: u64) -> Option<BudgetScore> {
    curve.points.iter().rev().find(|p| p.budget <= at).copied()
}

/// The entries ranked at `at`: the place, the place on the official board among them and the
/// score, best first. Entries with a wrong answer within the budget would be disqualified with
/// it: last, without a place.
pub(super) fn ranked_at(
    budgets: &DojoBudgets,
    at: u64,
) -> Vec<(Option<usize>, usize, DojoBudgetCurve, BudgetScore)> {
    let mut rows: Vec<(usize, DojoBudgetCurve, BudgetScore)> = budgets
        .curves
        .iter()
        .enumerate()
        .filter_map(|(official, c)| Some((official, c.clone(), point_at(c, at)?)))
        .collect();
    rows.sort_by(|a, b| {
        (a.2.wrong > 0)
            .cmp(&(b.2.wrong > 0))
            .then(a.2.par2.total_cmp(&b.2.par2))
            .then(b.2.solved.cmp(&a.2.solved))
            .then(a.0.cmp(&b.0))
    });
    rows.into_iter()
        .enumerate()
        .map(|(place, (official, curve, score))| {
            ((score.wrong == 0).then_some(place), official, curve, score)
        })
        .collect()
}

/// Fuel the way the crossovers name it: `2×10¹⁰`, `1.3×10⁹`.
pub(super) fn scientific(fuel: u64) -> String {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    if fuel < 1000 {
        return fuel.to_string();
    }
    #[expect(clippy::cast_precision_loss, reason = "only shown, two digits")]
    let value = fuel as f64;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the exponent of a u64 is at most 19"
    )]
    let mut exponent = value.log10().floor() as i32;
    let mut mantissa = (value / 10f64.powi(exponent) * 10.0).round() / 10.0;
    if mantissa >= 10.0 {
        mantissa /= 10.0;
        exponent += 1;
    }
    let power: String = exponent
        .to_string()
        .chars()
        .filter_map(|c| c.to_digit(10).map(|d| DIGITS[d as usize]))
        .collect();
    if (mantissa - 1.0).abs() < f64::EPSILON {
        format!("10{power}")
    } else {
        format!("{mantissa}×10{power}")
    }
}

/// Roughly how long a native solver takes for this much fuel, see
/// [`dojo::metrics::NATIVE_MS_PER_GFUEL`].
pub(super) fn native(fuel: u64) -> String {
    #[expect(clippy::cast_precision_loss, reason = "only shown, two digits")]
    let ms = fuel as f64 / 1e9 * NATIVE_MS_PER_GFUEL;
    // Two digits, more would claim a precision the scale doesn't have
    let unit = 10f64.powf(ms.log10().floor() - 1.0);
    let rounded = (ms / unit).round() * unit;
    if ms < 1.0 {
        "< 1 ms".into()
    } else if ms < 10.0 {
        format!("{rounded:.1} ms")
    } else if ms < 1000.0 {
        format!("{rounded:.0} ms")
    } else if ms < 10_000.0 {
        format!("{:.1} s", ms / 1000.0)
    } else {
        format!("{:.0} s", ms / 1000.0)
    }
}

/// Where values go in the chart: budgets (x) and PAR-2 (y), both on a log scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Scale {
    /// The first and the last budget.
    pub(super) left: u64,
    pub(super) right: u64,
    /// Powers of ten at or below the smallest PAR-2 and at or above the largest, in billions.
    pub(super) low: f64,
    pub(super) high: f64,
}

impl Scale {
    pub(super) fn of(budgets: &DojoBudgets) -> Self {
        let points = || budgets.curves.iter().flat_map(|c| &c.points);
        let left = points().map(|p| p.budget).min().unwrap_or(1).max(1);
        let right = points().map(|p| p.budget).max().unwrap_or(1).max(left + 1);
        let smallest = points()
            .map(|p| p.par2)
            .filter(|&p| p > 0.0)
            .fold(f64::INFINITY, f64::min);
        let largest = points().map(|p| p.par2).fold(0.0, f64::max);
        let low = if smallest.is_finite() {
            10f64.powf(smallest.log10().floor())
        } else {
            0.001
        };
        let high = 10f64.powf(largest.max(low).log10().ceil()).max(low * 10.0);
        Self {
            left,
            right,
            low,
            high,
        }
    }

    /// A budget as a share of the width, on a log scale.
    #[expect(
        clippy::cast_precision_loss,
        reason = "budgets are far below 2^53, only placed"
    )]
    pub(super) fn x(self, budget: u64) -> f64 {
        let (left, right) = ((self.left as f64).log10(), (self.right as f64).log10());
        (((budget.clamp(self.left, self.right) as f64).log10() - left) / (right - left))
            .clamp(0.0, 1.0)
    }

    /// PAR-2 as a share of the height from the top, on a log scale.
    pub(super) fn y(self, par2: f64) -> f64 {
        let (low, high) = (self.low.log10(), self.high.log10());
        1.0 - (par2.clamp(self.low, self.high).log10() - low) / (high - low)
    }

    /// Powers of ten of fuel from the left to the right end.
    pub(super) fn x_ticks(self) -> Vec<u64> {
        let mut tick = 1u64;
        let mut ticks = Vec::new();
        while tick <= self.right {
            if tick >= self.left {
                ticks.push(tick);
            }
            match tick.checked_mul(10) {
                Some(next) => tick = next,
                None => break,
            }
        }
        ticks
    }

    /// Powers of ten of PAR-2 from the bottom to the top.
    pub(super) fn y_ticks(self) -> Vec<f64> {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "both are powers of ten within a few decades of each other"
        )]
        let decades = (self.high / self.low).log10().round() as i32;
        (0..=decades).map(|k| self.low * 10f64.powi(k)).collect()
    }

    /// The SVG points of a curve.
    pub(super) fn points(self, curve: &DojoBudgetCurve) -> String {
        curve
            .points
            .iter()
            .map(|p| {
                format!(
                    "{:.1},{:.1}",
                    self.x(p.budget) * SPACE,
                    self.y(p.par2) * SPACE
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// PAR-2 as a tick label: `0.01`, `0.1`, `1`, `10`.
fn par2_tick(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The ranking at the chosen budget, the chart and the crossovers.
#[component]
pub(super) fn BudgetView(
    budgets: DojoBudgets,
    /// The board's entries, for authors and links.
    entries: StoredValue<Vec<DojoEntry>>,
    /// The row or sentence under the pointer, emphasised in the chart.
    hovered: RwSignal<Option<i64>>,
) -> impl IntoView {
    let i18n = use_i18n();
    let query = use_query_map();
    let location = use_location();
    let budgets = StoredValue::new(budgets);
    let selected = Signal::derive(move || {
        let q = query.with(|q| q.get("budget"));
        budgets.with_value(|b| chosen(b, q.as_deref()))
    });
    let (official_budget, options) = budgets.with_value(|b| (b.budget, choices(b)));
    let picker = options
        .into_iter()
        .map(|at| {
            let label = factor(official_budget, at);
            let value = query_value(official_budget, at);
            let href = {
                let value = value.clone();
                move || format!("{}?budget={value}", location.pathname.get())
            };
            view! {
                <A
                    href=href
                    scroll=false
                    attr:aria-current=move || (selected.get() == at).then_some("true")
                    attr:class=move || {
                        if selected.get() == at {
                            "rounded-md bg-background px-3 py-1 text-sm font-medium text-foreground shadow-sm"
                        } else {
                            "rounded-md px-3 py-1 text-sm font-medium text-muted-foreground hover:text-foreground"
                        }
                    }
                    attr:data-tooltip=format!("{} Gfuel", compact(at))
                    attr:data-budget=value
                >
                    <span class="font-pixel">{label}</span>
                    {(at == official_budget).then(|| view! { <span class="ms-1 text-xs">{t!(i18n, dojo.board.budget_official)}</span> })}
                </A>
            }
        })
        .collect_view();
    let rows = move || {
        let at = selected.get();
        budgets.with_value(|b| ranked_at(b, at))
            .into_iter()
            .map(|(place, official, curve, score)| {
                let id = curve.id;
                let entry = entries.with_value(|es| es.iter().find(|e| e.id == id).cloned());
                let author = entry.as_ref().map(|e| e.author.clone()).unwrap_or_default();
                let author = if author.is_empty() { t_string!(i18n, dojo.seasons.deleted).to_string() } else { author };
                let href = move || format!("{}/dojo/{id}", prefix(i18n.get_locale()));
                let name = if curve.name.is_empty() {
                    view! { <span class="text-muted-foreground">"—"</span> }.into_any()
                } else {
                    view! { <A href=href attr:class="font-medium hover:text-primary hover:underline">{curve.name.clone()}</A> }.into_any()
                };
                // How far it moved from its place on the official board
                let moved = place.map(|p| {
                    let up = i64::try_from(official).unwrap_or(0) - i64::try_from(p).unwrap_or(0);
                    match up.signum() {
                        1 => format!("▲{up}"),
                        -1 => format!("▼{}", -up),
                        _ => String::new(),
                    }
                });
                view! {
                    <tr
                        class="border-b last:border-0 hover:bg-muted/50"
                        on:pointerenter=move |_| hovered.set(Some(id))
                        on:pointerleave=move |_| hovered.set(None)
                        on:focusin=move |_| hovered.set(Some(id))
                        on:focusout=move |_| hovered.set(None)
                        data-entry=id
                    >
                        <td class="py-2.5 pe-2 font-pixel tabular-nums">
                            {place.map_or_else(|| "—".to_string(), |p| (p + 1).to_string())}
                            <span class="ms-1 text-[10px] text-muted-foreground">{moved}</span>
                        </td>
                        <td class="py-2.5 pe-3">
                            {name}
                            {curve.house.then(|| view! { <Badge variant=BadgeVariant::Secondary size=BadgeSize::Sm class="ms-2">{t!(i18n, dojo.board.house)}</Badge> })}
                            <span class="block text-xs text-muted-foreground">{author}</span>
                        </td>
                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{format!("{}/{}", score.solved, score.total)}</td>
                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">
                            {if score.wrong > 0 { t_string!(i18n, dojo.board.budget_wrong).to_string() } else { format!("{:.3}", score.par2) }}
                        </td>
                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{score.sat}</td>
                        <td class="py-2.5 text-end font-pixel tabular-nums">{score.unsat}</td>
                    </tr>
                }
            })
            .collect_view()
    };
    let names = move |id: i64| {
        budgets.with_value(|b| {
            b.curves
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.name.clone())
                .unwrap_or_default()
        })
    };
    let sentences = move || {
        let locale = i18n.get_locale();
        budgets.with_value(|b| {
            if b.crossovers.is_empty() {
                let none = td_string!(
                    locale,
                    dojo.board.no_crossovers,
                    fuel = scientific(b.run_budget)
                );
                return view! { <li>{none}</li> }.into_any();
            }
            b.crossovers
                .iter()
                .map(|c| {
                    let ahead = c.ahead;
                    let text = td_string!(
                        locale,
                        dojo.board.crossover,
                        ahead = names(c.ahead),
                        behind = names(c.behind),
                        fuel = scientific(c.budget),
                        time = native(c.budget)
                    );
                    view! {
                        <li
                            data-testid="crossover"
                            on:pointerenter=move |_| hovered.set(Some(ahead))
                            on:pointerleave=move |_| hovered.set(None)
                        >
                            {text}
                        </li>
                    }
                })
                .collect_view()
                .into_any()
        })
    };
    view! {
        <div class="flex flex-col gap-4" data-testid="budget-view">
            <div role="group" aria-label=move || t_string!(i18n, dojo.board.budget_pick) class="inline-flex w-fit flex-wrap gap-1 rounded-lg bg-muted p-1">
                {picker}
            </div>
            <div class="overflow-x-auto">
                <table class="w-full text-sm">
                    <thead>
                        <tr class="border-b text-start text-xs uppercase tracking-wider text-muted-foreground">
                            <th class="py-2 pe-2 text-start font-semibold">"#"</th>
                            <th class="py-2 pe-3 text-start font-semibold">{t!(i18n, dojo.board.solver)}</th>
                            <th class="py-2 pe-3 text-end font-semibold">{t!(i18n, dojo.board.solved)}</th>
                            <th class="py-2 pe-3 text-end font-semibold">{t!(i18n, dojo.board.par2)}</th>
                            <th class="py-2 pe-3 text-end font-semibold">"SAT"</th>
                            <th class="py-2 text-end font-semibold">"UNSAT"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
            <BudgetChart budgets=budgets.get_value() highlight=hovered selected />
            <ul class="flex list-disc flex-col gap-1 ps-5 text-sm text-muted-foreground" data-testid="crossovers">{sentences}</ul>
        </div>
    }
}

/// PAR-2 against the budget, one line per solver.
#[component]
pub(super) fn BudgetChart(
    budgets: DojoBudgets,
    #[prop(into)] highlight: Signal<Option<i64>>,
    #[prop(into)] selected: Signal<u64>,
) -> impl IntoView {
    let i18n = use_i18n();
    let scale = Scale::of(&budgets);
    let grid: Vec<u64> = budgets
        .curves
        .first()
        .map(|c| c.points.iter().map(|p| p.budget).collect())
        .unwrap_or_default();
    let steps = grid.len();
    let budgets = StoredValue::new(budgets);
    let grid = StoredValue::new(grid);
    // The crosshair: a budget of the grid, `None` while nothing points at the chart
    let cursor = RwSignal::new(None::<usize>);
    let pct = |share: f64| format!("{:.2}%", share * 100.0);
    let nearest = move |share: f64| {
        grid.with_value(|g| {
            g.iter()
                .enumerate()
                .min_by(|a, b| {
                    (scale.x(*a.1) - share)
                        .abs()
                        .total_cmp(&(scale.x(*b.1) - share).abs())
                })
                .map(|(i, _)| i)
        })
    };
    let point_at = move |ev: &PointerEvent| {
        let rect = ev
            .current_target()
            .and_then(|t| t.dyn_into::<Element>().ok())?
            .get_bounding_client_rect();
        nearest((f64::from(ev.client_x()) - rect.left()) / rect.width().max(1.0))
    };
    let keys = move |ev: KeyboardEvent| {
        let last = steps.saturating_sub(1);
        let next = match (ev.key().as_str(), cursor.get_untracked()) {
            ("ArrowRight" | "ArrowUp", None) | ("Home", _) => Some(0),
            ("ArrowRight" | "ArrowUp", Some(i)) => Some((i + 1).min(last)),
            ("ArrowLeft" | "ArrowDown", Some(i)) => i.checked_sub(1),
            ("ArrowLeft" | "ArrowDown", None) | ("Escape", _) => None,
            ("End", _) => Some(last),
            _ => return,
        };
        ev.prevent_default();
        cursor.set(next);
    };
    let (budget, low, high, count) =
        budgets.with_value(|b| (b.budget, scale.left, scale.right, b.curves.len()));
    let label = move || {
        td_string!(
            i18n.get_locale(),
            dojo.board.budget_chart_aria,
            count = count as u64,
            low = compact(low),
            high = compact(high)
        )
    };
    let line = move |curve: &DojoBudgetCurve| {
        let id = curve.id;
        let points = scale.points(curve);
        view! {
            <polyline
                points=points
                fill="none"
                stroke-linejoin="round"
                stroke-linecap="round"
                vector-effect="non-scaling-stroke"
                style=move || if highlight.get() == Some(id) { format!("stroke: {MARK}; stroke-width: 2px") } else { format!("stroke: {CONTEXT}; stroke-width: 1.5px") }
                data-curve=id
            />
        }
    };
    let highlighted = move || {
        let id = highlight.get()?;
        budgets.with_value(|b| b.curves.iter().find(|c| c.id == id).cloned())
    };
    let tooltip = move || {
        let i = cursor.get()?;
        let at = grid.with_value(|g| g.get(i).copied())?;
        let locale = i18n.get_locale();
        let marked = highlight.get();
        let mut rows: Vec<(BudgetScore, String, bool)> = budgets.with_value(|b| {
            b.curves
                .iter()
                .filter_map(|c| Some((*c.points.get(i)?, c.name.clone(), Some(c.id) == marked)))
                .collect()
        });
        rows.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.par2.total_cmp(&b.0.par2)));
        rows.truncate(5);
        let x = scale.x(at);
        Some(view! {
            <div
                class="pointer-events-none absolute top-0 z-10 flex min-w-48 flex-col gap-1 rounded-md border bg-popover px-2.5 py-2 text-xs text-popover-foreground shadow-md"
                style=format!("left: {}; transform: translateX({})", pct(x), if x > 0.6 { "calc(-100% - 10px)" } else { "10px" })
            >
                <p class="font-semibold">{td_string!(locale, dojo.board.budget_at, fuel = compact(at), time = native(at))}</p>
                {rows
                    .into_iter()
                    .map(|(p, name, mark)| {
                        view! {
                            <p class="flex items-center gap-2">
                                <LineKey color=if mark { MARK } else { CONTEXT } />
                                <strong class="tabular-nums">{format!("{:.3}", p.par2)}</strong>
                                <span class="max-w-36 truncate text-muted-foreground">{name}</span>
                            </p>
                        }
                    })
                    .collect_view()}
            </div>
        })
    };
    let marked_end = move || {
        let curve = highlighted()?;
        let last = curve.points.last().copied()?;
        let (x, y) = (scale.x(last.budget), scale.y(last.par2));
        Some(view! {
            <span
                aria-hidden="true"
                class="pointer-events-none absolute size-2 -translate-x-1/2 -translate-y-1/2 rounded-full"
                style=format!("left: {}; top: {}; background: {MARK}; box-shadow: 0 0 0 2px var(--card)", pct(x), pct(y))
            ></span>
            <span
                aria-hidden="true"
                class="pointer-events-none absolute max-w-40 truncate whitespace-nowrap text-xs font-semibold text-foreground"
                style=format!("left: {}; top: {}; transform: translate(calc(-100% - 8px), calc(-100% - 4px))", pct(x), pct(y))
            >
                {curve.name}
            </span>
        })
    };
    let official_x = scale.x(budget);
    let lines = budgets.with_value(|b| b.curves.iter().map(line).collect_view());

    view! {
        <figure class="flex flex-col gap-3" data-testid="budget-chart">
            <figcaption class="flex flex-col gap-1">
                <span class="font-bold">{t!(i18n, dojo.board.budget_chart)}</span>
                <span class="text-sm text-muted-foreground">{t!(i18n, dojo.board.budget_chart_hint)}</span>
            </figcaption>
            <ul class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                {move || highlighted().map(|c| view! { <li class="flex items-center gap-1.5"><LineKey color=MARK /><span class="max-w-48 truncate">{c.name}</span></li> })}
                <li class="flex items-center gap-1.5"><LineKey color=CONTEXT />{t!(i18n, dojo.cactus.solvers)}</li>
            </ul>
            <div dir="ltr" class="relative h-64 ps-12 pe-4 pt-6 pb-10 sm:h-72">
                <div
                    class="relative size-full touch-pan-y rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    role="img"
                    tabindex="0"
                    aria-label=label
                    on:pointermove=move |ev: PointerEvent| cursor.set(point_at(&ev))
                    on:pointerleave=move |_| cursor.set(None)
                    on:blur=move |_| cursor.set(None)
                    on:keydown=keys
                >
                    <svg class="absolute inset-0 size-full overflow-visible" viewBox="0 0 1000 1000" preserveAspectRatio="none" aria-hidden="true">
                        {scale
                            .y_ticks()
                            .into_iter()
                            .map(|tick| {
                                let y = scale.y(tick) * SPACE;
                                view! { <line x1="0" x2="1000" y1=y y2=y vector-effect="non-scaling-stroke" style="stroke: var(--chart-grid); stroke-width: 1px" /> }
                            })
                            .collect_view()}
                        <line x1="0" x2="1000" y1="1000" y2="1000" vector-effect="non-scaling-stroke" style="stroke: var(--chart-axis); stroke-width: 1px" />
                        // The official budget, and the one the table ranks by
                        <line
                            x1=official_x * SPACE
                            x2=official_x * SPACE
                            y1="0"
                            y2="1000"
                            vector-effect="non-scaling-stroke"
                            stroke-dasharray="4 3"
                            style="stroke: var(--chart-axis); stroke-width: 1.5px"
                            data-testid="official-budget"
                        />
                        {move || {
                            let at = selected.get();
                            (at != budget)
                                .then(|| {
                                    let x = scale.x(at) * SPACE;
                                    view! { <line x1=x x2=x y1="0" y2="1000" vector-effect="non-scaling-stroke" style="stroke: var(--chart-axis); stroke-width: 1px" /> }
                                })
                        }}
                        <g data-series="solvers">{lines}</g>
                        {move || {
                            highlighted()
                                .map(|c| {
                                    view! {
                                        <polyline
                                            points=scale.points(&c)
                                            fill="none"
                                            stroke-linejoin="round"
                                            stroke-linecap="round"
                                            vector-effect="non-scaling-stroke"
                                            style=format!("stroke: {MARK}; stroke-width: 2px")
                                            data-series="highlight"
                                        />
                                    }
                                })
                        }}
                        {move || {
                            let at = grid.with_value(|g| g.get(cursor.get()?).copied())?;
                            let x = scale.x(at) * SPACE;
                            Some(view! { <line x1=x x2=x y1="0" y2="1000" vector-effect="non-scaling-stroke" style="stroke: var(--chart-axis); stroke-width: 1px" /> })
                        }}
                    </svg>
                    {marked_end}
                    <span
                        class="pointer-events-none absolute top-0 -translate-x-1/2 -translate-y-full pb-0.5 text-[11px] whitespace-nowrap text-muted-foreground"
                        style=format!("left: {}", pct(official_x))
                    >
                        {t!(i18n, dojo.cactus.ranking)} " · ≈ " {native(budget)}
                    </span>
                    {scale
                        .y_ticks()
                        .into_iter()
                        .map(|tick| {
                            view! {
                                <span class="absolute end-full me-2 -translate-y-1/2 text-[11px] tabular-nums text-muted-foreground" style=format!("top: {}", pct(scale.y(tick)))>
                                    {par2_tick(tick)}
                                </span>
                            }
                        })
                        .collect_view()}
                    {scale
                        .x_ticks()
                        .into_iter()
                        .map(|tick| {
                            view! {
                                <span class="absolute top-full mt-1 -translate-x-1/2 text-[11px] tabular-nums text-muted-foreground" style=format!("left: {}", pct(scale.x(tick)))>
                                    {compact(tick)}
                                </span>
                            }
                        })
                        .collect_view()}
                    <span class="absolute top-full start-1/2 mt-5 -translate-x-1/2 whitespace-nowrap text-xs text-muted-foreground">
                        {t!(i18n, dojo.board.budget_chart_x)}
                    </span>
                    <div aria-live="polite">{tooltip}</div>
                </div>
            </div>
            <p class="text-xs text-muted-foreground">{t!(i18n, dojo.board.budget_chart_y)}</p>
        </figure>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DojoCrossover;

    const G: u64 = 1_000_000_000;

    fn point(budget: u64, solved: u32, par2: f64, wrong: u32) -> BudgetScore {
        BudgetScore {
            budget,
            solved,
            total: 3,
            par2,
            sat: solved,
            unsat: 0,
            wrong,
            unverified: 0,
        }
    }

    fn curve(id: i64, name: &str, points: Vec<BudgetScore>) -> DojoBudgetCurve {
        DojoBudgetCurve {
            id,
            name: name.into(),
            house: true,
            points,
        }
    }

    /// Two solvers: the quick one leads up to the budget, the slow one past it; a third gives a
    /// wrong answer past the budget.
    pub(super) fn budgets() -> DojoBudgets {
        let b = 7 * G;
        DojoBudgets {
            budget: b,
            run_budget: 10 * b,
            curves: vec![
                curve(
                    1,
                    "MiniSat",
                    vec![
                        point(b / 10, 1, 0.9, 0),
                        point(b, 2, 5.0, 0),
                        point(10 * b, 2, 47.0, 0),
                    ],
                ),
                curve(
                    2,
                    "Kissat",
                    vec![
                        point(b / 10, 0, 1.4, 0),
                        point(b, 2, 6.3, 0),
                        point(10 * b, 3, 8.3, 0),
                    ],
                ),
                curve(
                    3,
                    "Liar",
                    vec![
                        point(b / 10, 0, 1.4, 0),
                        point(b, 1, 9.0, 0),
                        point(10 * b, 3, 0.5, 1),
                    ],
                ),
            ],
            crossovers: vec![DojoCrossover {
                budget: 20 * G,
                ahead: 2,
                behind: 1,
            }],
        }
    }

    #[test]
    fn budgets_to_rank_by() {
        let b = budgets();
        assert_eq!(choices(&b), [700_000_000, 7 * G, 70 * G]);
        let labels: Vec<String> = choices(&b)
            .into_iter()
            .map(|at| factor(b.budget, at))
            .collect();
        assert_eq!(labels, ["⅒×", "1×", "10×"]);
        assert_eq!(factor(7 * G, 17_500_000_000), "2.5×");
        // The query parameter names the factor; anything else is the official budget
        assert_eq!(chosen(&b, Some("0.1")), 700_000_000);
        assert_eq!(chosen(&b, Some("10")), 70 * G);
        assert_eq!(chosen(&b, Some("1")), 7 * G);
        assert_eq!(chosen(&b, Some("3")), 7 * G);
        assert_eq!(chosen(&b, None), 7 * G);
        // Older rules: no runs past the budget, so no ten times
        let mut old = budgets();
        old.run_budget = old.budget;
        assert_eq!(choices(&old), [700_000_000, 7 * G]);
    }

    #[test]
    fn rankings_at_a_budget() {
        let b = budgets();
        let order = |at| {
            ranked_at(&b, at)
                .into_iter()
                .map(|(place, official, c, _)| (place, official, c.id))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            order(7 * G),
            [(Some(0), 0, 1), (Some(1), 1, 2), (Some(2), 2, 3)]
        );
        // Ten times: Kissat first; the liar would be disqualified, last without a place
        assert_eq!(
            order(70 * G),
            [(Some(0), 1, 2), (Some(1), 0, 1), (None, 2, 3)]
        );
        // Between the grid's budgets, the one below counts
        assert_eq!(order(8 * G), order(7 * G));
        assert_eq!(ranked_at(&b, 1).len(), 0);
    }

    #[test]
    fn numbers_as_shown() {
        assert_eq!(scientific(20 * G), "2×10¹⁰");
        assert_eq!(scientific(1_340_000_001), "1.3×10⁹");
        assert_eq!(scientific(9_960_000_000), "10¹⁰");
        assert_eq!(scientific(100_000_000), "10⁸");
        assert_eq!(scientific(999), "999");
        // About 81 ms native per billion
        assert_eq!(native(7 * G), "570 ms");
        assert_eq!(native(70 * G), "5.7 s");
        assert_eq!(native(700 * G), "57 s");
        assert_eq!(native(70_000_000), "5.7 ms");
        assert_eq!(native(1_000), "< 1 ms");
        assert_eq!(par2_tick(0.01), "0.01");
        assert_eq!(par2_tick(10.0), "10");
    }

    #[test]
    fn chart_scales() {
        let scale = Scale::of(&budgets());
        assert_eq!((scale.left, scale.right), (700_000_000, 70 * G));
        assert!((scale.low - 0.1).abs() < 1e-12 && (scale.high - 100.0).abs() < 1e-9);
        assert_eq!(scale.x(700_000_000), 0.0);
        assert_eq!(scale.x(70 * G), 1.0);
        assert!((scale.x(7 * G) - 0.5).abs() < 1e-12);
        assert_eq!(scale.y(100.0), 0.0);
        assert_eq!(scale.y(0.1), 1.0);
        assert_eq!(scale.y(0.0), 1.0);
        assert_eq!(scale.x_ticks(), [G, 10 * G]);
        assert_eq!(scale.y_ticks().len(), 4);
        assert_eq!(scale.points(&budgets().curves[0]).split(' ').count(), 3);
        // Nothing to show: still a scale
        let empty = Scale::of(&DojoBudgets {
            budget: 7 * G,
            run_budget: 70 * G,
            curves: Vec::new(),
            crossovers: Vec::new(),
        });
        assert!(empty.right > empty.left && empty.high > empty.low);
    }
}
