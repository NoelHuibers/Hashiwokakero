//! Mails to players about their account: confirmation links, reset links and notices
//! (`recovery`).
//!
//! They go out over SMTP, with the settings of the feedback form's mail (`contact::Smtp`).
//! Without them there are none, and the pages say that recovery emails aren't available.
//!
//! `HASHI_MAIL_DIR` writes every mail as a JSON file (`to`, `subject`, `body`) into that directory
//! instead, for the browser tests (`e2e/mail.ts`) and local development. Only with
//! `LEPTOS_ENV=dev` outside of Fly, like `HASHI_RATE_LIMITS=off`: production ignores it. The unit
//! and HTTP tests keep the mails in memory ([`sent_to`]).

use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Serialize;

/// A mail to a player, plain text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct Mail {
    pub(super) to: String,
    pub(super) subject: String,
    pub(super) body: String,
}

/// Where mail goes.
enum Outbox {
    #[cfg(not(test))]
    Smtp(super::contact::Smtp),
    #[cfg(not(test))]
    Dir(std::path::PathBuf),
    #[cfg(test)]
    Memory,
}

#[cfg(not(test))]
fn outbox() -> Option<Outbox> {
    if let Some(dir) = mail_dir(super::env_var) {
        return Some(Outbox::Dir(dir));
    }
    match super::contact::smtp() {
        Ok(smtp) => smtp.map(Outbox::Smtp),
        Err(e) => {
            eprintln!("mail: {e}");
            None
        }
    }
}

#[cfg(test)]
fn outbox() -> Option<Outbox> {
    (!tests::OFF.load(Ordering::SeqCst)).then_some(Outbox::Memory)
}

/// `HASHI_MAIL_DIR`, if it counts: only in development, never on Fly.
fn mail_dir(var: impl Fn(&str) -> Option<String>) -> Option<std::path::PathBuf> {
    let dev = var("LEPTOS_ENV").is_some_and(|v| v.eq_ignore_ascii_case("dev"))
        && var("FLY_APP_NAME").is_none();
    var("HASHI_MAIL_DIR")
        .filter(|dir| dev && !dir.is_empty())
        .map(std::path::PathBuf::from)
}

/// Whether mail can go out at all.
pub(super) fn available() -> bool {
    outbox().is_some()
}

/// Sends a mail and waits for the mail server.
#[cfg_attr(
    test,
    expect(clippy::unused_async, reason = "the tests' outbox takes mails at once")
)]
pub(super) async fn send(mail: Mail) -> Result<(), String> {
    match outbox().ok_or("no mail server set up")? {
        #[cfg(not(test))]
        Outbox::Smtp(smtp) => {
            super::issues::Mailer::send(&smtp, &mail.to, &mail.subject, &mail.body).await
        }
        #[cfg(not(test))]
        Outbox::Dir(dir) => write_to(&dir, &mail),
        #[cfg(test)]
        Outbox::Memory => {
            tests::SENT
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(mail);
            Ok(())
        }
    }
}

/// Mails being sent in the background ([`in_background`]).
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// Runs `job`, which sends mail, in the background: the request doesn't wait for the mail
/// server, and how long it takes says nothing about whether a mail went out at all (see
/// `recovery::forgot`). Failures are only logged, as `what`.
pub(super) fn in_background(
    what: &'static str,
    job: impl Future<Output = Result<(), String>> + Send + 'static,
) {
    IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
    tokio::spawn(async move {
        if let Err(e) = job.await {
            eprintln!("mail: {what} not sent: {e}");
        }
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    });
}

/// Sends `mail` in the background, see [`in_background`].
pub(super) fn send_later(what: &'static str, mail: Mail) {
    in_background(what, send(mail));
}

/// Writes `mail` as `<dir>/<time>-<n>.json`, complete or not at all: the browser tests read the
/// directory while the server writes.
#[cfg(not(test))]
fn write_to(dir: &std::path::Path, mail: &Mail) -> Result<(), String> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let name = format!("{}-{n:06}", super::now_ms());
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let json = serde_json::to_vec_pretty(mail).map_err(|e| e.to_string())?;
    let part = dir.join(format!("{name}.part"));
    std::fs::write(&part, json).map_err(|e| format!("{}: {e}", part.display()))?;
    std::fs::rename(&part, dir.join(format!("{name}.json"))).map_err(|e| e.to_string())
}

/// The mails sent to `address` so far, oldest first, after the ones still being sent.
#[cfg(test)]
pub(super) async fn sent_to(address: &str) -> Vec<Mail> {
    settled().await;
    tests::SENT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|m| m.to.eq_ignore_ascii_case(address))
        .cloned()
        .collect()
}

/// Waits until the mails sent in the background are out.
#[cfg(test)]
pub(super) async fn settled() {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while IN_FLIGHT.load(Ordering::SeqCst) > 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "mails still not sent after 10 s"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

/// As if no mail server was set up, until dropped. Only the HTTP tests use it, which take turns.
#[cfg(test)]
pub(super) struct NoMail(());

#[cfg(test)]
impl NoMail {
    pub(super) fn new() -> Self {
        tests::OFF.store(true, Ordering::SeqCst);
        Self(())
    }
}

#[cfg(test)]
impl Drop for NoMail {
    fn drop(&mut self) {
        tests::OFF.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;

    use super::*;

    pub(super) static SENT: Mutex<Vec<Mail>> = Mutex::new(Vec::new());
    pub(super) static OFF: AtomicBool = AtomicBool::new(false);

    /// The browser tests' mail directory never counts in production.
    #[test]
    fn the_mail_directory_is_for_development_only() {
        let vars = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| (*v).to_string())
            }
        };
        assert_eq!(
            mail_dir(vars(&[
                ("LEPTOS_ENV", "DEV"),
                ("HASHI_MAIL_DIR", "/run/mail")
            ])),
            Some("/run/mail".into())
        );
        for unset in [
            &[("LEPTOS_ENV", "PROD"), ("HASHI_MAIL_DIR", "/run/mail")][..],
            &[("HASHI_MAIL_DIR", "/run/mail")],
            &[
                ("LEPTOS_ENV", "dev"),
                ("FLY_APP_NAME", "hashi"),
                ("HASHI_MAIL_DIR", "/run/mail"),
            ],
            &[("LEPTOS_ENV", "dev"), ("HASHI_MAIL_DIR", "")],
            &[("LEPTOS_ENV", "dev")],
        ] {
            assert_eq!(mail_dir(vars(unset)), None, "{unset:?}");
        }
    }
}
