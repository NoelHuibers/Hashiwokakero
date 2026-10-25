//! Coalescing of identical concurrent work: requests that need the result of the same key at the
//! same time share one computation instead of each running it.
//!
//! The design of the public profiles' statistics (`daily::stats`, see
//! docs/performance/profile-cache-coalescing.md), without their cache: only work in flight is
//! shared, a finished result is not kept, and so neither are errors. Later requests compute
//! again (or find what the first ones stored, as the daily finds its puzzle in the database).
//!
//! - The first request of a key owns the computation, the others follow it. No task is spawned:
//!   the work runs in the owner's request. If the owner is cancelled, its followers wake up and
//!   one of them becomes the new owner with its own computation. A cancelled follower doesn't
//!   cancel anything.
//! - At most [`Flights::new`]'s `capacity` keys are in flight. A request for another key then
//!   computes on its own, as without coalescing.
//! - The lock is never held across an await.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Mutex, MutexGuard};

use tokio::sync::watch;

/// The computations in flight, by key.
#[derive(Debug)]
pub struct Flights<K, T> {
    capacity: usize,
    flights: Mutex<HashMap<K, watch::Sender<Option<T>>>>,
}

enum Lookup<'a, K: Eq + Hash, T> {
    Follow(watch::Receiver<Option<T>>),
    Owner(Owner<'a, K, T>),
    Alone,
}

/// The request that computes a key. Dropping it unfinished (cancelled) ends the flight:
/// followers see the channel closed without a result and elect a new owner.
struct Owner<'a, K: Eq + Hash, T> {
    flights: &'a Flights<K, T>,
    /// `None` once finished.
    key: Option<K>,
}

impl<K: Eq + Hash, T> Owner<'_, K, T> {
    /// Ends the flight and hands `result` to its followers.
    fn finish(mut self, result: T) {
        let flight = self
            .key
            .take()
            .and_then(|key| self.flights.lock().remove(&key));
        if let Some(flight) = flight {
            // Kept by the channel for followers that haven't looked yet
            flight.send_replace(Some(result));
        }
    }
}

impl<K: Eq + Hash, T> Drop for Owner<'_, K, T> {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            // The sender drops after the lock is released
            let flight = self.flights.lock().remove(&key);
            drop(flight);
        }
    }
}

