//! The server functions as the pages and the apps call them: their paths with their encoding
//! (`api_paths.txt`) and the JSON of their arguments (`requests.txt`), see [`super`]. The JSON
//! they answer with is in `responses.json` (`wire.rs`).
//!
//! An installed app keeps its version for months and calls the paths and sends the arguments of
//! the build it came with, so from the first release on these are a contract.
//!
//! Compatibility rules, for an intended change:
//!
//! - A new server function: give it a request in [`requests`] with every `Option` and `Vec`
//!   filled, and write the files with `GOLDEN_UPDATE=1`. It only adds a line to each.
//! - Never rename or remove a server function and never change its arguments or encoding: add a
//!   new function and keep the old one until no supported app calls it. A new argument of an
//!   existing function needs `#[server(default)]`, and a test that the request without it still
//!   loads (like [`requests_without_later_arguments_load`]).
//! - A new variant of an enum in a response: the enums with an `Unknown` fallback
//!   (`#[serde(other)]`) take one, old apps read it as `Unknown` and show nothing or a neutral
//!   text: `Stamp`, `HankoKind`, `Flag`, `DojoState`, `BenchmarkState`, `MarkKind`. The server
//!   never stores or sends `Unknown`, and refuses it as an argument.
//! - The other enums in responses can't take a new variant without a new server function: they
//!   carry data (`PaidOutcome`, `TodayState`, `LaneOutcome`, `Title`, `Movement`, `Feat`,
//!   `BanOutcome`, `SolveOutcome`, `Restarts`, `Rejection`), a page needs to know the variant
//!   (`PaidAction`, `Answer`, `Difficulty`, `Family`), they echo the page's own settings
//!   (`SolverKind`, `Heuristic`), or only the admin pages read them (`NameKind`,
//!   `ReportReason` in `AdminReport`, `ReportOutcome`). Error codes
//!   are no enum on the wire: an unknown `E:` code shows as a generic error.

use common::{Difficulty, GenerateParams, Heuristic, Restarts, SolverConfig, SolverKind};
use leptos::server_fn::axum_export::body::Body;
use leptos::server_fn::codec::{Encoding, JsonEncoding};
use leptos::server_fn::http_export::{Request, Response};
use leptos::server_fn::{ContentType, Encodes, Http, ServerFn, ServerFnTraitObj, inventory};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{check, path, puzzle};
use crate::api::{self, DailyOwner, PaidAction, PaidIntent};
use crate::model::{
    BenchmarkAction, Flag, HankoKind, MoveEvent, ReportOutcome, ReportReason, Stamp,
    SubmissionAction,
};

/// The paths and their encodings.
const PATHS: &str = "installed apps call these paths for months without an update: removing or \
                     renaming a server function, or changing its encoding, breaks every app that \
                     calls it. A new server function only adds a line";
/// The arguments.
const REQUESTS: &str = "installed apps send these requests for months without an update: a \
                        renamed, removed or retyped argument, or a new one without \
                        `#[server(default)]`, makes the server refuse their calls";

/// Method and content types of a server function's protocol, like `POST application/json ->
/// application/json`.
trait Wire {
    fn wire() -> String;
}

impl<I: Encoding, O: ContentType> Wire for Http<I, O> {
    fn wire() -> String {
        format!("{} {} -> {}", I::METHOD, I::CONTENT_TYPE, O::CONTENT_TYPE)
    }
}

/// One server function's request, as the client sends it.
struct Entry {
    path: &'static str,
    wire: String,
    body: String,
    /// Loads a body as the server does, and writes it again.
    load: fn(&str) -> serde_json::Result<String>,
}

fn entry<T>(args: &T) -> Entry
where
    T: ServerFn + Serialize + DeserializeOwned,
    T::Protocol: Wire,
{
    // What the `Json` input codec puts into the request body
    let body = JsonEncoding::encode(args).unwrap();
    Entry {
        path: T::PATH,
        wire: T::Protocol::wire(),
        body: String::from_utf8(body.to_vec()).unwrap(),
        load: |body| serde_json::to_string(&serde_json::from_str::<T>(body)?),
    }
}

/// The registered server functions' paths, sorted.
fn registered() -> Vec<&'static str> {
    let mut paths: Vec<_> = inventory::iter::<ServerFnTraitObj<Request<Body>, Response<Body>>>
        .into_iter()
        .map(ServerFnTraitObj::path)
        .collect();
    paths.sort_unstable();
    paths
}

fn log() -> Vec<MoveEvent> {
    vec![
        MoveEvent {
            t: 0,
            edge: 0,
            count: 1,
            tap: true,
        },
        MoveEvent {
            t: 1_250,
            edge: 0,
            count: 2,
            tap: false,
        },
    ]
}

fn owner() -> DailyOwner {
    DailyOwner {
        user: 7,
        attempt: "a1b2c3".into(),
    }
}

