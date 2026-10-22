//! Achievements: hanko stamps for the stamp book on the account page, worked out from a
//! player's dailies (see `server::stamps`).

use std::collections::{HashMap, HashSet};

use common::{Difficulty, Puzzle};
use serde::{Deserialize, Serialize};

use super::{
    MS_PER_DAY, MoveEvent, civil, daily_difficulty, format_duration, month_days, month_of, weekday,
};

/// A stamp in the stamp book. Listed in the order of the book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stamp {
    First,
    Solved10,
    Solved50,
    Solved100,
    Solved365,
    Bridges1000,
    Bridges10000,
    Islands1000,
    Streak7,
    Streak30,
    Streak100,
    FullWeek,
    FullMoon,
    FourSeasons,
    Oni,
    Clean,
    StoneBridge,
    Storm,
    Phoenix,
    Tortoise,
    Metronome,
    EarlyBird,
    ZeroHour,
    OnTheBrink,
    NightOwl,
    LunchBreak,
    Swift,
    Podium,
    Rainbow,
    PhotoFinish,
    Gold,
    HatTrick,
    Gold10,
    Champion,
    NewYear,
    Tanabata,
    Anniversary,
    Dojo,
    BlackBelt,
    Sensei,
    Collector,
    SecondThoughts,
    Palindrome,
    LanterneRouge,
    LeapDay,
    Yokozuna,
    Sansho,
    RiverCrew,
}

impl Stamp {
    pub const ALL: [Self; 48] = [
        Self::First,
        Self::Solved10,
        Self::Solved50,
        Self::Solved100,
        Self::Solved365,
        Self::Bridges1000,
        Self::Bridges10000,
        Self::Islands1000,
        Self::Streak7,
        Self::Streak30,
        Self::Streak100,
        Self::FullWeek,
        Self::FullMoon,
        Self::FourSeasons,
        Self::Oni,
        Self::Clean,
        Self::StoneBridge,
        Self::Storm,
        Self::Phoenix,
        Self::Tortoise,
        Self::Metronome,
        Self::EarlyBird,
        Self::ZeroHour,
        Self::OnTheBrink,
        Self::NightOwl,
        Self::LunchBreak,
        Self::Swift,
        Self::Podium,
        Self::Rainbow,
        Self::PhotoFinish,
        Self::Gold,
        Self::HatTrick,
        Self::Gold10,
        Self::Champion,
        Self::NewYear,
        Self::Tanabata,
        Self::Anniversary,
        Self::Dojo,
        Self::BlackBelt,
        Self::Sensei,
        Self::Collector,
        Self::SecondThoughts,
        Self::Palindrome,
        Self::LanterneRouge,
        Self::LeapDay,
        Self::Yokozuna,
        Self::Sansho,
        Self::RiverCrew,
    ];

