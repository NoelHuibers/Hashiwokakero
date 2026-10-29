//! What the app reports to the phone's games service, Game Center on iOS and Google Play Games
//! on Android: the arguments of the commands `games_sign_in`, `games_report` and `games_show` of
//! the app's plugin (`HashiPlugin.swift`, `HashiPlugin.kt`), sent by `native::games_*`.
//!
//! The server stays the source of truth: the page reports what the server says the player has
//! (a stamp earned or how far along it is, a score), the platforms only show it. Both take a
//! report again as often as it comes and keep the highest: an achievement never goes back, a
//! leaderboard keeps the best score. The ids are the platform's own, Apple's vendor ids or the
//! ids Play generates (`app/achievements-play.json`, built into the page): [`report`] makes the
//! report of the server's statistics for the phone's [`Platform`].

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::model::{GamesLeaderboard, Stamp, StampProgress, UserStats};

/// An achievement as the server sees it: `earned`, or `steps` of `target` done for one that
/// counts (solved 100 dailies), `None` for one that is only earned or not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Achievement {
    pub(crate) id: String,
    pub(crate) steps: Option<u32>,
    pub(crate) target: Option<u32>,
    pub(crate) earned: bool,
}

/// A score for the leaderboard `id`, in the board's own unit (the catalogue converts it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Score {
    pub(crate) id: String,
    pub(crate) value: i64,
}

/// The arguments of `games_report`, only what there is to report, in the form both platforms
/// take without further checks:
///
/// - a counting achievement has `1 ..= target` steps, all of them once it is earned (a streak
///   stamp stays earned when the streak breaks, its steps are the best streak's);
/// - one without a target, or with a target of 0, has no steps and is only sent once earned;
/// - nothing without an id, an achievement with nothing done yet, or a negative score.
///
/// iOS makes `percentComplete` of the steps, Android sets them (`setSteps`) or unlocks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct Report {
    pub(crate) achievements: Vec<Achievement>,
    pub(crate) scores: Vec<Score>,
}

impl Report {
    /// The report of `achievements` and `scores`, `None` when nothing is left to send.
    pub(crate) fn new(
        achievements: impl IntoIterator<Item = Achievement>,
        scores: impl IntoIterator<Item = Score>,
    ) -> Option<Self> {
        let achievements: Vec<_> = achievements
            .into_iter()
            .filter(|a| !a.id.is_empty())
            .filter_map(|a| match a.target.filter(|&t| t > 0) {
                Some(target) => {
                    let steps = if a.earned {
                        target
                    } else {
                        a.steps.unwrap_or_default().min(target)
                    };
                    (steps > 0).then_some(Achievement {
                        steps: Some(steps),
                        target: Some(target),
                        earned: steps == target,
                        ..a
                    })
                }
                None => a.earned.then_some(Achievement {
                    steps: None,
                    target: None,
                    ..a
                }),
            })
            .collect();
        let scores: Vec<_> = scores
            .into_iter()
            .filter(|s| !s.id.is_empty() && s.value >= 0)
            .collect();
        (!achievements.is_empty() || !scores.is_empty()).then_some(Self {
            achievements,
            scores,
        })
    }
}

/// What `games_sign_in` answers: whether the phone has the service (`available`: always on iOS,
/// on Android once the build has Play Games' project id), and whether a player is signed in, the
/// only case in which the platforms take reports. Without an answer (outside the app, a failed
/// call) neither.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Status {
    pub(crate) available: bool,
    pub(crate) signed_in: bool,
}

/// The system's own screens `games_show` opens: Game Center's dashboard or Play Games'
/// achievements and leaderboards, which also sign a player in first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Board {
    Achievements,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "for a button on the ranks once the leaderboards are reported"
        )
    )]
    Leaderboards,
}

/// The arguments of `games_show`.
#[derive(Debug, Serialize)]
pub(crate) struct Show {
    pub(crate) kind: Board,
}

/// The stamp book's button (`native::games_show`): `show` opens the service's screens, after
/// its sign-in when nobody is signed in yet. Once it answered, the sign-in is asked again
/// (`ask`): a player signed in now who wasn't `before` has nothing reported yet, `signed_in`
/// gets the new answer to report everything, once. The answer for the next tap, or `show`'s
/// error (nobody signed in, no service).
pub(crate) async fn shown<E, A: Future<Output = Status>>(
    before: Status,
    show: impl Future<Output = Result<(), E>>,
    ask: impl FnOnce() -> A,
    signed_in: impl FnOnce(Status),
) -> Result<Status, E> {
    show.await?;
    if before.signed_in {
        return Ok(before);
    }
    let now = ask().await;
    if now.signed_in {
        signed_in(now);
    }
    Ok(now)
}

