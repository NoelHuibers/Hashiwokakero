//! The stamps as achievements of Apple's Game Center and Google Play Games, and the leaderboards
//! next to them (app/README.md, Game achievements). The server decides what a player has, the
//! app reports it to the platform: these are the ids, points and step counts both sides use.
//!
//! The upload tooling (store/achievements) reads them from `store/achievements/model.json`,
//! which a test below writes and keeps in step with this file.

use super::Stamp;

/// The most points all achievements of a game may have on Game Center (Play allows 2000).
pub const MAX_POINTS: u32 = 1000;
/// The most points one achievement may have on Game Center (Play allows 200).
pub const MAX_POINTS_EACH: u32 = 100;
/// The most steps an incremental achievement may have on Play.
pub const MAX_STEPS: u32 = 10_000;

impl Stamp {
    /// The achievement's id: Game Center's vendor identifier, and the key of
    /// `app/achievements-play.json` for the id Play gave it. `stamp_` and [`Stamp::id`]. Never
    /// changes once uploaded: both platforms keep an achievement's id for good.
    pub const fn achievement_id(self) -> &'static str {
        match self {
            Self::First => "stamp_first",
            Self::Solved10 => "stamp_solved_10",
            Self::Solved50 => "stamp_solved_50",
            Self::Solved100 => "stamp_solved_100",
            Self::Solved365 => "stamp_solved_365",
            Self::Bridges1000 => "stamp_bridges_1000",
            Self::Bridges10000 => "stamp_bridges_10000",
            Self::Islands1000 => "stamp_islands_1000",
            Self::Streak7 => "stamp_streak_7",
            Self::Streak30 => "stamp_streak_30",
            Self::Streak100 => "stamp_streak_100",
            Self::FullWeek => "stamp_full_week",
            Self::FullMoon => "stamp_full_moon",
            Self::FourSeasons => "stamp_four_seasons",
            Self::Oni => "stamp_oni",
            Self::Clean => "stamp_clean",
            Self::StoneBridge => "stamp_stone_bridge",
            Self::Storm => "stamp_storm",
            Self::Phoenix => "stamp_phoenix",
            Self::Tortoise => "stamp_tortoise",
            Self::Metronome => "stamp_metronome",
            Self::EarlyBird => "stamp_early_bird",
            Self::ZeroHour => "stamp_zero_hour",
            Self::OnTheBrink => "stamp_on_the_brink",
            Self::NightOwl => "stamp_night_owl",
            Self::LunchBreak => "stamp_lunch_break",
            Self::Swift => "stamp_swift",
            Self::Podium => "stamp_podium",
            Self::Rainbow => "stamp_rainbow",
            Self::PhotoFinish => "stamp_photo_finish",
            Self::Gold => "stamp_gold",
            Self::HatTrick => "stamp_hat_trick",
            Self::Gold10 => "stamp_gold_10",
            Self::Champion => "stamp_champion",
            Self::NewYear => "stamp_new_year",
            Self::Tanabata => "stamp_tanabata",
            Self::Anniversary => "stamp_anniversary",
            Self::Dojo => "stamp_dojo",
            Self::BlackBelt => "stamp_black_belt",
            Self::Sensei => "stamp_sensei",
            Self::Collector => "stamp_collector",
            Self::SecondThoughts => "stamp_second_thoughts",
            Self::Palindrome => "stamp_palindrome",
            Self::LanterneRouge => "stamp_lanterne_rouge",
            Self::LeapDay => "stamp_leap_day",
            Self::Yokozuna => "stamp_yokozuna",
            Self::Sansho => "stamp_sansho",
            Self::RiverCrew => "stamp_river_crew",
            Self::Unknown => "stamp_unknown",
        }
    }

    /// The achievement's points on both platforms, by how long or how lucky it takes: multiples
    /// of 5 up to [`MAX_POINTS_EACH`], 855 of the [`MAX_POINTS`] together, the rest kept for
    /// stamps to come. Points can't be changed once an achievement is live.
    pub const fn points(self) -> u32 {
        match self {
            Self::First
            | Self::Tortoise
            | Self::NightOwl
            | Self::LunchBreak
            | Self::SecondThoughts => 5,
            Self::Solved10
            | Self::Bridges1000
            | Self::Islands1000
            | Self::Streak7
            | Self::Oni
            | Self::Clean
            | Self::Storm
            | Self::ZeroHour
            | Self::OnTheBrink
            | Self::Podium
            | Self::PhotoFinish
            | Self::NewYear
            | Self::Tanabata
            | Self::Palindrome
            | Self::LanterneRouge => 10,
            Self::Solved50
            | Self::FourSeasons
            | Self::StoneBridge
            | Self::Metronome
            | Self::EarlyBird
            | Self::Swift
            | Self::Dojo
            | Self::LeapDay
            | Self::Sansho
            | Self::RiverCrew => 15,
            Self::Solved100
            | Self::FullWeek
            | Self::Phoenix
            | Self::Gold
            | Self::Anniversary
            | Self::Yokozuna => 20,
            Self::Bridges10000 | Self::Streak30 | Self::BlackBelt | Self::Collector => 25,
            Self::FullMoon | Self::Rainbow | Self::HatTrick => 30,
            Self::Solved365 | Self::Gold10 | Self::Sensei => 40,
            Self::Streak100 | Self::Champion => 50,
            Self::Unknown => 0,
        }
    }

    /// For a stamp that counts up to a fixed target (dailies solved, bridges built, days in a
    /// row...), the target: an incremental achievement on Play, a percentage on Game Center,
    /// with the progress of the stamp book. `None` for one that is earned at once, and for
    /// [`Stamp::FullMoon`], whose target is the length of a month.
    pub const fn steps(self) -> Option<u32> {
        match self {
            Self::FourSeasons => Some(4),
            Self::HatTrick => Some(3),
            Self::Streak7 | Self::FullWeek | Self::Rainbow => Some(7),
            Self::Solved10 | Self::Clean | Self::Gold10 => Some(10),
            Self::Collector => Some(20),
            Self::Streak30 => Some(30),
            Self::Solved50 => Some(50),
            Self::Solved100 | Self::Streak100 => Some(100),
            Self::Solved365 => Some(365),
            Self::Bridges1000 | Self::Islands1000 => Some(1000),
            Self::Bridges10000 => Some(10_000),
            _ => None,
        }
    }
}

