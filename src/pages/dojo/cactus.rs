//! The cactus plot: formulas solved (x) against the fuel each one took (y, log scale), one line
//! per solver and one for the virtual best solver (VBS), like the SAT Competition shows results.
//!
//! An emphasis chart: the VBS in the accent colour, one solver highlighted (the submission on its
//! page, the hovered row on the leaderboard), every other solver as quiet context. The lines are
//! an SVG whose coordinates are percentages of the plot (`preserveAspectRatio="none"`,
//! strokes that don't scale), labels and dots are HTML on top of it, so text keeps its size on
//! any screen. Hovering or the arrow keys move a crosshair along the x axis, a tooltip lists the
//! curves there, and a table repeats the numbers for anyone who can't see the plot.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, KeyboardEvent, PointerEvent};

use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::model::{DojoCactus, DojoCurve};

/// The SVG's coordinate space: both axes from 0 to this.
const SPACE: f64 = 1000.0;

/// Where values go in the plot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Scale {
    /// The bottom of the y axis, a power of ten at or below the smallest fuel.
    pub(super) low: u64,
    /// The top: the budget.
    pub(super) high: u64,
    /// The right end of the x axis.
    pub(super) formulas: u32,
}

impl Scale {
    pub(super) fn of(cactus: &DojoCactus) -> Self {
        let smallest = cactus
            .curves
            .iter()
            .filter_map(|c| c.fuel.first())
            .chain(cactus.vbs.first())
            .copied()
            .min()
            .unwrap_or(cactus.budget)
            .max(1);
        let mut low = 1u64;
        while low.saturating_mul(10) <= smallest {
            low *= 10;
        }
        let high = cactus.budget.max(low.saturating_mul(10));
        let most = cactus
            .curves
            .iter()
            .map(|c| c.fuel.len())
            .chain([cactus.vbs.len()])
            .max();
        let formulas = cactus
            .formulas
            .max(u32::try_from(most.unwrap_or(0)).unwrap_or(u32::MAX))
            .max(1);
        Self {
            low,
            high,
            formulas,
        }
    }

    /// The k-th formula solved, as a share of the width (0 to 1).
    pub(super) fn x(self, k: usize) -> f64 {
        f64::from(u32::try_from(k).unwrap_or(u32::MAX)) / f64::from(self.formulas)
    }