    /// Stored in the `stamps` table.
    pub const fn id(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Solved10 => "solved_10",
            Self::Solved50 => "solved_50",
            Self::Solved100 => "solved_100",
            Self::Solved365 => "solved_365",
            Self::Bridges1000 => "bridges_1000",
            Self::Bridges10000 => "bridges_10000",
            Self::Islands1000 => "islands_1000",
            Self::Streak7 => "streak_7",
            Self::Streak30 => "streak_30",
            Self::Streak100 => "streak_100",
            Self::FullWeek => "full_week",
            Self::FullMoon => "full_moon",
            Self::FourSeasons => "four_seasons",
            Self::Oni => "oni",
            Self::Clean => "clean",
            Self::StoneBridge => "stone_bridge",
            Self::Storm => "storm",
            Self::Phoenix => "phoenix",
            Self::Tortoise => "tortoise",
            Self::Metronome => "metronome",
            Self::EarlyBird => "early_bird",
            Self::ZeroHour => "zero_hour",
            Self::OnTheBrink => "on_the_brink",
            Self::NightOwl => "night_owl",
            Self::LunchBreak => "lunch_break",
            Self::Swift => "swift",
            Self::Podium => "podium",
            Self::Rainbow => "rainbow",
            Self::PhotoFinish => "photo_finish",
            Self::Gold => "gold",
            Self::HatTrick => "hat_trick",
            Self::Gold10 => "gold_10",
            Self::Champion => "champion",
            Self::NewYear => "new_year",
            Self::Tanabata => "tanabata",
            Self::Anniversary => "anniversary",
            Self::Dojo => "dojo",
            Self::BlackBelt => "black_belt",
            Self::Sensei => "sensei",
            Self::Collector => "collector",
            Self::SecondThoughts => "second_thoughts",
            Self::Palindrome => "palindrome",
            Self::LanterneRouge => "lanterne_rouge",
            Self::LeapDay => "leap_day",
            Self::Yokozuna => "yokozuna",
            Self::Sansho => "sansho",
            Self::RiverCrew => "river_crew",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    /// The kanji on the stamp.
    pub const fn kanji(self) -> &'static str {
        match self {
            Self::First => "初",
            Self::Solved10 | Self::Solved50 | Self::Solved100 | Self::Solved365 => "百",
            Self::Bridges1000 => "千",
            Self::Bridges10000 => "万",
            Self::Islands1000 => "島",
            Self::Streak7 | Self::Streak30 | Self::Streak100 => "連",
            Self::FullWeek => "週",
            // 満月, the full moon
            Self::FullMoon => "満",
            Self::FourSeasons => "季",
            Self::Oni => "鬼",
            Self::Clean => "清",
            // 石橋を叩いて渡る: tap the stone bridge before you cross it
            Self::StoneBridge => "石",
            Self::Storm => "嵐",
            // 鳳凰, the phoenix
            Self::Phoenix => "鳳",
            Self::Tortoise => "亀",
            Self::Metronome => "律",
            Self::EarlyBird => "早",
            Self::ZeroHour => "零",
            Self::OnTheBrink => "際",
            Self::NightOwl => "梟",
            Self::LunchBreak => "昼",
            Self::Swift => "速",
            Self::Podium => "銅",
            Self::Rainbow => "虹",
            Self::PhotoFinish => "差",
            Self::Gold | Self::Gold10 => "金",
            Self::HatTrick => "三",
            Self::Champion => "王",
            // 元旦, New Year's Day
            Self::NewYear => "元",
            // The magpies that make a bridge across the Milky Way on Tanabata
            Self::Tanabata => "鵲",
            Self::Anniversary => "誕",
            Self::Dojo => "道",
            // 黒帯, the black belt
            Self::BlackBelt => "帯",
            // 師範, the master of a dojo
            Self::Sensei => "師",
            Self::Collector => "集",
            Self::SecondThoughts => "迷",
            // 回文, a palindrome
            Self::Palindrome => "回",
            // The lantern at the back of the race
            Self::LanterneRouge => "灯",
            // 閏日, the leap day
            Self::LeapDay => "閏",
            // The rope 綱 a yokozuna wears
            Self::Yokozuna => "綱",
            // 三賞, the special prizes of a sumo tournament
            Self::Sansho => "賞",
            // 架ける, to build a bridge across: a level of a group's river
            Self::RiverCrew => "架",
        }
    }

    /// The number of the tier (`10` solved, `7` days), for stamps that share a kanji.
    pub const fn tier(self) -> Option<i64> {
        match self {
            Self::Solved10 | Self::Gold10 => Some(10),
            Self::Solved50 => Some(50),
            Self::Solved100 | Self::Streak100 => Some(100),
            Self::Solved365 => Some(365),
            Self::Streak7 => Some(7),
            Self::Streak30 => Some(30),
            _ => None,
        }
    }

    /// Hidden in the stamp book until earned: finding out is half the fun.
    pub const fn secret(self) -> bool {
        matches!(
            self,
            Self::SecondThoughts | Self::Palindrome | Self::LanterneRouge | Self::LeapDay
        )
    }

    /// Kept once earned, even when what earned it is gone: a place on the Dojo board doesn't
    /// last past the season, the group weeks are only looked at for a while
    /// ([`super::HONOUR_WEEKS`]) and change when members come and go, a group and its river
    /// can go.
    pub const fn sticky(self) -> bool {
        matches!(
            self,
            Self::BlackBelt | Self::Sensei | Self::Yokozuna | Self::Sansho | Self::RiverCrew
        )
    }
}

/// How many ranked finishers a daily needs before its fastest tenth earns [`Stamp::Swift`].
pub const SWIFT_MIN_FINISHERS: i64 = 10;
/// Dailies without hints or checks for [`Stamp::Clean`].
pub const CLEAN_TARGET: i64 = 10;
/// Clicks within [`STORM_WINDOW_MS`] for [`Stamp::Storm`].
pub const STORM_TAPS: i64 = 15;
pub const STORM_WINDOW_MS: u32 = 10_000;
/// Bridges on the board before clearing it counts as starting over, for [`Stamp::Phoenix`].
pub const PHOENIX_BRIDGES: i64 = 5;
/// A time on the clock longer than this earns [`Stamp::Tortoise`].
pub const TORTOISE_MS: i64 = 3_600_000;
/// Three times of one difficulty this close together (in percent) earn [`Stamp::Metronome`].
pub const METRONOME_PERCENT: i64 = 3;
/// Starting a daily this soon after it came out earns [`Stamp::ZeroHour`]...
pub const ZERO_HOUR_MS: i64 = 60_000;
/// ...finishing it this shortly before the next one [`Stamp::OnTheBrink`].
pub const BRINK_MS: i64 = 600_000;
/// Finishing this close to the player above or below earns [`Stamp::PhotoFinish`].
pub const PHOTO_FINISH_MS: i64 = 500;
/// Ranked finishers a daily needs before finishing last earns [`Stamp::LanterneRouge`].
pub const LANTERNE_MIN_FINISHERS: i64 = 20;
/// Times one bridge is taken away again in a daily for [`Stamp::SecondThoughts`].
pub const SECOND_THOUGHTS: i64 = 10;
/// Other stamps for [`Stamp::Collector`].
pub const COLLECTOR_TARGET: i64 = 20;

