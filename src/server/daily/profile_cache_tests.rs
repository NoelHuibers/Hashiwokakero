//! Local caches and manually polled gates keep these tests independent of the HTTP singleton,
//! scheduler timing and sleeps. They exercise the same computation hook used by production.

use std::cell::Cell;
use std::future::pending;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use futures::poll;
use futures::task::{ArcWake, waker_ref};
use tokio::sync::oneshot;

use super::*;
use crate::model::Stamp;
use crate::server::db::TestConn;

fn snapshot(solved: i64) -> UserStats {
    UserStats {
        solved,
        current_streak: 2,
        best_streak: 3,
        best_time_ms: Some(1234),
        average_time_ms: Some(2345),
        recent: vec![DayResult {
            day: 42,
            time_ms: Some(1234),
            rank: Some(1),
        }],
        gold: 1,
        shown: Some(Stamp::First),
        ..UserStats::default()
    }
}

fn ready<T>(value: Poll<T>) -> T {
    match value {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected immediate completion"),
    }
}

fn assert_send<T: Send>(_: &T) {}

#[derive(Default)]
struct WakeCount(AtomicUsize);

impl ArcWake for WakeCount {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn observed_poll<F: Future>(future: Pin<&mut F>, wakes: &Arc<WakeCount>) -> Poll<F::Output> {
    let waker = waker_ref(wakes);
    future.poll(&mut Context::from_waker(&waker))
}

#[tokio::test]
async fn ten_cold_requests_share_one_complete_snapshot() {
    let cache = ProfileCache::default();
    let calls = Cell::new(0);
    let expected = snapshot(7);
    let (release, gate) = oneshot::channel();
    let mut owner = Box::pin(cache.get_or_compute(7, || async {
        calls.set(calls.get() + 1);
        gate.await.unwrap();
        Ok(expected.clone())
    }));
    assert!(poll!(owner.as_mut()).is_pending());
    let mut followers: Vec<_> = (0..9)
        .map(|_| {
            Box::pin(cache.get_or_compute(7, || async {
                calls.set(calls.get() + 1);
                Ok(snapshot(99))
            }))
        })
        .collect();
    let wakes: Vec<_> = (0..followers.len())
        .map(|_| Arc::new(WakeCount::default()))
        .collect();
    for (follower, wakes) in followers.iter_mut().zip(&wakes) {
        assert!(observed_poll(follower.as_mut(), wakes).is_pending());
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(cache.lock().active_flights, 1);
    release.send(()).unwrap();
    assert_eq!(ready(poll!(owner.as_mut())).unwrap(), expected);
    assert!(
        wakes
            .iter()
            .all(|wakes| wakes.0.load(Ordering::Relaxed) > 0)
    );
    for follower in &mut followers {
        assert_eq!(ready(poll!(follower.as_mut())).unwrap(), expected);
    }
    assert_eq!(calls.get(), 1);
    let state = cache.lock();
    assert_eq!(state.entries.len(), 1);
    assert!(state.flights.is_empty());
    assert_eq!(state.active_flights, 0);
}

#[tokio::test]
async fn unrelated_users_compute_while_another_owner_is_paused() {
    let cache = ProfileCache::default();
    let (release_a, gate_a) = oneshot::channel();
    let (release_b, gate_b) = oneshot::channel();
    let calls = Cell::new(0);
    let mut a = Box::pin(cache.get_or_compute(1, || async {
        calls.set(calls.get() + 1);
        gate_a.await.unwrap();
        Ok(snapshot(1))
    }));
    let mut b = Box::pin(cache.get_or_compute(2, || async {
        calls.set(calls.get() + 1);
        gate_b.await.unwrap();
        Ok(snapshot(2))
    }));
    assert!(poll!(a.as_mut()).is_pending());
    assert!(poll!(b.as_mut()).is_pending());
    assert_eq!(calls.get(), 2);
    assert_eq!(cache.lock().active_flights, 2);
    release_b.send(()).unwrap();
    assert_eq!(ready(poll!(b.as_mut())).unwrap(), snapshot(2));
    assert!(poll!(a.as_mut()).is_pending());
    release_a.send(()).unwrap();
    assert_eq!(ready(poll!(a.as_mut())).unwrap(), snapshot(1));
}

#[tokio::test]
async fn warm_hits_do_no_work_and_do_not_extend_the_thirty_second_ttl() {
    let cache = ProfileCache::default();
    cache
        .get_or_compute(1, || async { Ok(snapshot(1)) })
        .await
        .unwrap();
    let inserted = cache.lock().entries[&1].0;
    for _ in 0..10 {
        assert_eq!(
            cache
                .get_or_compute(1, || async { panic!("warm computation") })
                .await
                .unwrap(),
            snapshot(1)
        );
    }
    assert_eq!(cache.lock().entries[&1].0, inserted);
    assert_eq!(PROFILE_TTL, Duration::from_secs(30));
    // Model an entry from before the UTC day changed. Day-sensitive aggregates must refresh
    // by the same TTL, even after repeated warm reads, without waiting 30 seconds in a test.
    cache.lock().entries.get_mut(&1).unwrap().0 = Instant::now() - PROFILE_TTL;
    let calls = Cell::new(0);
    assert_eq!(
        cache
            .get_or_compute(1, || async {
                calls.set(calls.get() + 1);
                Ok(snapshot(2))
            })
            .await
            .unwrap(),
        snapshot(2)
    );
    assert_eq!(calls.get(), 1);
}

#[tokio::test]
async fn invalidation_separates_generations_and_old_completion_cannot_insert_or_remove() {
    let cache = ProfileCache::default();
    let (release_old, gate_old) = oneshot::channel();
    let mut old = Box::pin(cache.get_or_compute(1, || async {
        gate_old.await.unwrap();
        Ok(snapshot(1))
    }));
    assert!(poll!(old.as_mut()).is_pending());
    let mut old_follower = Box::pin(cache.get_or_compute(1, || async { panic!("old follower") }));
    assert!(poll!(old_follower.as_mut()).is_pending());
    cache.invalidate();
    let (release_new, gate_new) = oneshot::channel();
    let mut new = Box::pin(cache.get_or_compute(1, || async {
        gate_new.await.unwrap();
        Ok(snapshot(2))
    }));
    assert!(poll!(new.as_mut()).is_pending());
    assert_eq!(cache.lock().active_flights, 2);
    release_old.send(()).unwrap();
    assert_eq!(ready(poll!(old.as_mut())).unwrap(), snapshot(1));
    assert_eq!(ready(poll!(old_follower.as_mut())).unwrap(), snapshot(1));
    {
        let state = cache.lock();
        assert!(state.entries.is_empty());
        assert_eq!(state.flights.len(), 1);
        assert_eq!(state.active_flights, 1);
    }
    let mut new_follower = Box::pin(cache.get_or_compute(1, || async { panic!("new follower") }));
    assert!(poll!(new_follower.as_mut()).is_pending());
    release_new.send(()).unwrap();
    assert_eq!(ready(poll!(new.as_mut())).unwrap(), snapshot(2));
    assert_eq!(ready(poll!(new_follower.as_mut())).unwrap(), snapshot(2));
    assert_eq!(cache.lock().entries[&1].1, snapshot(2));
    assert_eq!(cache.lock().active_flights, 0);
}

#[tokio::test]
async fn stale_completion_cannot_overwrite_an_already_cached_new_generation() {
    let cache = ProfileCache::default();
    let (release, gate) = oneshot::channel();
    let mut old = Box::pin(cache.get_or_compute(1, || async {
        gate.await.unwrap();
        Ok(snapshot(1))
    }));
    assert!(poll!(old.as_mut()).is_pending());
    cache.invalidate();
    cache
        .get_or_compute(1, || async { Ok(snapshot(2)) })
        .await
        .unwrap();
    release.send(()).unwrap();
    assert_eq!(ready(poll!(old.as_mut())).unwrap(), snapshot(1));
    assert_eq!(cache.lock().entries[&1].1, snapshot(2));
}

#[tokio::test]
async fn owner_cancellation_wakes_followers_to_elect_one_replacement() {
    let cache = ProfileCache::default();
    let mut owner = Box::pin(cache.get_or_compute(1, pending::<StatsResult>));
    assert_send(&owner);
    assert!(poll!(owner.as_mut()).is_pending());
    let calls = Cell::new(0);
    let (release, gate) = oneshot::channel();
    let mut replacement = Box::pin(cache.get_or_compute(1, || async {
        calls.set(calls.get() + 1);
        gate.await.unwrap();
        Ok(snapshot(2))
    }));
    let mut follower = Box::pin(cache.get_or_compute(1, || async {
        calls.set(calls.get() + 1);
        Ok(snapshot(99))
    }));
    let replacement_wakes = Arc::new(WakeCount::default());
    let follower_wakes = Arc::new(WakeCount::default());
    assert!(observed_poll(replacement.as_mut(), &replacement_wakes).is_pending());
    assert!(observed_poll(follower.as_mut(), &follower_wakes).is_pending());
    drop(owner);
    assert!(replacement_wakes.0.load(Ordering::Relaxed) > 0);
    assert!(follower_wakes.0.load(Ordering::Relaxed) > 0);
    assert!(cache.lock().flights.is_empty());
    assert_eq!(cache.lock().active_flights, 0);
    assert!(poll!(replacement.as_mut()).is_pending());
    assert!(poll!(follower.as_mut()).is_pending());
    assert_eq!(calls.get(), 1);
    release.send(()).unwrap();
    assert_eq!(ready(poll!(replacement.as_mut())).unwrap(), snapshot(2));
    assert_eq!(ready(poll!(follower.as_mut())).unwrap(), snapshot(2));
    assert_eq!(cache.lock().active_flights, 0);
}

#[tokio::test]
async fn obsolete_owner_cancellation_does_not_remove_a_new_generation_flight() {
    let cache = ProfileCache::default();
    let mut old = Box::pin(cache.get_or_compute(1, pending::<StatsResult>));
    assert!(poll!(old.as_mut()).is_pending());
    cache.invalidate();
    let mut new = Box::pin(cache.get_or_compute(1, pending::<StatsResult>));
    assert!(poll!(new.as_mut()).is_pending());
    let token = cache.lock().flights[&1].token.clone();
    drop(old);
    assert!(Arc::ptr_eq(&cache.lock().flights[&1].token, &token));
    assert_eq!(cache.lock().active_flights, 1);
    drop(new);
    assert!(cache.lock().flights.is_empty());
    assert_eq!(cache.lock().active_flights, 0);
}

#[tokio::test]
async fn cancelling_a_follower_leaves_the_owner_and_other_followers_running() {
    let cache = ProfileCache::default();
    let (release, gate) = oneshot::channel();
    let mut owner = Box::pin(cache.get_or_compute(1, || async {
        gate.await.unwrap();
        Ok(snapshot(1))
    }));
    assert!(poll!(owner.as_mut()).is_pending());
    let mut cancelled =
        Box::pin(cache.get_or_compute(1, || async { panic!("cancelled follower") }));
    let mut survivor = Box::pin(cache.get_or_compute(1, || async { panic!("surviving follower") }));
    assert!(poll!(cancelled.as_mut()).is_pending());
    assert!(poll!(survivor.as_mut()).is_pending());
    drop(cancelled);
    assert_eq!(cache.lock().active_flights, 1);
    release.send(()).unwrap();
    assert_eq!(ready(poll!(owner.as_mut())).unwrap(), snapshot(1));
    assert_eq!(ready(poll!(survivor.as_mut())).unwrap(), snapshot(1));
    assert_eq!(cache.lock().entries[&1].1, snapshot(1));
}

#[tokio::test]
async fn errors_are_published_to_all_followers_then_removed_for_recovery() {
    let cache = ProfileCache::default();
    let (release, gate) = oneshot::channel();
    let error = ServerFnError::new("controlled failure");
    let mut owner = Box::pin(cache.get_or_compute(1, || async {
        gate.await.unwrap();
        Err(error.clone())
    }));
    assert!(poll!(owner.as_mut()).is_pending());
    let mut followers: Vec<_> = (0..9)
        .map(|_| Box::pin(cache.get_or_compute(1, || async { panic!("error follower") })))
        .collect();
    let wakes: Vec<_> = (0..followers.len())
        .map(|_| Arc::new(WakeCount::default()))
        .collect();
    for (follower, wakes) in followers.iter_mut().zip(&wakes) {
        assert!(observed_poll(follower.as_mut(), wakes).is_pending());
    }
    release.send(()).unwrap();
    assert_eq!(ready(poll!(owner.as_mut())), Err(error.clone()));
    assert!(
        wakes
            .iter()
            .all(|wakes| wakes.0.load(Ordering::Relaxed) > 0)
    );
    for follower in &mut followers {
        assert_eq!(ready(poll!(follower.as_mut())), Err(error.clone()));
    }
    {
        let state = cache.lock();
        assert!(state.entries.is_empty());
        assert!(state.flights.is_empty());
        assert_eq!(state.active_flights, 0);
    }
    assert_eq!(
        cache
            .get_or_compute(1, || async { Ok(snapshot(2)) })
            .await
            .unwrap(),
        snapshot(2)
    );
}

#[tokio::test]
async fn a_full_snapshot_cache_still_publishes_to_existing_waiters() {
    let cache = ProfileCache::default();
    assert_eq!(PROFILE_ENTRIES, 512);
    let (release, gate) = oneshot::channel();
    let mut owner = Box::pin(cache.get_or_compute(-1, || async {
        gate.await.unwrap();
        Ok(snapshot(7))
    }));
    assert!(poll!(owner.as_mut()).is_pending());
    let mut follower =
        Box::pin(cache.get_or_compute(-1, || async { panic!("full-cache follower") }));
    assert!(poll!(follower.as_mut()).is_pending());
    for user in 0..i64::try_from(PROFILE_ENTRIES).unwrap() {
        cache
            .get_or_compute(user, || async { Ok(snapshot(user)) })
            .await
            .unwrap();
    }
    assert_eq!(cache.lock().entries.len(), PROFILE_ENTRIES);
    release.send(()).unwrap();
    assert_eq!(ready(poll!(owner.as_mut())).unwrap(), snapshot(7));
    assert_eq!(ready(poll!(follower.as_mut())).unwrap(), snapshot(7));
    {
        let state = cache.lock();
        assert_eq!(state.entries.len(), PROFILE_ENTRIES);
        assert!(!state.entries.contains_key(&-1));
        assert!(state.flights.is_empty());
        assert_eq!(state.active_flights, 0);
    }
}

#[tokio::test]
async fn active_key_limit_falls_back_without_metadata_even_across_invalidation() {
    let cache = ProfileCache::default();
    assert_eq!(PROFILE_FLIGHTS, 64);
    let mut owners: Vec<_> = (0..i64::try_from(PROFILE_FLIGHTS).unwrap())
        .map(|user| Box::pin(cache.get_or_compute(user, pending::<StatsResult>)))
        .collect();
    for owner in &mut owners {
        assert!(poll!(owner.as_mut()).is_pending());
    }
    let mut follower = Box::pin(cache.get_or_compute(0, || async { panic!("join at capacity") }));
    assert!(poll!(follower.as_mut()).is_pending());
    for _ in 0..3 {
        assert_eq!(
            cache
                .get_or_compute(-1, || async { Ok(snapshot(9)) })
                .await
                .unwrap(),
            snapshot(9)
        );
        assert_eq!(cache.lock().active_flights, PROFILE_FLIGHTS);
        assert!(cache.lock().entries.is_empty());
    }
    assert_eq!(cache.lock().flights.len(), PROFILE_FLIGHTS);
    cache.invalidate();
    assert!(cache.lock().flights.is_empty());
    assert_eq!(cache.lock().active_flights, PROFILE_FLIGHTS);
    // Even the same key now uses the fallback, rather than joining obsolete work.
    assert_eq!(
        cache
            .get_or_compute(0, || async { Ok(snapshot(10)) })
            .await
            .unwrap(),
        snapshot(10)
    );
    assert!(cache.lock().entries.is_empty());
    drop(follower);
    drop(owners);
    assert_eq!(cache.lock().active_flights, 0);
    cache
        .get_or_compute(0, || async { Ok(snapshot(11)) })
        .await
        .unwrap();
    assert_eq!(cache.lock().entries[&0].1, snapshot(11));
}

#[tokio::test]
async fn completion_between_inspection_and_wait_is_retained() {
    let cache = ProfileCache::default();
    let (release, gate) = oneshot::channel();
    let mut owner = Box::pin(cache.get_or_compute(1, || async {
        gate.await.unwrap();
        Ok(snapshot(1))
    }));
    assert!(poll!(owner.as_mut()).is_pending());
    let Lookup::Follow(mut completion) = cache.lookup(1) else {
        panic!("expected follower");
    };
    assert!(completion.borrow_and_update().is_none());
    release.send(()).unwrap();
    assert_eq!(ready(poll!(owner.as_mut())).unwrap(), snapshot(1));
    // Complete and drop the sender before even creating the changed() future.
    assert_eq!(
        ready(poll!(Box::pin(completion.changed()).as_mut())),
        Ok(())
    );
    assert_eq!(
        completion.borrow_and_update().clone(),
        Some(Ok(snapshot(1)))
    );
}

#[tokio::test]
async fn cached_http_assembly_matches_domain_stats_and_rechecks_every_identity_field() {
    let conn = TestConn::new("profile-cache-identity").await;
    conn.execute_batch(
        "INSERT INTO users (id, name, created_at) VALUES (1, 'guest', 123), (2, 'rival', 456);
         INSERT INTO attempts (user_id, day, started_at, finished_at, time_ms) VALUES
            (1, 1, 100, 300, 200), (2, 1, 100, 200, 100), (1, 2, 100, 250, 150);
         INSERT INTO stamps (user_id, stamp, earned_at) VALUES (1, 'first', 300);
         UPDATE users SET stamp = 'first' WHERE id = 1;",
    )
    .await
    .unwrap();
    let cache = ProfileCache::default();
    let expected = profile(&conn, 1).await.unwrap().unwrap();
    assert_eq!(
        profile_with_cache(&cache, &conn, 1).await.unwrap(),
        Some(expected.clone())
    );
    assert_eq!(expected.stats, stats(&conn, 1).await.unwrap());
    assert_eq!(expected.stats.solved, 2);
    assert_eq!(expected.stats.shown, Some(Stamp::First));
    conn.execute_batch(
        "UPDATE users SET name = 'renamed', username = 'claimed', created_at = 789 WHERE id = 1;",
    )
    .await
    .unwrap();
    let claimed = profile_with_cache(&cache, &conn, 1).await.unwrap().unwrap();
    assert_eq!(claimed.name, "renamed");
    assert!(!claimed.guest);
    assert_eq!(claimed.created_at, 789);
    assert_eq!(claimed.stats, expected.stats);
    for column in ["banned_at", "deleted_at"] {
        conn.execute(&format!("UPDATE users SET {column} = 1 WHERE id = 1"), ())
            .await
            .unwrap();
        assert_eq!(profile_with_cache(&cache, &conn, 1).await.unwrap(), None);
        conn.execute(
            &format!("UPDATE users SET {column} = NULL WHERE id = 1"),
            (),
        )
        .await
        .unwrap();
    }
    assert_eq!(profile_with_cache(&cache, &conn, 99).await.unwrap(), None);
    // Neither ordinary domain helper reads this local HTTP cache. Mutations are immediately
    // visible there, and another database with the same numeric ID has its own result.
    conn.execute_batch("DELETE FROM attempts WHERE user_id = 1")
        .await
        .unwrap();
    assert_eq!(stats(&conn, 1).await.unwrap().solved, 0);
    assert_eq!(profile(&conn, 1).await.unwrap().unwrap().stats.solved, 0);
    assert_eq!(
        profile_with_cache(&cache, &conn, 1)
            .await
            .unwrap()
            .unwrap()
            .stats
            .solved,
        2
    );
    let other = TestConn::new("profile-cache-independent-domain").await;
    other
        .execute_batch("INSERT INTO users (id, name, created_at) VALUES (1, 'other', 0)")
        .await
        .unwrap();
    assert_eq!(stats(&other, 1).await.unwrap().solved, 0);
    assert_eq!(profile(&other, 1).await.unwrap().unwrap().name, "other");
    cache.invalidate();
    assert_eq!(
        profile_with_cache(&cache, &conn, 1).await.unwrap(),
        profile(&conn, 1).await.unwrap()
    );
}
