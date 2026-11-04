//! The recovery email and "Forgot password?" (`server::recovery`): adding, confirming, changing
//! and removing an address, the reset links, what nobody can learn from them, and their limits.
//! The mails stay in the tests' outbox (`mail::sent_to`).

use super::*;
use crate::api::{
    AdminAttempts, AdminBanPlayer, AdminBannedPlayers, ChangePassword, ConfirmRecoveryEmail,
    CurrentUser, DeleteAccount, ForgotPassword, LoadPlayer, LoadRecoveryEmail,
    PasswordResetAvailable, RemoveRecoveryEmail, ResetPassword, SetRecoveryEmail, StartDaily,
};
use crate::i18n::td_string;
use crate::model::{RecoveryEmail, UserInfo};
use crate::server::mail::{self, Mail, NoMail};

const NEW_PASSWORD: &str = "battery staple";

fn set(email: &str) -> SetRecoveryEmail {
    set_with(email, PASSWORD)
}

fn set_with(email: &str, password: &str) -> SetRecoveryEmail {
    SetRecoveryEmail {
        email: email.into(),
        password: password.into(),
        locale: "en".into(),
    }
}

fn remove(password: &str) -> RemoveRecoveryEmail {
    RemoveRecoveryEmail {
        password: password.into(),
        locale: "en".into(),
    }
}

fn confirm(token: &str) -> ConfirmRecoveryEmail {
    ConfirmRecoveryEmail {
        token: token.into(),
    }
}

fn forgot(login: &str) -> ForgotPassword {
    ForgotPassword {
        login: login.into(),
        locale: "en".into(),
    }
}

fn reset(token: &str, password: &str) -> ResetPassword {
    ResetPassword {
        token: token.into(),
        password: password.into(),
    }
}

fn login(username: &str, password: &str) -> crate::api::Login {
    crate::api::Login {
        username: username.into(),
        password: password.into(),
    }
}

/// The token of the link in a mail: what follows the `#`.
#[track_caller]
fn token(mail: &Mail) -> String {
    let at = mail.body.find('#').expect("no link in the mail");
    mail.body[at + 1..at + 65].to_string()
}

/// The last mail to `address`, which there must be.
async fn last(address: &str) -> Mail {
    mail::sent_to(address)
        .await
        .pop()
        .unwrap_or_else(|| panic!("no mail to {address}"))
}

