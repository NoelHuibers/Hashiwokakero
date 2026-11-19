//! Notifications: the streak's last call, races, rivals, shared puzzles and group puzzles.
//!
//! No third party in between but the platforms' own push services: everything lives in our
//! database. A player's choice is per account ([`prefs`], all off until they turn them on), the
//! devices that get them are registered with a secret of their own ([`register_device`]), and
//! what happens goes into an outbox ([`enqueue`]): the social events at the moment they happen
//! (`race_opened`, `puzzle_posted`, `rival_finished`, `shared_solved`), the last calls from a job
//! every few minutes ([`start`], [`queue_last_calls`]). The outbox keeps what the text needs, not
//! the text: each device gets it in its own language ([`render::text`]).
//!
//! How a notification gets to a device is a channel's business ([`Channel`]): Web Push for
//! browsers and APNs for the iOS app push it ([`deliver`]), the Android app asks for it itself
//! ([`poll`], with the device's secret). Each device has a cursor, the last outbox row it got;
//! moving it is what claims the rows, so two machines running the job never send one twice. The
//! channels are Web Push ([`web_push`]) and APNs ([`apns`]), each when its keys are configured.
//!
//! Nobody gets more than a handful a day: the same notification (`collapse_key`) comes once in a
//! while ([`Rule`]), each kind has a daily limit, and all but the last call share one. Nothing
//! comes from a player the recipient blocked or a banned one, and a last call never comes once
//! today's daily is done, wherever it was played: the outbox is checked again when a device gets
//! it ([`pending`]). Rows go two days after they were made ([`delete_old`]).

pub mod apns;
mod render;
#[cfg(test)]
mod tests;
pub mod timing;
pub mod web_push;

use std::fmt::Write as _;

use futures::future::BoxFuture;
use leptos::prelude::ServerFnError;
use leptos_i18n::Locale as _;
use libsql::{Connection, params};

use super::db::db_err;
use super::now_ms;
use crate::i18n::Locale;
use crate::model::{
    ApiError, ApnsEnvironment, DeviceKind, DeviceRegistration, LOBBY_MS, MS_PER_DAY,
    NotificationPoll, NotificationPrefs, NotifyKind, PushMessage, day_of,
};
pub use render::{Args, locale_of};

/// How long the outbox keeps a notification.
pub const RETENTION_MS: i64 = 2 * MS_PER_DAY;
/// How often the job runs.
pub const JOB_EVERY_MS: i64 = 5 * 60_000;
/// No last call later than this before the daily changes: too late to be of use.
const LAST_CALL_UNTIL_MS: i64 = 15 * 60_000;
/// The most notifications a player gets a day, besides the last call.
pub const SOCIAL_PER_DAY: i64 = 8;
/// The most devices per player: a new one beyond replaces the one not seen the longest.
pub const MAX_DEVICES: i64 = 10;
/// A device not seen for this long is forgotten (an app that was deleted, an old browser).
pub const DEVICE_IDLE_MS: i64 = 90 * MS_PER_DAY;
/// The most rows a device gets at once.
const BATCH: i64 = 20;

/// How often the same notification may come, and how many of a kind a day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rule {
    /// The same `collapse_key` comes once within this.
    pub window_ms: i64,
    /// The most of the kind within 24 hours.
    pub per_day: i64,
}

impl Rule {
    pub const fn of(kind: NotifyKind) -> Self {
        match kind {
            // Once per daily (the key is the day)
            NotifyKind::LastCall => Self {
                window_ms: RETENTION_MS,
                per_day: 1,
            },
            // A lobby lives 15 minutes; one race a group an hour at most
            NotifyKind::Race => Self {
                window_ms: 3_600_000,
                per_day: 3,
            },
            // Once per rival and daily
            NotifyKind::Rival => Self {
                window_ms: RETENTION_MS,
                per_day: 3,
            },
            // A popular puzzle: once in three hours, with the count of the solves since
            NotifyKind::Shared => Self {
                window_ms: 3 * 3_600_000,
                per_day: 4,
            },
            // Once per puzzle
            NotifyKind::Group => Self {
                window_ms: RETENTION_MS,
                per_day: 4,
            },
        }
    }
}

/// The column of a kind's switch in `notification_prefs`.
const fn column(kind: NotifyKind) -> &'static str {
    match kind {
        NotifyKind::LastCall => "last_call",
        NotifyKind::Race => "race",
        NotifyKind::Rival => "rival",
        NotifyKind::Shared => "shared",
        NotifyKind::Group => "group_puzzle",
    }
}

/// SQL: whether the prefs `p` let a row `o` of the outbox through, by its kind.
fn wanted_sql() -> String {
    let mut sql = String::from("p.enabled = 1 AND CASE o.kind");
    for kind in NotifyKind::ALL {
        let _ = write!(sql, " WHEN '{}' THEN p.{}", kind.id(), column(kind));
    }
    sql.push_str(" ELSE 0 END = 1");
    sql
}

async fn scalar(
    conn: &Connection,
    sql: &str,
    values: impl params::IntoParams,
) -> Result<Option<i64>, ServerFnError> {
    let mut rows = conn.query(sql, values).await.map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get::<Option<i64>>(0).map_err(db_err),
        None => Ok(None),
    }
}

// --- Preferences ---

