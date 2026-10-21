//! The SAT Dojo's own routes: uploads, house entries, the worker's reports, the board for
//! scripts. The dispatcher stays off (no `DOJO_URL`), submissions just wait in the queue.

use reqwest::multipart::{Form, Part};

use super::*;
use crate::api::{LoadDojoBoard, LoadDojoMe, LoadDojoSeasons, LoadDojoSubmission, StartDaily};
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
    assert!(
        submission
            .source_url
            .is_some_and(|url| url.ends_with("/tree/main/dojo/house"))
    );
}

/// The reference solvers link to their own directory, and a new version of a house entry
/// replaces the old one (scripts/dojo-house.sh uploads every house solver after a deploy).
#[tokio::test]
async fn house_entries_are_replaced_by_new_versions() {
    let server = Server::start().await;
    let script = server.browser();
    let house = |query: &str, wasm: Vec<u8>| {
        script
            .post(&format!("/dojo/house?{query}"))
            .bearer_auth(ADMIN_TOKEN)
            .body(wasm)
    };
    let res = house("name=Kissat&source=dojo/reference", module(50))
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::CREATED);
    let first: i64 = res.text().await.unwrap().parse().unwrap();
    let other = house("name=MiniSat&source=dojo/reference", module(51))
        .send()
        .await
        .unwrap();
    assert_status(&other, StatusCode::CREATED);
    let other: i64 = other.text().await.unwrap().parse().unwrap();
    let submission = script
        .call(LoadDojoSubmission { id: first })
        .await
        .unwrap()
        .unwrap();
    assert!(
        submission
            .source_url
            .is_some_and(|url| url.ends_with("/tree/main/dojo/reference"))
    );

    // Unchanged: nothing happens
    let res = house("name=Kissat&source=dojo/reference", module(50))
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.text().await.unwrap(), "duplicate");
    // A new version: the old one goes, the other solver stays
    let res = house("name=Kissat&source=dojo/reference", module(52))
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::CREATED);
    let second: i64 = res.text().await.unwrap().parse().unwrap();
    for (id, kept) in [(first, false), (second, true), (other, true)] {
        let found = script.call(LoadDojoSubmission { id }).await.unwrap();
        assert_eq!(found.is_some(), kept, "#{id}");
    }

    for bad in ["../elsewhere", "/abs", "dojo//x", "dojo/x?y", ""] {
        let res = house(&format!("name=Bad&source={bad}"), module(53))
            .send()
            .await
            .unwrap();
        assert_status(&res, StatusCode::BAD_REQUEST);
    }
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
            "UPDATE dojo_submissions SET job_token = ?2 WHERE id = ?1",
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
    for key in [
        "season",
        "formulas",
        "budget",
        "entries",
        "queued",
        "commitment",
        "generator_version",
        "rules_version",
        "portfolio",
    ] {
        assert!(json.get(key).is_some(), "{key} missing in {json}");
    }
    assert_eq!(json["season"], season());
    assert!(json["entries"].is_array());
    // The same as the page gets
    let board = browser.call(LoadDojoBoard {}).await.unwrap();
    assert_eq!(serde_json::to_value(&board).unwrap(), json);
}