/// The games service of the app's `platform` (`api::app`), as its owner names it. The page
/// shows it on the button that opens it, unchanged in every language.
pub(crate) fn service(platform: &str) -> Option<&'static str> {
    match platform {
        "ios" => Some("Game Center"),
        "android" => Some("Google Play Games"),
        _ => None,
    }
}

/// The phone's games service, by the ids it knows the achievements and leaderboards by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    /// iOS: Apple's vendor ids, `Stamp::achievement_id` and `GamesLeaderboard::id`.
    GameCenter,
    /// Android: the ids Play generated for them, from `app/achievements-play.json`.
    PlayGames,
}

impl Platform {
    /// The service of the app's `platform` (`api::app`), `None` outside the app.
    pub(crate) fn of(platform: &str) -> Option<Self> {
        match platform {
            "ios" => Some(Self::GameCenter),
            "android" => Some(Self::PlayGames),
            _ => None,
        }
    }

    /// The id of `stamp`'s achievement, `None` for one Play doesn't have (yet), and for a stamp
    /// of a newer server that this page doesn't know.
    fn achievement(self, stamp: Stamp, play: &PlayIds) -> Option<&str> {
        if stamp == Stamp::Unknown {
            return None;
        }
        match self {
            Self::GameCenter => Some(stamp.achievement_id()),
            Self::PlayGames => play.achievement(stamp),
        }
    }

    /// The id of `board`, `None` for one Play doesn't have: never the daily's time
    /// (`GamesLeaderboard::on_play`), the others once they are in the file.
    fn leaderboard(self, board: GamesLeaderboard, play: &PlayIds) -> Option<&str> {
        match self {
            Self::GameCenter => Some(board.id()),
            Self::PlayGames => board.on_play().then(|| play.leaderboard(board)).flatten(),
        }
    }

    /// `value` in `board`'s unit on this platform: the daily's time in hundredths of a second
    /// on Game Center, in milliseconds on Play.
    const fn score(self, board: GamesLeaderboard, value: i64) -> i64 {
        match self {
            Self::GameCenter => board.apple_score(value),
            Self::PlayGames => value,
        }
    }
}

/// `app/achievements-play.json`: the ids Play generated, by `Stamp::achievement_id` and
/// `GamesLeaderboard::id` (its `project_id` is the Android build's, see app/README.md).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct PlayIds {
    achievements: HashMap<String, String>,
    leaderboards: HashMap<String, String>,
}

/// The file as it was when the page was built: a new id needs a new app version anyway.
static PLAY_IDS: LazyLock<PlayIds> =
    LazyLock::new(|| PlayIds::parse(include_str!("../../app/achievements-play.json")));

impl PlayIds {
    /// The ids of `json`; none when it doesn't parse (a test keeps the checked-in file valid).
    fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_else(|e| {
            leptos::logging::warn!("app/achievements-play.json: {e}");
            Self::default()
        })
    }

    /// The ids built into the page.
    pub(crate) fn embedded() -> &'static Self {
        &PLAY_IDS
    }

    fn achievement(&self, stamp: Stamp) -> Option<&str> {
        Self::id(&self.achievements, stamp.achievement_id())
    }

    fn leaderboard(&self, board: GamesLeaderboard) -> Option<&str> {
        Self::id(&self.leaderboards, board.id())
    }

    fn id<'a>(ids: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
        ids.get(key).map(String::as_str).filter(|id| !id.is_empty())
    }
}

/// The report of the server's statistics of the player (`my_stats`) for `platform`: every
/// stamp of the book, earned or how far along (the steps toward `Stamp::steps`), and the
/// leaderboards' scores, the dailies solved, the best streak, and the time of today's daily
/// if the player solved it ranked (the last day of `recent` is the server's today). Without a
/// score of 0: nobody goes on a leaderboard for nothing. `None` when nothing is left to send.
pub(crate) fn report(platform: Platform, play: &PlayIds, stats: &UserStats) -> Option<Report> {
    let achievements = stats.stamps.iter().filter_map(|p| {
        let id = platform.achievement(p.stamp, play)?;
        Some(achievement(id, p))
    });
    let today = stats
        .recent
        .last()
        .and_then(|day| day.time_ms)
        .unwrap_or_default();
    let scores = [
        (GamesLeaderboard::DailyTime, today),
        (GamesLeaderboard::PuzzlesSolved, stats.solved),
        (GamesLeaderboard::BestStreak, stats.best_streak),
    ]
    .into_iter()
    .filter(|&(_, value)| value > 0)
    .filter_map(|(board, value)| {
        Some(Score {
            id: platform.leaderboard(board, play)?.to_owned(),
            value: platform.score(board, value),
        })
    });
    Report::new(achievements, scores)
}

