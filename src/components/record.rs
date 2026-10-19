//! A player's daily record, on the account page and on their profile: stat tiles and the
//! calendar of the last weeks.

use leptos::prelude::*;

use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::date_short;
use crate::model::{UserStats, date_string, format_duration};

#[component]
fn StatTile(label: String, jp: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex flex-col items-center rounded-lg border bg-background px-2 py-3">
            <span class="font-pixel text-2xl tabular-nums leading-tight">{value}</span>
            <span class="text-center text-[10px] font-bold uppercase leading-tight tracking-wider text-muted-foreground">
                {label}
            </span>
            <span class="font-pixel text-[10px] leading-tight text-primary/80">{jp}</span>
        </div>
    }
}

/// The last weeks as a grid of pixel squares, gold for top 3 finishes.
#[component]
fn Calendar(stats: UserStats) -> impl IntoView {
    let i18n = use_i18n();
    let locale = i18n.get_locale_untracked();
    view! {
        <div class="grid grid-cols-7 gap-1.5">
            {stats
                .recent
                .into_iter()
                .map(|d| {
                    let (class, title) = match (d.time_ms, d.rank) {
                        (Some(t), r) => {
                            let rank = r
                                .map(|rank| format!(" · {}", td_string!(locale, account.calendar_rank, rank)))
                                .unwrap_or_default();
                            (
                                if r.is_some_and(|r| r <= 3) { "bg-[var(--px-gold)]" } else { "bg-[var(--px-grass)]" },
                                format!("{} · {}{rank}", date_short(locale, d.day), format_duration(t)),
                            )
                        }
                        _ => (
                            "bg-muted",
                            format!("{} · {}", date_short(locale, d.day), td_string!(locale, account.not_solved)),
                        ),
                    };
                    view! {
                        <div
                            class=format!("aspect-square rounded-[3px] border border-black/5 {class}")
                            data-tooltip=title
                            data-date=date_string(d.day)
                        ></div>
                    }
                })
                .collect_view()}
        </div>
    }
}

/// The stat tiles, then the calendar. Two blocks, laid out by the parent.
#[component]
pub fn Record(stats: UserStats) -> impl IntoView {
    let i18n = use_i18n();
    let s = stats;
    let week = s
        .recent
        .iter()
        .rev()
        .take(7)
        .filter(|d| d.time_ms.is_some())
        .count();
    view! {
        <div class="grid grid-cols-3 gap-2">
            <StatTile label=t_string!(i18n, account.stats.solved).to_string() jp="解" value=s.solved.to_string() />
            <StatTile label=t_string!(i18n, account.stats.streak).to_string() jp="連続" value=s.current_streak.to_string() />
            <StatTile label=t_string!(i18n, account.stats.best).to_string() jp="最長" value=s.best_streak.to_string() />
            <StatTile
                label=t_string!(i18n, account.stats.fastest).to_string()
                jp="最速"
                value=s.best_time_ms.map_or_else(|| "–".into(), format_duration)
            />
            <StatTile
                label=t_string!(i18n, account.stats.average).to_string()
                jp="平均"
                value=s.average_time_ms.map_or_else(|| "–".into(), format_duration)
            />
            <StatTile label=t_string!(i18n, account.stats.week).to_string() jp="週" value=week.to_string() />
            <StatTile
                label=t_string!(i18n, account.stats.month).to_string()
                jp="月"
                value=s.month_rank.map_or_else(|| "–".into(), |r| format!("#{r}"))
            />
            <StatTile label=t_string!(i18n, account.stats.points).to_string() jp="点" value=s.month_points.to_string() />
            <StatTile
                label=t_string!(i18n, account.stats.medals).to_string()
                jp="金銀銅"
                value=format!("{}·{}·{}", s.gold, s.silver, s.bronze)
            />
        </div>
        <div class="flex flex-col gap-2">
            <p class="text-xs font-bold uppercase tracking-[0.16em] text-muted-foreground">
                {t!(i18n, account.last_weeks)}
            </p>
            <Calendar stats=s />
        </div>
    }
}