/// A request for every server function, every `Option` `Some` and every `Vec` filled, so the
/// golden file has every field's name and shape.
fn requests() -> Vec<Entry> {
    let day = 20_381;
    let bridges = vec![2, 1];
    vec![
        // account
        entry(&api::CurrentUser {}),
        entry(&api::Register {
            username: "noel".into(),
            password: "correct horse".into(),
        }),
        entry(&api::Login {
            username: "noel".into(),
            password: "correct horse".into(),
        }),
        entry(&api::ChangeUsername {
            username: "noel2".into(),
            password: "correct horse".into(),
        }),
        entry(&api::Logout {}),
        entry(&api::DeleteAccount {
            password: Some("correct horse".into()),
        }),
        entry(&api::MyStats {}),
        entry(&api::LoadPlayer { id: 7 }),
        entry(&api::ShowStamp {
            stamp: Some(Stamp::Gold),
        }),
        // admin
        entry(&api::LoadAdminOverview {}),
        entry(&api::AdminAttempts { day: Some(day) }),
        entry(&api::AdminLog { user: 7, day }),
        entry(&api::AdminFlag {
            user: 7,
            day,
            flag: Some(Flag::Review),
        }),
        entry(&api::AdminSubmissions {}),
        entry(&api::UpdateAdminSubmission {
            id: 12,
            action: SubmissionAction::Requeue,
        }),
        entry(&api::AdminBenchmarks {}),
        entry(&api::UpdateAdminBenchmark {
            id: 4,
            action: BenchmarkAction::Accept,
        }),
        entry(&api::AdminMessages { before: Some(9) }),
        entry(&api::DeleteAdminMessage { id: 9 }),
        entry(&api::AdminNames {}),
        entry(&api::AdminRenamePlayer { id: 9 }),
        entry(&api::AdminBanPlayer {
            id: 9,
            reason: "a bot".into(),
        }),
        entry(&api::AdminUnbanPlayer { id: 9 }),
        entry(&api::AdminBannedPlayers {}),
        entry(&api::AdminReports {}),
        entry(&api::AdminResolveReport {
            id: 3,
            outcome: ReportOutcome::Renamed,
        }),
        // banzuke
        entry(&api::LoadBanzuke {
            id: 2,
            monday: Some(20_377),
        }),
        // contact
        entry(&api::SendMessage {
            name: "Ada".into(),
            email: "ada@example.com".into(),
            message: "Hallo!".into(),
            website: String::new(),
        }),
        // daily
        entry(&api::DailyToday {}),
        entry(&api::StartDaily {
            device: Some("0123456789abcdef0123456789abcdef".into()),
        }),
        entry(&api::DailyHint {
            day,
            bridges: bridges.clone(),
        }),
        entry(&api::DailyCheck {
            day,
            bridges: bridges.clone(),
        }),
        entry(&api::DailyRecoverActions {
            day,
            started_at: 1_760_000_000_000,
            intents: vec![PaidIntent {
                attempt_id: "a1b2c3".into(),
                user: 7,
                day,
                started_at: 1_760_000_000_000,
                action: PaidAction::Hint,
                bridges: bridges.clone(),
                id: "r1".into(),
            }],
            expected_user: 7,
        }),
        entry(&api::DailyPaidAction {
            day,
            bridges: bridges.clone(),
            action: PaidAction::Check,
            request_id: "r2".into(),
            expected_user: 7,
            expected_attempt: "a1b2c3".into(),
        }),
        entry(&api::DailyReveal { day }),
        entry(&api::SubmitDaily {
            day,
            bridges: bridges.clone(),
            moves: 2,
            log: log(),
            utc_offset: Some(120),
        }),
        entry(&api::DailyAttemptInfo {
            day,
            owner: owner(),
        }),
        entry(&api::SubmitDailyAttempt {
            day,
            bridges,
            moves: 2,
            log: log(),
            utc_offset: Some(-300),
            owner: owner(),
        }),
        entry(&api::DailyLeaderboard { day: Some(day) }),
        entry(&api::LoadHallOfFame {}),
        entry(&api::MonthLeaderboard {
            month: Some(24_321),
        }),
        entry(&api::ArchivedDaily { day }),
        // dojo
        entry(&api::LoadDojoBoard {}),
        entry(&api::LoadDojoSeasons {}),
        entry(&api::LoadDojoSubmission { id: 12 }),
        entry(&api::LoadDojoMe {}),
        entry(&api::LoadDojoBenchmarks {}),
        // groups
        entry(&api::MyGroups {}),
        entry(&api::CreateGroup {
            name: "Tea club".into(),
        }),
        entry(&api::GroupInvite {
            code: "k3y9q2".into(),
        }),
        entry(&api::JoinGroup {
            code: "k3y9q2".into(),
        }),
        entry(&api::LoadGroup { id: 2 }),
        entry(&api::LoadGroupWeek {
            id: 2,
            monday: Some(20_377),
        }),
        entry(&api::LeaveGroup { id: 2 }),
        entry(&api::RemoveMember { id: 2, member: 9 }),
        entry(&api::RotateInvite { id: 2 }),
        entry(&api::GroupBoards {}),
        // hanko
        entry(&api::SetHanko {
            group: 2,
            target: 9,
            day,
            kind: HankoKind::Cheers,
            on: true,
        }),
        entry(&api::OpenHanko { group: Some(2) }),
        entry(&api::UnreadHankoCounts {}),
        // kadai
        entry(&api::PostGroupPuzzle {
            group: 2,
            puzzle: puzzle(),
            title: "Spiral".into(),
        }),
        entry(&api::LoadGroupPuzzles { group: 2 }),
        entry(&api::LoadGroupPuzzle { id: 4 }),
        entry(&api::StartGroupPuzzle { id: 4 }),
        entry(&api::GroupPuzzleHint {
            id: 4,
            bridges: vec![2, 1, 0],
        }),
        entry(&api::GroupPuzzleCheck {
            id: 4,
            bridges: vec![2, 1, 0],
        }),
        entry(&api::GroupPuzzleReveal { id: 4 }),
        entry(&api::SubmitGroupPuzzle {
            id: 4,
            bridges: vec![2, 1, 0],
            moves: 12,
            log: log(),
        }),
        entry(&api::WithdrawGroupPuzzle { id: 4 }),
        entry(&api::SeenGroupPuzzleKudos { id: 4 }),
        entry(&api::LoadPuzzleReplay { id: 4 }),
        // moderation
        entry(&api::ReportPlayer {
            player: 9,
            reason: ReportReason::Name,
            note: "rude".into(),
        }),
        entry(&api::BlockPlayer {
            player: 9,
            on: true,
        }),
        entry(&api::MyBlocks {}),
        // puzzle
        entry(&api::GeneratePuzzle {
            params: GenerateParams {
                width: 7,
                height: 7,
                difficulty: Difficulty::Hard,
                seed: u64::MAX,
            },
        }),
        entry(&api::SolvePuzzle {
            puzzle: puzzle(),
            config: SolverConfig {
                kind: SolverKind::Cdcl,
                heuristic: Heuristic::Vsids,
                restarts: Restarts::Geometric {
                    threshold: 100,
                    factor_percent: 150,
                },
                subsumption: true,
                timeout_ms: 10_000,
            },
            unique: true,
        }),
        entry(&api::PuzzleCnf { puzzle: puzzle() }),
        // replay
        entry(&api::LoadReplay {
            group: Some(2),
            day,
        }),
        // rivals
        entry(&api::MyRivals {}),
        entry(&api::SetRival {
            player: 9,
            on: true,
        }),
        entry(&api::TodaysRivals {}),
        entry(&api::LoadRivalsBoard {}),
        entry(&api::RivalRecord { player: 9 }),
        // river
        entry(&api::LoadGroupRiver { id: 2 }),
    ]
}