/// A player's settings, everything off if they never chose.
pub async fn prefs(conn: &Connection, user: i64) -> Result<NotificationPrefs, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT enabled, last_call, race, rival, shared, group_puzzle, offer_shown
             FROM notification_prefs WHERE user_id = ?1",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(NotificationPrefs::default());
    };
    let on = |i: i32| row.get::<i64>(i).map(|v| v != 0).map_err(db_err);
    Ok(NotificationPrefs {
        enabled: on(0)?,
        last_call: on(1)?,
        race: on(2)?,
        rival: on(3)?,
        shared: on(4)?,
        group: on(5)?,
        offer_shown: on(6)?,
    })
}

/// The player's time zone as stored, `None` if no client reported a valid one.
pub async fn time_zone(conn: &Connection, user: i64) -> Result<Option<String>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT time_zone FROM notification_prefs WHERE user_id = ?1",
            params![user],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err),
        None => Ok(None),
    }
}

/// Saves a player's settings ([`NotificationPrefs::normalized`]) and the time zone their client
/// reported (an unknown one keeps the stored one). The offer stays shown once it was.
pub async fn save_prefs(
    conn: &Connection,
    user: i64,
    chosen: NotificationPrefs,
    zone: Option<&str>,
    now: i64,
) -> Result<NotificationPrefs, ServerFnError> {
    let chosen = chosen.normalized();
    let flag = i64::from;
    conn.execute(
        "INSERT INTO notification_prefs
            (user_id, enabled, last_call, race, rival, shared, group_puzzle, offer_shown, time_zone, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT (user_id) DO UPDATE SET
            enabled = excluded.enabled, last_call = excluded.last_call, race = excluded.race,
            rival = excluded.rival, shared = excluded.shared, group_puzzle = excluded.group_puzzle,
            offer_shown = MAX(offer_shown, excluded.offer_shown),
            time_zone = COALESCE(excluded.time_zone, time_zone), updated_at = excluded.updated_at",
        params![
            user,
            flag(chosen.enabled),
            flag(chosen.last_call),
            flag(chosen.race),
            flag(chosen.rival),
            flag(chosen.shared),
            flag(chosen.group),
            flag(chosen.offer_shown),
            timing::valid_zone(zone),
            now
        ],
    )
    .await
    .map_err(db_err)?;
    prefs(conn, user).await
}

/// The answer to the one-time offer after a finished daily: yes turns everything on.
pub async fn answer_offer(
    conn: &Connection,
    user: i64,
    accept: bool,
    zone: Option<&str>,
    now: i64,
) -> Result<NotificationPrefs, ServerFnError> {
    let prefs = prefs(conn, user).await?.answered_offer(accept);
    save_prefs(conn, user, prefs, zone, now).await
}

/// Notes the time zone a client reported, if it is one.
pub async fn note_time_zone(
    conn: &Connection,
    user: i64,
    zone: Option<&str>,
    now: i64,
) -> Result<(), ServerFnError> {
    let Some(zone) = timing::valid_zone(zone) else {
        return Ok(());
    };
    conn.execute(
        "INSERT INTO notification_prefs (user_id, time_zone, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (user_id) DO UPDATE SET time_zone = excluded.time_zone",
        params![user, zone, now],
    )
    .await
    .map_err(db_err)?;
    Ok(())
}

// --- Devices ---

/// A device registered for notifications.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: i64,
    pub user_id: i64,
    pub kind: DeviceKind,
    /// The Web Push endpoint or the APNs device token.
    pub endpoint: Option<String>,
    /// Web Push's keys (JSON).
    pub keys: Option<String>,
    pub locale: Locale,
    pub cursor: i64,
}

const DEVICE_COLUMNS: &str = "id, user_id, kind, endpoint, keys, locale, cursor";

fn row_to_device(row: &libsql::Row) -> Result<Option<Device>, ServerFnError> {
    let kind: String = row.get(2).map_err(db_err)?;
    let Some(kind) = DeviceKind::from_id(&kind) else {
        return Ok(None);
    };
    Ok(Some(Device {
        id: row.get(0).map_err(db_err)?,
        user_id: row.get(1).map_err(db_err)?,
        kind,
        endpoint: row.get(3).map_err(db_err)?,
        keys: row.get(4).map_err(db_err)?,
        locale: locale_of(&row.get::<String>(5).map_err(db_err)?),
        cursor: row.get(6).map_err(db_err)?,
    }))
}

/// What a device brings along, checked: a browser its Web Push endpoint (https) and keys, the
/// iOS app its APNs token (hex) and which of Apple's push services it is for
/// ([`ApnsEnvironment::keys`]), the Android app nothing.
fn check_device(
    kind: DeviceKind,
    endpoint: Option<&str>,
    keys: Option<&str>,
) -> Result<(), ServerFnError> {
    let ok = match kind {
        DeviceKind::Web => {
            endpoint.is_some_and(|e| e.starts_with("https://") && e.len() <= 1024)
                && keys.is_some_and(|k| !k.is_empty() && k.len() <= 512)
        }
        DeviceKind::Apns => {
            endpoint.is_some_and(|t| {
                (32..=200).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_hexdigit())
            }) && keys.and_then(ApnsEnvironment::of_keys).is_some()
        }
        DeviceKind::Android => endpoint.is_none() && keys.is_none(),
    };
    if ok {
        Ok(())
    } else {
        Err(ApiError::Forbidden.into())
    }
}

