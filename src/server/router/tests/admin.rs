//! The admin tools: server functions for the dashboard, routes with the admin token.

use reqwest::multipart::{Form, Part};

use super::*;
use crate::api::{
    AdminAttempts, AdminFlag, AdminLog, AdminNames, AdminRenamePlayer, AdminSubmissions,
    ChangeUsername, CurrentUser, LoadAdminOverview, LoadPlayer, Login, StartDaily,
    UpdateAdminSubmission,
};
use crate::model::{Flag, FlaggedName, NameKind, SubmissionAction};

/// A module the Dojo accepts, made different by `extra`.
fn module(extra: u8) -> Vec<u8> {
    vec![0, b'a', b's', b'm', 1, 0, 0, 0, 5, 3, 1, 0, extra]
}

fn house_form(name: &str, wasm: Vec<u8>) -> Form {
    Form::new()
        .text("name", name.to_string())
        .text("description", "ours")
        .part("wasm", Part::bytes(wasm).file_name("solver.wasm"))
}

#[tokio::test]
async fn the_dashboard_is_for_admins_only() {
    let server = Server::start().await;
    let guest = server.browser();
    guest.call(StartDaily { device: None }).await.unwrap();
    let player = Browser::signed_up(&server, "just_a_player").await;
    let day = daily::today();
    for browser in [&server.browser(), &guest, &player] {
        assert_eq!(
            api_error(browser.call(LoadAdminOverview {}).await),
            ApiError::Forbidden
        );
        assert_eq!(
            api_error(browser.call(AdminAttempts { day: None }).await),
            ApiError::Forbidden
        );
        assert_eq!(
            api_error(browser.call(AdminLog { user: 1, day }).await),
            ApiError::Forbidden
        );
        assert_eq!(
            api_error(
                browser
                    .call(AdminFlag {
                        user: 1,
                        day,
                        flag: Some(Flag::Review),
                    })
                    .await
            ),
            ApiError::Forbidden
        );
        assert_eq!(
            api_error(browser.call(AdminSubmissions {}).await),
            ApiError::Forbidden
        );
        assert_eq!(
            api_error(browser.call(AdminNames {}).await),
            ApiError::Forbidden
        );
        assert_eq!(
            api_error(browser.call(AdminRenamePlayer { id: 1 }).await),
            ApiError::Forbidden
        );
        assert_eq!(
            api_error(
                browser
                    .call(UpdateAdminSubmission {
                        id: 1,
                        action: SubmissionAction::Delete,
                    })
                    .await
            ),
            ApiError::Forbidden
        );
        // Unknown to anyone else, the form doesn't even exist
        let res = browser
            .post("/admin/house")
            .multipart(house_form("sneaky", module(90)))
            .send()
            .await
            .unwrap();
        assert_status(&res, StatusCode::NOT_FOUND);
    }
    assert!(!player.call(CurrentUser {}).await.unwrap().unwrap().admin);
}

#[tokio::test]
async fn admins_review_results_and_add_house_entries() {
    let server = Server::start().await;
    let player = server.browser();
    player.call(StartDaily { device: None }).await.unwrap();
    let user = player.call(CurrentUser {}).await.unwrap().unwrap().id;

    let admin = Browser::admin(&server).await;
    assert!(admin.call(CurrentUser {}).await.unwrap().unwrap().admin);
    let overview = admin.call(LoadAdminOverview {}).await.unwrap();
    assert!(overview.users >= 2 && overview.registered >= 1 && overview.started >= 1);
    let (day, attempts) = admin.call(AdminAttempts { day: None }).await.unwrap();
    assert_eq!(day, daily::today());
    assert!(attempts.iter().any(|a| a.user_id == user));
    assert_eq!(admin.call(AdminLog { user, day }).await.unwrap(), vec![]);
    admin
        .call(AdminFlag {
            user,
            day,
            flag: Some(Flag::Review),
        })
        .await
        .unwrap();
    let (_, attempts) = admin.call(AdminAttempts { day: Some(day) }).await.unwrap();
    let flagged = attempts.iter().find(|a| a.user_id == user).unwrap();
    assert_eq!(flagged.flag, Some(Flag::Review));

    // The form on the dashboard redirects back to it
    let res = admin
        .post("/admin/house")
        .multipart(house_form("House CDCL", module(91)))
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::SEE_OTHER);
    let location = res.headers()["location"].to_str().unwrap();
    let id: i64 = location
        .strip_prefix("/admin?tab=dojo&added=")
        .unwrap()
        .parse()
        .unwrap();
    let res = admin
        .post("/admin/house")
        .multipart(house_form("House CDCL again", module(91)))
        .send()
        .await
        .unwrap();
    assert_eq!(res.headers()["location"], "/admin?tab=dojo&error=duplicate");
    let submissions = admin.call(AdminSubmissions {}).await.unwrap();
    let entry = submissions.iter().find(|s| s.id == id).unwrap();
    assert!(entry.house);
    admin
        .call(UpdateAdminSubmission {
            id,
            action: SubmissionAction::Delete,
        })
        .await
        .unwrap();
    let submissions = admin.call(AdminSubmissions {}).await.unwrap();
    assert!(submissions.iter().all(|s| s.id != id));
}

