//! The JSON the site stores and sends, see [`super`].

use common::{
    Difficulty, GenerateParams, GeneratedPuzzle, Heuristic, Island, Pos, Puzzle, Restarts,
    SolveOutcome, SolveReport, SolveStats, SolverConfig, SolverKind,
};
use dojo::{Answer, Family, FamilyScore, Rejection, Report, Score};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::{check, check_json};
use crate::model::{
    AdminAttempt, AdminOverview, AdminSubmission, AllTimeEntry, ApiError, AttemptInfo,
    ContactMessage, DailyInfo, DayResult, Daypart, DojoBoard, DojoEntry, DojoMe, DojoRow,
    DojoState, DojoSubmission, Flag, HallOfFame, Leaderboard, LeaderboardEntry, LogSummary,
    MonthBoard, MonthEntry, MoveEvent, PlayerProfile, Run, Stamp, StampProgress, SubmissionAction,
    SubmitResult, UserInfo, UserStats,
};

/// What the server functions send to the pages.
const RESPONSES: &str = "a page that is still open after a deploy runs the old code against the \
                         new server: changed JSON breaks it until it is reloaded";
/// What sits in the database and the browsers.
const STORED: &str = "rows in the database and the local storage of every browser are in this \
                      format: changed JSON makes old dailies, attempts, Dojo scores or saved games \
                      fail to load";
/// What the Dojo worker and the website send each other.
const WORKER: &str = "the Dojo worker deploys separately from the website, they must keep \
                      understanding each other's reports and results";
/// Ids in database columns, URLs and translation keys.
const IDS: &str = "these ids are stored in the database (stamps, flags, difficulties, Dojo states), \
                   are part of URLs or name translations: renaming one loses old rows or texts";

/// Three islands, the solution is `[2, 1]`.
pub(crate) fn puzzle() -> Puzzle {
    Puzzle::new(
        3,
        3,
        vec![
            Island {
                pos: Pos::new(0, 0),
                required: 2,
            },
            Island {
                pos: Pos::new(2, 0),
                required: 3,
            },
            Island {
                pos: Pos::new(2, 2),
                required: 1,
            },
        ],
    )
}

fn score() -> Score {
    Score {
        par2: 2.5,
        solved: 30,
        total: 34,
        wrong: 0,
        budget: 10_000_000_000,
        fuel: 12.125,
        families: vec![
            FamilyScore {
                family: Family::Hashi,
                solved: 8,
                total: 8,
                par2: 0.75,
            },
            FamilyScore {
                family: Family::Queens,
                solved: 4,
                total: 5,
                par2: 4.0,
            },
        ],
    }
}

fn stamp_progress() -> Vec<StampProgress> {
    vec![
        StampProgress {
            stamp: Stamp::First,
            earned_at: Some(1_760_000_000_000),
            progress: 1,
            target: 1,
            holders: 12,
        },
        StampProgress {
            stamp: Stamp::Streak7,
            earned_at: None,
            progress: 3,
            target: 7,
            holders: 0,
        },
    ]
}

fn leaderboard_entry(rank: i64, stamp: Option<Stamp>) -> LeaderboardEntry {
    LeaderboardEntry {
        rank,
        user_id: 7,
        name: "noel".into(),
        guest: false,
        time_ms: 95_000,
        hints: 1,
        checks: 0,
        stamp,
    }
}

fn month_entry() -> MonthEntry {
    MonthEntry {
        rank: 2,
        user_id: 7,
        name: "noel".into(),
        guest: false,
        points: 37,
        solved: 2,
        stamp: Some(Stamp::Gold),
    }
}

fn all_time_entry() -> AllTimeEntry {
    AllTimeEntry {
        rank: 1,
        user_id: 3,
        name: "Brave Heron 42".into(),
        guest: true,
        solved: 120,
        gold: 9,
        silver: 4,
        bronze: 2,
        best_streak: 30,
        current_streak: 5,
        stamp: None,
    }
}

fn user_stats() -> UserStats {
    UserStats {
        solved: 42,
        current_streak: 3,
        best_streak: 12,
        best_time_ms: Some(61_000),
        average_time_ms: Some(180_500),
        recent: vec![
            DayResult {
                day: 20_380,
                time_ms: Some(61_000),
                rank: Some(1),
            },
            DayResult {
                day: 20_381,
                time_ms: None,
                rank: None,
            },
        ],
        gold: 2,
        silver: 1,
        bronze: 0,
        month_rank: Some(4),
        month_points: 55,
        stamps: stamp_progress(),
        players: 80,
        shown: Some(Stamp::Oni),
    }
}