/// Registers a device of `user` for notifications and returns its id and secret.
///
/// A push endpoint (a browser's, an iPhone's token) is one device: registering it again (another
/// account in the same browser, a new subscription) replaces it. The device starts after the
/// notifications that are already there. Beyond [`MAX_DEVICES`], the one not seen the longest
/// goes. The time zone counts for the player's last call.
#[expect(clippy::too_many_arguments, reason = "what a device brings along")]
pub async fn register_device(
    conn: &Connection,
    user: i64,
    kind: DeviceKind,
    endpoint: Option<&str>,
    keys: Option<&str>,
    locale: Locale,
    zone: Option<&str>,
    now: i64,
) -> Result<DeviceRegistration, ServerFnError> {
    check_device(kind, endpoint, keys)?;
    let zone = timing::valid_zone(zone);
    let secret = super::auth::new_token();
    let tx = conn.transaction().await.map_err(db_err)?;
    if let Some(endpoint) = endpoint {
        tx.execute(
            "DELETE FROM notification_devices WHERE endpoint = ?1",
            params![endpoint],
        )
        .await
        .map_err(db_err)?;
    }
    tx.execute(
        "INSERT INTO notification_devices
            (user_id, kind, endpoint, keys, secret_hash, locale, time_zone, cursor, created_at, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
            (SELECT COALESCE(MAX(id), 0) FROM notification_outbox), ?8, ?8)",
        params![
            user,
            kind.id(),
            endpoint,
            keys,
            super::auth::hash_token(&secret),
            locale.as_str(),
            zone.clone(),
            now
        ],
    )
    .await
    .map_err(db_err)?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "DELETE FROM notification_devices WHERE user_id = ?1 AND id NOT IN (
            SELECT id FROM notification_devices WHERE user_id = ?1
            ORDER BY last_seen DESC, id DESC LIMIT ?2)",
        params![user, MAX_DEVICES],
    )
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    note_time_zone(conn, user, zone.as_deref(), now).await?;
    Ok(DeviceRegistration { id, secret })
}

/// The device with this id and secret.
pub async fn device(
    conn: &Connection,
    id: i64,
    secret: &str,
) -> Result<Option<Device>, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT {DEVICE_COLUMNS} FROM notification_devices WHERE id = ?1 AND secret_hash = ?2"
            ),
            params![id, super::auth::hash_token(secret)],
        )
        .await
        .map_err(db_err)?;
    match rows.next().await.map_err(db_err)? {
        Some(row) => row_to_device(&row),
        None => Ok(None),
    }
}

/// Forgets a device, with its secret: also after the player signed out on it.
pub async fn unregister_device(
    conn: &Connection,
    id: i64,
    secret: &str,
) -> Result<bool, ServerFnError> {
    let gone = conn
        .execute(
            "DELETE FROM notification_devices WHERE id = ?1 AND secret_hash = ?2",
            params![id, super::auth::hash_token(secret)],
        )
        .await
        .map_err(db_err)?;
    Ok(gone > 0)
}

async fn forget_device(conn: &Connection, id: i64) -> Result<(), ServerFnError> {
    conn.execute(
        "DELETE FROM notification_devices WHERE id = ?1",
        params![id],
    )
    .await
    .map_err(db_err)?;
    Ok(())
}

// --- The outbox ---

/// A notification to queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub kind: NotifyKind,
    /// Notifications with the same key are the same one, see [`Rule::window_ms`].
    pub key: String,
    /// The player it's about: nobody who blocked them gets it.
    pub actor: Option<i64>,
    pub args: Args,
    /// Where a tap goes, without the language (`/groups/3/races/7`).
    pub path: String,
    pub expires_at: i64,
}

/// Who a notification is for.
#[derive(Clone, Copy, Debug)]
pub enum Recipients<'a> {
    Users(&'a [i64]),
    /// Every member of a group.
    Group(i64),
}

/// Queues a notification for the recipients who want it, and returns for how many.
///
/// One statement, so two requests (or two machines) at the same moment can't both queue the
/// same one. Only for a recipient who turned its kind on, has a device to get it, isn't banned
/// or deleted, isn't the actor and didn't block them, didn't get the same one within its window
/// and is below the limits of the day ([`Rule`], [`SOCIAL_PER_DAY`]).
pub async fn enqueue(
    conn: &Connection,
    note: &Note,
    to: Recipients<'_>,
    now: i64,
) -> Result<u64, ServerFnError> {
    let (recipients, value) = match to {
        Recipients::Users(users) => (
            "SELECT DISTINCT value AS id FROM json_each(?11)",
            serde_json::to_string(users).map_err(ServerFnError::new)?,
        ),
        Recipients::Group(group) => (
            "SELECT user_id AS id FROM group_members WHERE group_id = ?11",
            group.to_string(),
        ),
    };
    let rule = Rule::of(note.kind);
    let sql = format!(
        "INSERT INTO notification_outbox
            (user_id, kind, collapse_key, actor_id, args, path, created_at, expires_at)
         SELECT r.id, ?1, ?2, ?3, ?4, ?5, ?6, ?7
         FROM ({recipients}) r
         JOIN notification_prefs p ON p.user_id = r.id AND p.enabled = 1 AND p.{column} = 1
         JOIN users u ON u.id = r.id AND u.deleted_at IS NULL AND u.banned_at IS NULL
         WHERE r.id IS NOT ?3
           AND NOT EXISTS (SELECT 1 FROM users a WHERE a.id = ?3
                AND (a.deleted_at IS NOT NULL OR a.banned_at IS NOT NULL))
           AND EXISTS (SELECT 1 FROM notification_devices d WHERE d.user_id = r.id)
           AND NOT EXISTS (SELECT 1 FROM blocks b WHERE b.user_id = r.id AND b.blocked_id = ?3)
           AND NOT EXISTS (SELECT 1 FROM notification_outbox o WHERE o.user_id = r.id
                AND o.kind = ?1 AND o.collapse_key = ?2 AND o.created_at > ?6 - ?8)
           AND (SELECT COUNT(*) FROM notification_outbox o WHERE o.user_id = r.id
                AND o.kind = ?1 AND o.created_at > ?6 - {MS_PER_DAY}) < ?9
           AND (?1 = 'last_call' OR (SELECT COUNT(*) FROM notification_outbox o
                WHERE o.user_id = r.id AND o.kind != 'last_call'
                  AND o.created_at > ?6 - {MS_PER_DAY}) < ?10)",
        column = column(note.kind),
    );
    conn.execute(
        &sql,
        params![
            note.kind.id(),
            note.key.as_str(),
            note.actor,
            note.args.to_json(),
            note.path.as_str(),
            now,
            note.expires_at,
            rule.window_ms,
            rule.per_day,
            SOCIAL_PER_DAY,
            value
        ],
    )
    .await
    .map_err(db_err)
}