#[tokio::test]
async fn scripts_need_the_admin_token() {
    let server = Server::start().await;
    let player = server.browser();
    player.call(StartDaily { device: None }).await.unwrap();
    let user = player.call(CurrentUser {}).await.unwrap().unwrap().id;
    let day = daily::today();
    let script = server.browser();
    let attempt = format!("/admin/attempt?user={user}&day={day}");
    let flag = format!("/admin/flag?user={user}&day={day}");

    for token in [None, Some("wrong"), Some("admin-token-for-tests-and-more")] {
        let get = script.http.get(format!("{}{attempt}", server.url));
        let post = script.post(&format!("{flag}&flag=review"));
        let (get, post) = match token {
            Some(token) => (get.bearer_auth(token), post.bearer_auth(token)),
            None => (get, post),
        };
        assert_status(&get.send().await.unwrap(), StatusCode::UNAUTHORIZED);
        assert_status(&post.send().await.unwrap(), StatusCode::UNAUTHORIZED);
    }
    // Being signed in as the admin isn't enough, the routes are for scripts
    let admin = Browser::admin(&server).await;
    assert_status(&admin.get(&attempt).await, StatusCode::UNAUTHORIZED);

    let res = script
        .http
        .get(format!("{}{attempt}", server.url))
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap();
    assert_status(&res, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
    for key in [
        "started_at",
        "finished_at",
        "time_ms",
        "hints",
        "checks",
        "revealed",
        "flag",
    ] {
        assert!(json.get(key).is_some(), "{key} missing in {json}");
    }
    assert_eq!(json["flag"], serde_json::Value::Null);
    let missing = script
        .http
        .get(format!("{}/admin/attempt?user=0&day={day}", server.url))
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap();
    assert_status(&missing, StatusCode::NOT_FOUND);
    let bad_query = script
        .http
        .get(format!("{}/admin/attempt?user=me", server.url))
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap();
    assert_status(&bad_query, StatusCode::BAD_REQUEST);

    let post = |query: String| script.post(&query).bearer_auth(ADMIN_TOKEN).send();
    assert_status(
        &post(format!("{flag}&flag=nonsense")).await.unwrap(),
        StatusCode::BAD_REQUEST,
    );
    assert_status(
        &post(format!("/admin/flag?user=0&day={day}&flag=review"))
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
    );
    assert_status(
        &post(format!("{flag}&flag=review")).await.unwrap(),
        StatusCode::NO_CONTENT,
    );
    let res = script
        .http
        .get(format!("{}{attempt}", server.url))
        .bearer_auth(ADMIN_TOKEN)
        .send()
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
    assert_eq!(json["flag"], "review");
    // Without a flag the result goes back on the board
    assert_status(&post(flag).await.unwrap(), StatusCode::NO_CONTENT);
}

/// A name chosen before its word was on the list shows up on the dashboard, and renaming it gives
/// the player a neutral name everywhere, without locking them out.
#[tokio::test]
async fn admins_rename_players_with_names_the_list_flags() {
    let server = Server::start().await;
    let player = Browser::signed_up(&server, "flagged_fred").await;
    let conn = conn().await;
    let id = user_id(&conn, "flagged_fred").await;
    // As if the name had been allowed when it was chosen
    conn.execute(
        "UPDATE users SET username = 'Fred_Fuckface', name = 'Fred_Fuckface' WHERE id = ?1",
        params![id],
    )
    .await
    .unwrap();

    let admin = Browser::admin(&server).await;
    let flagged = admin.call(AdminNames {}).await.unwrap();
    let fred = FlaggedName {
        kind: NameKind::Player,
        id,
        name: "Fred_Fuckface".into(),
        word: "fuck".into(),
    };
    assert!(flagged.contains(&fred), "{flagged:?}");
    // Admins may look official, the test admin's name has "admin" in it
    assert!(
        flagged.iter().all(|f| !f.name.eq_ignore_ascii_case(ADMIN)),
        "{flagged:?}"
    );
    assert!(admin.call(LoadAdminOverview {}).await.unwrap().names >= 1);

    let renamed = admin.call(AdminRenamePlayer { id }).await.unwrap();
    assert_eq!(renamed.as_deref(), Some(format!("Player {id}").as_str()));
    assert_eq!(
        admin.call(AdminRenamePlayer { id: 999_999 }).await.unwrap(),
        None
    );
    assert!(!admin.call(AdminNames {}).await.unwrap().contains(&fred));
    let profile = server
        .browser()
        .call(LoadPlayer { id })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(profile.name, format!("Player {id}"));

    // Fred is still signed in, and signs in with the old username
    let me = player.call(CurrentUser {}).await.unwrap().unwrap();
    assert_eq!(
        (me.id, me.name.as_str()),
        (id, format!("Player {id}").as_str())
    );
    let other = server.browser();
    let login = Login {
        username: "fred_fuckface".into(),
        password: PASSWORD.into(),
    };
    assert_eq!(other.call(login).await.unwrap().id, id);
    // A new name right away, even after a recent change of his own, but not the old one again
    conn.execute(
        "UPDATE users SET renamed_at = ?2 WHERE id = ?1",
        params![id, crate::server::now_ms()],
    )
    .await
    .unwrap();
    admin.call(AdminRenamePlayer { id }).await.unwrap();
    let rename = |username: &str| ChangeUsername {
        username: username.into(),
        password: PASSWORD.into(),
    };
    assert_eq!(
        api_error(player.call(rename("Fred_Fuckface")).await),
        ApiError::UsernameNotAllowed
    );
    let fixed = player.call(rename("friendly_fred")).await.unwrap();
    assert_eq!(fixed.name, "friendly_fred");
}