/// The board for scripts with the metrics beyond PAR-2: two house entries with results, their
/// breakdown, contributions and the cactus plot, the held-out family as the mystery.
#[tokio::test]
async fn the_board_with_its_metrics() {
    let server = Server::start().await;
    let browser = server.browser();
    let conn = conn().await;
    let result = |name: &str, family: &str, expected: &str, fuel: Option<u64>| {
        serde_json::json!({
            "name": name, "family": family, "expected": expected,
            "outcome": match fuel { Some(_) => serde_json::json!({"Solved": expected}), None => "Timeout".into() },
            "fuel": fuel.unwrap_or(::dojo::DEFAULT_BUDGET),
        })
    };
    let score = serde_json::json!({"par2": 5.0, "solved": 2, "total": 3, "wrong": 0,
        "budget": ::dojo::DEFAULT_BUDGET, "fuel": 0.1, "families": []})
    .to_string();
    for (id, name, par2, results) in [
        (
            9101,
            "Metrics A",
            5.0,
            [Some(1_000_000), Some(2_000_000), None],
        ),
        (
            9102,
            "Metrics B",
            6.0,
            [Some(5_000_000), None, Some(3_000_000)],
        ),
    ] {
        let results = serde_json::json!([
            result("a", "Hashi", "Sat", results[0]),
            result("b", "Pigeonhole", "Unsat", results[1]),
            result("c", "Quasigroup", "Sat", results[2]),
        ])
        .to_string();
        conn.execute(
            "INSERT INTO dojo_submissions (id, user_id, house, name, description, wasm, sha256, size, created_at,
                updated_at, state, season, progress, total, score, par2, solved, wrong, code_hash, rules_version, results)
             VALUES (?1, NULL, 1, ?2, '', X'00', ?2, 1, 1, 1, 'done', ?3, 3, 3, ?4, ?5, 2, 0, ?2, ?6, ?7)",
            params![id, name, season(), score.clone(), par2, ::dojo::RULES_VERSION, results],
        )
        .await
        .unwrap();
    }
    let json: serde_json::Value =
        serde_json::from_slice(&browser.get("/dojo/board.json").await.bytes().await.unwrap())
            .unwrap();
    let entries = json["entries"].as_array().unwrap();
    let a = entries.iter().find(|e| e["name"] == "Metrics A").unwrap();
    let b = entries.iter().find(|e| e["name"] == "Metrics B").unwrap();
    // Each solved one formula the other didn't: SOTA 1.5 each
    assert_eq!(a["contribution"]["sota"], 1.5);
    assert_eq!(b["contribution"]["sota"], 1.5);
    assert_eq!(a["breakdown"]["held_out"], 1);
    assert_eq!(a["breakdown"]["total"]["sat"]["total"], 1);
    assert_eq!(a["breakdown"]["families"][2]["family"], "Mystery");
    assert!(
        !json.to_string().contains("Quasigroup"),
        "the held-out family leaked"
    );
    let portfolio = &json["portfolio"];
    assert_eq!(portfolio["entries"], 2);
    assert_eq!(portfolio["vbs"]["solved"], 3);
    assert_eq!(
        portfolio["cactus"]["vbs"],
        serde_json::json!([1_000_000, 2_000_000, 3_000_000])
    );
    // The submission page has its plot too
    let page = browser
        .call(LoadDojoSubmission { id: 9102 })
        .await
        .unwrap()
        .unwrap();
    assert!(page.contribution.is_some());
    assert_eq!(page.portfolio.unwrap().cactus.curves.len(), 2);
    assert_status(&browser.get("/dojo/9102").await, StatusCode::OK);
}

/// A finished season (the one before the tests' current season), as the archive keeps it: the
/// season, its set, two archived entries. The current season's row has a seed that must never
/// be shown.
async fn archive_a_season(conn: &Connection) -> Vec<u8> {
    let current = season();
    let past = current - 1;
    let set = vec![0x1f, 0x8b, 8, 0, 1, 2, 3];
    let results = serde_json::json!([
        {"name": "hashi-01", "family": "Hashi", "expected": "Sat", "outcome": {"Solved": "Sat"}, "fuel": 1234},
        {"name": "pigeonhole-01", "family": "Pigeonhole", "expected": "Unsat", "outcome": "Timeout", "fuel": 10_000_000_000_u64}
    ])
    .to_string();
    let score = serde_json::json!({"par2": 10.0, "solved": 1, "total": 2, "wrong": 0, "budget": 10_000_000_000_u64,
        "fuel": 0.0, "families": []})
    .to_string();
    for sql in [
        "INSERT OR REPLACE INTO dojo_seasons (season, started_at, ended_at, commitment, seed, generator_version, rules_version)
         VALUES (?1, 1000, 2000, 'archived-commitment', 'archived-seed', 1, 1)",
        "INSERT OR REPLACE INTO dojo_seasons (season, started_at, ended_at, commitment, seed, generator_version, rules_version)
         VALUES (?1 + 1, 2000, NULL, 'current-commitment', 'must-not-leak', 1, 1)",
        "INSERT OR REPLACE INTO dojo_archive (season, submission_id, user_id, house, name, created_at, par2, solved, wrong, score, results, rules_version, rank)
         VALUES (?1, 9001, NULL, 1, 'Archived house', 1000, 10.0, 1, 0, ?3, ?4, 1, 1)",
        "INSERT OR REPLACE INTO dojo_archive (season, submission_id, user_id, house, name, created_at, par2, solved, wrong, score, results, rules_version, rank)
         VALUES (?1, 9002, NULL, 0, 'Wrong one', 1000, 20.0, 1, 1, ?3, ?4, 1, NULL)",
        "INSERT OR REPLACE INTO dojo_season_sets (season, generator_version, archive, sha256, created_at) VALUES (?1, 1, ?2, 'sha', 2000)",
        "INSERT OR REPLACE INTO dojo_season_sets (season, generator_version, archive, sha256, created_at) VALUES (?1 + 1, 1, ?2, 'sha', 2000)",
    ] {
        conn.execute(sql, params![past, set.clone(), score.clone(), results.clone()])
            .await
            .unwrap();
    }
    set
}