/// Runs an event's queueing in the background: the request that caused it doesn't wait, and a
/// failure only goes to the log.
pub fn spawn<F>(label: &'static str, work: impl FnOnce(Connection) -> F + Send + 'static)
where
    F: Future<Output = Result<u64, ServerFnError>> + Send,
{
    tokio::spawn(async move {
        let done = match super::db::conn() {
            Ok(conn) => work(conn).await,
            Err(e) => Err(e),
        };
        if let Err(e) = done {
            eprintln!("notify: {label}: {e}");
        }
    });
}

// --- Events ---

/// A race was opened in a group: its other members may join the lobby. Not for rematches,
/// whose players are on the race's page anyway.
pub async fn race_opened(conn: &Connection, race: i64, now: i64) -> Result<u64, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT r.group_id, r.host_id, r.created_at, g.name, u.name FROM group_races r
             JOIN groups g ON g.id = r.group_id JOIN users u ON u.id = r.host_id
             WHERE r.id = ?1 AND r.starts_at IS NULL",
            params![race],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(0);
    };
    let group: i64 = row.get(0).map_err(db_err)?;
    let created: i64 = row.get(2).map_err(db_err)?;
    let note = Note {
        kind: NotifyKind::Race,
        key: format!("group:{group}"),
        actor: Some(row.get(1).map_err(db_err)?),
        args: Args {
            name: row.get(4).map_err(db_err)?,
            group: row.get(3).map_err(db_err)?,
            ..Args::default()
        },
        path: format!("/groups/{group}/races/{race}"),
        expires_at: created + LOBBY_MS,
    };
    enqueue(conn, &note, Recipients::Group(group), now).await
}

/// A puzzle was posted to a group: its other members get to know.
pub async fn puzzle_posted(conn: &Connection, puzzle: i64, now: i64) -> Result<u64, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT p.group_id, p.author_id, p.title, g.name, u.name FROM group_puzzles p
             JOIN groups g ON g.id = p.group_id JOIN users u ON u.id = p.author_id
             WHERE p.id = ?1",
            params![puzzle],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(0);
    };
    let group: i64 = row.get(0).map_err(db_err)?;
    let note = Note {
        kind: NotifyKind::Group,
        key: format!("puzzle:{puzzle}"),
        actor: Some(row.get(1).map_err(db_err)?),
        args: Args {
            name: row.get(4).map_err(db_err)?,
            group: row.get(3).map_err(db_err)?,
            title: row
                .get::<Option<String>>(2)
                .map_err(db_err)?
                .unwrap_or_default(),
            ..Args::default()
        },
        path: format!("/groups/{group}/puzzles/{puzzle}"),
        expires_at: now + RETENTION_MS,
    };
    enqueue(conn, &note, Recipients::Group(group), now).await
}

/// `user` finished the daily of `day`: the players who have them as a rival and were slower get
/// to know. Only a ranked result counts.
pub async fn rival_finished(
    conn: &Connection,
    user: i64,
    day: i64,
    now: i64,
) -> Result<u64, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT a.time_ms, u.name FROM attempts a JOIN users u ON u.id = a.user_id
             WHERE a.user_id = ?1 AND a.day = ?2 AND a.finished_at IS NOT NULL
               AND a.revealed = 0 AND a.flag IS NULL AND a.time_ms IS NOT NULL",
            params![user, day],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(0);
    };
    let time_ms: i64 = row.get(0).map_err(db_err)?;
    let name: String = row.get(1).map_err(db_err)?;
    let mut rows = conn
        .query(
            "SELECT r.user_id FROM rivals r
             JOIN attempts a ON a.user_id = r.user_id AND a.day = ?2
             WHERE r.rival_id = ?1 AND a.finished_at IS NOT NULL AND a.revealed = 0
               AND a.flag IS NULL AND a.time_ms > ?3",
            params![user, day, time_ms],
        )
        .await
        .map_err(db_err)?;
    let mut slower = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        slower.push(row.get::<i64>(0).map_err(db_err)?);
    }
    if slower.is_empty() {
        return Ok(0);
    }
    let note = Note {
        kind: NotifyKind::Rival,
        key: format!("day:{day}:{user}"),
        actor: Some(user),
        args: Args {
            name,
            time_ms,
            ..Args::default()
        },
        path: "/".into(),
        expires_at: (day + 1) * MS_PER_DAY,
    };
    enqueue(conn, &note, Recipients::Users(&slower), now).await
}