/// What a daily's move log says about how it was solved, worked out when it's submitted
/// (see [`summarize`]) and kept with the attempt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// Bridges on the finished board, a double bridge counts twice.
    pub bridges: i64,
    pub islands: i64,
    /// Never took a bridge away: no undo, no restart, no click from a double bridge back to none.
    pub steady: bool,
    /// The most times one bridge was taken away again.
    pub dithers: i64,
    /// Cleared the board after building [`PHOENIX_BRIDGES`] bridges, and started over.
    pub cleared: bool,
    /// The most clicks within [`STORM_WINDOW_MS`].
    pub burst: i64,
}

/// Sums up the move log of a finished daily.
pub fn summarize(puzzle: &Puzzle, bridges: &[u8], log: &[MoveEvent]) -> Run {
    let mut counts: HashMap<u16, u8> = HashMap::new();
    let mut taken: HashMap<u16, i64> = HashMap::new();
    let mut on_board = 0;
    let mut most = 0;
    let mut run = Run {
        bridges: bridges.iter().map(|&b| i64::from(b)).sum(),
        islands: count(puzzle.islands.len()),
        steady: true,
        ..Run::default()
    };
    for e in log {
        let before = counts.insert(e.edge, e.count).unwrap_or(0);
        on_board += i64::from(e.count) - i64::from(before);
        most = most.max(on_board);
        if !e.tap {
            run.steady = false;
        }
        if e.count < before {
            run.steady = false;
            let n = taken.entry(e.edge).or_default();
            *n += 1;
            run.dithers = run.dithers.max(*n);
        }
        if on_board == 0 && most >= PHOENIX_BRIDGES {
            run.cleared = true;
        }
    }
    // Clicks in order of time: the longest stretch within the window
    let taps: Vec<u32> = log.iter().filter(|e| e.tap).map(|e| e.t).collect();
    let mut from = 0;
    for (to, &t) in taps.iter().enumerate() {
        while t.saturating_sub(taps[from]) >= STORM_WINDOW_MS {
            from += 1;
        }
        run.burst = run.burst.max(count(to - from + 1));
    }
    run
}

/// The part of the day (the player's own time) a daily was finished in, for [`Stamp::NightOwl`]
/// and [`Stamp::LunchBreak`]. Only these two are kept, not the time zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Daypart {
    /// Midnight to 4 am.
    Night,
    /// Noon to 2 pm.
    Lunch,
}

impl Daypart {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Night => "night",
            Self::Lunch => "lunch",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [Self::Night, Self::Lunch]
            .into_iter()
            .find(|d| d.id() == id)
    }

    /// The part of the day at `ms` for a browser whose clock is `utc_offset` minutes behind UTC
    /// (JavaScript's `getTimezoneOffset`), if it is one of the two.
    pub fn at(ms: i64, utc_offset: i32) -> Option<Self> {
        // Time zones run from UTC-12 to UTC+14
        if !(-14 * 60..=12 * 60).contains(&utc_offset) {
            return None;
        }
        let local = ms - i64::from(utc_offset) * 60_000;
        match local.rem_euclid(MS_PER_DAY) / 3_600_000 {
            0..=3 => Some(Self::Night),
            12..=13 => Some(Self::Lunch),
            _ => None,
        }
    }
}

/// One ranked daily of a player, as the stamps see it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Solve {
    pub day: i64,
    pub hints: i64,
    pub checks: i64,
    /// Place among the day's ranked finishers, 1 = fastest.
    pub place: i64,
    pub finishers: i64,
    /// The first of the day to finish (not necessarily the fastest).
    pub first_finish: bool,
    /// Final time including penalties.
    pub time_ms: i64,
    pub started_at: i64,
    pub finished_at: i64,
    /// How far the closest time above or below was, `None` alone on the board.
    pub gap_ms: Option<i64>,
    pub run: Run,
    pub daypart: Option<Daypart>,
}