impl<K: Eq + Hash, T> Flights<K, T> {
    fn lock(&self) -> MutexGuard<'_, HashMap<K, watch::Sender<Option<T>>>> {
        self.flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<K: Clone + Eq + Hash + Send, T: Clone + Send + Sync> Flights<K, T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            flights: Mutex::default(),
        }
    }

    fn lookup(&self, key: &K) -> Lookup<'_, K, T> {
        let mut flights = self.lock();
        if let Some(flight) = flights.get(key) {
            return Lookup::Follow(flight.subscribe());
        }
        if flights.len() >= self.capacity {
            return Lookup::Alone;
        }
        flights.insert(key.clone(), watch::channel(None).0);
        drop(flights);
        Lookup::Owner(Owner {
            flights: self,
            key: Some(key.clone()),
        })
    }

    /// The result of `compute` for `key`, computed once for all concurrent callers.
    pub async fn run<F, Fut>(&self, key: K, compute: F) -> T
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = T> + Send,
    {
        loop {
            match self.lookup(&key) {
                Lookup::Alone => return compute().await,
                Lookup::Owner(owner) => {
                    let result = compute().await;
                    owner.finish(result.clone());
                    return result;
                }
                Lookup::Follow(mut completion) => loop {
                    let result = completion.borrow_and_update().clone();
                    if let Some(result) = result {
                        return result;
                    }
                    // The channel remembers a result sent between looking and waiting. Closed
                    // without one: the owner was cancelled, try to become the owner.
                    if completion.changed().await.is_err() {
                        break;
                    }
                },
            }
        }
    }

    /// How many requests follow the computation of `key`.
    #[cfg(test)]
    pub fn followers(&self, key: &K) -> usize {
        self.lock()
            .get(key)
            .map_or(0, watch::Sender::receiver_count)
    }

    /// How many keys are in flight.
    #[cfg(test)]
    pub fn in_flight(&self) -> usize {
        self.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::poll;
    use tokio::sync::oneshot;

    use super::*;

    type Outcome = Result<u32, String>;

    /// How often something was computed.
    #[derive(Default)]
    struct Calls(AtomicUsize);

    impl Calls {
        fn add(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }

        fn get(&self) -> usize {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn ready<T>(state: std::task::Poll<T>) -> T {
        match state {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("expected a result"),
        }
    }

    /// Ten requests at once: one computation, its result for all of them, and nothing stays
    /// behind.
    #[tokio::test]
    async fn concurrent_requests_share_one_computation() {
        let flights = Flights::<u8, Outcome>::new(4);
        let calls = Calls::default();
        let (release, gate) = oneshot::channel();
        let mut owner = Box::pin(flights.run(1, || async {
            calls.add();
            gate.await.unwrap();
            Ok(7)
        }));
        assert!(poll!(owner.as_mut()).is_pending());
        let mut followers: Vec<_> = (0..9)
            .map(|_| {
                Box::pin(flights.run(1, || async {
                    calls.add();
                    Ok(99)
                }))
            })
            .collect();
        for follower in &mut followers {
            assert!(poll!(follower.as_mut()).is_pending());
        }
        assert_eq!(flights.followers(&1), 9);
        release.send(()).unwrap();
        assert_eq!(ready(poll!(owner.as_mut())), Ok(7));
        for follower in &mut followers {
            assert_eq!(ready(poll!(follower.as_mut())), Ok(7));
        }
        assert_eq!(calls.get(), 1);
        assert_eq!(flights.in_flight(), 0);
        // Nothing is kept: the next request computes again
        assert_eq!(flights.run(1, || async { Ok(8) }).await, Ok(8));
    }

    /// Another key doesn't wait for a computation in flight.
    #[tokio::test]
    async fn other_keys_compute_on_their_own() {
        let flights = Flights::<u8, Outcome>::new(4);
        let (release, gate) = oneshot::channel::<()>();
        let mut slow = Box::pin(flights.run(1, || async {
            gate.await.unwrap();
            Ok(1)
        }));
        assert!(poll!(slow.as_mut()).is_pending());
        assert_eq!(flights.run(2, || async { Ok(2) }).await, Ok(2));
        assert_eq!(flights.in_flight(), 1);
        release.send(()).unwrap();
        assert_eq!(slow.await, Ok(1));
    }

    /// A cancelled owner hands the work to a follower, which computes it itself. A cancelled
    /// follower cancels nothing.
    #[tokio::test]
    async fn a_follower_takes_over_from_a_cancelled_owner() {
        let flights = Flights::<u8, Outcome>::new(4);
        let calls = Calls::default();
        let mut owner = Box::pin(flights.run(1, || async {
            calls.add();
            std::future::pending().await
        }));
        assert!(poll!(owner.as_mut()).is_pending());
        let (release, gate) = oneshot::channel();
        let mut follower = Box::pin(flights.run(1, || async {
            calls.add();
            gate.await.unwrap();
            Ok(2)
        }));
        let mut cancelled = Box::pin(flights.run(1, || async { Ok(3) }));
        let mut late = Box::pin(flights.run(1, || async { Ok(4) }));
        assert!(poll!(follower.as_mut()).is_pending());
        assert!(poll!(cancelled.as_mut()).is_pending());
        assert!(poll!(late.as_mut()).is_pending());
        drop(cancelled);
        assert_eq!(flights.followers(&1), 2);
        drop(owner);
        // The follower polled first becomes the owner and computes, the other one follows it
        assert!(poll!(follower.as_mut()).is_pending());
        assert!(poll!(late.as_mut()).is_pending());
        assert_eq!(calls.get(), 2);
        assert_eq!(flights.followers(&1), 1);
        release.send(()).unwrap();
        assert_eq!(ready(poll!(follower.as_mut())), Ok(2));
        assert_eq!(ready(poll!(late.as_mut())), Ok(2));
        assert_eq!(flights.in_flight(), 0);
    }

    /// An error reaches every request that waited for it, but isn't kept: the next request
    /// tries again.
    #[tokio::test]
    async fn errors_reach_every_waiter_and_the_next_request_retries() {
        let flights = Flights::<u8, Outcome>::new(4);
        let (release, gate) = oneshot::channel();
        let mut owner = Box::pin(flights.run(1, || async {
            gate.await.unwrap();
            Err("busy".to_string())
        }));
        assert!(poll!(owner.as_mut()).is_pending());
        let mut followers: Vec<_> = (0..3)
            .map(|_| Box::pin(flights.run(1, || async { Ok(9) })))
            .collect();
        for follower in &mut followers {
            assert!(poll!(follower.as_mut()).is_pending());
        }
        release.send(()).unwrap();
        assert_eq!(ready(poll!(owner.as_mut())), Err("busy".into()));
        for follower in &mut followers {
            assert_eq!(ready(poll!(follower.as_mut())), Err("busy".into()));
        }
        assert_eq!(flights.run(1, || async { Ok(5) }).await, Ok(5));
    }

    /// At capacity a new key computes alone, without a flight, and the keys in flight can
    /// still be joined.
    #[tokio::test]
    async fn at_capacity_new_keys_compute_alone() {
        let flights = Flights::<u8, Outcome>::new(1);
        let (release, gate) = oneshot::channel::<()>();
        let mut first = Box::pin(flights.run(1, || async {
            gate.await.unwrap();
            Ok(1)
        }));
        assert!(poll!(first.as_mut()).is_pending());
        let calls = Calls::default();
        for _ in 0..2 {
            let other = flights.run(2, || async {
                calls.add();
                Ok(2)
            });
            assert_eq!(other.await, Ok(2));
        }
        assert_eq!(calls.get(), 2);
        let mut joined = Box::pin(flights.run(1, || async { Ok(9) }));
        assert!(poll!(joined.as_mut()).is_pending());
        assert_eq!(flights.in_flight(), 1);
        release.send(()).unwrap();
        assert_eq!(ready(poll!(first.as_mut())), Ok(1));
        assert_eq!(ready(poll!(joined.as_mut())), Ok(1));
    }
}