/// Someone solved a puzzle through the link `share`: its sharer gets to know.
///
/// At most once in [`Rule::window_ms`], with how many solved it since the last time (the job
/// comes back for the solves in between, see [`flush_shared`]). Blocked and banned players don't
/// count.
pub async fn shared_solved(conn: &Connection, share: i64, now: i64) -> Result<u64, ServerFnError> {
    let key = format!("share:{share}");
    let mut rows = conn
        .query(
            "SELECT s.user_id, s.code, (SELECT MAX(o.created_at) FROM notification_outbox o
                WHERE o.user_id = s.user_id AND o.kind = 'shared' AND o.collapse_key = ?2)
             FROM puzzle_shares s WHERE s.id = ?1 AND s.user_id IS NOT NULL",
            params![share, key.as_str()],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(0);
    };
    let sharer: i64 = row.get(0).map_err(db_err)?;
    let code: String = row.get(1).map_err(db_err)?;
    let last: Option<i64> = row.get(2).map_err(db_err)?;
    if last.is_some_and(|last| last > now - Rule::of(NotifyKind::Shared).window_ms) {
        return Ok(0);
    }
    let since = last.unwrap_or(now - RETENTION_MS);
    let mut rows = conn
        .query(
            "SELECT r.user_id, u.name FROM shared_results r
             JOIN users u ON u.id = r.user_id AND u.banned_at IS NULL AND u.deleted_at IS NULL
             WHERE r.share_id = ?1 AND r.user_id != ?2 AND r.finished_at > ?3
               AND NOT EXISTS (SELECT 1 FROM blocks b WHERE b.user_id = ?2 AND b.blocked_id = r.user_id)
             ORDER BY r.finished_at DESC, r.user_id DESC",
            params![share, sharer, since],
        )
        .await
        .map_err(db_err)?;
    let mut solvers = 0;
    let mut latest = None;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        if latest.is_none() {
            latest = Some((
                row.get::<i64>(0).map_err(db_err)?,
                row.get::<String>(1).map_err(db_err)?,
            ));
        }
        solvers += 1;
    }
    let Some((actor, name)) = latest else {
        return Ok(0);
    };
    let note = Note {
        kind: NotifyKind::Shared,
        key,
        actor: Some(actor),
        args: Args {
            name,
            count: solvers,
            ..Args::default()
        },
        path: format!("/p/{code}"),
        expires_at: now + RETENTION_MS,
    };
    enqueue(conn, &note, Recipients::Users(&[sharer]), now).await
}

/// [`shared_solved`] for the link with this code.
pub async fn shared_solved_code(
    conn: &Connection,
    code: &str,
    now: i64,
) -> Result<u64, ServerFnError> {
    match scalar(
        conn,
        "SELECT id FROM puzzle_shares WHERE code = ?1",
        params![code],
    )
    .await?
    {
        Some(share) => shared_solved(conn, share, now).await,
        None => Ok(0),
    }
}

/// The solves of shared puzzles nobody was told about yet: those that came while their sharer's
/// last notification was too recent, and those through a race on a shared puzzle.
pub async fn flush_shared(conn: &Connection, now: i64) -> Result<u64, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT DISTINCT r.share_id FROM shared_results r
             JOIN puzzle_shares s ON s.id = r.share_id AND s.user_id != r.user_id
             JOIN notification_prefs p ON p.user_id = s.user_id AND p.enabled = 1 AND p.shared = 1
             WHERE r.finished_at > ?1
               AND NOT EXISTS (SELECT 1 FROM notification_outbox o WHERE o.user_id = s.user_id
                    AND o.kind = 'shared' AND o.collapse_key = 'share:' || r.share_id
                    AND o.created_at >= r.finished_at)",
            params![now - RETENTION_MS],
        )
        .await
        .map_err(db_err)?;
    let mut shares = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        shares.push(row.get::<i64>(0).map_err(db_err)?);
    }
    let mut queued = 0;
    for share in shares {
        queued += shared_solved(conn, share, now).await?;
    }
    Ok(queued)
}

/// SQL: whether the player `p.user_id` can still get the last call of the day `?1` (its key
/// `?2`): today's daily not done (solved or the solution shown) and none queued for it yet.
/// Whether they have a streak to keep is [`streak_open`]'s.
const LAST_CALL_DUE: &str =
    "NOT EXISTS (SELECT 1 FROM attempts a WHERE a.user_id = p.user_id AND a.day = ?1
        AND (a.finished_at IS NOT NULL OR a.revealed != 0))
    AND NOT EXISTS (SELECT 1 FROM notification_outbox o WHERE o.user_id = p.user_id
        AND o.kind = 'last_call' AND o.collapse_key = ?2)";

