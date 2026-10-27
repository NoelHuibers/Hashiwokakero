//! What the app reports to the phone's games service, Game Center on iOS and Google Play Games
//! on Android: the arguments of the commands `games_sign_in`, `games_report` and `games_show` of
//! the app's plugin (`HashiPlugin.swift`, `HashiPlugin.kt`), sent by `native::games_*`.
//!
//! The server stays the source of truth: the page reports what the server says the player has
//! (a stamp earned or how far along it is, a score), the platforms only show it. Both take a
//! report again as often as it comes and keep the highest: an achievement never goes back, a
//! leaderboard keeps the best score. The ids are the platform's own, Apple's vendor ids or the
//! ids Play generates (`app/achievements-play.json`); this module doesn't know them.

use serde::{Deserialize, Serialize};

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

/// The games service of the app's `platform` (`api::app`), as its owner names it. The page
/// shows it on the button that opens it, unchanged in every language.
pub(crate) fn service(platform: &str) -> Option<&'static str> {
    match platform {
        "ios" => Some("Game Center"),
        "android" => Some("Google Play Games"),
        _ => None,
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
    }
}