#[tokio::test]
async fn the_season_archive() {
    let server = Server::start().await;
    let browser = server.browser();
    let conn = conn().await;
    let set = archive_a_season(&conn).await;
    let (current, past) = (season(), season() - 1);

    let res = browser.get("/dojo/seasons.json").await;
    assert_status(&res, StatusCode::OK);
    assert_eq!(res.headers()["access-control-allow-origin"], "*");
    let json: serde_json::Value = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
    assert_eq!(json["current"]["season"], current);
    assert_eq!(json["current"]["commitment"], "current-commitment");
    assert_eq!(json["current"]["seed"], serde_json::Value::Null);
    assert_eq!(json["current"]["set_size"], serde_json::Value::Null);
    assert_eq!(json["past"][0]["season"], past);
    assert_eq!(json["past"][0]["seed"], "archived-seed");
    assert_eq!(json["past"][0]["set_size"], set.len());
    assert_eq!(json["past"][0]["entries"], 2);
    // Only the ranked entry is on the final board
    assert_eq!(json["past"][0]["board"][0]["name"], "Archived house");
    assert_eq!(json["past"][0]["board"][0]["author"], "Hashi");
    assert_eq!(json["past"][0]["board"].as_array().unwrap().len(), 1);
    // Its cactus plot: the entry's curve and the VBS, the same
    let cactus = &json["past"][0]["portfolio"]["cactus"];
    assert_eq!(cactus["vbs"], serde_json::json!([1230]));
    assert_eq!(cactus["curves"][0]["fuel"], serde_json::json!([1230]));
    assert_eq!(cactus["curves"][0]["name"], "Archived house");
    assert_eq!(cactus["formulas"], 2);
    assert!(!json.to_string().contains("must-not-leak"));
    // The same as the page gets
    let seasons = browser.call(LoadDojoSeasons {}).await.unwrap();
    assert_eq!(serde_json::to_value(&seasons).unwrap(), json);
    // The board shows the current season's commitment
    let board = browser.call(LoadDojoBoard {}).await.unwrap();
    assert_eq!(board.commitment, "current-commitment");

    let res = browser
        .get(&format!("/dojo/seasons/{past}/set.tar.gz"))
        .await;
    assert_status(&res, StatusCode::OK);
    assert_eq!(res.headers()[CONTENT_TYPE], "application/gzip");
    assert_eq!(
        res.headers()["content-disposition"],
        format!("attachment; filename=\"dojo-season-{past}.tar.gz\"").as_str()
    );
    assert_eq!(res.bytes().await.unwrap().to_vec(), set);

    let res = browser
        .get(&format!("/dojo/seasons/{past}/results.csv"))
        .await;
    assert_status(&res, StatusCode::OK);
    assert_eq!(res.headers()[CONTENT_TYPE], "text/csv; charset=utf-8");
    let csv = res.text().await.unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(
        lines,
        [
            "season,entry,solver,author,house,rank,rules_version,instance,family,expected,outcome,fuel,budget,run_budget",
            &*format!(
                "{past},9001,Archived house,Hashi,1,1,1,hashi-01,hashi,SAT,solved,1234,10000000000,10000000000"
            ),
            &*format!(
                "{past},9001,Archived house,Hashi,1,1,1,pigeonhole-01,pigeonhole,UNSAT,timeout,10000000000,10000000000,10000000000"
            ),
            &*format!(
                "{past},9002,Wrong one,,0,,1,hashi-01,hashi,SAT,solved,1234,10000000000,10000000000"
            ),
            &*format!(
                "{past},9002,Wrong one,,0,,1,pigeonhole-01,pigeonhole,UNSAT,timeout,10000000000,10000000000,10000000000"
            ),
        ]
    );

    let res = browser
        .get(&format!("/dojo/seasons/{past}/results.json"))
        .await;
    assert_status(&res, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
    assert_eq!(json["seed"], "archived-seed");
    assert_eq!(json["commitment"], "archived-commitment");
    assert_eq!(json["entries"][0]["rank"], 1);
    assert_eq!(json["entries"][1]["rank"], serde_json::Value::Null);
    assert_eq!(json["entries"][0]["results"][0]["fuel"], 1234);
    // The metrics beyond PAR-2: among the final board, which has the house entry only
    assert_eq!(json["vbs"]["solved"], 1);
    assert_eq!(json["vbs"]["total"], 2);
    let ranked = &json["entries"][0];
    assert_eq!(ranked["breakdown"]["total"]["sat"]["solved"], 1);
    assert_eq!(ranked["breakdown"]["total"]["unsat"]["total"], 1);
    assert_eq!(ranked["contribution"]["sota"], 1.0);
    assert_eq!(ranked["contribution"]["relative"], 1.0);
    let shapley = ranked["contribution"]["shapley"].as_f64().unwrap();
    assert!((shapley - (2e10 - 1234.0) / 2.0 / 1e9).abs() < 1e-9);
    assert_eq!(json["entries"][1]["contribution"], serde_json::Value::Null);
    assert_eq!(json["entries"][1]["breakdown"]["total"]["all"]["solved"], 1);

    // Nothing of the current season, later ones or seasons that don't exist
    for path in [
        format!("/dojo/seasons/{current}/set.tar.gz"),
        format!("/dojo/seasons/{current}/results.csv"),
        format!("/dojo/seasons/{current}/results.json"),
        format!("/dojo/seasons/{}/results.json", current + 5),
        "/dojo/seasons/0/set.tar.gz".to_string(),
    ] {
        assert_status(&browser.get(&path).await, StatusCode::NOT_FOUND);
    }
    assert_status(
        &browser.get("/dojo/seasons/first/results.csv").await,
        StatusCode::BAD_REQUEST,
    );
    // The page itself
    assert_status(&browser.get("/dojo/seasons").await, StatusCode::OK);
    assert_status(&browser.get("/ja/dojo/seasons").await, StatusCode::OK);
}

#[tokio::test]
async fn sets_need_the_token_of_their_job() {
    let server = Server::start().await;
    let worker = server.browser();
    let res = worker
        .post("/dojo/house?name=Exported")
        .bearer_auth(ADMIN_TOKEN)
        .body(module(60))
        .send()
        .await
        .unwrap();
    let id: i64 = res.text().await.unwrap().parse().unwrap();
    let conn = conn().await;
    conn.execute(
        "UPDATE dojo_submissions SET job_token = 'set-token' WHERE id = ?1",
        params![id],
    )
    .await
    .unwrap();
    conn.execute(
        "DELETE FROM dojo_season_sets WHERE season = ?1",
        params![season()],
    )
    .await
    .unwrap();
    let url = format!(
        "/dojo/set?id={id}&season={}&generator={}",
        season(),
        ::dojo::GENERATOR_VERSION
    );
    let gz = vec![0x1f, 0x8b, 8, 0, 42];
    let send = |token: &str, body: Vec<u8>| worker.post(&url).bearer_auth(token).body(body).send();
    assert_status(
        &send("wrong", gz.clone()).await.unwrap(),
        StatusCode::UNAUTHORIZED,
    );
    assert_status(
        &send(ADMIN_TOKEN, gz.clone()).await.unwrap(),
        StatusCode::UNAUTHORIZED,
    );
    assert_status(
        &send("set-token", b"plain".to_vec()).await.unwrap(),
        StatusCode::UNPROCESSABLE_ENTITY,
    );
    assert_status(
        &send("set-token", vec![0; crate::server::dojo::MAX_SET_BYTES + 1])
            .await
            .unwrap(),
        StatusCode::PAYLOAD_TOO_LARGE,
    );
    assert_status(
        &send("set-token", gz.clone()).await.unwrap(),
        StatusCode::NO_CONTENT,
    );
    let mut rows = conn
        .query(
            "SELECT archive, generator_version FROM dojo_season_sets WHERE season = ?1",
            params![season()],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    assert_eq!(row.get::<Vec<u8>>(0).unwrap(), gz);
    assert_eq!(row.get::<u32>(1).unwrap(), ::dojo::GENERATOR_VERSION);
    // Not served while the season runs
    assert_status(
        &worker
            .get(&format!("/dojo/seasons/{}/set.tar.gz", season()))
            .await,
        StatusCode::NOT_FOUND,
    );
}

fn benchmark(family: &str, cnf: &str) -> Form {
    Form::new()
        .text("family", family.to_string())
        .text("description", "unit clauses")
        .text("claimed", "SAT")
        .text("license", "1")
        .part(
            "cnf",
            Part::bytes(cnf.as_bytes().to_vec()).file_name("f.cnf"),
        )
}

async fn submit_benchmark(browser: &Browser, form: Form) -> String {
    let res = browser
        .post("/dojo/benchmarks/submit")
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::SEE_OTHER);
    res.headers()["location"].to_str().unwrap().to_string()
}

/// Community benchmarks: the form checks who submits and what, the worker's verdict needs the
/// check's token.
#[tokio::test]
async fn community_benchmarks() {
    use crate::api::LoadDojoBenchmarks;
    use crate::model::BenchmarkState;

    let server = Server::start().await;
    let nobody = server.browser();
    assert_eq!(
        submit_benchmark(&nobody, benchmark("x", "p cnf 1 1\n1 0\n")).await,
        "/dojo/benchmarks?error=sign_in#submit"
    );
    let browser = Browser::signed_up(&server, "bench_bea").await;
    assert_eq!(
        submit_benchmark(&browser, benchmark("x", "p cnf 1 1\n1 0\n")).await,
        "/dojo/benchmarks?error=account_age#submit"
    );
    let conn = conn().await;
    let user = user_id(&conn, "bench_bea").await;
    conn.execute(
        "UPDATE users SET created_at = 0 WHERE id = ?1",
        params![user],
    )
    .await
    .unwrap();
    // A file or a GBD hash, not both and not neither
    let both = benchmark("x", "p cnf 1 1\n1 0\n").text("gbd", "a".repeat(32));
    assert_eq!(
        submit_benchmark(&browser, both).await,
        "/dojo/benchmarks?error=source#submit"
    );
    let neither = Form::new().text("family", "x").text("license", "1");
    assert_eq!(
        submit_benchmark(&browser, neither).await,
        "/dojo/benchmarks?error=source#submit"
    );
    let bad_hash = Form::new()
        .text("family", "x")
        .text("license", "1")
        .text("gbd", "not-a-hash");
    assert_eq!(
        submit_benchmark(&browser, bad_hash).await,
        "/dojo/benchmarks?error=gbd#submit"
    );
    let unlicensed = Form::new()
        .text("family", "x")
        .part("cnf", Part::bytes(b"p cnf 1 1\n1 0\n".to_vec()));
    assert_eq!(
        submit_benchmark(&browser, unlicensed).await,
        "/dojo/benchmarks?error=license#submit"
    );
    let added = submit_benchmark(
        &browser,
        benchmark("units", "p cnf 2 2\n1 0\n2 0\n").text("prefix", "/de"),
    )
    .await;
    let id: i64 = added
        .strip_prefix("/de/dojo/benchmarks?submitted=")
        .and_then(|rest| rest.strip_suffix("#mine"))
        .unwrap()
        .parse()
        .unwrap();
    let page = browser.call(LoadDojoBenchmarks {}).await.unwrap();
    assert_eq!(page.mine.len(), 1);
    assert_eq!(
        (page.mine[0].id, page.mine[0].state),
        (id, BenchmarkState::Queued)
    );
    assert!(!nobody.call(LoadDojoBenchmarks {}).await.unwrap().registered);

    // The verdict, with the check's token only
    let report = serde_json::json!({
        "id": id,
        "verdict": {"type": "verified", "answer": "Sat", "model": [1, 2], "solve_ms": 10},
    })
    .to_string();
    let send = |token: &str| {
        nobody
            .post("/dojo/benchmark-report")
            .header(CONTENT_TYPE, "application/json")
            .bearer_auth(token)
            .body(report.clone())
            .send()
    };
    assert_status(&send(ADMIN_TOKEN).await.unwrap(), StatusCode::UNAUTHORIZED);
    conn.execute(
        "UPDATE dojo_benchmarks SET job_token = ?2, state = 'checking' WHERE id = ?1",
        params![id, "c".repeat(64)],
    )
    .await
    .unwrap();
    assert_status(
        &send(&"d".repeat(64)).await.unwrap(),
        StatusCode::UNAUTHORIZED,
    );
    assert_status(
        &send(&"c".repeat(64)).await.unwrap(),
        StatusCode::NO_CONTENT,
    );
    let page = browser.call(LoadDojoBenchmarks {}).await.unwrap();
    assert_eq!(page.mine[0].state, BenchmarkState::Verified);
    assert_eq!(page.mine[0].answer, Some(::dojo::Answer::Sat));
}