/// SQL: whether the player `p.user_id` has a day that counts for the streak (ranked, or caught
/// up from `saves_from` on) in `?1 - back ..= ?1 - 1`.
fn counted_within(back: i64, saves_from: Option<i64>) -> String {
    let ranked = format!(
        "EXISTS (SELECT 1 FROM attempts a WHERE a.user_id = p.user_id
            AND a.day BETWEEN ?1 - {back} AND ?1 - 1 AND a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL)"
    );
    // Numbers, nothing to escape
    saves_from.map_or_else(
        || ranked.clone(),
        |from| {
            format!(
                "({ranked} OR EXISTS (SELECT 1 FROM catch_ups c WHERE c.user_id = p.user_id
                    AND c.day BETWEEN ?1 - {back} AND ?1 - 1 AND c.day >= {from} AND c.finished_at IS NOT NULL))"
            )
        },
    )
}

/// How many days back the last day that counts can be for a streak to be open: yesterday, or
/// before an open gap of at most `CATCH_UP_DAYS` (with saves).
fn lookback(saves_from: Option<i64>) -> i64 {
    1 + saves_from.map_or(0, |_| crate::model::CATCH_UP_DAYS)
}

/// SQL: the columns [`streak_open`] needs of the player `p.user_id` on the day `?1`: whether
/// yesterday counts, and whether a day within the [`lookback`] does.
fn streak_columns(saves_from: Option<i64>) -> String {
    format!(
        "{}, {}",
        counted_within(1, saves_from),
        counted_within(lookback(saves_from), saves_from)
    )
}

/// Of the players with the [`streak_columns`] `(user, yesterday, recent)`, those whose streak is
/// open on `day` while today's daily isn't done: yesterday counts, or an open gap follows the
/// streak (see `model::streak_state`, which only those need).
async fn streak_open(
    conn: &Connection,
    players: &[(i64, bool, bool)],
    day: i64,
    saves_from: Option<i64>,
) -> Result<Vec<i64>, ServerFnError> {
    let gaps: Vec<i64> = players
        .iter()
        .filter(|&&(_, yesterday, recent)| !yesterday && recent)
        .map(|p| p.0)
        .collect();
    let states = if gaps.is_empty() || saves_from.is_none() {
        std::collections::HashMap::new()
    } else {
        super::daily::streak::states(conn, &gaps, day, saves_from).await?
    };
    Ok(players
        .iter()
        .filter(|&&(user, yesterday, _)| {
            yesterday || states.get(&user).is_some_and(|s| s.current > 0)
        })
        .map(|p| p.0)
        .collect())
}

/// Queues the last calls that are due.
///
/// For the players who want them, with a streak to keep (yesterday's daily solved and ranked or
/// caught up, or an open gap after the streak, see [`streak_open`]) and today's daily not done
/// yet, once [`timing::last_call_at`] came in their time zone. Once per player and daily.
pub async fn queue_last_calls(conn: &Connection, now: i64) -> Result<u64, ServerFnError> {
    queue_last_calls_of(conn, None, now).await
}

/// [`queue_last_calls`], of one player only with `only`.
async fn queue_last_calls_of(
    conn: &Connection,
    only: Option<i64>,
    now: i64,
) -> Result<u64, ServerFnError> {
    let day = day_of(now);
    let change = (day + 1) * MS_PER_DAY;
    if now >= change - LAST_CALL_UNTIL_MS {
        return Ok(0);
    }
    let saves_from = super::daily::streak::saves_from();
    let mut rows = conn
        .query(
            &format!(
                "SELECT p.user_id, p.time_zone, {yesterday} FROM notification_prefs p
                 WHERE p.enabled = 1 AND p.last_call = 1 AND (?3 IS NULL OR p.user_id = ?3)
                   AND EXISTS (SELECT 1 FROM notification_devices d WHERE d.user_id = p.user_id)
                   AND {LAST_CALL_DUE} AND {recent}",
                yesterday = counted_within(1, saves_from),
                recent = counted_within(lookback(saves_from), saves_from),
            ),
            params![day, day.to_string(), only],
        )
        .await
        .map_err(db_err)?;
    let mut due = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let user: i64 = row.get(0).map_err(db_err)?;
        let zone: Option<String> = row.get(1).map_err(db_err)?;
        if now >= timing::last_call_at(day, zone.as_deref()) {
            due.push((user, row.get::<i64>(2).map_err(db_err)? != 0, true));
        }
    }
    drop(rows);
    let due = streak_open(conn, &due, day, saves_from).await?;
    if due.is_empty() {
        return Ok(0);
    }
    let note = Note {
        kind: NotifyKind::LastCall,
        key: day.to_string(),
        actor: None,
        args: Args::default(),
        path: "/".into(),
        expires_at: change,
    };
    enqueue(conn, &note, Recipients::Users(&due), now).await
}

