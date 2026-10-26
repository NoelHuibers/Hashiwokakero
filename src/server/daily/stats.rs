//! A player's daily statistics: solved days, streaks, times, recent results, medals, the
//! monthly race and the stamp book. Also on their public profile.

use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::fame::month_points;
use super::stamps::{book, holders, shown_stamp};
use super::today;
use crate::model::{DayResult, PlayerProfile, UserStats, month_of, streaks};
use crate::server::count;
use crate::server::db::db_err;

/// Days shown in the player's recent results.
const RECENT_DAYS: i64 = 35;

async fn solved_days(conn: &Connection, user: i64) -> Result<Vec<i64>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT day FROM attempts WHERE user_id = ?1 AND finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL ORDER BY day",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut days = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        days.push(row.get(0).map_err(db_err)?);
    }
    Ok(days)
}

pub async fn stats(conn: &Connection, user: i64) -> Result<UserStats, ServerFnError> {
    let today = today();
    let days = solved_days(conn, user).await?;
    let (current_streak, best_streak) = streaks(&days, today);

    // (day, time, rank) of every ranked finish, ranks in the same query to save round trips
    let mut rows = conn
        .query(
            "SELECT a.day, a.time_ms, (
                SELECT COUNT(*) + 1 FROM attempts b
                WHERE b.day = a.day AND b.finished_at IS NOT NULL AND b.revealed = 0 AND b.flag IS NULL
                  AND (b.rank_ms < a.rank_ms OR (b.rank_ms = a.rank_ms AND b.rank_finish < a.rank_finish))
             ) FROM attempts a
             WHERE a.user_id = ?1 AND a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let mut finished = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let day: i64 = row.get(0).map_err(db_err)?;
        let time: i64 = row.get(1).map_err(db_err)?;
        let rank: i64 = row.get(2).map_err(db_err)?;
        finished.push((day, time, rank));
    }
    let best_time_ms = finished.iter().map(|f| f.1).min();
    let average_time_ms = (!finished.is_empty())
        .then(|| finished.iter().map(|f| f.1).sum::<i64>() / count(finished.len()));

    let recent = (today - RECENT_DAYS + 1..=today)
        .map(|day| {
            let found = finished.iter().find(|f| f.0 == day);
            DayResult {
                day,
                time_ms: found.map(|f| f.1),
                rank: found.map(|f| f.2),
            }
        })
        .collect();

    // Medals only count once the day is over
    let medals = |place| {
        count(
            finished
                .iter()
                .filter(|f| f.0 < today && f.2 == place)
                .count(),
        )
    };
    let month = month_points(conn, month_of(today)).await?;
    let month_place = month.iter().position(|p| p.0 == user);
    let mut stamps = book(conn, user).await?;
    let (holders, players) = holders(conn).await?;
    for p in &mut stamps {
        p.holders = holders.get(&p.stamp).copied().unwrap_or_default();
    }

    Ok(UserStats {
        solved: count(days.len()),
        current_streak,
        best_streak,
        best_time_ms,
        average_time_ms,
        recent,
        gold: medals(1),
        silver: medals(2),
        bronze: medals(3),
        month_rank: month_place.map(|i| count(i + 1)),
        month_points: month_place.map_or(0, |i| month[i].1),
        stamps,
        players,
        shown: shown_stamp(conn, user).await?,
    })
}

/// The public profile of a player, `None` if there is no such player (or they are banned).
pub async fn profile(conn: &Connection, user: i64) -> Result<Option<PlayerProfile>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT name, username IS NULL, created_at FROM users
             WHERE id = ?1 AND deleted_at IS NULL AND banned_at IS NULL",
            params![user],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    let name: String = row.get(0).map_err(db_err)?;
    let guest: i64 = row.get(1).map_err(db_err)?;
    let created_at: i64 = row.get(2).map_err(db_err)?;
    Ok(Some(PlayerProfile {
        id: user,
        name,
        guest: guest != 0,
        created_at,
        stats: stats(conn, user).await?,
    }))
}

// Only the installed HTTP database uses this cache. Domain helpers above also serve independent
// databases in tests and deliberately do not share entries between those databases.
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tokio::sync::watch;

const PROFILE_TTL: Duration = Duration::from_secs(30);
const PROFILE_ENTRIES: usize = 512;
const PROFILE_FLIGHTS: usize = 64;

type StatsResult = Result<UserStats, ServerFnError>;
type Completion = watch::Receiver<Option<StatsResult>>;

#[derive(Debug)]
struct Flight {
    token: Arc<()>,
    completion: Completion,
}

#[derive(Debug, Default)]
struct Profiles {
    generation: u64,
    entries: HashMap<i64, (Instant, UserStats)>,
    flights: HashMap<i64, Flight>,
    // Includes obsolete owners still running after invalidation. Clearing the lookup map must
    // not allow repeated invalidation to bypass the active-flight metadata bound.
    active_flights: usize,
}

#[derive(Debug, Default)]
struct ProfileCache(Mutex<Profiles>);

enum Lookup<'a> {
    Cached(UserStats),
    Follow(Completion),
    Owner(FlightOwner<'a>),
    Uncached,
}

/// The request owns its computation: no detached task outlives it. Dropping this guard closes
/// the watch channel, waking followers so one can retry with its own computation closure.
struct FlightOwner<'a> {
    cache: &'a ProfileCache,
    user: i64,
    generation: u64,
    token: Arc<()>,
    completion: watch::Sender<Option<StatsResult>>,
}

