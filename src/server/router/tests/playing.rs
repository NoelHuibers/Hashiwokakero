//! The daily over HTTP: a guest plays, the boards, and what the puzzle tools refuse.

use common::{Board, SolveOutcome, SolverConfig};

use super::*;
use crate::api::{
    ArchivedDaily, CurrentUser, DailyCheck, DailyHint, DailyLeaderboard, DailyReveal, DailyToday,
    GeneratePuzzle, LoadHallOfFame, MonthLeaderboard, PuzzleCnf, SolvePuzzle, StartDaily,
    SubmitDaily,
};
use crate::model::MoveEvent;

/// One click per bridge, 1.5 s apart, as a player would place them.
fn played(solution: &[u8]) -> Vec<MoveEvent> {
    let mut log = Vec::new();
    for (edge, &count) in solution.iter().enumerate() {
        for c in 1..=count {
            log.push(MoveEvent {
                t: u32::try_from(log.len() + 1).unwrap() * 1500,
                edge: u16::try_from(edge).unwrap(),
                count: c,
                tap: true,
            });
        }
    }
    log
}

fn submit(day: i64, bridges: Vec<u8>, log: Vec<MoveEvent>) -> SubmitDaily {
    SubmitDaily {
        day,
        bridges,
        moves: 1,
        log,
        utc_offset: Some(-120),
    }
}

#[tokio::test]
async fn a_guest_plays_the_daily() {
    let server = Server::start().await;
    let browser = server.browser();
    let before = browser.call(DailyToday {}).await.unwrap();
    // Nothing to study before the clock runs, and nobody to remember yet
    assert!(before.puzzle.is_none() && before.attempt.is_none());
    assert!(browser.session().is_none());

    let started = browser
        .call_with_headers(StartDaily {
            device: Some("device-of-the-guest".into()),
        })
        .await;
    let cookie = session_cookie(&started.headers);
    assert!(cookie.contains("HttpOnly"));
    let started = started.result.unwrap();
    let day = started.day;
    let puzzle = started.puzzle.unwrap();
    assert!(started.attempt.is_some());
    assert!(browser.call(CurrentUser {}).await.unwrap().unwrap().guest);
    // Starting again neither resets the clock nor makes another guest
    let again = browser.call_with_headers(StartDaily { device: None }).await;
    assert!(again.headers.get(reqwest::header::SET_COOKIE).is_none());
    assert_eq!(
        again.result.unwrap().attempt.unwrap().started_at,
        started.attempt.unwrap().started_at
    );

    let empty = Board::new(puzzle.clone()).empty_bridges();
    let (edge, count) = browser
        .call(DailyHint {
            day,
            bridges: empty.clone(),
        })
        .await
        .unwrap()
        .unwrap();
    assert!(edge < empty.len() && (1..=2).contains(&count));
    let mut wrong = empty.clone();
    wrong[0] = 2;
    let checked = browser
        .call(DailyCheck {
            day,
            bridges: wrong.clone(),
        })
        .await
        .unwrap();
    assert!(checked.iter().all(|&e| e < empty.len()));
    assert_eq!(
        api_error(
            browser
                .call(DailyCheck {
                    day,
                    bridges: vec![1]
                })
                .await
        ),
        ApiError::WrongPuzzle
    );
    assert_eq!(
        api_error(browser.call(submit(day, wrong, vec![])).await),
        ApiError::NotSolved
    );

    // Played honestly for ten minutes: ranked
    let solution = daily::get_or_create(&conn().await, day)
        .await
        .unwrap()
        .solution;
    let id = browser.call(CurrentUser {}).await.unwrap().unwrap().id;
    conn()
        .await
        .execute(
            "UPDATE attempts SET started_at = started_at - 600000 WHERE user_id = ?1 AND day = ?2",
            params![id, day],
        )
        .await
        .unwrap();
    let result = browser
        .call(submit(day, solution.clone(), played(&solution)))
        .await
        .unwrap();
    assert!(result.flag.is_none(), "{:?}", result.flag);
    assert!(result.rank >= 1 && result.finishers >= 1);
    assert!(result.time_ms >= 600_000);
    assert_eq!(
        api_error(
            browser
                .call(submit(day, solution.clone(), played(&solution)))
                .await
        ),
        ApiError::AlreadyFinished
    );
    assert_eq!(
        api_error(
            browser
                .call(DailyHint {
                    day,
                    bridges: empty
                })
                .await
        ),
        ApiError::AlreadyFinished
    );

    let board = browser.call(DailyLeaderboard { day: None }).await.unwrap();
    assert_eq!(board.day, day);
    let me = board.me.unwrap();
    assert_eq!((me.user_id, me.rank), (id, result.rank));
    assert!(board.entries.iter().any(|e| e.user_id == id && e.guest));
    // Days to come are capped at today
    let later = browser
        .call(DailyLeaderboard { day: Some(day + 5) })
        .await
        .unwrap();
    assert_eq!(later.day, day);
    let month = browser
        .call(MonthLeaderboard { month: None })
        .await
        .unwrap();
    assert_eq!(month.me.unwrap().user_id, id);
    let fame = browser.call(LoadHallOfFame {}).await.unwrap();
    assert!(fame.solved.iter().any(|e| e.user_id == id));
    // Finished, the solvers may look at it
    let cnf = browser.call(PuzzleCnf { puzzle }).await.unwrap();
    assert!(cnf.contains("p cnf"));
}