/// When a player's next last call would be due, `None` while they don't want last calls.
///
/// Today's while it can still come, else tomorrow's (which comes only if today's daily is solved
/// by then; the poll at that time asks the database again).
///
/// For the Android app, which asks for its notifications every 15 minutes at best, and much less
/// often while the phone sleeps: it asks once more around this moment ([`poll`]).
pub async fn next_last_call(
    conn: &Connection,
    user: i64,
    now: i64,
) -> Result<Option<i64>, ServerFnError> {
    let day = day_of(now);
    let saves_from = super::daily::streak::saves_from();
    let mut rows = conn
        .query(
            &format!(
                "SELECT p.time_zone, {LAST_CALL_DUE}, {columns} FROM notification_prefs p
                 WHERE p.user_id = ?3 AND p.enabled = 1 AND p.last_call = 1",
                columns = streak_columns(saves_from),
            ),
            params![day, day.to_string(), user],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    let zone: Option<String> = row.get(0).map_err(db_err)?;
    let due = row.get::<i64>(1).map_err(db_err)? != 0;
    let streak = (
        user,
        row.get::<i64>(2).map_err(db_err)? != 0,
        row.get::<i64>(3).map_err(db_err)? != 0,
    );
    drop(rows);
    let open = due
        && !streak_open(conn, &[streak], day, saves_from)
            .await?
            .is_empty();
    let today = timing::last_call_at(day, zone.as_deref());
    // Past today's moment, its last call is queued (by the job or the poll) or not to come
    Ok(Some(if open && now < today {
        today
    } else {
        timing::last_call_at(day + 1, zone.as_deref())
    }))
}

// --- Delivery ---

/// The notifications a device hasn't got yet, as it shows them, and the cursor after them.
///
/// The cursor goes past every row looked at, also those that don't go out any more (switched
/// off, from a player blocked since, a last call for a daily done since, expired).
pub async fn pending(
    conn: &Connection,
    device: &Device,
    now: i64,
) -> Result<(Vec<PushMessage>, i64), ServerFnError> {
    let sql = format!(
        "SELECT o.id, o.kind, o.collapse_key, o.args, o.path, o.expires_at,
            COALESCE(({wanted})
                AND o.expires_at > ?3
                AND NOT EXISTS (SELECT 1 FROM blocks b WHERE b.user_id = o.user_id AND b.blocked_id = o.actor_id)
                AND NOT (o.kind = 'last_call' AND EXISTS (SELECT 1 FROM attempts a
                    WHERE a.user_id = o.user_id AND a.day = CAST(o.collapse_key AS INTEGER)
                      AND (a.finished_at IS NOT NULL OR a.revealed != 0))), 0)
         FROM notification_outbox o
         LEFT JOIN notification_prefs p ON p.user_id = o.user_id
         WHERE o.user_id = ?1 AND o.id > ?2
         ORDER BY o.id LIMIT {BATCH}",
        wanted = wanted_sql(),
    );
    let mut rows = conn
        .query(&sql, params![device.user_id, device.cursor, now])
        .await
        .map_err(db_err)?;
    let mut messages = Vec::new();
    let mut cursor = device.cursor;
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let id: i64 = row.get(0).map_err(db_err)?;
        cursor = cursor.max(id);
        if row.get::<i64>(6).map_err(db_err)? == 0 {
            continue;
        }
        let Some(kind) = NotifyKind::from_id(&row.get::<String>(1).map_err(db_err)?) else {
            continue;
        };
        let args = Args::from_json(&row.get::<String>(3).map_err(db_err)?);
        let (title, body) = render::text(kind, &args, device.locale);
        let key: String = row.get(2).map_err(db_err)?;
        messages.push(PushMessage {
            id,
            kind: kind.id().into(),
            title,
            body,
            url: render::url(&row.get::<String>(4).map_err(db_err)?, device.locale),
            tag: format!("{}:{key}", kind.id()),
            expires_at: row.get(5).map_err(db_err)?,
        });
    }
    Ok((messages, cursor))
}

/// Claims a device's pending notifications: moves its cursor past them.
///
/// Only if nobody else did in between (another machine's job, a second poll). What it claimed is
/// the caller's to send; nothing is sent twice, a failed send is lost.
pub async fn take(
    conn: &Connection,
    device: &Device,
    now: i64,
) -> Result<Vec<PushMessage>, ServerFnError> {
    let (messages, cursor) = pending(conn, device, now).await?;
    let claimed = conn
        .execute(
            "UPDATE notification_devices SET cursor = ?3, last_seen = MAX(last_seen, ?4)
             WHERE id = ?1 AND cursor = ?2",
            params![device.id, device.cursor, cursor, now],
        )
        .await
        .map_err(db_err)?;
    Ok(if claimed == 1 { messages } else { Vec::new() })
}

/// What the Android app gets when it asks: its new notifications, and when to ask again for the
/// streak's last call ([`next_last_call`]).
///
/// With the device's secret instead of a session (the app asks in the background). `None` for a
/// device that isn't registered (any more): the app forgets it. A last call that is due is
/// queued here if the job hasn't yet, so the app's poll at that moment gets it, not the one
/// after the job's next round.
pub async fn poll(
    conn: &Connection,
    id: i64,
    secret: &str,
    now: i64,
) -> Result<Option<NotificationPoll>, ServerFnError> {
    let Some(device) = device(conn, id, secret).await? else {
        return Ok(None);
    };
    queue_last_calls_of(conn, Some(device.user_id), now).await?;
    let messages = take(conn, &device, now).await?;
    let next_last_call = next_last_call(conn, device.user_id, now).await?;
    Ok(Some(NotificationPoll {
        messages,
        next_last_call,
    }))
}

/// How a send went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    /// The push service says the device is gone (unsubscribed, app deleted): forget it.
    Gone,
    /// Didn't work this time.
    Failed,
}

/// A way to push notifications to devices of one kind: Web Push, APNs.
pub trait Channel: Send + Sync {
    fn kind(&self) -> DeviceKind;
    fn send<'a>(&'a self, device: &'a Device, message: &'a PushMessage) -> BoxFuture<'a, Delivery>;
}