/// What a player has done, everything a stamp depends on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// Sorted by day.
    pub solves: Vec<Solve>,
    /// Months the player finished first, see [`super::month_of`].
    pub champion_months: i64,
    /// Has a scored solver on the SAT Dojo board.
    pub dojo: bool,
    /// Place of the player's best solver on this season's Dojo board.
    pub dojo_rank: Option<u32>,
    /// The day the account was created.
    pub joined: i64,
    /// [`Stamp::sticky`] stamps already earned, they stay.
    pub kept: Vec<Stamp>,
    /// Weeks the player was the yokozuna of a group's banzuke, of the last ones that are over
    /// (see `model::honours`).
    pub yokozuna_weeks: i64,
    /// Weeks the player won one of a group's weekly awards.
    pub award_weeks: i64,
    /// Levels of a group's river (see `model::river`) the player laid planks of.
    pub river_levels: i64,
}

/// A stamp and how far along the player is, for the stamp book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StampProgress {
    pub stamp: Stamp,
    /// When it was earned (ms), `None` while it's still to get.
    pub earned_at: Option<i64>,
    pub progress: i64,
    pub target: i64,
    /// Players who have it, see [`share_percent`].
    #[serde(default)]
    pub holders: i64,
}

impl Record {
    /// Progress towards every stamp, in the order of [`Stamp::ALL`]: `(stamp, progress, target)`,
    /// earned when `progress >= target`. Medals, places and the fastest tenth only count once the
    /// day is over (`today` is still running), the ranking can change until then.
    pub fn progress(&self, today: i64) -> Vec<(Stamp, i64, i64)> {
        let all = &self.solves;
        let over = || all.iter().filter(|s| s.day < today);
        let any = |f: &dyn Fn(&Solve) -> bool| i64::from(all.iter().any(f));
        let any_over = |f: &dyn Fn(&Solve) -> bool| i64::from(over().any(f));
        let solved = count(all.len());
        let best_streak = best_streak(all);
        let golds = count(over().filter(|s| s.place == 1).count());
        let podiums = count(over().filter(|s| s.place <= 3).count());
        let clean = count(all.iter().filter(|s| s.hints == 0 && s.checks == 0).count());
        let bridges: i64 = all.iter().map(|s| s.run.bridges).sum();
        let islands: i64 = all.iter().map(|s| s.run.islands).sum();
        let podium_weekdays = count(
            over()
                .filter(|s| s.place <= 3)
                .map(|s| weekday(s.day))
                .collect::<HashSet<_>>()
                .len(),
        );
        let seasons = count(
            all.iter()
                .map(|s| civil(s.day).1 % 12 / 3)
                .collect::<HashSet<_>>()
                .len(),
        );
        let (moon, moon_days) = best_month(all);
        let date = |s: &Solve, month: u32, day: u32| {
            let (_, m, d) = civil(s.day);
            (m, d) == (month, day)
        };
        let (_, joined_m, joined_d) = civil(self.joined);
        let kept = |stamp: Stamp| self.kept.contains(&stamp);

        let mut progress: Vec<(Stamp, i64, i64)> = Stamp::ALL
            .into_iter()
            .map(|stamp| {
                let (progress, target) = match stamp {
                    Stamp::First => (solved, 1),
                    Stamp::Solved10 | Stamp::Solved50 | Stamp::Solved100 | Stamp::Solved365 => {
                        (solved, stamp.tier().unwrap_or_default())
                    }
                    Stamp::Bridges1000 => (bridges, 1000),
                    Stamp::Bridges10000 => (bridges, 10_000),
                    Stamp::Islands1000 => (islands, 1000),
                    Stamp::Streak7 | Stamp::Streak30 | Stamp::Streak100 => {
                        (best_streak, stamp.tier().unwrap_or_default())
                    }
                    Stamp::FullWeek => (best_week(all), 7),
                    Stamp::FullMoon => (moon, moon_days),
                    Stamp::FourSeasons => (seasons, 4),
                    Stamp::Oni => (any(&|s| daily_difficulty(s.day) == Difficulty::Oni), 1),
                    Stamp::Clean => (clean, CLEAN_TARGET),
                    Stamp::StoneBridge => {
                        (any(&|s| s.hints == 0 && s.checks == 0 && s.run.steady), 1)
                    }
                    Stamp::Storm => (any(&|s| s.run.burst >= STORM_TAPS), 1),
                    Stamp::Phoenix => (any_over(&|s| s.run.cleared && s.place <= 3), 1),
                    Stamp::Tortoise => (any(&|s| s.time_ms > TORTOISE_MS), 1),
                    Stamp::Metronome => (i64::from(metronome(all)), 1),
                    Stamp::EarlyBird => (any(&|s| s.first_finish), 1),
                    Stamp::ZeroHour => (
                        any(&|s| (0..ZERO_HOUR_MS).contains(&(s.started_at - s.day * MS_PER_DAY))),
                        1,
                    ),
                    Stamp::OnTheBrink => (
                        any(&|s| {
                            (1..=BRINK_MS).contains(&((s.day + 1) * MS_PER_DAY - s.finished_at))
                        }),
                        1,
                    ),
                    Stamp::NightOwl => (any(&|s| s.daypart == Some(Daypart::Night)), 1),
                    Stamp::LunchBreak => (any(&|s| s.daypart == Some(Daypart::Lunch)), 1),
                    Stamp::Swift => (
                        any_over(&|s| {
                            s.finishers >= SWIFT_MIN_FINISHERS && s.place * 10 <= s.finishers
                        }),
                        1,
                    ),
                    Stamp::Podium => (podiums, 1),
                    Stamp::Rainbow => (podium_weekdays, 7),
                    Stamp::PhotoFinish => (
                        any_over(&|s| s.gap_ms.is_some_and(|g| g <= PHOTO_FINISH_MS)),
                        1,
                    ),
                    Stamp::Gold => (golds, 1),
                    Stamp::HatTrick => (golds_in_a_row(all, today), 3),
                    Stamp::Gold10 => (golds, 10),
                    Stamp::Champion => (self.champion_months, 1),
                    Stamp::NewYear => (any(&|s| date(s, 1, 1)), 1),
                    Stamp::Tanabata => (any(&|s| date(s, 7, 7)), 1),
                    Stamp::Anniversary => (
                        any(&|s| s.day > self.joined && date(s, joined_m, joined_d)),
                        1,
                    ),
                    Stamp::Dojo => (i64::from(self.dojo), 1),
                    Stamp::BlackBelt => (
                        i64::from(self.dojo_rank.is_some_and(|r| r <= 3) || kept(stamp)),
                        1,
                    ),
                    Stamp::Sensei => (i64::from(self.dojo_rank == Some(1) || kept(stamp)), 1),
                    // Counted below, from the others
                    Stamp::Collector => (0, COLLECTOR_TARGET),
                    Stamp::SecondThoughts => (any(&|s| s.run.dithers >= SECOND_THOUGHTS), 1),
                    Stamp::Palindrome => (any(&|s| palindrome(s.time_ms)), 1),
                    Stamp::LanterneRouge => (
                        any_over(&|s| {
                            s.finishers >= LANTERNE_MIN_FINISHERS && s.place == s.finishers
                        }),
                        1,
                    ),
                    Stamp::LeapDay => (any(&|s| date(s, 2, 29)), 1),
                    Stamp::Yokozuna => (i64::from(self.yokozuna_weeks > 0 || kept(stamp)), 1),
                    Stamp::Sansho => (i64::from(self.award_weeks > 0 || kept(stamp)), 1),
                    Stamp::RiverCrew => (i64::from(self.river_levels > 0 || kept(stamp)), 1),
                };
                (stamp, progress.min(target), target)
            })
            .collect();
        let others = count(
            progress
                .iter()
                .filter(|(_, done, target)| done >= target)
                .count(),
        );
        if let Some(collector) = progress.iter_mut().find(|p| p.0 == Stamp::Collector) {
            collector.1 = others.min(COLLECTOR_TARGET);
        }
        progress
    }

