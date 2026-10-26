//! Diagnostic move timing. It never supplies a ranked duration to the server.

#[derive(Clone, Debug)]
pub(super) struct MoveClock {
    pub(super) attempt: String,
    elapsed: u32,
    anchor: f64,
}

impl MoveClock {
    pub(super) fn new(attempt: String, elapsed: u32, anchor: f64) -> Self {
        Self {
            attempt,
            elapsed,
            anchor,
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped monotonic milliseconds"
    )]
    pub(super) fn at(&self, now: f64) -> u32 {
        self.elapsed
            .saturating_add((now - self.anchor).max(0.0).min(f64::from(u32::MAX)) as u32)
    }

    /// Sleeping clocks do not tick on every OS. A server observation can advance
    /// the anchor, but cannot change any previously saved event.
    pub(super) fn observe(&mut self, elapsed: u32, now: f64) {
        if elapsed > self.at(now).saturating_add(1000) {
            self.elapsed = elapsed;
            self.anchor = now;
        }
    }
}

#[cfg(feature = "browser")]
pub(super) fn monotonic_now() -> f64 {
    leptos::prelude::window()
        .performance()
        .expect("browser performance clock")
        .now()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_within_one_display_tick_have_distinct_times() {
        let mut clock = MoveClock::new("attempt".into(), 7000, 100.0);
        assert_eq!(clock.attempt, "attempt");
        assert_eq!((clock.at(126.0), clock.at(131.0)), (7026, 7031));
        clock.observe(6900, 132.0);
        assert_eq!(clock.at(135.0), 7035);
        clock.observe(20_000, 140.0);
        assert_eq!(clock.at(145.0), 20_005);
    }
}