#[tokio::test]
async fn giving_up_and_playing_without_a_session() {
    let server = Server::start().await;
    let day = daily::today();
    let nobody = server.browser();
    let empty = vec![0; 3];
    assert_eq!(
        api_error(
            nobody
                .call(DailyHint {
                    day,
                    bridges: empty.clone()
                })
                .await
        ),
        ApiError::SessionExpired
    );
    assert_eq!(
        api_error(nobody.call(DailyReveal { day }).await),
        ApiError::SessionExpired
    );
    assert_eq!(
        api_error(nobody.call(submit(day, empty.clone(), vec![])).await),
        ApiError::SessionExpired
    );
    let signed_up = Browser::signed_up(&server, "not_started").await;
    assert_eq!(
        api_error(
            signed_up
                .call(DailyCheck {
                    day,
                    bridges: empty
                })
                .await
        ),
        ApiError::NotStarted
    );

    let browser = server.browser();
    let puzzle = browser
        .call(StartDaily { device: None })
        .await
        .unwrap()
        .puzzle
        .unwrap();
    // Our solvers don't help with today's daily
    let config = SolverConfig::default();
    assert_eq!(
        api_error(
            browser
                .call(SolvePuzzle {
                    puzzle: puzzle.clone(),
                    config,
                    unique: false,
                })
                .await
        ),
        ApiError::DailyLocked
    );
    assert_eq!(
        api_error(
            browser
                .call(PuzzleCnf {
                    puzzle: puzzle.clone()
                })
                .await
        ),
        ApiError::DailyLocked
    );
    let solution = browser.call(DailyReveal { day }).await.unwrap();
    assert_eq!(solution.len(), Board::new(puzzle.clone()).edges.len());
    assert_eq!(
        api_error(browser.call(submit(day, solution, vec![])).await),
        ApiError::AlreadyFinished
    );
    let id = browser.call(CurrentUser {}).await.unwrap().unwrap().id;
    let board = browser
        .call(DailyLeaderboard { day: Some(day) })
        .await
        .unwrap();
    assert!(board.entries.iter().all(|e| e.user_id != id));
    // Now it can be solved
    let report = browser
        .call(SolvePuzzle {
            puzzle,
            config,
            unique: true,
        })
        .await
        .unwrap();
    assert!(
        matches!(
            report.outcome,
            SolveOutcome::Solved {
                unique: Some(true),
                ..
            }
        ),
        "{report:?}"
    );
}

#[tokio::test]
async fn puzzles_and_the_archive() {
    let server = Server::start().await;
    let browser = server.browser();
    let (width, height) = Difficulty::Easy.size();
    let generated = browser
        .call(GeneratePuzzle {
            params: GenerateParams {
                width,
                height,
                difficulty: Difficulty::Easy,
                seed: 7,
            },
        })
        .await
        .unwrap();
    assert_eq!(
        generated.solution.len(),
        Board::new(generated.puzzle.clone()).edges.len()
    );
    let cnf = browser
        .call(PuzzleCnf {
            puzzle: generated.puzzle,
        })
        .await
        .unwrap();
    assert!(cnf.starts_with("c ") || cnf.starts_with("p cnf"), "{cnf}");

    let today = daily::today();
    assert_eq!(
        api_error(browser.call(ArchivedDaily { day: today }).await),
        ApiError::ArchiveOnlyPast
    );
    // Days before the tests began have no daily
    assert_eq!(
        browser
            .call(ArchivedDaily { day: today - 400 })
            .await
            .unwrap(),
        None
    );
    // A browser decodes even an error it doesn't know (here: a body that isn't the arguments)
    let res = browser
        .post(DailyHint::PATH)
        .header(CONTENT_TYPE, "application/json")
        .body("{\"day\": \"soon\"}")
        .send()
        .await
        .unwrap();
    assert!(res.status().is_client_error() || res.status().is_server_error());
    let error: ServerFnError = ServerFnError::de(res.bytes().await.unwrap());
    assert_eq!(ApiError::from_message(&error.to_string()), None);
}

/// Admission runs before decoding or spawning work, and equivalent IPv6 /64 spellings cannot
/// get a fresh budget. An arbitrary forwarding header must not select another bucket.
#[tokio::test]
async fn expensive_requests_share_the_network_budget_before_parsing() {
    let server = Server::start().await;
    let browser = server.browser_from("2001:db8:aa11::1");
    for n in 0..32 {
        let response = browser
            .post(GeneratePuzzle::PATH)
            .header(CONTENT_TYPE, "application/json")
            .header("x-forwarded-for", format!("198.51.100.{n}"))
            .body("not JSON")
            .send()
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }
    let response = server
        .browser_from("2001:0db8:aa11:0000::2")
        .post(GeneratePuzzle::PATH)
        .body("not JSON")
        .send()
        .await
        .unwrap();
    assert_status(&response, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["retry-after"], "60");
}