/// A leaderboard of Game Center and Play Games, scored from what the server knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamesLeaderboard {
    /// Today's daily: the time in ms, penalties included, lower is better. On Game Center a
    /// leaderboard that starts over every day at midnight UTC, with the puzzle day
    /// ([`super::day_of`]). Not on Play: its daily leaderboards start over at 07:00 UTC, so
    /// one of its days would mix the times of two puzzles, and its all-time board would put
    /// Monday's easy times ahead of every other day's.
    DailyTime,
    /// Dailies solved, all time.
    PuzzlesSolved,
    /// The longest streak of days in a row, all time.
    BestStreak,
}

impl GamesLeaderboard {
    pub const ALL: [Self; 3] = [Self::DailyTime, Self::PuzzlesSolved, Self::BestStreak];

    /// Game Center's vendor identifier, and the key of `app/achievements-play.json`.
    pub const fn id(self) -> &'static str {
        match self {
            Self::DailyTime => "daily_time",
            Self::PuzzlesSolved => "puzzles_solved",
            Self::BestStreak => "best_streak",
        }
    }

    /// The kanji on its icon: 時 time, 解 solved, 連 in a row (the streak stamps').
    pub const fn kanji(self) -> &'static str {
        match self {
            Self::DailyTime => "時",
            Self::PuzzlesSolved => "解",
            Self::BestStreak => "連",
        }
    }

    pub const fn lower_is_better(self) -> bool {
        matches!(self, Self::DailyTime)
    }

    /// Whether Play Games has it (see [`GamesLeaderboard::DailyTime`]).
    pub const fn on_play(self) -> bool {
        !matches!(self, Self::DailyTime)
    }

    /// The score Game Center gets for `value`: it has no format for milliseconds, the daily's
    /// time goes in hundredths of a second (`ELAPSED_TIME_CENTISECOND`). Play takes `value` as
    /// it is (milliseconds are its `TIME_DURATION`).
    pub const fn apple_score(self, value: i64) -> i64 {
        match self {
            Self::DailyTime => value.div_euclid(10),
            Self::PuzzlesSolved | Self::BestStreak => value,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::model::{LAUNCH_DAY, MS_PER_DAY, Record, Solve, StampProgress};

    /// `store/achievements/model.json`, as the upload tooling reads it.
    fn model() -> String {
        let achievements: Vec<serde_json::Value> = Stamp::ALL
            .into_iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.achievement_id(),
                    "stamp": s.id(),
                    "kanji": s.kanji(),
                    "tier": s.tier(),
                    "points": s.points(),
                    "steps": s.steps(),
                    "secret": s.secret(),
                })
            })
            .collect();
        let leaderboards: Vec<serde_json::Value> = GamesLeaderboard::ALL
            .into_iter()
            .map(|b| {
                serde_json::json!({
                    "id": b.id(),
                    "kanji": b.kanji(),
                    "lower_is_better": b.lower_is_better(),
                    "play": b.on_play(),
                })
            })
            .collect();
        let json = serde_json::json!({
            "max_points": MAX_POINTS,
            "achievements": achievements,
            "leaderboards": leaderboards,
        });
        format!("{}\n", serde_json::to_string_pretty(&json).unwrap())
    }

    /// The checked-in model is this file's: a stamp, its points or steps changed here and not
    /// there would upload something else than the app reports. `UPDATE_ACHIEVEMENTS=1` writes it.
    #[test]
    fn the_checked_in_model_is_up_to_date() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/store/achievements/model.json");
        let expected = model();
        if std::env::var_os("UPDATE_ACHIEVEMENTS").is_some() {
            std::fs::write(path, &expected).unwrap();
        }
        let actual = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            actual == expected,
            "store/achievements/model.json is out of date: run \
             `UPDATE_ACHIEVEMENTS=1 cargo test --no-default-features --features ssr -p hashi \
             --lib achievements` and commit it"
        );
    }

    #[test]
    fn ids_are_unique_and_stable() {
        let ids: HashSet<&str> = Stamp::ALL.into_iter().map(Stamp::achievement_id).collect();
        assert_eq!(ids.len(), Stamp::ALL.len());
        for stamp in Stamp::ALL {
            let id = stamp.achievement_id();
            assert_eq!(id, format!("stamp_{}", stamp.id()));
            // Game Center: up to 100 single-byte characters, letters, digits, `_` and `.`
            assert!(id.len() <= 100, "{id}");
            assert!(
                id.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.'),
                "{id}"
            );
        }
        // Uploaded ids are for good: these never change
        assert_eq!(Stamp::First.achievement_id(), "stamp_first");
        assert_eq!(Stamp::Solved100.achievement_id(), "stamp_solved_100");
        assert_eq!(Stamp::RiverCrew.achievement_id(), "stamp_river_crew");
        let boards: HashSet<&str> = GamesLeaderboard::ALL
            .into_iter()
            .map(GamesLeaderboard::id)
            .collect();
        assert_eq!(
            boards,
            HashSet::from(["daily_time", "puzzles_solved", "best_streak"])
        );
        assert!(boards.is_disjoint(&ids));
    }

    #[test]
    fn points_fit_both_platforms() {
        let total: u32 = Stamp::ALL.into_iter().map(Stamp::points).sum();
        assert!(total <= MAX_POINTS, "{total} points");
        // Room left for stamps to come
        assert_eq!(total, 855);
        for stamp in Stamp::ALL {
            let p = stamp.points();
            // Play: multiples of 5, at least 5; Game Center: at most 100 each
            assert!(
                p.is_multiple_of(5) && (5..=MAX_POINTS_EACH).contains(&p),
                "{stamp:?}: {p}"
            );
        }
        // A tier is worth more than the one before it
        for tiers in [
            &[
                Stamp::Solved10,
                Stamp::Solved50,
                Stamp::Solved100,
                Stamp::Solved365,
            ][..],
            &[Stamp::Streak7, Stamp::Streak30, Stamp::Streak100],
            &[Stamp::Bridges1000, Stamp::Bridges10000],
            &[Stamp::Gold, Stamp::Gold10],
            &[Stamp::Dojo, Stamp::BlackBelt, Stamp::Sensei],
        ] {
            assert!(
                tiers.windows(2).all(|w| w[0].points() < w[1].points()),
                "{tiers:?}"
            );
        }
        assert_eq!(Stamp::Unknown.points(), 0);
    }

    fn solve(day: i64, place: i64) -> Solve {
        Solve {
            day,
            place,
            finishers: 12,
            time_ms: 100_000,
            started_at: day * MS_PER_DAY + 3_600_000,
            finished_at: day * MS_PER_DAY + 3_700_000,
            ..Solve::default()
        }
    }

    /// The stamp book's progress of `record`, as the server hands it out.
    fn book(record: &Record, today: i64) -> Vec<StampProgress> {
        record
            .progress(today)
            .into_iter()
            .map(|(stamp, progress, target)| StampProgress {
                stamp,
                earned_at: None,
                progress,
                target,
                holders: 0,
            })
            .collect()
    }

    /// The steps are the stamp book's targets: the progress the server sends is what the app
    /// reports. Stamps earned at once have a target of 1, and the full moon's depends on the
    /// month.
    #[test]
    fn steps_are_the_stamp_books_targets() {
        let records = [
            Record::default(),
            Record {
                solves: (LAUNCH_DAY..LAUNCH_DAY + 400)
                    .map(|d| solve(d, 1))
                    .collect(),
                ..Record::default()
            },
        ];
        for record in &records {
            let book = book(record, LAUNCH_DAY + 400);
            assert_eq!(book.iter().map(|p| p.stamp).collect::<Vec<_>>(), Stamp::ALL);
            for p in book {
                match p.stamp.steps() {
                    Some(steps) => {
                        assert_eq!(i64::from(steps), p.target, "{:?}", p.stamp);
                        assert!((2..=MAX_STEPS).contains(&steps), "{:?}", p.stamp);
                    }
                    None if p.stamp == Stamp::FullMoon => assert!((28..=31).contains(&p.target)),
                    None => assert_eq!(p.target, 1, "{:?}", p.stamp),
                }
            }
        }
        assert_eq!(Stamp::Unknown.steps(), None);
    }

    #[test]
    fn every_stamp_is_covered() {
        assert!(!Stamp::ALL.contains(&Stamp::Unknown));
        let json: serde_json::Value = serde_json::from_str(&model()).unwrap();
        let ids: Vec<&str> = json["achievements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            Stamp::ALL.map(Stamp::achievement_id),
            "every stamp, in the order of the book"
        );
        // Hidden on both platforms are the stamp book's secrets
        let hidden: Vec<Stamp> = Stamp::ALL.into_iter().filter(|s| s.secret()).collect();
        assert_eq!(
            hidden,
            [
                Stamp::SecondThoughts,
                Stamp::Palindrome,
                Stamp::LanterneRouge,
                Stamp::LeapDay
            ]
        );
    }

    #[test]
    fn leaderboards() {
        use GamesLeaderboard::{BestStreak, DailyTime, PuzzlesSolved};
        assert!(DailyTime.lower_is_better() && !DailyTime.on_play());
        assert!(!PuzzlesSolved.lower_is_better() && PuzzlesSolved.on_play());
        assert!(!BestStreak.lower_is_better() && BestStreak.on_play());
        // 1:23.456 is 8345 hundredths on Game Center
        assert_eq!(DailyTime.apple_score(83_456), 8345);
        assert_eq!(PuzzlesSolved.apple_score(42), 42);
        assert_eq!(BestStreak.apple_score(7), 7);
    }
}