/// The achievement `id` of a stamp of the book: its progress as steps of `Stamp::steps` for
/// one that counts, else only whether it is earned.
fn achievement(id: &str, progress: &StampProgress) -> Achievement {
    let target = progress.stamp.steps();
    Achievement {
        id: id.to_owned(),
        steps: target.map(|_| u32::try_from(progress.progress.max(0)).unwrap_or(u32::MAX)),
        target,
        earned: progress.earned_at.is_some(),
    }
}

/// What this page reported already, so that the next report has only what changed since: a
/// stamp earned or further along, a new score. The platforms take the same report again, this
/// only keeps them from getting all 48 stamps after every daily. A report the plugin didn't
/// take is forgotten again ([`Reported::failed`]), so that the next one sends it anew.
#[derive(Debug, Default)]
pub(crate) struct Reported {
    achievements: HashMap<String, Achievement>,
    scores: HashMap<String, i64>,
}

impl Reported {
    /// The part of `report` that wasn't reported yet, noted as reported. `None`: nothing new.
    pub(crate) fn fresh(&mut self, report: Report) -> Option<Report> {
        let achievements: Vec<_> = report
            .achievements
            .into_iter()
            .filter(|a| self.achievements.get(&a.id) != Some(a))
            .collect();
        let scores: Vec<_> = report
            .scores
            .into_iter()
            .filter(|s| self.scores.get(&s.id) != Some(&s.value))
            .collect();
        for a in &achievements {
            self.achievements.insert(a.id.clone(), a.clone());
        }
        for s in &scores {
            self.scores.insert(s.id.clone(), s.value);
        }
        Report::new(achievements, scores)
    }