    /// The stamps earned.
    pub fn earned(&self, today: i64) -> Vec<Stamp> {
        self.progress(today)
            .into_iter()
            .filter(|(_, progress, target)| progress >= target)
            .map(|(stamp, ..)| stamp)
            .collect()
    }
}

fn count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn best_streak(solves: &[Solve]) -> i64 {
    let days: Vec<i64> = solves.iter().map(|s| s.day).collect();
    super::streaks(&days, 0).1
}

/// Most dailies solved in one Monday to Sunday week, 7 means every difficulty in a row.
fn best_week(solves: &[Solve]) -> i64 {
    let mut best = 0;
    let mut week = None;
    let mut run = 0;
    for s in solves {
        let monday = s.day - count(weekday(s.day));
        run = if week == Some(monday) { run + 1 } else { 1 };
        week = Some(monday);
        best = best.max(run);
    }
    best
}

/// The calendar month closest to having every daily solved: `(solved, days)`, a complete one
/// first.
fn best_month(solves: &[Solve]) -> (i64, i64) {
    let mut months: HashMap<i64, i64> = HashMap::new();
    for s in solves {
        *months.entry(month_of(s.day)).or_default() += 1;
    }
    months
        .into_iter()
        .map(|(month, solved)| {
            let (from, to) = month_days(month);
            (solved, to - from + 1)
        })
        .max_by_key(|&(solved, days)| (solved == days, solved, -days))
        .unwrap_or((0, 30))
}