/// The request of every registered server function, by path. Fails for a server function
/// without one.
fn by_path() -> Vec<Entry> {
    let mut entries = requests();
    entries.sort_by_key(|e| e.path);
    for pair in entries.windows(2) {
        assert_ne!(pair[0].path, pair[1].path, "two requests for one path");
    }
    let listed: Vec<_> = entries.iter().map(|e| e.path).collect();
    for path in registered() {
        assert!(
            listed.contains(&path),
            "{path} has no request: add one with every argument filled in to `requests` in \
             src/golden/api.rs, then write the golden files with GOLDEN_UPDATE=1"
        );
    }
    entries
}

#[test]
fn server_function_paths() {
    let lines: Vec<_> = by_path()
        .iter()
        .map(|e| format!("{} {}", e.path, e.wire))
        .collect();
    check("api_paths.txt", &lines.join("\n"), PATHS);
}

#[test]
fn server_function_requests() {
    let entries = by_path();
    let lines: Vec<_> = entries
        .iter()
        .map(|e| format!("{} {}", e.path, e.body))
        .collect();
    check("requests.txt", &lines.join("\n"), REQUESTS);
    // Every request of the golden file loads as its function's arguments
    let golden = std::fs::read_to_string(path("requests.txt")).unwrap();
    for line in golden.lines() {
        let (path, body) = line.split_once(' ').unwrap();
        let entry = entries
            .iter()
            .find(|e| e.path == path)
            .unwrap_or_else(|| panic!("{path}: no such server function any more. {REQUESTS}"));
        let loaded = (entry.load)(body).unwrap_or_else(|e| {
            panic!("{path}: the request doesn't load any more ({e}). {REQUESTS}")
        });
        assert_eq!(
            loaded, body,
            "{path}: the request loads differently. {REQUESTS}"
        );
    }
}

/// Apps from before an argument with `#[server(default)]` existed don't send it.
#[test]
fn requests_without_later_arguments_load() {
    let submit: api::SubmitDaily =
        serde_json::from_str(r#"{"day":20381,"bridges":[2,1],"moves":2}"#).unwrap();
    assert!(
        submit.log.is_empty() && submit.utc_offset.is_none(),
        "{REQUESTS}"
    );
    let messages: api::AdminMessages = serde_json::from_str("{}").unwrap();
    assert_eq!(messages.before, None, "{REQUESTS}");
    // Intents saved before they had the attempt's id
    let intent: PaidIntent = serde_json::from_str(
        r#"{"user":7,"day":20381,"started_at":0,"action":"Hint","bridges":[],"id":"r1"}"#,
    )
    .unwrap();
    assert_eq!(intent.attempt_id, "", "{REQUESTS}");
}