    /// Forgets `report`, which [`Reported::fresh`] gave and the plugin didn't take (offline, a
    /// platform's error): what of it is still noted, not what a later report noted since.
    pub(crate) fn failed(&mut self, report: &Report) {
        for a in &report.achievements {
            if self.achievements.get(&a.id) == Some(a) {
                self.achievements.remove(&a.id);
            }
        }
        for s in &report.scores {
            if self.scores.get(&s.id) == Some(&s.value) {
                self.scores.remove(&s.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn achievement(id: &str, steps: Option<u32>, target: Option<u32>, earned: bool) -> Achievement {
        Achievement {
            id: id.to_owned(),
            steps,
            target,
            earned,
        }
    }

    fn score(id: &str, value: i64) -> Score {
        Score {
            id: id.to_owned(),
            value,
        }
    }

    /// The JSON both plugins parse (`GamesReportArgs` in Kotlin and Swift): the contract's names,
    /// `null` for the steps of an achievement that doesn't count.
    #[test]
    fn a_report_is_the_json_the_plugins_parse() {
        let report = Report::new(
            [
                achievement("stamp_solved_100", Some(42), Some(100), false),
                achievement("stamp_first", None, None, true),
            ],
            [score("daily_time", 83_456)],
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&report).unwrap(),
            json!({
                "achievements": [
                    { "id": "stamp_solved_100", "steps": 42, "target": 100, "earned": false },
                    { "id": "stamp_first", "steps": null, "target": null, "earned": true },
                ],
                "scores": [{ "id": "daily_time", "value": 83_456 }],
            })
        );
    }

    #[test]
    fn counting_achievements_have_steps_within_their_target() {
        let report = Report::new(
            [
                // Past the target: the target
                achievement("over", Some(130), Some(100), false),
                // Earned with fewer steps (a broken streak): all of them, on both platforms
                achievement("streak", Some(3), Some(7), true),
                // Exactly there without the server's mark yet: earned all the same
                achievement("there", Some(10), Some(10), false),
                // Earned without steps
                achievement("no_steps", None, Some(4), true),
            ],
            [],
        )
        .unwrap();
        assert_eq!(
            report.achievements,
            [
                achievement("over", Some(100), Some(100), true),
                achievement("streak", Some(7), Some(7), true),
                achievement("there", Some(10), Some(10), true),
                achievement("no_steps", Some(4), Some(4), true),
            ]
        );
    }

    /// Play refuses `setSteps` with 0 steps, and an unearned achievement without steps has
    /// nothing to say.
    #[test]
    fn nothing_done_is_not_reported() {
        assert_eq!(
            Report::new(
                [
                    achievement("zero", Some(0), Some(10), false),
                    achievement("none", None, Some(10), false),
                    achievement("locked", None, None, false),
                    achievement("locked_with_steps", Some(3), None, false),
                    achievement("", None, None, true),
                ],
                [score("", 5), score("negative", -1)],
            ),
            None
        );
        assert_eq!(Report::new([], []), None);
    }

    #[test]
    fn achievements_without_a_target_are_unlocked_only() {
        let report = Report::new(
            [
                achievement("zero_target", Some(5), Some(0), true),
                achievement("plain", Some(1), None, true),
            ],
            [],
        )
        .unwrap();
        assert_eq!(
            report.achievements,
            [
                achievement("zero_target", None, None, true),
                achievement("plain", None, None, true),
            ]
        );
    }

    #[test]
    fn scores_alone_are_a_report() {
        let report = Report::new([], [score("puzzles_solved", 0), score("best_streak", 12)]);
        assert_eq!(
            report.map(|r| r.scores),
            Some(vec![score("puzzles_solved", 0), score("best_streak", 12)])
        );
    }

    /// The plugins' answer to `games_sign_in`; anything else (an older plugin, no answer) is a
    /// service that isn't there.
    #[test]
    fn the_status_comes_from_the_plugins_answer() {
        let status: Status =
            serde_json::from_value(json!({ "available": true, "signedIn": true })).unwrap();
        assert_eq!(
            status,
            Status {
                available: true,
                signed_in: true
            }
        );
        let status: Status = serde_json::from_value(json!({ "available": true })).unwrap();
        assert!(status.available && !status.signed_in);
        assert_eq!(
            serde_json::from_value::<Status>(json!({})).unwrap(),
            Status::default()
        );
    }

    #[test]
    fn the_boards_are_the_kinds_the_plugins_know() {
        assert_eq!(
            serde_json::to_value(Show {
                kind: Board::Achievements
            })
            .unwrap(),
            json!({ "kind": "achievements" })
        );
        assert_eq!(
            serde_json::to_value(Show {
                kind: Board::Leaderboards
            })
            .unwrap(),
            json!({ "kind": "leaderboards" })
        );
    }

    #[test]
    fn each_phone_has_its_service() {
        assert_eq!(service("ios"), Some("Game Center"));
        assert_eq!(service("android"), Some("Google Play Games"));
        assert_eq!(service("web"), None);
        assert_eq!(Platform::of("ios"), Some(Platform::GameCenter));
        assert_eq!(Platform::of("android"), Some(Platform::PlayGames));
        assert_eq!(Platform::of("web"), None);
    }

    fn stamp(stamp: Stamp, progress: i64, earned: bool) -> StampProgress {
        StampProgress {
            stamp,
            earned_at: earned.then_some(1_700_000_000_000),
            progress,
            target: stamp.steps().map_or(1, i64::from),
            holders: 3,
        }
    }

    fn day(day: i64, time_ms: Option<i64>) -> crate::model::DayResult {
        crate::model::DayResult {
            day,
            time_ms,
            rank: time_ms.map(|_| 4),
        }
    }

    /// A player with the first stamp, 42 of 100 dailies, a streak stamp earned with a streak
    /// that broke since, nothing of the clean one, and today's daily solved in 1:23.456.
    fn stats() -> UserStats {
        UserStats {
            solved: 42,
            current_streak: 2,
            best_streak: 9,
            recent: vec![day(100, Some(70_000)), day(101, Some(83_456))],
            stamps: vec![
                stamp(Stamp::First, 1, true),
                stamp(Stamp::Solved100, 42, false),
                stamp(Stamp::Streak7, 2, true),
                stamp(Stamp::Clean, 0, false),
                stamp(Stamp::Oni, 0, false),
                stamp(Stamp::Unknown, 1, true),
            ],
            ..UserStats::default()
        }
    }

    fn play_ids(json: serde_json::Value) -> PlayIds {
        serde_json::from_value(json).unwrap()
    }

    /// Game Center: Apple's vendor ids, every leaderboard, the daily's time in hundredths.
    #[test]
    fn game_center_gets_the_vendor_ids() {
        let report = report(Platform::GameCenter, &PlayIds::default(), &stats()).unwrap();
        assert_eq!(
            report.achievements,
            [
                achievement("stamp_first", None, None, true),
                achievement("stamp_solved_100", Some(42), Some(100), false),
                achievement("stamp_streak_7", Some(7), Some(7), true),
            ]
        );
        assert_eq!(
            report.scores,
            [
                score("daily_time", 8345),
                score("puzzles_solved", 42),
                score("best_streak", 9),
            ]
        );
    }

    /// Play: its own ids from the file, nothing it has no id for, never the daily's time.
    #[test]
    fn play_gets_its_own_ids_and_no_daily_time() {
        let play = play_ids(json!({
            "project_id": "123456789012",
            "achievements": {
                "stamp_first": "CgkI_first",
                "stamp_solved_100": "CgkI_solved_100",
                // No stamp_streak_7: not reported
                "stamp_clean": "CgkI_clean",
                "stamp_unknown": "CgkI_unknown",
            },
            "leaderboards": {
                "daily_time": "CgkI_daily",
                "puzzles_solved": "CgkI_solved",
                // No best_streak
            },
        }));
        let report = report(Platform::PlayGames, &play, &stats()).unwrap();
        assert_eq!(
            report.achievements,
            [
                achievement("CgkI_first", None, None, true),
                achievement("CgkI_solved_100", Some(42), Some(100), false),
            ]
        );
        assert_eq!(report.scores, [score("CgkI_solved", 42)]);
    }

    /// Before Play has any ids (the checked-in file until the first upload) there's nothing
    /// to report on Android.
    #[test]
    fn without_play_ids_android_reports_nothing() {
        assert_eq!(
            report(Platform::PlayGames, &PlayIds::default(), &stats()),
            None
        );
        let empty = play_ids(json!({
            "project_id": null,
            "achievements": { "stamp_first": "" },
            "leaderboards": {},
        }));
        assert_eq!(report(Platform::PlayGames, &empty, &stats()), None);
        // Not the file's form: no ids at all
        assert_eq!(
            report(Platform::PlayGames, &PlayIds::parse("[]"), &stats()),
            None
        );
    }

    /// The file built into the page parses, and its ids are those of stamps and leaderboards.
    #[test]
    fn the_checked_in_play_ids_are_valid() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("../../app/achievements-play.json")).unwrap();
        let ids: PlayIds = serde_json::from_value(json).unwrap();
        for key in ids.achievements.keys() {
            assert!(
                Stamp::ALL.iter().any(|s| s.achievement_id() == key),
                "{key}"
            );
        }
        for key in ids.leaderboards.keys() {
            assert!(
                GamesLeaderboard::ALL
                    .iter()
                    .any(|b| b.id() == key && b.on_play()),
                "{key}"
            );
        }
        assert_eq!(
            PlayIds::embedded().achievements.len(),
            ids.achievements.len()
        );
    }

    /// Steps go to the platform within the stamp's target, whatever the server counts.
    #[test]
    fn steps_are_clamped_to_the_stamps_target() {
        let stats = UserStats {
            stamps: vec![
                stamp(Stamp::Bridges1000, 1234, false),
                stamp(Stamp::Solved10, -3, false),
                stamp(Stamp::Bridges10000, i64::MAX, false),
            ],
            ..UserStats::default()
        };
        let report = report(Platform::GameCenter, &PlayIds::default(), &stats).unwrap();
        assert_eq!(
            report.achievements,
            [
                achievement("stamp_bridges_1000", Some(1000), Some(1000), true),
                achievement("stamp_bridges_10000", Some(10_000), Some(10_000), true),
            ]
        );
    }

    /// Without today's daily solved (or with it unranked) no time; no score of 0.
    #[test]
    fn only_scores_there_are() {
        let stats = UserStats {
            solved: 1,
            best_streak: 0,
            recent: vec![day(100, Some(70_000)), day(101, None)],
            ..UserStats::default()
        };
        let sent = report(Platform::GameCenter, &PlayIds::default(), &stats).unwrap();
        assert_eq!(sent.scores, [score("puzzles_solved", 1)]);
        assert_eq!(
            report(
                Platform::GameCenter,
                &PlayIds::default(),
                &UserStats::default()
            ),
            None
        );
    }

    /// The second report of a session has only what changed since the first.
    #[test]
    fn a_report_sends_only_what_changed() {
        let mut reported = Reported::default();
        let first = report(Platform::GameCenter, &PlayIds::default(), &stats()).unwrap();
        assert_eq!(reported.fresh(first.clone()), Some(first.clone()));
        // The same again: nothing
        assert_eq!(reported.fresh(first), None);

        // A daily later: one more solved, the hundred further along, a new stamp, a new time
        let mut later = stats();
        later.solved = 43;
        later.stamps[1].progress = 43;
        later.stamps[3] = stamp(Stamp::Clean, 1, false);
        later.recent.push(day(102, Some(65_000)));
        let fresh = reported
            .fresh(report(Platform::GameCenter, &PlayIds::default(), &later).unwrap())
            .unwrap();
        assert_eq!(
            fresh.achievements,
            [
                achievement("stamp_solved_100", Some(43), Some(100), false),
                achievement("stamp_clean", Some(1), Some(10), false),
            ]
        );
        assert_eq!(
            fresh.scores,
            [score("daily_time", 6500), score("puzzles_solved", 43)]
        );
        assert_eq!(
            reported.fresh(report(Platform::GameCenter, &PlayIds::default(), &later).unwrap()),
            None
        );
    }
    /// A report the plugin didn't take goes again with the next one; what a later report noted
    /// meanwhile stays noted.
    #[test]
    fn a_failed_report_is_sent_again() {
        let mut reported = Reported::default();
        let first = report(Platform::GameCenter, &PlayIds::default(), &stats()).unwrap();
        let sent = reported.fresh(first.clone()).unwrap();
        reported.failed(&sent);
        assert_eq!(reported.fresh(first.clone()), Some(first.clone()));
        // Taken this time: not again
        assert_eq!(reported.fresh(first), None);

        // Two dailies later, the first one's report fails while the second's is on its way
        let mut later = stats();
        later.solved = 43;
        later.stamps[1].progress = 43;
        let failing = reported
            .fresh(report(Platform::GameCenter, &PlayIds::default(), &later).unwrap())
            .unwrap();
        assert_eq!(
            failing,
            Report::new(
                [achievement("stamp_solved_100", Some(43), Some(100), false)],
                [score("puzzles_solved", 43)],
            )
            .unwrap()
        );
        later.solved = 44;
        later.stamps[1].progress = 44;
        let newer = reported
            .fresh(report(Platform::GameCenter, &PlayIds::default(), &later).unwrap())
            .unwrap();
        reported.failed(&failing);
        assert_eq!(
            reported.fresh(report(Platform::GameCenter, &PlayIds::default(), &later).unwrap()),
            None
        );
        // Then the second one fails too: it goes again
        reported.failed(&newer);
        assert_eq!(
            reported.fresh(report(Platform::GameCenter, &PlayIds::default(), &later).unwrap()),
            Some(newer)
        );
    }

    /// The stamp book's button signs a player in: one sync, not again on the next tap; none
    /// without a sign-in, nor for a player the start signed in.
    #[test]
    fn a_sign_in_through_the_button_syncs_once() {
        use futures::executor::block_on;
        use std::cell::{Cell, RefCell};

        const OUT: Status = Status {
            available: true,
            signed_in: false,
        };
        const IN: Status = Status {
            available: true,
            signed_in: true,
        };
        let asked = Cell::new(0);
        let synced = RefCell::new(Vec::new());
        let tap = |before: Status, opened: bool, now: Status| {
            block_on(shown(
                before,
                async move { if opened { Ok(()) } else { Err("not signed in") } },
                || {
                    asked.set(asked.get() + 1);
                    async move { now }
                },
                |status| synced.borrow_mut().push(status),
            ))
        };

        // Cancelled: nothing asked, nothing synced
        assert_eq!(tap(OUT, false, IN), Err("not signed in"));
        assert_eq!((asked.get(), synced.borrow().len()), (0, 0));
        // Signed in through the button: one sync, the next tap starts from the new answer
        let now = tap(OUT, true, IN).unwrap();
        assert_eq!(now, IN);
        assert_eq!(*synced.borrow(), [IN]);
        assert_eq!(tap(now, true, IN), Ok(IN));
        assert_eq!((asked.get(), synced.borrow().len()), (1, 1));
        // Opened, but nobody signed in: no sync, the next tap asks again
        assert_eq!(tap(OUT, true, OUT), Ok(OUT));
        assert_eq!((asked.get(), synced.borrow().len()), (2, 1));
        // Signed in since the start, which reported everything already
        assert_eq!(tap(IN, true, IN), Ok(IN));
        assert_eq!((asked.get(), synced.borrow().len()), (2, 1));
    }
}