/// The most golds on consecutive days, of days that are over.
fn golds_in_a_row(solves: &[Solve], today: i64) -> i64 {
    let days: Vec<i64> = solves
        .iter()
        .filter(|s| s.day < today && s.place == 1)
        .map(|s| s.day)
        .collect();
    super::streaks(&days, 0).1
}

/// Three times of one difficulty within [`METRONOME_PERCENT`] of each other.
fn metronome(solves: &[Solve]) -> bool {
    let mut times: HashMap<Difficulty, Vec<i64>> = HashMap::new();
    for s in solves {
        times
            .entry(daily_difficulty(s.day))
            .or_default()
            .push(s.time_ms);
    }
    times.into_values().any(|mut times| {
        times.sort_unstable();
        times
            .windows(3)
            .any(|w| w[2] * 100 <= w[0] * (100 + METRONOME_PERCENT))
    })
}

/// A time whose digits on the clock read the same backwards, like `1:22.1` or `12:21`.
fn palindrome(time_ms: i64) -> bool {
    let digits: Vec<char> = format_duration(time_ms)
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    digits.len() >= 3 && digits.iter().eq(digits.iter().rev())
}

/// The share of players who have a stamp, as a whole percent: `"12"`, `"<1"` when only a few do.
/// `None` without players.
pub fn share_percent(holders: i64, players: i64) -> Option<String> {
    if players <= 0 {
        return None;
    }
    let holders = holders.clamp(0, players);
    // Rounded to the nearest percent
    let percent = (holders * 200 + players) / (players * 2);
    Some(if percent == 0 && holders > 0 {
        "<1".into()
    } else {
        percent.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LAUNCH_DAY, weekday};

    fn solve(day: i64, place: i64, finishers: i64) -> Solve {
        Solve {
            day,
            checks: 1,
            place,
            finishers,
            time_ms: 100_000,
            started_at: day * MS_PER_DAY + 3_600_000,
            finished_at: day * MS_PER_DAY + 3_700_000,
            ..Solve::default()
        }
    }

    fn tap(t: u32, edge: u16, count: u8) -> MoveEvent {
        MoveEvent {
            t,
            edge,
            count,
            tap: true,
        }
    }

    fn earns(solves: Vec<Solve>, stamp: Stamp) -> bool {
        let today = solves.iter().map(|s| s.day).max().unwrap_or(LAUNCH_DAY) + 1;
        Record {
            solves,
            ..Record::default()
        }
        .earned(today)
        .contains(&stamp)
    }

    #[test]
    fn ids_round_trip() {
        for stamp in Stamp::ALL {
            assert_eq!(Stamp::from_id(stamp.id()), Some(stamp));
        }
        assert_eq!(Stamp::from_id("nope"), None);
    }

    #[test]
    fn nothing_without_solves() {
        assert_eq!(Record::default().earned(LAUNCH_DAY), []);
    }

    #[test]
    fn streaks_weeks_and_counts() {
        let monday = (LAUNCH_DAY..LAUNCH_DAY + 7)
            .find(|&d| weekday(d) == 0)
            .unwrap();
        // Monday to Sunday of one week, then a gap and three more days
        let mut solves: Vec<Solve> = (monday..monday + 7).map(|d| solve(d, 5, 9)).collect();
        solves.extend((monday + 9..monday + 12).map(|d| solve(d, 5, 9)));
        let record = Record {
            solves,
            ..Record::default()
        };
        let today = monday + 30;
        let earned = record.earned(today);
        for stamp in [
            Stamp::First,
            Stamp::Solved10,
            Stamp::Streak7,
            Stamp::FullWeek,
            Stamp::Oni,
        ] {
            assert!(earned.contains(&stamp), "{stamp:?}");
        }
        for stamp in [Stamp::Solved50, Stamp::Streak30, Stamp::Clean, Stamp::Gold] {
            assert!(!earned.contains(&stamp), "{stamp:?}");
        }
        let progress = record.progress(today);
        let streak30 = progress.iter().find(|p| p.0 == Stamp::Streak30).unwrap();
        assert_eq!((streak30.1, streak30.2), (7, 30));
    }

    #[test]
    fn medals_only_count_once_the_day_is_over() {
        let day = LAUNCH_DAY + 10;
        let record = Record {
            solves: vec![solve(day, 1, 12)],
            ..Record::default()
        };
        let running = record.earned(day);
        assert!(!running.contains(&Stamp::Gold) && !running.contains(&Stamp::Swift));
        let over = record.earned(day + 1);
        for stamp in [Stamp::Gold, Stamp::Podium, Stamp::Swift] {
            assert!(over.contains(&stamp), "{stamp:?}");
        }
        // The fastest tenth needs enough players
        let small = Record {
            solves: vec![solve(day, 1, 9)],
            ..Record::default()
        };
        assert!(!small.earned(day + 1).contains(&Stamp::Swift));
    }

    #[test]
    fn runs_from_the_move_log() {
        let puzzle = Puzzle::new(3, 3, Vec::new());
        // A double bridge built in two clicks, nothing taken back
        let steady = summarize(
            &puzzle,
            &[2, 1],
            &[tap(500, 0, 1), tap(900, 0, 2), tap(1500, 1, 1)],
        );
        assert_eq!(
            (steady.bridges, steady.steady, steady.dithers),
            (3, true, 0)
        );
        assert_eq!(steady.burst, 3);
        // Clicked round to none, undone, and cleared after five bridges
        let mut log = vec![tap(0, 0, 1), tap(100, 0, 2), tap(200, 0, 0)];
        log.push(MoveEvent {
            tap: false,
            ..tap(300, 0, 2)
        });
        log.extend([tap(20_000, 1, 1), tap(20_100, 2, 2)]);
        log.extend((0..3).map(|e| MoveEvent {
            tap: false,
            ..tap(30_000, e, 0)
        }));
        let run = summarize(&puzzle, &[1], &log);
        assert!(!run.steady && run.cleared);
        assert_eq!((run.dithers, run.burst), (2, 3));
    }

    #[test]
    fn times_and_days() {
        assert!(palindrome(82_100)); // 1:22.1
        assert!(palindrome(741_000)); // 12:21
        assert!(!palindrome(83_100));
        assert!(!palindrome(1_000)); // 0:01.0
        let day = LAUNCH_DAY * MS_PER_DAY;
        // 1 am in Berlin in summer (UTC+2) is 11 pm UTC the day before
        assert_eq!(Daypart::at(day - 3_600_000, -120), Some(Daypart::Night));
        assert_eq!(Daypart::at(day + 12 * 3_600_000, 0), Some(Daypart::Lunch));
        assert_eq!(Daypart::at(day + 9 * 3_600_000, 0), None);
        assert_eq!(Daypart::at(day, 24 * 60), None);

        let at = |y, m, d| solve(crate::model::days_from_civil(y, m, d), 5, 9);
        assert!(earns(vec![at(2027, 7, 7)], Stamp::Tanabata));
        assert!(earns(vec![at(2028, 2, 29)], Stamp::LeapDay));
        assert!(earns(vec![at(2027, 1, 1)], Stamp::NewYear));
        assert!(!earns(vec![at(2027, 1, 2)], Stamp::NewYear));
        let seasons = vec![
            at(2027, 1, 5),
            at(2027, 4, 5),
            at(2027, 7, 5),
            at(2027, 10, 5),
        ];
        assert!(earns(seasons.clone(), Stamp::FourSeasons));
        assert!(!earns(seasons[..3].to_vec(), Stamp::FourSeasons));
        let february: Vec<Solve> = (1..=28).map(|d| at(2027, 2, d)).collect();
        assert!(earns(february.clone(), Stamp::FullMoon));
        assert!(!earns(february[1..].to_vec(), Stamp::FullMoon));

        let mut zero = solve(LAUNCH_DAY + 3, 5, 9);
        zero.started_at = (LAUNCH_DAY + 3) * MS_PER_DAY + 30_000;
        assert!(earns(vec![zero], Stamp::ZeroHour));
        let mut brink = solve(LAUNCH_DAY + 3, 5, 9);
        brink.finished_at = (LAUNCH_DAY + 4) * MS_PER_DAY - 60_000;
        assert!(earns(vec![brink], Stamp::OnTheBrink));
        assert!(!earns(vec![solve(LAUNCH_DAY + 3, 5, 9)], Stamp::OnTheBrink));
    }

    #[test]
    fn places_and_paces() {
        let day = LAUNCH_DAY + 10;
        let golds: Vec<Solve> = (day..day + 3).map(|d| solve(d, 1, 4)).collect();
        assert!(earns(golds.clone(), Stamp::HatTrick));
        assert!(!earns(vec![golds[0], golds[2]], Stamp::HatTrick));
        // Every weekday on the podium
        assert!(earns(
            (day..day + 7).map(|d| solve(d, 3, 4)).collect(),
            Stamp::Rainbow
        ));
        assert!(!earns(
            (day..day + 6).map(|d| solve(d, 3, 4)).collect(),
            Stamp::Rainbow
        ));
        assert!(earns(vec![solve(day, 20, 20)], Stamp::LanterneRouge));
        assert!(!earns(vec![solve(day, 19, 19)], Stamp::LanterneRouge));
        let close = Solve {
            gap_ms: Some(300),
            ..solve(day, 2, 5)
        };
        assert!(earns(vec![close], Stamp::PhotoFinish));
        // Three Mondays within 3 %
        let mondays: Vec<Solve> = [100_000, 102_000, 102_900]
            .into_iter()
            .enumerate()
            .map(|(i, time_ms)| Solve {
                time_ms,
                ..solve(day + 7 * i64::try_from(i).unwrap(), 5, 9)
            })
            .collect();
        assert!(earns(mondays.clone(), Stamp::Metronome));
        let mut off = mondays;
        off[2].time_ms = 104_000;
        assert!(!earns(off, Stamp::Metronome));
    }

    #[test]
    fn dojo_places_stay_and_collector_counts() {
        let record = Record {
            dojo: true,
            dojo_rank: Some(2),
            ..Record::default()
        };
        let earned = record.earned(LAUNCH_DAY);
        assert!(earned.contains(&Stamp::BlackBelt) && !earned.contains(&Stamp::Sensei));
        // Next season the place is gone, the stamp isn't
        let later = Record {
            kept: vec![Stamp::BlackBelt],
            ..Record::default()
        };
        assert_eq!(later.earned(LAUNCH_DAY), [Stamp::BlackBelt]);

        let busy = Record {
            solves: (LAUNCH_DAY..LAUNCH_DAY + 400)
                .map(|d| solve(d, 1, 12))
                .collect(),
            ..Record::default()
        };
        assert!(busy.earned(LAUNCH_DAY + 400).contains(&Stamp::Collector));
        let collector = busy
            .progress(LAUNCH_DAY + 400)
            .into_iter()
            .find(|p| p.0 == Stamp::Collector)
            .unwrap();
        assert_eq!(
            (collector.1, collector.2),
            (COLLECTOR_TARGET, COLLECTOR_TARGET)
        );
    }

    /// Group weeks earn a stamp each, which stays when the week is too long ago to be looked at.
    #[test]
    fn group_weeks_earn_stamps_that_stay() {
        let earned = |record: Record| record.earned(LAUNCH_DAY);
        assert_eq!(earned(Record::default()), []);
        assert_eq!(
            earned(Record {
                yokozuna_weeks: 1,
                ..Record::default()
            }),
            [Stamp::Yokozuna]
        );
        assert_eq!(
            earned(Record {
                award_weeks: 2,
                ..Record::default()
            }),
            [Stamp::Sansho]
        );
        assert_eq!(
            earned(Record {
                kept: vec![Stamp::Yokozuna, Stamp::Sansho],
                ..Record::default()
            }),
            [Stamp::Yokozuna, Stamp::Sansho]
        );
        assert!(Stamp::Yokozuna.sticky() && Stamp::Sansho.sticky());
        assert!(!Stamp::Yokozuna.secret());
    }

    /// Planks of a completed level of a group's river earn a stamp, which stays when the group
    /// is gone.
    #[test]
    fn a_river_crossed_together_earns_a_stamp_that_stays() {
        let earned = |record: Record| record.earned(LAUNCH_DAY);
        assert_eq!(earned(Record::default()), []);
        assert_eq!(
            earned(Record {
                river_levels: 2,
                ..Record::default()
            }),
            [Stamp::RiverCrew]
        );
        assert_eq!(
            earned(Record {
                kept: vec![Stamp::RiverCrew],
                ..Record::default()
            }),
            [Stamp::RiverCrew]
        );
        assert!(Stamp::RiverCrew.sticky() && !Stamp::RiverCrew.secret());
        assert_eq!(Stamp::from_id("river_crew"), Some(Stamp::RiverCrew));
    }

    #[test]
    fn kanji_are_distinct() {
        let kanji: HashSet<&str> = Stamp::ALL
            .into_iter()
            .filter(|s| s.tier().is_none())
            .map(Stamp::kanji)
            .collect();
        // Only tiers share a kanji
        assert_eq!(
            kanji.len(),
            Stamp::ALL.iter().filter(|s| s.tier().is_none()).count()
        );
    }

    #[test]
    fn shares_of_players() {
        assert_eq!(share_percent(1, 0), None);
        assert_eq!(share_percent(0, 7).as_deref(), Some("0"));
        assert_eq!(share_percent(1, 3).as_deref(), Some("33"));
        assert_eq!(share_percent(1, 8).as_deref(), Some("13"));
        assert_eq!(share_percent(1, 500).as_deref(), Some("<1"));
        assert_eq!(share_percent(5, 5).as_deref(), Some("100"));
    }
}