impl FlightOwner<'_> {
    fn is_current(&self, cache: &Profiles) -> bool {
        cache.generation == self.generation
            && cache
                .flights
                .get(&self.user)
                .is_some_and(|flight| Arc::ptr_eq(&flight.token, &self.token))
    }

    fn complete(self, result: StatsResult) -> StatsResult {
        {
            let mut cache = self.cache.lock();
            if self.is_current(&cache)
                && cache.entries.len() < PROFILE_ENTRIES
                && let Ok(stats) = &result
            {
                cache
                    .entries
                    .insert(self.user, (Instant::now(), stats.clone()));
            }
        }
        // Completion belongs to the flight, independently of cache admission or invalidation.
        // Retain the value even if every follower was cancelled before publication.
        self.completion.send_replace(Some(result.clone()));
        result
    }
}

impl Drop for FlightOwner<'_> {
    fn drop(&mut self) {
        let mut cache = self.cache.lock();
        if self.is_current(&cache) {
            cache.flights.remove(&self.user);
        }
        cache.active_flights -= 1;
        // The sender drops after this lock is released. Old owners cannot remove replacements.
    }
}

impl ProfileCache {
    fn lock(&self) -> MutexGuard<'_, Profiles> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn invalidate(&self) {
        let mut cache = self.lock();
        cache.generation = cache.generation.wrapping_add(1);
        cache.entries.clear();
        // Existing followers keep their receivers. New requests cannot join obsolete work.
        cache.flights.clear();
    }

    fn lookup(&self, user: i64) -> Lookup<'_> {
        let mut cache = self.lock();
        cache
            .entries
            .retain(|_, (at, _)| at.elapsed() < PROFILE_TTL);
        if let Some((_, stats)) = cache.entries.get(&user) {
            return Lookup::Cached(stats.clone());
        }
        if let Some(flight) = cache.flights.get(&user) {
            return Lookup::Follow(flight.completion.clone());
        }
        if cache.active_flights >= PROFILE_FLIGHTS {
            // Capacity fallback: run the original uncached computation in this request. It
            // creates no flight metadata and does not populate the cache. Existing keys can
            // still be joined at capacity; this is not a limit on concurrent database work.
            return Lookup::Uncached;
        }
        let (completion, receiver) = watch::channel(None);
        let token = Arc::new(());
        cache.flights.insert(
            user,
            Flight {
                token: token.clone(),
                completion: receiver,
            },
        );
        cache.active_flights += 1;
        Lookup::Owner(FlightOwner {
            cache: self,
            user,
            generation: cache.generation,
            token,
            completion,
        })
    }

    async fn get_or_compute<F, Fut>(&self, user: i64, compute: F) -> StatsResult
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = StatsResult>,
    {
        loop {
            match self.lookup(user) {
                Lookup::Cached(stats) => return Ok(stats),
                Lookup::Owner(owner) => return owner.complete(compute().await),
                Lookup::Uncached => return compute().await,
                Lookup::Follow(mut completion) => loop {
                    let completed = completion.borrow_and_update().clone();
                    if let Some(result) = completed {
                        return result;
                    }
                    // watch remembers completion between inspecting the value and awaiting;
                    // there is no Notify registration/lost-wakeup window. Closure means the
                    // owner was cancelled: retry election without consuming our FnOnce.
                    if completion.changed().await.is_err() {
                        break;
                    }
                },
            }
        }
    }
}

static PROFILES: LazyLock<ProfileCache> = LazyLock::new(ProfileCache::default);

pub(crate) fn invalidate() {
    PROFILES.invalidate();
}

/// Bounded snapshots of public aggregates. A mutation invalidates them; the TTL also refreshes
/// other replicas' writes and changes of day. No user-controlled collection can grow without limit.
pub async fn cached_profile(user: i64) -> Result<Option<PlayerProfile>, ServerFnError> {
    let conn = crate::server::db::conn()?;
    profile_with_cache(&PROFILES, &conn, user).await
}

async fn profile_with_cache(
    cache: &ProfileCache,
    conn: &Connection,
    user: i64,
) -> Result<Option<PlayerProfile>, ServerFnError> {
    // Always revalidate identity, deletion and bans, including writes on another replica. Only
    // the aggregates are cached; neither may keep a public identity alive for the TTL.
    // Finish the identity statement before joining a flight. A follower may become a
    // replacement owner after invalidation; retaining this read would reuse its old SQLite
    // snapshot and could insert stale statistics into the new generation.
    let (name, guest, created_at) = {
        let mut rows = conn
            .query(
                "SELECT name, username IS NULL, created_at FROM users
         WHERE id = ?1 AND deleted_at IS NULL AND banned_at IS NULL",
                params![user],
            )
            .await
            .map_err(db_err)?;
        let Some(row) = rows.next().await.map_err(db_err)? else {
            return Ok(None);
        };
        (
            row.get::<String>(0).map_err(db_err)?,
            row.get::<i64>(1).map_err(db_err)? != 0,
            row.get::<i64>(2).map_err(db_err)?,
        )
    };
    let stats = cache.get_or_compute(user, || stats(conn, user)).await?;
    Ok(Some(PlayerProfile {
        id: user,
        name,
        guest,
        created_at,
        stats,
    }))
}

#[cfg(test)]
#[path = "profile_cache_tests.rs"]
mod profile_cache_tests;