fn dojo_submission() -> DojoSubmission {
    DojoSubmission {
        id: 12,
        name: "minisat-ish".into(),
        description: "CDCL with VSIDS".into(),
        source_url: Some("https://github.com/someone/solver".into()),
        author: "noel".into(),
        house: false,
        created_at: 1_760_000_000_000,
        state: DojoState::Done,
        season: 1,
        progress: 34,
        total: 34,
        score: Some(score()),
        rejection: None,
        rank: Some(3),
        similar_to: Some(9),
        size: 123_456,
        sha256: "ab".repeat(32),
        mine: true,
    }
}

/// Every server function response type, in one file.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Responses {
    user: UserInfo,
    daily: DailyInfo,
    daily_before_start: DailyInfo,
    submit: SubmitResult,
    leaderboard: Leaderboard,
    month: MonthBoard,
    fame: HallOfFame,
    profile: PlayerProfile,
    dojo_board: DojoBoard,
    dojo_submission: DojoSubmission,
    dojo_rejected: DojoSubmission,
    dojo_me: DojoMe,
    admin_overview: AdminOverview,
    admin_attempt: AdminAttempt,
    admin_submission: AdminSubmission,
    admin_message: ContactMessage,
    generated: GeneratedPuzzle,
    solved: SolveReport,
    unsatisfiable: SolveOutcome,
    timeout: SolveOutcome,
    // Every variant, the pages also send some of them as arguments
    stamps: Vec<Stamp>,
    flags: Vec<Flag>,
    difficulties: Vec<Difficulty>,
    dojo_states: Vec<DojoState>,
    families: Vec<Family>,
    submission_actions: Vec<SubmissionAction>,
}

fn responses() -> Responses {
    let attempt = AttemptInfo {
        started_at: 1_760_000_000_000,
        finished_at: Some(1_760_000_095_000),
        hints: 1,
        checks: 2,
        revealed: false,
        time_ms: Some(145_000),
        bridges: Some(vec![2, 1]),
        flag: Some(Flag::Pace),
    };
    Responses {
        user: UserInfo {
            id: 7,
            name: "noel".into(),
            guest: false,
            created_at: 1_750_000_000_000,
            admin: true,
            renamed_at: Some(1_755_000_000_000),
        },
        daily: DailyInfo {
            day: 20_381,
            difficulty: Difficulty::Hard,
            puzzle: Some(puzzle()),
            attempt: Some(attempt),
            server_now: 1_760_000_100_000,
            finishers: 17,
            stamps: vec![Stamp::Gold, Stamp::Streak7],
        },
        daily_before_start: DailyInfo {
            day: 20_381,
            difficulty: Difficulty::Oni,
            puzzle: None,
            attempt: None,
            server_now: 1_760_000_100_000,
            finishers: 0,
            stamps: Vec::new(),
        },
        submit: SubmitResult {
            time_ms: 145_000,
            rank: 0,
            finishers: 17,
            flag: Some(Flag::TooFast),
            stamps: vec![Stamp::First],
        },
        leaderboard: Leaderboard {
            day: 20_381,
            difficulty: Difficulty::Hard,
            entries: vec![leaderboard_entry(1, Some(Stamp::Gold10))],
            me: Some(leaderboard_entry(5, None)),
            finishers: 17,
        },
        month: MonthBoard {
            month: 24_321,
            entries: vec![month_entry()],
            me: Some(month_entry()),
            players: 33,
        },
        fame: HallOfFame {
            solved: vec![all_time_entry()],
            medals: vec![all_time_entry()],
            streaks: Vec::new(),
        },
        profile: PlayerProfile {
            id: 7,
            name: "noel".into(),
            guest: false,
            created_at: 1_750_000_000_000,
            stats: user_stats(),
        },
        dojo_board: DojoBoard {
            season: 1,
            formulas: 34,
            budget: 10_000_000_000,
            entries: vec![DojoEntry {
                rank: 1,
                id: 12,
                name: "minisat-ish".into(),
                author: "noel".into(),
                house: false,
                score: score(),
                created_at: 1_760_000_000_000,
            }],
            queued: 2,
        },
        dojo_submission: dojo_submission(),
        dojo_rejected: DojoSubmission {
            state: DojoState::Rejected,
            score: None,
            rejection: Some(Rejection::Import("env.f".into())),
            rank: None,
            similar_to: None,
            source_url: None,
            mine: false,
            ..dojo_submission()
        },
        dojo_me: DojoMe {
            registered: true,
            left_today: 2,
            submissions: vec![DojoRow {
                id: 12,
                name: "minisat-ish".into(),
                state: DojoState::Queued,
                created_at: 1_760_000_000_000,
                score: None,
            }],
        },
        admin_overview: AdminOverview {
            users: 100,
            registered: 40,
            day: 20_381,
            started: 20,
            finished: 17,
            flagged: 1,
            dojo_pending: 2,
            dojo_total: 30,
            messages: 3,
            names: 1,
        },
        admin_attempt: AdminAttempt {
            user_id: 7,
            name: "noel".into(),
            username: Some("noel".into()),
            started_at: 1_760_000_000_000,
            finished_at: Some(1_760_000_095_000),
            time_ms: Some(145_000),
            hints: 1,
            checks: 2,
            moves: Some(40),
            revealed: false,
            flag: Some(Flag::Review),
            log: LogSummary {
                events: 40,
                taps: 38,
                span_ms: 90_000,
                fastest_gap_ms: Some(120),
                median_gap_ms: Some(900),
            },
        },
        admin_submission: AdminSubmission {
            id: 12,
            name: "minisat-ish".into(),
            author: "noel".into(),
            house: false,
            state: DojoState::Running,
            season: 1,
            solved: Some(30),
            total: 34,
            par2: Some(2.5),
            wrong: 0,
            similar_to: None,
            created_at: 1_760_000_000_000,
        },
        admin_message: ContactMessage {
            id: 1,
            name: String::new(),
            email: "someone@example.com".into(),
            message: "Hallo! \"Brücken\" <3".into(),
            created_at: 1_760_000_000_000,
            mailed: false,
        },
        generated: GeneratedPuzzle {
            puzzle: puzzle(),
            solution: vec![2, 1],
            params: GenerateParams {
                width: 3,
                height: 3,
                difficulty: Difficulty::Easy,
                seed: u64::MAX,
            },
            unique: true,
            attempts: 3,
            generate_ms: 1.5,
        },
        solved: SolveReport {
            outcome: SolveOutcome::Solved {
                bridges: vec![2, 1],
                unique: Some(true),
            },
            stats: SolveStats {
                config: SolverConfig::splr().normalized(),
                variables: 6,
                clauses: 20,
                encode_ms: 0.25,
                solve_ms: 0.5,
                decisions: Some(3),
                restarts: None,
                sat_calls: 2,
                connectivity_cuts: 1,
            },
        },
        unsatisfiable: SolveOutcome::Unsatisfiable,
        timeout: SolveOutcome::Timeout,
        stamps: Stamp::ALL.to_vec(),
        flags: Flag::ALL.to_vec(),
        difficulties: Difficulty::ALL.to_vec(),
        dojo_states: vec![
            DojoState::Queued,
            DojoState::Running,
            DojoState::Done,
            DojoState::Rejected,
            DojoState::Failed,
        ],
        families: Family::ALL.to_vec(),
        submission_actions: vec![
            SubmissionAction::Requeue,
            SubmissionAction::ClearCopy,
            SubmissionAction::Delete,
        ],
    }
}

