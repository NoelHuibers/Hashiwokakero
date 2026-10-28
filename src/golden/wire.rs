//! The JSON the site stores and sends, see [`super`].

use common::{
    Difficulty, GenerateParams, GeneratedPuzzle, Heuristic, Island, Pos, Puzzle, Restarts,
    SolveOutcome, SolveReport, SolveStats, SolverConfig, SolverKind,
};
use dojo::metrics::{Breakdown, BudgetScore, Contribution, EntryBreakdown, FamilyBreakdown, Split};
use dojo::{Answer, Family, FamilyScore, InstanceResult, Outcome, Rejection, Report, Score};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::{check, check_json};
use crate::model::{
    AdminAttempt, AdminBenchmark, AdminOverview, AdminSubmission, AllTimeEntry, ApiError,
    AttemptInfo, BanOutcome, BannedPlayer, BenchmarkAction, BenchmarkState, CommunityCredit,
    CommunitySummary, ContactMessage, DailyInfo, DailyStart, DayResult, Daypart, DojoBenchmark,
    DojoBenchmarks, DojoBoard, DojoBudgetCurve, DojoBudgets, DojoCactus, DojoCrossover, DojoCurve,
    DojoEntry, DojoMe, DojoPortfolio, DojoRow, DojoSeason, DojoSeasons, DojoState, DojoSubmission,
    Flag, GroupToday, HallOfFame, HankoKind, Leaderboard, LeaderboardEntry, LogSummary, Mark,
    MarkKind, MonthBoard, MonthEntry, MoveEvent, PlayerProfile, Run, Stamp, StampProgress,
    SubmissionAction, SubmitResult, UserInfo, UserStats,
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
                unverified: 0,
            },
            FamilyScore {
                family: Family::Queens,
                solved: 4,
                total: 5,
                par2: 4.0,
                unverified: 1,
            },
        ],
        unverified: 1,
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

fn breakdown() -> EntryBreakdown {
    let split = |solved, total, par2| Split {
        solved,
        total,
        par2,
    };
    EntryBreakdown {
        total: Breakdown {
            all: split(30, 34, 2.5),
            sat: Some(split(16, 17, 1.25)),
            unsat: Some(split(12, 13, 3.5)),
            unverified: 1,
        },
        held_out: 4,
        families: vec![FamilyBreakdown {
            family: Family::Mystery,
            breakdown: Breakdown {
                all: split(2, 4, 5.0),
                sat: None,
                unsat: None,
                unverified: 0,
            },
        }],
    }
}

fn contribution() -> Contribution {
    Contribution {
        marginal: 0.5,
        log_ratio: 0.125,
        relative: 0.25,
        sota: 3.5,
        shapley: 1.75,
    }
}

fn portfolio() -> DojoPortfolio {
    DojoPortfolio {
        entries: 2,
        vbs: Split {
            solved: 32,
            total: 34,
            par2: 1.5,
        },
        cactus: DojoCactus {
            budget: 7_000_000_000,
            run_budget: 70_000_000_000,
            formulas: 34,
            vbs: vec![1_000, 25_000_000],
            curves: vec![DojoCurve {
                id: 12,
                name: "minisat-ish".into(),
                house: false,
                fuel: vec![2_000, 25_000_000],
            }],
        },
        budgets: Some(DojoBudgets {
            budget: 7_000_000_000,
            run_budget: 70_000_000_000,
            curves: vec![DojoBudgetCurve {
                id: 12,
                name: "minisat-ish".into(),
                house: false,
                points: vec![BudgetScore {
                    budget: 7_000_000_000,
                    solved: 32,
                    total: 34,
                    par2: 1.5,
                    sat: 20,
                    unsat: 12,
                    wrong: 0,
                    unverified: 1,
                }],
            }],
            crossovers: vec![DojoCrossover {
                budget: 31_000_000_000,
                ahead: 12,
                behind: 13,
            }],
        }),
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
        breakdown: Some(breakdown()),
        contribution: Some(contribution()),
        portfolio: Some(portfolio()),
    }
}