async fn count(conn: &Connection, sql: &str, user: i64) -> i64 {
    let mut rows = conn.query(sql, params![user]).await.unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

/// The links of `user` that still work.
async fn open_links(conn: &Connection, user: i64) -> i64 {
    count(
        conn,
        "SELECT COUNT(*) FROM account_tokens WHERE user_id = ?1 AND used_at IS NULL",
        user,
    )
    .await
}

/// Lets the link of `token` expire.
async fn expire(conn: &Connection, token: &str) {
    conn.execute(
        "UPDATE account_tokens SET expires_at = ?2 WHERE token_hash = ?1",
        params![auth::hash_token(token), crate::server::now_ms() - 1],
    )
    .await
    .unwrap();
}

/// Gives the signed in account of `browser` the recovery email `address`, confirmed.
async fn confirmed(browser: &Browser, address: &str) {
    browser.call(set(address)).await.unwrap();
    let token = token(&last(address).await);
    assert_eq!(browser.call(confirm(&token)).await.unwrap(), address);
}

async fn me(browser: &Browser) -> UserInfo {
    browser.call(CurrentUser {}).await.unwrap().unwrap()
}

#[tokio::test]
async fn a_recovery_email_needs_the_password_and_a_confirmation() {
    let server = Server::start().await;
    assert_eq!(
        api_error(server.browser().call(LoadRecoveryEmail {}).await),
        ApiError::SessionExpired
    );
    assert_eq!(
        api_error(server.browser().call(set("nobody@example.org")).await),
        ApiError::SessionExpired
    );
    // Guests have no password to put it behind
    let guest = server.browser();
    guest.call(StartDaily { device: None }).await.unwrap();
    assert_eq!(
        api_error(guest.call(LoadRecoveryEmail {}).await),
        ApiError::Forbidden
    );
    assert_eq!(
        api_error(guest.call(set("guest@example.org")).await),
        ApiError::Forbidden
    );

    let browser = Browser::signed_up(&server, "rita_recover").await;
    assert_eq!(
        browser.call(LoadRecoveryEmail {}).await.unwrap(),
        RecoveryEmail {
            address: None,
            pending: None,
            mail: true,
        }
    );
    for (email, password, error) in [
        ("not an address", PASSWORD, ApiError::EmailInvalid),
        ("Rita <rita@example.org>", PASSWORD, ApiError::EmailInvalid),
        (
            "rita@example.org",
            "not my password",
            ApiError::WrongPassword,
        ),
    ] {
        assert_eq!(
            api_error(browser.call(set_with(email, password)).await),
            error,
            "{email}"
        );
    }
    assert_eq!(mail::sent_to("rita@example.org").await, Vec::<Mail>::new());

    // Waiting for its confirmation, the mail on its way
    let pending = browser.call(set(" rita@example.org ")).await.unwrap();
    assert_eq!(
        (pending.address, pending.pending.as_deref()),
        (None, Some("rita@example.org"))
    );
    let mails = mail::sent_to("rita@example.org").await;
    let [mail] = &mails[..] else {
        panic!("{mails:?}")
    };
    assert_eq!(mail.subject, "Hashi: Confirm your recovery email");
    for line in [
        "Hello rita_recover,",
        "https://hashi.rs/account/confirm#",
        "The link works once, for 24 hours.",
        "If this wasn't you, ignore this mail",
    ] {
        assert!(mail.body.contains(line), "{line}: {}", mail.body);
    }
    let token = token(mail);

    // Opened anywhere, without a session
    let elsewhere = server.browser();
    for wrong in ["", "garbage", &"ab".repeat(32), &token[..63]] {
        assert_eq!(
            api_error(elsewhere.call(confirm(wrong)).await),
            ApiError::LinkExpired,
            "{wrong}"
        );
    }
    assert_eq!(
        elsewhere.call(confirm(&token)).await.unwrap(),
        "rita@example.org"
    );
    assert_eq!(
        browser.call(LoadRecoveryEmail {}).await.unwrap(),
        RecoveryEmail {
            address: Some("rita@example.org".into()),
            pending: None,
            mail: true,
        }
    );
    // Once
    assert_eq!(
        api_error(elsewhere.call(confirm(&token)).await),
        ApiError::LinkExpired
    );
    // Nobody signed in or out by it
    assert_eq!(elsewhere.session(), None);
    assert_eq!(me(&browser).await.name, "rita_recover");
}

#[tokio::test]
async fn confirmation_links_expire_and_a_newer_one_replaces_them() {
    let server = Server::start().await;
    let browser = Browser::signed_up(&server, "cora_confirm").await;
    let conn = conn().await;

    browser.call(set("cora@example.org")).await.unwrap();
    let first = token(&last("cora@example.org").await);
    expire(&conn, &first).await;
    assert_eq!(
        api_error(browser.call(confirm(&first)).await),
        ApiError::LinkExpired
    );
    // Expired, it waits for nothing any more
    assert_eq!(
        browser.call(LoadRecoveryEmail {}).await.unwrap().pending,
        None
    );

    browser.call(set("cora@example.com")).await.unwrap();
    let older = token(&last("cora@example.com").await);
    let newer = browser.call(set("cora@example.net")).await.unwrap();
    assert_eq!(newer.pending.as_deref(), Some("cora@example.net"));
    assert_eq!(
        api_error(browser.call(confirm(&older)).await),
        ApiError::LinkExpired
    );
    let newest = token(&last("cora@example.net").await);
    browser.call(confirm(&newest)).await.unwrap();
    assert_eq!(
        browser
            .call(LoadRecoveryEmail {})
            .await
            .unwrap()
            .address
            .as_deref(),
        Some("cora@example.net")
    );
}

/// A change waiting for its confirmation leaves the address that is there: reset links still go
/// to it, until the new one is confirmed. Then the old address hears of it, and its links stop.
#[tokio::test]
async fn a_pending_change_leaves_the_old_address_active() {
    let server = Server::start().await;
    let browser = Browser::signed_up(&server, "olga_old").await;
    confirmed(&browser, "olga@example.org").await;

    let status = browser.call(set("olga@example.com")).await.unwrap();
    assert_eq!(
        (status.address.as_deref(), status.pending.as_deref()),
        (Some("olga@example.org"), Some("olga@example.com"))
    );
    let change = token(&last("olga@example.com").await);
    server.browser().call(forgot("olga_old")).await.unwrap();
    let to_old = last("olga@example.org").await;
    assert_eq!(to_old.subject, "Hashi: Reset your password");
    assert_eq!(mail::sent_to("olga@example.com").await.len(), 1);
    let reset_link = token(&to_old);

    browser.call(confirm(&change)).await.unwrap();
    let status = browser.call(LoadRecoveryEmail {}).await.unwrap();
    assert_eq!(
        (status.address.as_deref(), status.pending),
        (Some("olga@example.com"), None)
    );
    // The old address is told, without the new one
    let notice = last("olga@example.org").await;
    assert_eq!(notice.subject, "Hashi: Your recovery email was changed");
    assert!(!notice.body.contains("olga@example.com"), "{}", notice.body);
    assert!(
        notice.body.contains("https://hashi.rs/kontakt"),
        "{}",
        notice.body
    );
    // The reset link that went there no longer works
    assert_eq!(
        api_error(
            server
                .browser()
                .call(reset(&reset_link, NEW_PASSWORD))
                .await
        ),
        ApiError::LinkExpired
    );

    // The address it has, in other letters: nothing to confirm, nothing mailed
    let mails = mail::sent_to("olga@example.com").await.len();
    let status = browser.call(set("OLGA@example.COM")).await.unwrap();
    assert_eq!(
        (status.address.as_deref(), status.pending),
        (Some("olga@example.com"), None)
    );
    assert_eq!(mail::sent_to("olga@example.com").await.len(), mails);
}

#[tokio::test]
async fn removing_the_recovery_email_needs_the_password() {
    let server = Server::start().await;
    let browser = Browser::signed_up(&server, "remi_remove").await;
    confirmed(&browser, "remi@example.org").await;
    browser.call(set("remi@example.com")).await.unwrap();

    assert_eq!(
        api_error(browser.call(remove("not my password")).await),
        ApiError::WrongPassword
    );
    assert_eq!(
        browser
            .call(LoadRecoveryEmail {})
            .await
            .unwrap()
            .address
            .as_deref(),
        Some("remi@example.org")
    );
    let pending = token(&last("remi@example.com").await);

    let status = browser.call(remove(PASSWORD)).await.unwrap();
    assert_eq!((status.address, status.pending), (None, None));
    assert_eq!(
        last("remi@example.org").await.subject,
        "Hashi: Your recovery email was removed"
    );
    // The change waiting for its confirmation went along, and no reset link goes anywhere
    assert_eq!(
        api_error(browser.call(confirm(&pending)).await),
        ApiError::LinkExpired
    );
    let mails = mail::sent_to("remi@example.org").await.len();
    server.browser().call(forgot("remi_remove")).await.unwrap();
    server
        .browser()
        .call(forgot("remi@example.org"))
        .await
        .unwrap();
    assert_eq!(mail::sent_to("remi@example.org").await.len(), mails);
}

/// An address is the recovery email of one account, whatever its case. Only its mailbox learns
/// that it is taken: the account asking hears the same as for any address.
#[tokio::test]
async fn an_address_belongs_to_one_account() {
    let server = Server::start().await;
    let first = Browser::signed_up(&server, "una_unique").await;
    confirmed(&first, "Una@Example.org").await;

    let second = Browser::signed_up(&server, "uwe_unique").await;
    let status = second.call(set("una@example.ORG")).await.unwrap();
    assert_eq!(status.pending.as_deref(), Some("una@example.ORG"));
    let mail = last("una@example.org").await;
    assert_eq!(mail.subject, "Hashi: Confirm your recovery email");
    assert_eq!(
        api_error(second.call(confirm(&token(&mail))).await),
        ApiError::EmailTaken
    );
    assert_eq!(
        second.call(LoadRecoveryEmail {}).await.unwrap().address,
        None
    );
    let conn = conn().await;
    let mut rows = conn
        .query(
            "SELECT username FROM users WHERE email = 'UNA@EXAMPLE.ORG'",
            (),
        )
        .await
        .unwrap();
    let owner: String = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(owner, "una_unique");
    assert!(rows.next().await.unwrap().is_none());

    // Freed, the other account can have it
    first.call(remove(PASSWORD)).await.unwrap();
    confirmed(&second, "una@example.org").await;
}

/// What "Forgot password?" answers, byte for byte.
async fn forgot_response(browser: &Browser, login: &str) -> (StatusCode, Vec<String>, String) {
    let res = browser
        .post(ForgotPassword::PATH)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .body(serde_json::to_vec(&forgot(login)).unwrap())
        .send()
        .await
        .unwrap();
    let mut headers: Vec<String> = res
        .headers()
        .iter()
        .filter(|(name, _)| *name != "date")
        .map(|(name, value)| format!("{name}: {}", value.to_str().unwrap()))
        .collect();
    headers.sort();
    (res.status(), headers, res.text().await.unwrap())
}

/// The answer is the same for a username nobody has, an account without an address, a banned or
/// deleted one and a real one: nobody learns which accounts there are or have an address. Only
/// the real one gets a mail.
#[tokio::test]
async fn forgot_password_answers_the_same_whoever_asks() {
    let server = Server::start().await;
    let real = Browser::signed_up(&server, "fred_forgot").await;
    confirmed(&real, "fred@example.org").await;
    let without = Browser::signed_up(&server, "nina_noaddress").await;
    let banned = Browser::signed_up(&server, "bert_banned").await;
    confirmed(&banned, "bert@example.org").await;
    let banned_id = me(&banned).await.id;
    let gone = Browser::signed_up(&server, "doris_deleted").await;
    confirmed(&gone, "doris@example.org").await;
    gone.call(DeleteAccount {
        password: Some(PASSWORD.into()),
    })
    .await
    .unwrap();
    let conn = conn().await;
    conn.execute(
        "UPDATE users SET banned_at = 1, ban_reason = 'test' WHERE id = ?1",
        params![banned_id],
    )
    .await
    .unwrap();
    let addresses = ["fred@example.org", "bert@example.org", "doris@example.org"];
    let before = mail_counts(&addresses).await;

    let expected = forgot_response(&server.browser(), "fred_forgot").await;
    assert_eq!(expected.0, StatusCode::OK, "{expected:?}");
    for login in [
        "nobody_by_that_name",
        "nobody@example.org",
        "nina_noaddress",
        "bert_banned",
        "bert@example.org",
        "doris_deleted",
        "doris@example.org",
        "",
        "FRED@example.org",
        "Fred_Forgot",
    ] {
        assert_eq!(
            forgot_response(&server.browser(), login).await,
            expected,
            "{login}"
        );
    }
    // By username and by address, in any case: three links to the real one, none to the others
    let after = mail_counts(&addresses).await;
    assert_eq!(after, [before[0] + 3, before[1], before[2]]);
    let mails = mail::sent_to("fred@example.org").await;
    for mail in &mails[mails.len() - 3..] {
        assert_eq!(mail.subject, "Hashi: Reset your password");
        for line in [
            "Hello fred_forgot,",
            "https://hashi.rs/account/reset#",
            "The link works once, for one hour.",
            "If this wasn't you, ignore this mail",
        ] {
            assert!(mail.body.contains(line), "{line}: {}", mail.body);
        }
    }
    // No links stored for the others
    let without_id = me(&without).await.id;
    assert_eq!(open_links(&conn, without_id).await, 0);
    assert_eq!(open_links(&conn, banned_id).await, 0);
}

async fn mail_counts(addresses: &[&str]) -> Vec<usize> {
    let mut counts = Vec::new();
    for address in addresses {
        counts.push(mail::sent_to(address).await.len());
    }
    counts
}

/// The link sets the password, ends every session of the account and signs in the browser that
/// opened it, taking along the results of the guest playing there, like signing in does. Once.
#[tokio::test]
async fn a_reset_link_sets_the_password_and_signs_in_here() {
    let server = Server::start().await;
    let owner = Browser::signed_up(&server, "reese_reset").await;
    confirmed(&owner, "reese@example.org").await;
    let id = me(&owner).await.id;
    let laptop = server.browser();
    laptop.call(login("reese_reset", PASSWORD)).await.unwrap();
    // A guest playing today's daily in the browser that opens the link
    let here = server.browser();
    here.call(StartDaily { device: None }).await.unwrap();
    let guest = me(&here).await;
    assert!(guest.guest);

    here.call(ForgotPassword {
        login: "reese_reset".into(),
        locale: "de".into(),
    })
    .await
    .unwrap();
    // In the page's language, to its language's page
    let mail = last("reese@example.org").await;
    assert_eq!(
        mail.subject,
        format!(
            "Hashi: {}",
            td_string!(crate::i18n::Locale::de, account.mail.reset_subject)
        )
    );
    assert!(
        mail.body.contains("https://hashi.rs/de/account/reset#"),
        "{}",
        mail.body
    );
    let link = token(&mail);

    // A password that can't be stored keeps the link working
    assert_eq!(
        api_error(here.call(reset(&link, "short")).await),
        ApiError::PasswordTooShort
    );
    let called = here.call_with_headers(reset(&link, NEW_PASSWORD)).await;
    let user = called.result.unwrap();
    assert_eq!((user.id, user.guest), (id, false));
    session_cookie(&called.headers);
    assert_eq!(me(&here).await.id, id);
    // Every other session ended
    for other in [&owner, &laptop] {
        assert_eq!(other.call(CurrentUser {}).await.unwrap(), None);
    }
    // The guest's daily is the account's now, the guest is gone
    let conn = conn().await;
    assert!(
        daily::attempt(&conn, id, daily::today())
            .await
            .unwrap()
            .is_some()
    );
    assert!(auth::user_by_id(&conn, guest.id).await.unwrap().is_none());
    // The new password signs in, the old one doesn't
    assert_eq!(
        api_error(server.browser().call(login("reese_reset", PASSWORD)).await),
        ApiError::WrongLogin
    );
    server
        .browser()
        .call(login("reese_reset", NEW_PASSWORD))
        .await
        .unwrap();
    // Once
    assert_eq!(
        api_error(
            server
                .browser()
                .call(reset(&link, "a third password"))
                .await
        ),
        ApiError::LinkExpired
    );
}

#[tokio::test]
async fn reset_links_expire_and_a_newer_one_replaces_them() {
    let server = Server::start().await;
    let owner = Browser::signed_up(&server, "erik_expire").await;
    confirmed(&owner, "erik@example.org").await;
    let conn = conn().await;

    server.browser().call(forgot("erik_expire")).await.unwrap();
    let expired = token(&last("erik@example.org").await);
    expire(&conn, &expired).await;
    assert_eq!(
        api_error(server.browser().call(reset(&expired, NEW_PASSWORD)).await),
        ApiError::LinkExpired
    );

    server
        .browser()
        .call(forgot("erik@example.org"))
        .await
        .unwrap();
    let older = token(&last("erik@example.org").await);
    server.browser().call(forgot("erik_expire")).await.unwrap();
    let newer = token(&last("erik@example.org").await);
    assert_ne!(older, newer);
    assert_eq!(
        api_error(server.browser().call(reset(&older, NEW_PASSWORD)).await),
        ApiError::LinkExpired
    );
    // A confirmation link is no reset link, nor the other way round
    assert_eq!(
        api_error(server.browser().call(confirm(&newer)).await),
        ApiError::LinkExpired
    );
    // Changing the password uses up the reset links: they were for the old one
    owner
        .call(ChangePassword {
            current: PASSWORD.into(),
            new: NEW_PASSWORD.into(),
        })
        .await
        .unwrap();
    assert_eq!(
        api_error(
            server
                .browser()
                .call(reset(&newer, "a third password"))
                .await
        ),
        ApiError::LinkExpired
    );
    assert_eq!(me(&owner).await.name, "erik_expire");
}

/// An account banned after its link went out can't use it.
#[tokio::test]
async fn banned_accounts_cant_reset() {
    let server = Server::start().await;
    let player = Browser::signed_up(&server, "bella_banned").await;
    confirmed(&player, "bella@example.org").await;
    let id = me(&player).await.id;
    server.browser().call(forgot("bella_banned")).await.unwrap();
    let link = token(&last("bella@example.org").await);

    let admin = Browser::admin(&server).await;
    admin
        .call(AdminBanPlayer {
            id,
            reason: "a bot".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        api_error(server.browser().call(reset(&link, NEW_PASSWORD)).await),
        ApiError::LinkExpired
    );
    let mails = mail::sent_to("bella@example.org").await.len();
    server.browser().call(forgot("bella_banned")).await.unwrap();
    assert_eq!(mail::sent_to("bella@example.org").await.len(), mails);
    assert_eq!(
        api_error(
            server
                .browser()
                .call(login("bella_banned", NEW_PASSWORD))
                .await
        ),
        ApiError::WrongLogin
    );
}

/// Accounts with an address, made directly in the database: hashing a password for each would
/// take a while.
async fn accounts_with_addresses(conn: &Connection, prefix: &str, n: usize) -> Vec<String> {
    let hash = auth::test_password_hash(PASSWORD);
    let mut names = Vec::new();
    for i in 0..n {
        let name = format!("{prefix}_{i}");
        conn.execute(
            "INSERT INTO users (username, name, password_hash, email, created_at)
             VALUES (?1, ?1, ?2, ?3, 0)",
            params![name.clone(), hash.clone(), format!("{name}@example.org")],
        )
        .await
        .unwrap();
        names.push(name);
    }
    names
}

/// Nobody fills an inbox: three reset mails per account and day, ten mails with links per network
/// and day. Past the limits the answer stays the same, only no mail goes out.
#[tokio::test]
async fn reset_mails_are_limited() {
    let server = Server::start().await;
    let conn = conn().await;
    let names = accounts_with_addresses(&conn, "lim", 12).await;

    // Per account, from different networks
    for _ in 0..5 {
        server.browser().call(forgot(&names[0])).await.unwrap();
    }
    assert_eq!(mail::sent_to("lim_0@example.org").await.len(), 3);

    // Per network, for different accounts
    let network = server.browser_from("203.0.113.77");
    for name in &names[1..] {
        network.call(forgot(name)).await.unwrap();
    }
    let mut mailed = 0;
    for name in &names[1..] {
        mailed += mail::sent_to(&format!("{name}@example.org")).await.len();
    }
    assert_eq!(mailed, 10);
    // Another network still can
    server.browser().call(forgot(&names[11])).await.unwrap();
    assert_eq!(mail::sent_to("lim_11@example.org").await.len(), 1);
}

/// Confirmation mails are limited too, and the account asking is told.
#[tokio::test]
async fn confirmation_mails_are_limited() {
    let server = Server::start().await;
    let browser = Browser::signed_up(&server, "carl_confirms").await;
    for i in 0..5 {
        browser
            .call(set(&format!("carl{i}@example.org")))
            .await
            .unwrap();
    }
    assert_eq!(
        api_error(browser.call(set("carl5@example.org")).await),
        ApiError::TooManyMessages
    );
    assert_eq!(mail::sent_to("carl5@example.org").await, Vec::<Mail>::new());
}

/// Without a mail server there are no recovery emails, and the pages say so before anyone types
/// a password.
#[tokio::test]
async fn without_mail_nothing_is_promised() {
    let server = Server::start().await;
    let browser = Browser::signed_up(&server, "moritz_nomail").await;
    assert!(
        server
            .browser()
            .call(PasswordResetAvailable {})
            .await
            .unwrap()
    );
    let _off = NoMail::new();
    assert!(
        !server
            .browser()
            .call(PasswordResetAvailable {})
            .await
            .unwrap()
    );
    assert!(!browser.call(LoadRecoveryEmail {}).await.unwrap().mail);
    for password in [PASSWORD, "not my password"] {
        assert_eq!(
            api_error(browser.call(set_with("moritz@example.org", password)).await),
            ApiError::MailUnavailable
        );
    }
    assert_eq!(
        api_error(server.browser().call(forgot("moritz_nomail")).await),
        ApiError::MailUnavailable
    );
    let conn = conn().await;
    assert_eq!(open_links(&conn, me(&browser).await.id).await, 0);
}

/// Deleting an account deletes its address and its links: the address is free for another one.
#[tokio::test]
async fn deleting_an_account_removes_its_address_and_links() {
    let server = Server::start().await;
    let browser = Browser::signed_up(&server, "edda_erased").await;
    let id = me(&browser).await.id;
    confirmed(&browser, "edda@example.org").await;
    browser.call(set("edda@example.com")).await.unwrap();
    server.browser().call(forgot("edda_erased")).await.unwrap();
    let link = token(&last("edda@example.org").await);
    let conn = conn().await;
    assert_eq!(open_links(&conn, id).await, 2);

    browser
        .call(DeleteAccount {
            password: Some(PASSWORD.into()),
        })
        .await
        .unwrap();
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM account_tokens WHERE user_id = ?1",
            id
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM users WHERE id = ?1 AND email IS NOT NULL",
            id
        )
        .await,
        0
    );
    assert_eq!(
        api_error(server.browser().call(reset(&link, NEW_PASSWORD)).await),
        ApiError::LinkExpired
    );
    let other = Browser::signed_up(&server, "edda_again").await;
    confirmed(&other, "edda@example.org").await;
}

/// The admins' pages show no addresses.
#[tokio::test]
async fn admin_pages_show_no_addresses() {
    let server = Server::start().await;
    let player = Browser::signed_up(&server, "anna_seen_by_us").await;
    confirmed(&player, "anna.hidden@example.org").await;
    player.call(StartDaily { device: None }).await.unwrap();
    let id = me(&player).await.id;
    let admin = Browser::admin(&server).await;
    let mut seen = vec![
        serde_json::to_string(&admin.call(AdminAttempts { day: None }).await.unwrap()).unwrap(),
        serde_json::to_string(&admin.call(LoadPlayer { id }).await.unwrap()).unwrap(),
    ];
    admin
        .call(AdminBanPlayer {
            id,
            reason: "a test".into(),
        })
        .await
        .unwrap();
    seen.push(serde_json::to_string(&admin.call(AdminBannedPlayers {}).await.unwrap()).unwrap());
    for json in seen {
        assert!(!json.contains("anna.hidden"), "{json}");
    }
}