    /// Fuel as a share of the height from the top (0 to 1), on a log scale.
    #[expect(
        clippy::cast_precision_loss,
        reason = "fuel is far below 2^53, and only placed on the screen"
    )]
    pub(super) fn y(self, fuel: u64) -> f64 {
        let (low, high) = ((self.low as f64).log10(), (self.high as f64).log10());
        let at = (fuel.clamp(self.low, self.high) as f64).log10();
        1.0 - (at - low) / (high - low)
    }

    /// Powers of ten from the bottom to just under the top.
    pub(super) fn y_ticks(self) -> Vec<u64> {
        let mut ticks = Vec::new();
        let mut tick = self.low;
        while tick < self.high {
            ticks.push(tick);
            tick = tick.saturating_mul(10);
        }
        ticks
    }

    /// Round steps along the x axis, at most about eight, and the right end.
    pub(super) fn x_ticks(self) -> Vec<u32> {
        let step = [1, 2, 5, 10, 20, 25, 50, 100, 200, 500]
            .into_iter()
            .find(|s| self.formulas / s <= 8)
            .unwrap_or(1000);
        let mut ticks: Vec<u32> = (0..=self.formulas).step_by(step as usize).collect();
        // The end, unless a tick is too close to it
        if ticks.last().is_some_and(|&t| self.formulas - t > step / 2) {
            ticks.push(self.formulas);
        } else if let Some(last) = ticks.last_mut() {
            *last = self.formulas;
        }
        ticks
    }

    /// The SVG points of a curve: the k-th smallest fuel at x = k.
    pub(super) fn points(self, fuel: &[u64]) -> String {
        fuel.iter()
            .enumerate()
            .map(|(i, &f)| format!("{:.1},{:.1}", self.x(i + 1) * SPACE, self.y(f) * SPACE))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Fuel in three digits: `950`, `12.3K`, `4.56M`, `7G`.
pub(super) fn compact(fuel: u64) -> String {
    #[expect(clippy::cast_precision_loss, reason = "only shown, three digits")]
    let value = fuel as f64;
    let (scaled, unit) = [(1e9, "G"), (1e6, "M"), (1e3, "K")]
        .into_iter()
        .find(|(size, _)| value >= *size)
        .map_or((value, ""), |(size, unit)| (value / size, unit));
    let digits = if scaled >= 100.0 || unit.is_empty() {
        0
    } else if scaled >= 10.0 {
        1
    } else {
        2
    };
    let text = format!("{scaled:.digits$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    };
    format!("{text}{unit}")
}

/// The fuel at the k-th solved formula (1-based), if the curve gets that far.
fn at(fuel: &[u64], k: usize) -> Option<u64> {
    k.checked_sub(1).and_then(|i| fuel.get(i)).copied()
}

/// The fastest, median and slowest solved formula.
fn summary(fuel: &[u64]) -> Option<(u64, u64, u64)> {
    Some((*fuel.first()?, fuel[fuel.len() / 2], *fuel.last()?))
}

/// A short line in a series' colour, the key of a line in the legend and the tooltip.
#[component]
fn LineKey(color: &'static str) -> impl IntoView {
    view! { <span aria-hidden="true" class="inline-block h-0.5 w-4 shrink-0 rounded-full" style=format!("background: {color}")></span> }
}

const VBS: &str = "var(--chart-vbs)";
const MARK: &str = "var(--chart-mark)";
const CONTEXT: &str = "var(--chart-context)";

#[component]
pub(super) fn Cactus(
    cactus: DojoCactus,
    /// The submission to emphasise, if any.
    #[prop(into)]
    highlight: Signal<Option<i64>>,
) -> impl IntoView {
    let i18n = use_i18n();
    let scale = Scale::of(&cactus);
    let cactus = StoredValue::new(cactus);
    // The crosshair: how many formulas solved, `None` while nothing points at the plot
    let cursor = RwSignal::new(None::<usize>);
    let longest = cactus.with_value(|c| {
        c.curves
            .iter()
            .map(|c| c.fuel.len())
            .chain([c.vbs.len()])
            .max()
            .unwrap_or(0)
    });
    let pct = |share: f64| format!("{:.2}%", share * 100.0);
    let point_at = move |ev: &PointerEvent| {
        let rect = ev
            .current_target()
            .and_then(|t| t.dyn_into::<Element>().ok())?
            .get_bounding_client_rect();
        let share = (f64::from(ev.client_x()) - rect.left()) / rect.width().max(1.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to the formulas of a set"
        )]
        let k = (share * f64::from(scale.formulas)).round().max(1.0) as usize;
        Some(k.min(longest.max(1)))
    };
    let keys = move |ev: KeyboardEvent| {
        let k = cursor.get_untracked().unwrap_or(0);
        let next = match ev.key().as_str() {
            "ArrowRight" | "ArrowUp" => k + 1,
            "ArrowLeft" | "ArrowDown" => k.saturating_sub(1),
            "Home" => 1,
            "End" => longest,
            "Escape" => 0,
            _ => return,
        };
        ev.prevent_default();
        cursor.set((next > 0).then(|| next.min(longest.max(1))));
    };
    let label = move || {
        cactus.with_value(|c| {
            let vbs = summary(&c.vbs);
            td_string!(
                i18n.get_locale(),
                dojo.cactus.aria,
                count = c.curves.len() as u64,
                solved = c.vbs.len() as u64,
                total = c.formulas,
                low = vbs.map(|v| compact(v.0)).unwrap_or_default(),
                high = vbs.map(|v| compact(v.2)).unwrap_or_default()
            )
        })
    };
    let curve_view = move |curve: DojoCurve| {
        let id = curve.id;
        let points = scale.points(&curve.fuel);
        let marked = move || highlight.get() == Some(id);
        view! {
            <polyline
                points=points
                fill="none"
                stroke-linejoin="round"
                stroke-linecap="round"
                vector-effect="non-scaling-stroke"
                style=move || if marked() { format!("stroke: {MARK}; stroke-width: 2px") } else { format!("stroke: {CONTEXT}; stroke-width: 1.5px") }
                data-curve=id
            />
        }
    };
    // A dot and a label at the end of an emphasised curve
    let end = move |fuel: &[u64], color: &'static str, name: String, below: bool| {
        let last = fuel.last().copied()?;
        let (x, y) = (scale.x(fuel.len()), scale.y(last));
        let near_right = x > 0.7;
        Some(view! {
            <span
                aria-hidden="true"
                class="pointer-events-none absolute size-2 -translate-x-1/2 -translate-y-1/2 rounded-full"
                style=format!("left: {}; top: {}; background: {color}; box-shadow: 0 0 0 2px var(--card)", pct(x), pct(y))
            ></span>
            <span
                aria-hidden="true"
                class="pointer-events-none absolute max-w-40 truncate whitespace-nowrap text-xs font-semibold text-foreground"
                style=format!(
                    "left: {}; top: {}; transform: translate({}, {})",
                    pct(x),
                    pct(y),
                    if near_right { "calc(-100% - 8px)" } else { "8px" },
                    if below { "4px" } else { "calc(-100% - 4px)" },
                )
            >
                {name}
            </span>
        })
    };
    let tooltip = move || {
        let k = cursor.get()?;
        let locale = i18n.get_locale();
        cactus.with_value(|c| {
            let vbs = at(&c.vbs, k);
            let marked = highlight.get();
            let mut rows: Vec<(u64, String, &'static str)> = c
                .curves
                .iter()
                .filter_map(|curve| at(&curve.fuel, k).map(|f| (f, curve)))
                .map(|(f, curve)| (f, curve.name.clone(), if Some(curve.id) == marked { MARK } else { CONTEXT }))
                .collect();
            let reached = rows.len();
            rows.sort_by_key(|r| (r.2 != MARK, r.0));
            rows.truncate(4);
            let x = scale.x(k);
            Some(view! {
                <div
                    class="pointer-events-none absolute top-0 z-10 flex min-w-44 flex-col gap-1 rounded-md border bg-popover px-2.5 py-2 text-xs text-popover-foreground shadow-md"
                    style=format!("left: {}; transform: translateX({})", pct(x), if x > 0.6 { "calc(-100% - 10px)" } else { "10px" })
                >
                    <p class="font-semibold">{td_string!(locale, dojo.cactus.solved_at, count = k as u64)}</p>
                    {vbs
                        .map(|f| {
                            view! {
                                <p class="flex items-center gap-2">
                                    <LineKey color=VBS />
                                    <strong class="tabular-nums">{compact(f)}</strong>
                                    <span class="text-muted-foreground">{td_string!(locale, dojo.cactus.vbs_short)}</span>
                                </p>
                            }
                        })}
                    {rows
                        .into_iter()
                        .map(|(f, name, color)| {
                            view! {
                                <p class="flex items-center gap-2">
                                    <LineKey color />
                                    <strong class="tabular-nums">{compact(f)}</strong>
                                    <span class="max-w-36 truncate text-muted-foreground">{name}</span>
                                </p>
                            }
                        })
                        .collect_view()}
                    <p class="text-muted-foreground">
                        {td_string!(locale, dojo.cactus.reached, count = reached as u64, total = c.curves.len() as u64)}
                    </p>
                </div>
            })
        })
    };
    let (vbs_points, vbs_curve) = cactus.with_value(|c| (scale.points(&c.vbs), c.vbs.clone()));
    let highlighted = move || {
        let id = highlight.get()?;
        cactus.with_value(|c| c.curves.iter().find(|c| c.id == id).cloned())
    };
    let vbs_end_y = vbs_curve.last().map(|&f| scale.y(f));
    let marked_end = move || {
        let curve = highlighted()?;
        // Below the VBS's label when the two ends are close
        let below = match (curve.fuel.last(), vbs_end_y) {
            (Some(&f), Some(y)) => (scale.y(f) - y).abs() < 0.08 && scale.y(f) >= y,
            _ => false,
        };
        end(&curve.fuel, MARK, curve.name, below)
    };
    let vbs_end = end(
        &vbs_curve,
        VBS,
        t_string!(i18n, dojo.cactus.vbs_short).to_string(),
        false,
    );
    let rows = cactus.with_value(|c| c.curves.clone());
    let vbs_row = summary(&vbs_curve);
    let vbs_count = vbs_curve.len();
    let total = cactus.with_value(|c| c.formulas);
    let cell = |v: Option<u64>| v.map_or_else(|| "—".to_string(), compact);

    view! {
        <figure class="flex flex-col gap-3" data-testid="cactus">
            <figcaption class="flex flex-col gap-1">
                <span class="font-bold">{t!(i18n, dojo.cactus.title)}</span>
                <span class="text-sm text-muted-foreground">{t!(i18n, dojo.cactus.hint)}</span>
            </figcaption>
            // The legend, above the plot
            <ul class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                <li class="flex items-center gap-1.5"><LineKey color=VBS />{t!(i18n, dojo.cactus.vbs)}</li>
                {move || {
                    highlighted()
                        .map(|c| view! { <li class="flex items-center gap-1.5"><LineKey color=MARK /><span class="max-w-48 truncate">{c.name}</span></li> })
                }}
                <li class="flex items-center gap-1.5"><LineKey color=CONTEXT />{t!(i18n, dojo.cactus.solvers)}</li>
            </ul>
            <div dir="ltr" class="relative h-64 ps-12 pe-4 pt-4 pb-10 sm:h-72">
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
                    <svg
                        class="absolute inset-0 size-full overflow-visible"
                        viewBox="0 0 1000 1000"
                        preserveAspectRatio="none"
                        aria-hidden="true"
                    >
                        // Recessive grid: a hairline per power of ten, the budget on top
                        {scale
                            .y_ticks()
                            .into_iter()
                            .chain([scale.high])
                            .map(|tick| {
                                let y = scale.y(tick) * SPACE;
                                view! {
                                    <line x1="0" x2="1000" y1=y y2=y vector-effect="non-scaling-stroke" style="stroke: var(--chart-grid); stroke-width: 1px" />
                                }
                            })
                            .collect_view()}
                        <line x1="0" x2="1000" y1="1000" y2="1000" vector-effect="non-scaling-stroke" style="stroke: var(--chart-axis); stroke-width: 1px" />
                        <g data-series="solvers">{rows.into_iter().map(curve_view).collect_view()}</g>
                        // The highlighted curve again, on top of the others
                        {move || {
                            highlighted()
                                .map(|c| {
                                    view! {
                                        <polyline
                                            points=scale.points(&c.fuel)
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
                        <polyline
                            points=vbs_points
                            fill="none"
                            stroke-linejoin="round"
                            stroke-linecap="round"
                            vector-effect="non-scaling-stroke"
                            style=format!("stroke: {VBS}; stroke-width: 2px")
                            data-series="vbs"
                        />
                        {move || {
                            cursor
                                .get()
                                .map(|k| {
                                    let x = scale.x(k) * SPACE;
                                    view! {
                                        <line x1=x x2=x y1="0" y2="1000" vector-effect="non-scaling-stroke" style="stroke: var(--chart-axis); stroke-width: 1px" />
                                    }
                                })
                        }}
                    </svg>
                    {vbs_end}
                    {marked_end}
                    // Axis labels
                    {scale
                        .y_ticks()
                        .into_iter()
                        .map(|tick| {
                            view! {
                                <span
                                    class="absolute end-full me-2 -translate-y-1/2 text-[11px] tabular-nums text-muted-foreground"
                                    style=format!("top: {}", pct(scale.y(tick)))
                                >
                                    {compact(tick)}
                                </span>
                            }
                        })
                        .collect_view()}
                    <span class="absolute end-full top-0 me-2 -translate-y-1/2 text-[11px] font-semibold tabular-nums text-muted-foreground">
                        {compact(scale.high)}
                    </span>
                    <span class="absolute start-1 top-0 -translate-y-full pb-0.5 text-[11px] text-muted-foreground">{t!(i18n, dojo.cactus.budget)}</span>
                    {scale
                        .x_ticks()
                        .into_iter()
                        .map(|tick| {
                            view! {
                                <span
                                    class="absolute top-full mt-1 -translate-x-1/2 text-[11px] tabular-nums text-muted-foreground"
                                    style=format!("left: {}", pct(scale.x(tick as usize)))
                                >
                                    {tick}
                                </span>
                            }
                        })
                        .collect_view()}
                    <span class="absolute top-full start-1/2 mt-5 -translate-x-1/2 whitespace-nowrap text-xs text-muted-foreground">
                        {t!(i18n, dojo.cactus.x)}
                    </span>
                    <div aria-live="polite">{tooltip}</div>
                </div>
            </div>
            <p class="text-xs text-muted-foreground">{t!(i18n, dojo.cactus.y)}</p>
            // The same numbers without the plot
            <details class="text-sm">
                <summary class="cursor-pointer text-primary">{t!(i18n, dojo.cactus.table)}</summary>
                <div class="overflow-x-auto pt-2">
                    <table class="w-full text-sm">
                        <thead>
                            <tr class="border-b text-xs uppercase tracking-wider text-muted-foreground">
                                <th class="py-1.5 pe-3 text-start font-semibold">{t!(i18n, dojo.board.solver)}</th>
                                <th class="py-1.5 pe-3 text-end font-semibold">{t!(i18n, dojo.board.solved)}</th>
                                <th class="py-1.5 pe-3 text-end font-semibold">{t!(i18n, dojo.cactus.fastest)}</th>
                                <th class="py-1.5 pe-3 text-end font-semibold">{t!(i18n, dojo.cactus.median)}</th>
                                <th class="py-1.5 text-end font-semibold">{t!(i18n, dojo.cactus.slowest)}</th>
                            </tr>
                        </thead>
                        <tbody class="tabular-nums">
                            <tr class="border-b font-semibold">
                                <td class="py-1.5 pe-3">{t!(i18n, dojo.cactus.vbs)}</td>
                                <td class="py-1.5 pe-3 text-end">{format!("{vbs_count}/{total}")}</td>
                                <td class="py-1.5 pe-3 text-end">{cell(vbs_row.map(|r| r.0))}</td>
                                <td class="py-1.5 pe-3 text-end">{cell(vbs_row.map(|r| r.1))}</td>
                                <td class="py-1.5 text-end">{cell(vbs_row.map(|r| r.2))}</td>
                            </tr>
                            {cactus
                                .with_value(|c| c.curves.clone())
                                .into_iter()
                                .map(|c| {
                                    let row = summary(&c.fuel);
                                    view! {
                                        <tr class="border-b last:border-0">
                                            <td class="py-1.5 pe-3">{c.name.clone()}</td>
                                            <td class="py-1.5 pe-3 text-end">{format!("{}/{total}", c.fuel.len())}</td>
                                            <td class="py-1.5 pe-3 text-end">{cell(row.map(|r| r.0))}</td>
                                            <td class="py-1.5 pe-3 text-end">{cell(row.map(|r| r.1))}</td>
                                            <td class="py-1.5 text-end">{cell(row.map(|r| r.2))}</td>
                                        </tr>
                                    }
                                })
                                .collect_view()}
                        </tbody>
                    </table>
                </div>
            </details>
        </figure>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cactus() -> DojoCactus {
        DojoCactus {
            budget: 7_000_000_000,
            formulas: 48,
            vbs: vec![1_200_000, 30_000_000, 2_000_000_000],
            curves: vec![
                DojoCurve {
                    id: 1,
                    name: "Plain DPLL".into(),
                    house: true,
                    fuel: vec![1_200_000, 40_000_000],
                },
                DojoCurve {
                    id: 2,
                    name: "<b>CDCL</b>".into(),
                    house: false,
                    fuel: vec![30_000_000, 2_000_000_000],
                },
            ],
        }
    }

    #[test]
    fn scales() {
        let scale = Scale::of(&cactus());
        assert_eq!(
            scale,
            Scale {
                low: 1_000_000,
                high: 7_000_000_000,
                formulas: 48
            }
        );
        assert_eq!(scale.y(7_000_000_000), 0.0);
        assert_eq!(scale.y(1_000_000), 1.0);
        // Out of range values stay on the plot
        assert_eq!(scale.y(5), 1.0);
        assert_eq!(scale.y(u64::MAX), 0.0);
        assert!(scale.y(10_000_000) > scale.y(100_000_000));
        assert_eq!(
            scale.y_ticks(),
            [1_000_000, 10_000_000, 100_000_000, 1_000_000_000]
        );
        assert_eq!(scale.x_ticks(), [0, 10, 20, 30, 40, 48]);
        assert_eq!(scale.x(24), 0.5);
        // Points: x = k, the sorted fuel going up
        assert_eq!(scale.points(&[7_000_000_000]), "20.8,0.0");
        let small = Scale {
            low: 1,
            high: 10,
            formulas: 5,
        };
        assert_eq!(small.x_ticks(), [0, 1, 2, 3, 4, 5]);
        let odd = Scale {
            low: 1,
            high: 10,
            formulas: 54,
        };
        assert_eq!(odd.x_ticks(), [0, 10, 20, 30, 40, 54]);
        // Nothing solved yet: a decade below the budget
        let empty = Scale::of(&DojoCactus {
            budget: 7_000_000_000,
            formulas: 0,
            vbs: Vec::new(),
            curves: Vec::new(),
        });
        assert_eq!((empty.low, empty.formulas), (1_000_000_000, 1));
    }

    #[test]
    fn compact_numbers() {
        let cases = [
            (0, "0"),
            (950, "950"),
            (1_000, "1K"),
            (12_340, "12.3K"),
            (4_560_000, "4.56M"),
            (100_000_000, "100M"),
            (7_000_000_000, "7G"),
            (1_250_000_000, "1.25G"),
        ];
        for (fuel, text) in cases {
            assert_eq!(compact(fuel), text);
        }
        assert_eq!(summary(&[1, 2, 3, 4]), Some((1, 3, 4)));
        assert_eq!(summary(&[]), None);
        assert_eq!(
            (at(&[5, 6], 2), at(&[5, 6], 3), at(&[5], 0)),
            (Some(6), None, None)
        );
    }

    /// The plot rendered on the server: a line per solver and the VBS, the highlighted one on
    /// top, the axes, the table, and names as text, never as markup.
    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn renders_lines_axes_and_a_table() {
        use crate::i18n::{Locale, use_i18n};
        // For the tasks leptos_i18n starts
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        let html = owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::en);
            view! { <Cactus cactus=cactus() highlight=Some(2) /> }.to_html()
        });
        assert_eq!(html.matches("<polyline").count(), 4, "{html}");
        assert_eq!(html.matches("data-curve=").count(), 2);
        assert!(html.contains("data-series=\"vbs\""));
        assert!(html.contains("data-series=\"highlight\""));
        assert!(html.contains("role=\"img\""));
        assert!(
            html.contains("aria-label=\"Cactus plot of 2 solvers."),
            "{html}"
        );
        for tick in ["1M", "10M", "100M", "1G", "7G", ">48<"] {
            assert!(html.contains(tick), "{tick} missing");
        }
        assert!(html.contains("&lt;b&gt;CDCL&lt;/b&gt;"));
        assert!(!html.contains("<b>CDCL"));
        // The table: the VBS and both solvers
        assert!(html.contains("3/48") && html.contains("2/48"));
        assert!(html.contains("Show as a table"));
    }
}