/// Every server function response type, in one file.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Responses {
    user: UserInfo,
    daily: DailyInfo,
    daily_before_start: DailyInfo,
    daily_start: DailyStart,
    submit: SubmitResult,
    leaderboard: Leaderboard,
    month: MonthBoard,
    fame: HallOfFame,
    profile: PlayerProfile,
    dojo_board: DojoBoard,
    dojo_submission: DojoSubmission,
    dojo_rejected: DojoSubmission,
    dojo_me: DojoMe,
    dojo_seasons: DojoSeasons,
    dojo_benchmarks: DojoBenchmarks,
    admin_overview: AdminOverview,
    admin_attempt: AdminAttempt,
    admin_submission: AdminSubmission,
    admin_message: ContactMessage,
    admin_banned: BannedPlayer,
    ban_outcomes: Vec<BanOutcome>,
    admin_benchmark: AdminBenchmark,
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
    benchmark_states: Vec<BenchmarkState>,
    benchmark_actions: Vec<BenchmarkAction>,
}

fn responses() -> Responses {
    let attempt = AttemptInfo {
        attempt_id: String::new(),
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
            name_locked: false,
        },
        daily: DailyInfo {
            day: 20_381,
            difficulty: Difficulty::Hard,
            puzzle: Some(puzzle()),
            attempt: Some(attempt.clone()),
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
        daily_start: DailyStart {
            info: DailyInfo {
                day: 20_381,
                difficulty: Difficulty::Easy,
                puzzle: Some(puzzle()),
                attempt: Some(AttemptInfo {
                    finished_at: None,
                    time_ms: None,
                    bridges: None,
                    flag: None,
                    ..attempt
                }),
                server_now: 1_760_000_000_000,
                finishers: 3,
                stamps: Vec::new(),
            },
            user: Some(UserInfo {
                id: 8,
                name: "Guest 1234".into(),
                guest: true,
                created_at: 1_760_000_000_000,
                admin: false,
                renamed_at: None,
                name_locked: false,
            }),
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
            formulas: 54,
            budget: 10_000_000_000,
            entries: vec![DojoEntry {
                rank: 1,
                id: 12,
                name: "minisat-ish".into(),
                author: "noel".into(),
                house: false,
                score: score(),
                created_at: 1_760_000_000_000,
                breakdown: Some(breakdown()),
                contribution: Some(contribution()),
            }],
            queued: 2,
            commitment: "c0".repeat(32),
            generator_version: 1,
            rules_version: 1,
            community: CommunitySummary {
                benchmarks: 3,
                authors: vec!["noel".into(), "ada".into()],
                per_set: 6,
            },
            portfolio: Some(portfolio()),
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
            breakdown: None,
            contribution: None,
            portfolio: None,
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
        dojo_seasons: DojoSeasons {
            current: DojoSeason {
                season: 2,
                started_at: Some(1_760_000_000_000),
                ended_at: None,
                commitment: "c0".repeat(32),
                seed: None,
                generator_version: 1,
                rules_version: 1,
                set_size: None,
                set_sha256: None,
                entries: 0,
                mystery: None,
                community: Vec::new(),
                board: Vec::new(),
                portfolio: None,
            },
            past: vec![DojoSeason {
                season: 1,
                started_at: Some(1_750_000_000_000),
                ended_at: Some(1_760_000_000_000),
                commitment: "ab".repeat(32),
                seed: Some("5e".repeat(32)),
                generator_version: 1,
                rules_version: 1,
                set_size: Some(1_400_000),
                set_sha256: Some("cd".repeat(32)),
                entries: 4,
                mystery: Some(Family::Tseitin),
                community: vec![CommunityCredit {
                    id: 3,
                    family: "crypto-sha1".into(),
                    author: String::new(),
                    answer: Answer::Unsat,
                    vars: 1200,
                    clauses: 5600,
                    hash: "ab".repeat(16),
                }],
                board: vec![DojoEntry {
                    rank: 1,
                    id: 12,
                    name: String::new(),
                    author: String::new(),
                    house: false,
                    score: score(),
                    created_at: 1_755_000_000_000,
                    breakdown: None,
                    contribution: None,
                }],
                portfolio: Some(portfolio()),
            }],
        },
        dojo_benchmarks: DojoBenchmarks {
            registered: true,
            left_today: 2,
            mine: vec![DojoBenchmark {
                id: 4,
                family: "scheduling".into(),
                description: "job shop".into(),
                state: BenchmarkState::Refused,
                reason: Some("too_easy".into()),
                answer: Some(Answer::Sat),
                vars: 300,
                clauses: 2000,
                hash: "cd".repeat(16),
                created_at: 1_760_000_000_000,
            }],
            pool: vec![CommunityCredit {
                id: 3,
                family: "crypto-sha1".into(),
                author: "ada".into(),
                answer: Answer::Unsat,
                vars: 1200,
                clauses: 5600,
                hash: "ab".repeat(16),
            }],
            per_set: 6,
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
        admin_banned: BannedPlayer {
            id: 9,
            name: "Sleepy Koi 12".into(),
            username: None,
            banned_at: 1_760_000_000_000,
            reason: "a bot".into(),
        },
        ban_outcomes: vec![
            BanOutcome::Banned("bob".into()),
            BanOutcome::NoSuchPlayer,
            BanOutcome::Admin,
        ],
        admin_benchmark: AdminBenchmark {
            id: 4,
            family: "scheduling".into(),
            description: "job shop".into(),
            author: "noel".into(),
            state: BenchmarkState::Verified,
            reason: None,
            claimed: Some(Answer::Sat),
            answer: Some(Answer::Sat),
            vars: 300,
            clauses: 2000,
            solve_ms: Some(1234),
            from_gbd: true,
            hash: "cd".repeat(16),
            created_at: 1_760_000_000_000,
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
        families: Family::ALL
            .into_iter()
            .chain([Family::Mystery, Family::Community])
            .collect(),
        submission_actions: vec![
            SubmissionAction::Requeue,
            SubmissionAction::ClearCopy,
            SubmissionAction::Delete,
        ],
        benchmark_states: BenchmarkState::ALL.to_vec(),
        benchmark_actions: vec![
            BenchmarkAction::Accept,
            BenchmarkAction::Reject,
            BenchmarkAction::Requeue,
            BenchmarkAction::Delete,
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
    let board: DojoBoard = serde_json::from_str(
        r#"{"season":1,"formulas":34,"budget":10000000000,"entries":[],"queued":0}"#,
    )
    .unwrap();
    assert_eq!(
        (board.commitment.as_str(), board.rules_version),
        ("", 0),
        "{RESPONSES}"
    );
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

/// The start of the daily sends the daily with the player at its side: a page or app from before
/// reads it as the daily it was, and the reply of an older server, only the daily, loads without
/// a player.
#[test]
fn the_start_of_the_daily_reads_as_the_daily() {
    let start = responses().daily_start;
    let json = serde_json::to_string(&start).unwrap();
    let before: DailyInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(before, start.info, "{RESPONSES}");
    let older: DailyStart =
        serde_json::from_str(&serde_json::to_string(&start.info).unwrap()).unwrap();
    assert_eq!(older.info, start.info);
    assert_eq!(older.user, None);
}

/// `value` with the string at `pointer` replaced by `variant`, loaded again.
fn with_variant<T: Serialize + DeserializeOwned>(value: &T, pointer: &str, variant: &str) -> T {
    let mut json = serde_json::to_value(value).unwrap();
    *json.pointer_mut(pointer).unwrap() = variant.into();
    serde_json::from_value(json).unwrap_or_else(|e| panic!("{pointer} = {variant}: {e}. {NEWER}"))
}

/// What a newer server can send an older app.
const NEWER: &str = "installed apps keep their version for months: a variant they don't know \
                     has to load as the fallback (`#[serde(other)]`), not fail the whole call";

/// Variants a newer server added load as `Unknown` in the responses that carry them.
#[test]
fn responses_with_newer_variants_load() {
    let r = responses();
    let daily = with_variant(&r.daily, "/stamps/0", "Kintsugi");
    assert_eq!(daily.stamps, [Stamp::Unknown, Stamp::Streak7], "{NEWER}");
    let daily = with_variant(&r.daily, "/attempt/flag", "Nap");
    assert_eq!(daily.attempt.unwrap().flag, Some(Flag::Unknown), "{NEWER}");
    let submit = with_variant(&r.submit, "/flag", "Nap");
    assert_eq!(submit.flag, Some(Flag::Unknown), "{NEWER}");
    let board = with_variant(&r.leaderboard, "/entries/0/stamp", "Kintsugi");
    assert_eq!(board.entries[0].stamp, Some(Stamp::Unknown), "{NEWER}");
    let profile = with_variant(&r.profile, "/stats/stamps/0/stamp", "Kintsugi");
    assert_eq!(profile.stats.stamps[0].stamp, Stamp::Unknown, "{NEWER}");
    let profile = with_variant(&r.profile, "/stats/shown", "Kintsugi");
    assert_eq!(profile.stats.shown, Some(Stamp::Unknown), "{NEWER}");
    let me = with_variant(&r.dojo_me, "/submissions/0/state", "Paused");
    assert_eq!(me.submissions[0].state, DojoState::Unknown, "{NEWER}");
    let submission = with_variant(&r.dojo_submission, "/state", "Paused");
    assert_eq!(submission.state, DojoState::Unknown, "{NEWER}");
    let benchmarks = with_variant(&r.dojo_benchmarks, "/mine/0/state", "Archived");
    assert_eq!(benchmarks.mine[0].state, BenchmarkState::Unknown, "{NEWER}");
    let today: GroupToday = serde_json::from_str(
        r#"{"group_id":2,"group_name":"Team","day":20381,"open":true,"entries":[],
            "hanko":[{"target":7,"giver":9,"kind":"Hug"}]}"#,
    )
    .unwrap();
    assert_eq!(today.hanko[0].kind, HankoKind::Unknown, "{NEWER}");
    let mark: Mark = serde_json::from_str(r#"{"t":500,"kind":"Undo"}"#).unwrap();
    assert_eq!(mark.kind, MarkKind::Unknown, "{NEWER}");
    // The ids and the stored formats don't know the fallbacks: they never leave a page
    assert!(
        !Stamp::ALL.contains(&Stamp::Unknown)
            && !Flag::ALL.contains(&Flag::Unknown)
            && !HankoKind::ALL.contains(&HankoKind::Unknown)
            && !BenchmarkState::ALL.contains(&BenchmarkState::Unknown),
        "{IDS}"
    );
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
        line("group_puzzles.puzzle", &puzzle()),
        line(
            "group_puzzle_attempts.helps",
            &vec![
                crate::model::Help {
                    t: 1_250,
                    kind: MarkKind::Hint,
                    edges: vec![3],
                },
                crate::model::Help {
                    t: 4_000,
                    kind: MarkKind::Check,
                    edges: vec![],
                },
            ],
        ),
        line("dojo_submissions.score", &score()),
        line(
            "dojo_submissions.fingerprint",
            &vec![1_000_000_000u64, u64::MAX, u64::MAX - 1, u64::MAX - 2],
        ),
        line("dojo_submissions.results", &instance_results()),
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

/// One formula of every outcome.
fn instance_results() -> Vec<InstanceResult> {
    [
        (
            Family::Hashi,
            Answer::Sat,
            Outcome::Solved(Answer::Sat),
            1_000,
        ),
        (
            Family::Pigeonhole,
            Answer::Unsat,
            Outcome::Solved(Answer::Unsat),
            2_000,
        ),
        (
            Family::Random3Sat,
            Answer::Sat,
            Outcome::Timeout,
            10_000_000_000,
        ),
        (Family::Queens, Answer::Sat, Outcome::Unknown, 3),
        (Family::HashiParity, Answer::Unsat, Outcome::Crashed, 4),
        (Family::Hashi, Answer::Sat, Outcome::Wrong, 5),
        (Family::Queens, Answer::Unsat, Outcome::Unverified, 6),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (family, expected, outcome, fuel))| InstanceResult {
        name: format!("{}-{:02}", family.id(), i + 1),
        family,
        expected,
        outcome,
        fuel,
    })
    .collect()
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
            results: instance_results(),
            rules_version: 1,
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
        Report::Heartbeat { id: 12, season: 1 },
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
    // Workers from before fingerprints, per-formula results and rules versions existed
    let finished = &lines[1];
    let old = &finished[..finished.find(r#","fingerprint""#).unwrap()];
    let old: Report = serde_json::from_str(&format!("{old}}}")).unwrap();
    assert!(
        matches!(old, Report::Finished { fingerprint, results, rules_version: 0, .. }
            if fingerprint.is_empty() && results.is_empty()),
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
    lines.extend(BenchmarkState::ALL.map(|s| format!("benchmark_state {:?} {}", s, s.id())));
    lines.extend(
        Family::ALL
            .into_iter()
            .chain([Family::Mystery, Family::Community])
            .map(|f| format!("family {:?} {}", f, f.id())),
    );
    lines.extend(SolverKind::ALL.map(|k| format!("solver {:?} {}", k, k.id())));
    lines.extend(Heuristic::ALL.map(|h| format!("heuristic {:?} {}", h, h.id())));
    lines.extend(ApiError::ALL.map(|e| format!("error {:?} {}", e, e.code())));
    lines.extend(HankoKind::ALL.map(|k| format!("hanko {:?} {}", k, k.id())));
    check("ids.txt", &lines.join("\n"), IDS);
}