/// The channels this server pushes through: those configured.
pub fn channels() -> Vec<Box<dyn Channel>> {
    let mut channels: Vec<Box<dyn Channel>> = Vec::new();
    if let Some(web) = web_push::WebPush::from_env() {
        channels.push(Box::new(web));
    }
    if let Some(apns) = apns::Apns::from_env() {
        channels.push(Box::new(apns));
    }
    channels
}

/// Pushes the pending notifications of every device a channel serves. Returns how many went out.
pub async fn deliver(
    conn: &Connection,
    channels: &[Box<dyn Channel>],
    now: i64,
) -> Result<usize, ServerFnError> {
    let mut sent = 0;
    for channel in channels {
        let mut rows = conn
            .query(
                &format!(
                    "SELECT {DEVICE_COLUMNS} FROM notification_devices d WHERE kind = ?1
                     AND EXISTS (SELECT 1 FROM notification_outbox o
                        WHERE o.user_id = d.user_id AND o.id > d.cursor)"
                ),
                params![channel.kind().id()],
            )
            .await
            .map_err(db_err)?;
        let mut devices = Vec::new();
        while let Some(row) = rows.next().await.map_err(db_err)? {
            devices.extend(row_to_device(&row)?);
        }
        for device in devices {
            for message in take(conn, &device, now).await? {
                match channel.send(&device, &message).await {
                    Delivery::Sent => sent += 1,
                    Delivery::Gone => {
                        forget_device(conn, device.id).await?;
                        break;
                    }
                    Delivery::Failed => {
                        eprintln!(
                            "notify: sending #{} to device #{} failed",
                            message.id, device.id
                        );
                    }
                }
            }
        }
    }
    Ok(sent)
}

// --- Keeping it tidy ---

/// Deletes the notifications older than [`RETENTION_MS`] and the devices not seen for
/// [`DEVICE_IDLE_MS`].
pub async fn delete_old(conn: &Connection, now: i64) -> Result<(), ServerFnError> {
    conn.execute(
        "DELETE FROM notification_outbox WHERE created_at < ?1",
        params![now - RETENTION_MS],
    )
    .await
    .map_err(db_err)?;
    conn.execute(
        "DELETE FROM notification_devices WHERE last_seen < ?1",
        params![now - DEVICE_IDLE_MS],
    )
    .await
    .map_err(db_err)?;
    Ok(())
}

/// One round of the job: the due last calls, the shared puzzles' leftovers, the deliveries and
/// the cleanup. Returns how many were queued and sent.
pub async fn run_once(
    conn: &Connection,
    channels: &[Box<dyn Channel>],
    now: i64,
) -> Result<(u64, usize), ServerFnError> {
    let queued = queue_last_calls(conn, now).await? + flush_shared(conn, now).await?;
    let sent = deliver(conn, channels, now).await?;
    delete_old(conn, now).await?;
    Ok((queued, sent))
}

/// The job, every [`JOB_EVERY_MS`]. Every machine of the site runs it, the statements are safe
/// against each other (see [`enqueue`], [`take`]).
pub fn start() {
    tokio::spawn(async {
        let channels = channels();
        loop {
            match super::db::conn() {
                Ok(conn) => match run_once(&conn, &channels, now_ms()).await {
                    Ok((0, 0)) => {}
                    Ok((queued, sent)) => {
                        println!("notify: queued {queued}, sent {sent}");
                    }
                    Err(e) => eprintln!("notify: {e}"),
                },
                Err(e) => eprintln!("notify: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_millis(
                JOB_EVERY_MS.unsigned_abs(),
            ))
            .await;
        }
    });
}

/// What deleting a user does here, for `auth::erase` (`?1` the user): their settings, devices and
/// notifications go, and those about them (they carry their name).
pub const ERASE: [&str; 3] = [
    "DELETE FROM notification_prefs WHERE user_id = ?1",
    "DELETE FROM notification_devices WHERE user_id = ?1",
    "DELETE FROM notification_outbox WHERE user_id = ?1 OR actor_id = ?1",
];

/// For `auth::merge_guest`: `?1` the account, `?2` the guest of the browser.
///
/// The account keeps its settings (or takes the guest's if it has none), the guest's devices are
/// the account's now, the guest's notifications go.
pub const MERGE_GUEST: [&str; 5] = [
    "INSERT OR IGNORE INTO notification_prefs
        (user_id, enabled, last_call, race, rival, shared, group_puzzle, offer_shown, time_zone, updated_at)
     SELECT ?1, enabled, last_call, race, rival, shared, group_puzzle, offer_shown, time_zone, updated_at
     FROM notification_prefs WHERE user_id = ?2",
    "DELETE FROM notification_prefs WHERE user_id = ?2",
    "UPDATE notification_devices SET user_id = ?1 WHERE user_id = ?2",
    "DELETE FROM notification_outbox WHERE user_id = ?2",
    "UPDATE notification_outbox SET actor_id = ?1 WHERE actor_id = ?2",
];

/// How many notifications are waiting for a player.
#[cfg(test)]
pub async fn queued(conn: &Connection, user: i64) -> Result<i64, ServerFnError> {
    Ok(scalar(
        conn,
        "SELECT COUNT(*) FROM notification_outbox WHERE user_id = ?1",
        params![user],
    )
    .await?
    .unwrap_or_default())
}
