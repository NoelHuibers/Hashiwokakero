//! The SAT Dojo's own routes: uploads, house entries, the worker's reports, the board for
//! scripts. The dispatcher stays off (no `DOJO_URL`), submissions just wait in the queue.

use reqwest::multipart::{Form, Part};

use super::*;
use crate::api::{LoadDojoBoard, LoadDojoMe, LoadDojoSubmission, StartDaily};
use crate::model::DojoState;
use crate::server::dojo::season;

/// A module the Dojo accepts, made different by `extra`.
fn module(extra: u8) -> Vec<u8> {
    vec![0, b'a', b's', b'm', 1, 0, 0, 0, 5, 3, 1, 0, extra]
}

fn upload(name: &str, wasm: Vec<u8>) -> Form {
    Form::new()
        .text("name", name.to_string())
        .text("description", "tries hard")
        .text("source_url", "https://example.com/solver")
        .part("wasm", Part::bytes(wasm).file_name("solver.wasm"))
}

async fn submit(browser: &Browser, form: Form) -> String {
    let res = browser
        .post("/dojo/submit")
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::SEE_OTHER);
    res.headers()["location"].to_str().unwrap().to_string()
}

#[tokio::test]
async fn uploads_are_checked() {
    let server = Server::start().await;
    let nobody = server.browser();
    assert_eq!(
        submit(&nobody, upload("Mine", module(1))).await,
        "/dojo?error=sign_in#submit"
    );
    let guest = server.browser();
    guest.call(StartDaily { device: None }).await.unwrap();
    assert_eq!(
        submit(&guest, upload("Mine", module(1))).await,
        "/dojo?error=sign_in#submit"
    );
    let browser = Browser::signed_up(&server, "dojo_dan").await;
    assert_eq!(
        submit(&browser, upload("Mine", module(1))).await,
        "/dojo?error=account_age#submit"
    );

    // A day later
    let conn = conn().await;
    let id = user_id(&conn, "dojo_dan").await;
    conn.execute("UPDATE users SET created_at = 0 WHERE id = ?1", params![id])
        .await
        .unwrap();
    assert_eq!(
        submit(&browser, upload("Mine", b"not wasm at all".to_vec())).await,
        "/dojo?error=not_wasm#submit"
    );
    // Back to the page in the player's language, but nowhere else
    assert_eq!(
        submit(&browser, upload("", module(1)).text("prefix", "/de")).await,
        "/de/dojo?error=name#submit"
    );
    let added = submit(
        &browser,
        upload("Mine", module(1)).text("prefix", "https://evil.example"),
    )
    .await;
    let id: i64 = added.strip_prefix("/dojo/").unwrap().parse().unwrap();
    assert_eq!(
        submit(&browser, upload("Mine again", module(1))).await,
        "/dojo?error=duplicate#submit"
    );
    // Over the body limit: reading the form stops there
    assert_eq!(
        submit(
            &browser,
            upload("Huge", vec![0; ::dojo::MAX_WASM_BYTES + 128 * 1024])
        )
        .await,
        "/dojo?error=size#submit"
    );

    let mine = browser.call(LoadDojoMe {}).await.unwrap();
    assert!(mine.registered);
    assert_eq!(mine.left_today, crate::server::dojo::PER_DAY - 1);
    assert!(mine.submissions.iter().any(|s| s.id == id));
    let submission = browser
        .call(LoadDojoSubmission { id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(submission.state, DojoState::Queued);
    assert!(submission.mine);
    assert_eq!(
        nobody.call(LoadDojoSubmission { id: -1 }).await.unwrap(),
        None
    );
    assert!(!nobody.call(LoadDojoMe {}).await.unwrap().registered);
}

#[tokio::test]
async fn house_entries_need_the_admin_token() {
    let server = Server::start().await;
    let script = server.browser();
    let house =
        |name: &str, wasm: Vec<u8>| script.post(&format!("/dojo/house?name={name}")).body(wasm);
    let res = house("House", module(40)).send().await.unwrap();
    assert_status(&res, StatusCode::UNAUTHORIZED);
    let res = house("House", module(40))
        .bearer_auth("not the token")
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::UNAUTHORIZED);
    let res = house("House", module(40))
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::CREATED);
    let id: i64 = res.text().await.unwrap().parse().unwrap();
    let res = house("House%20again", module(40))
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.text().await.unwrap(), "duplicate");
    let res = house("Huge", vec![0; ::dojo::MAX_WASM_BYTES + 1])
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::PAYLOAD_TOO_LARGE);
    let res = script
        .post("/dojo/house")
        .bearer_auth(ADMIN_TOKEN)
        .body(module(41))
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::BAD_REQUEST);

    let submission = script
        .call(LoadDojoSubmission { id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(submission.name, "House");
}

#[tokio::test]
async fn reports_need_the_token_of_their_job() {
    let server = Server::start().await;
    let worker = server.browser();
    let res = worker
        .post("/dojo/house?name=Reported")
        .bearer_auth(ADMIN_TOKEN)
        .body(module(50))
        .send()
        .await
        .unwrap();
    let id: i64 = res.text().await.unwrap().parse().unwrap();
    let report = serde_json::json!({
        "evaluation_version": ::dojo::EVALUATION_VERSION,
        "type": "progress",
        "id": id,
        "season": season(),
        "done": 3,
        "total": 20,
    })
    .to_string();
    let send = |token: Option<&str>, body: String| {
        let req = worker
            .post("/dojo/report")
            .header(CONTENT_TYPE, "application/json")
            .body(body);
        match token {
            Some(token) => req.bearer_auth(token),
            None => req,
        }
        .send()
    };
    // Before the job was sent there is no token at all
    for token in [None, Some(""), Some(ADMIN_TOKEN)] {
        assert_status(
            &send(token, report.clone()).await.unwrap(),
            StatusCode::UNAUTHORIZED,
        );
    }
    let job_token = "f".repeat(64);
    conn()
        .await
        .execute(
            "UPDATE dojo_submissions SET job_token = ?2, state = 'running' WHERE id = ?1",
            params![id, job_token.clone()],
        )
        .await
        .unwrap();
    for token in [None, Some("f"), Some(&*"e".repeat(64)), Some(ADMIN_TOKEN)] {
        assert_status(
            &send(token, report.clone()).await.unwrap(),
            StatusCode::UNAUTHORIZED,
        );
    }
    let res = send(Some(&job_token), "{\"type\": \"progress\"}".into())
        .await
        .unwrap();
    assert_status(&res, StatusCode::UNPROCESSABLE_ENTITY);
    assert_status(
        &send(Some(&job_token), report).await.unwrap(),
        StatusCode::NO_CONTENT,
    );
    let submission = worker
        .call(LoadDojoSubmission { id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(submission.state, DojoState::Running);
}

#[tokio::test]
async fn the_board_for_scripts() {
    let server = Server::start().await;
    let browser = server.browser();
    let res = browser.get("/dojo/board.json").await;
    assert_status(&res, StatusCode::OK);
    // Read from other sites too
    assert_eq!(res.headers()["access-control-allow-origin"], "*");
    assert_eq!(res.headers()[CONTENT_TYPE], "application/json");
    let json: serde_json::Value = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
    for key in ["season", "formulas", "budget", "entries", "queued"] {
        assert!(json.get(key).is_some(), "{key} missing in {json}");
    }
    assert_eq!(json["season"], season());
    assert!(json["entries"].is_array());
    // The same as the page gets
    let board = browser.call(LoadDojoBoard {}).await.unwrap();
    assert_eq!(serde_json::to_value(&board).unwrap(), json);
}