#[test]
fn server_function_responses() {
    check_json("responses.json", &responses(), RESPONSES);
}

/// Older rows and responses lack the fields that came later (`#[serde(default)]`), they still
/// have to load.
#[test]
fn responses_without_later_fields_load() {
    let user: UserInfo =
        serde_json::from_str(r#"{"id":1,"name":"a","guest":true,"created_at":0}"#).unwrap();
    assert!(!user.admin, "{RESPONSES}");
    let attempt: AttemptInfo = serde_json::from_str(
        r#"{"started_at":0,"finished_at":null,"hints":0,"checks":0,"revealed":false,"time_ms":null,"bridges":null}"#,
    )
    .unwrap();
    assert_eq!(attempt.flag, None, "{RESPONSES}");
    let submit: SubmitResult =
        serde_json::from_str(r#"{"time_ms":1,"rank":1,"finishers":1}"#).unwrap();
    assert!(
        submit.flag.is_none() && submit.stamps.is_empty(),
        "{RESPONSES}"
    );
    let progress: StampProgress =
        serde_json::from_str(r#"{"stamp":"First","earned_at":null,"progress":0,"target":1}"#)
            .unwrap();
    assert_eq!(progress.holders, 0, "{RESPONSES}");
}

/// A value as one line of compact JSON, the way it is written to a column or local storage,
/// and checks that the line loads as the value again.
fn line<T>(label: &str, value: &T) -> String
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(value).unwrap();
    let loaded: T = serde_json::from_str(&json).unwrap();
    assert_eq!(&loaded, value, "{label}: {STORED}");
    format!("{label} {json}")
}

#[test]
fn stored_formats() {
    let log = vec![
        MoveEvent {
            t: 0,
            edge: 0,
            count: 1,
            tap: true,
        },
        MoveEvent {
            t: 1_250,
            edge: 2,
            count: 2,
            tap: false,
        },
    ];
    let run = Run {
        bridges: 3,
        islands: 3,
        steady: true,
        dithers: 1,
        cleared: false,
        burst: 4,
    };
    let solver = |restarts| SolverConfig {
        kind: SolverKind::Cdcl,
        heuristic: Heuristic::Vsids,
        restarts,
        subsumption: true,
        timeout_ms: 10_000,
    };
    let lines = [
        line("dailies.puzzle", &puzzle()),
        line("dailies.solution", &vec![2u8, 1]),
        line("attempts.bridges", &vec![2u8, 1]),
        line("attempts.log", &log),
        line("attempts.run", &run),
        line("dojo_submissions.score", &score()),
        line(
            "dojo_submissions.fingerprint",
            &vec![1_000_000_000u64, u64::MAX, u64::MAX - 1, u64::MAX - 2],
        ),
        line("dojo_submissions.error", &Rejection::TooLarge),
        line("dojo_submissions.error", &Rejection::Invalid("bad".into())),
        line("dojo_submissions.error", &Rejection::Import("env.f".into())),
        line("dojo_submissions.error", &Rejection::NoStart),
        line("hashi.editor.v2", &puzzle()),
        line("hashi.solver.v2", &SolverConfig::splr()),
        line("hashi.solver.v2", &solver(Restarts::None)),
        line(
            "hashi.solver.v2",
            &solver(Restarts::Fixed { threshold: 100 }),
        ),
        line(
            "hashi.solver.v2",
            &solver(Restarts::Geometric {
                threshold: 100,
                factor_percent: 150,
            }),
        ),
        line("hashi.solver.v2", &solver(Restarts::Luby { unit: 64 })),
        line("hashi.sound", &true),
        line(
            "hashi.device",
            &"0123456789abcdef0123456789abcdef".to_string(),
        ),
    ];
    check("stored.txt", &lines.join("\n"), STORED);
}

#[test]
fn worker_reports() {
    let reports = [
        Report::Progress {
            id: 12,
            season: 1,
            done: 3,
            total: 34,
        },
        Report::Finished {
            id: 12,
            season: 1,
            score: score(),
            fingerprint: vec![1_000, u64::MAX],
        },
        Report::Rejected {
            id: 12,
            season: 1,
            reason: Rejection::NoStart,
        },
        Report::Failed {
            id: 12,
            season: 1,
            message: "worker restarted".into(),
        },
    ];
    let lines: Vec<String> = reports
        .iter()
        .map(|r| {
            let json = serde_json::to_string(r).unwrap();
            assert_eq!(
                &serde_json::from_str::<Report>(&json).unwrap(),
                r,
                "{WORKER}"
            );
            json
        })
        .collect();
    check("worker.txt", &lines.join("\n"), WORKER);
    // Workers from before fingerprints existed
    let old: Report = serde_json::from_str(
        &lines[1].replace(r#","fingerprint":[1000,18446744073709551615]"#, ""),
    )
    .unwrap();
    assert!(
        matches!(old, Report::Finished { fingerprint, .. } if fingerprint.is_empty()),
        "{WORKER}"
    );
    // The worker answers a rejected upload with the reason as JSON (see `server::dojo::worker`)
    assert_eq!(
        serde_json::from_str::<Rejection>(r#"{"Invalid":"bad"}"#).unwrap(),
        Rejection::Invalid("bad".into()),
        "{WORKER}"
    );
    assert_eq!(
        serde_json::from_str::<Answer>(r#""Unsat""#).unwrap(),
        Answer::Unsat,
        "{WORKER}"
    );
}

#[test]
fn stored_ids() {
    let mut lines = Vec::new();
    lines.extend(Stamp::ALL.map(|s| format!("stamp {:?} {}", s, s.id())));
    lines.extend(Flag::ALL.map(|f| format!("flag {:?} {}", f, f.id())));
    lines.extend([Daypart::Night, Daypart::Lunch].map(|d| format!("daypart {:?} {}", d, d.id())));
    lines.extend(Difficulty::ALL.map(|d| format!("difficulty {:?} {}", d, d.id())));
    lines.extend(
        [
            DojoState::Queued,
            DojoState::Running,
            DojoState::Done,
            DojoState::Rejected,
            DojoState::Failed,
        ]
        .map(|s| format!("dojo_state {:?} {}", s, s.id())),
    );
    lines.extend(Family::ALL.map(|f| format!("family {:?} {}", f, f.id())));
    lines.extend(SolverKind::ALL.map(|k| format!("solver {:?} {}", k, k.id())));
    lines.extend(Heuristic::ALL.map(|h| format!("heuristic {:?} {}", h, h.id())));
    lines.extend(ApiError::ALL.map(|e| format!("error {:?} {}", e, e.code())));
    check("ids.txt", &lines.join("\n"), IDS);
}
