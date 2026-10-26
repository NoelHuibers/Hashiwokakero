//! Daily scoring policy. Raw server milliseconds remain the audit record.

/// Shared hundredth-second places start on 2026-10-26, including that day's existing results.
/// Keep in step with the generated ranking columns in the database migration.
pub const SHARED_PLACES_DAY: i64 = 20_752;

/// New receipt timing starts with a complete UTC day; today's stored times stay intact.
pub const RECEIPT_TIMING_DAY: i64 = SHARED_PLACES_DAY + 1;

/// Nearest hundredth, represented in milliseconds. Division first avoids overflow.
pub fn rounded_time(ms: i64) -> i64 {
    let ms = ms.max(0);
    (ms / 10 + i64::from(ms % 10 >= 5)).saturating_mul(10)
}

/// The time that decides a daily's place, without changing the stored measurement.
pub fn score_time(day: i64, ms: i64) -> i64 {
    if day >= SHARED_PLACES_DAY {
        rounded_time(ms)
    } else {
        ms
    }
}

/// Earlier days retain their finish tie-breaker. Equal new scores share a place.
pub fn ranking_key(day: i64, ms: i64, finished_at: i64) -> (i64, i64) {
    (
        score_time(day, ms),
        if day < SHARED_PLACES_DAY {
            finished_at
        } else {
            0
        },
    )
}

/// A ranked time always displays all the precision that decides its new-policy place.
pub fn format_score(ms: i64) -> String {
    let cs = rounded_time(ms) / 10;
    let seconds = cs / 100;
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{:02}", cs % 100)
    } else {
        format!("{m}:{s:02}.{:02}", cs % 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_hundredths_are_the_entire_ranking_key() {
        assert_eq!(
            ranking_key(SHARED_PLACES_DAY, 7826, 1),
            ranking_key(SHARED_PLACES_DAY, 7831, 2)
        );
        assert_eq!(format_score(7826), "0:07.83");
        assert_eq!(format_score(7831), "0:07.83");
        assert_eq!(format_score(7824), "0:07.82");
        assert_eq!(format_score(7835), "0:07.84");
        assert_eq!(format_score(59_995), "1:00.00");
        assert_eq!(format_score(3_599_995), "1:00:00.00");
        assert_eq!(format_score(-1), "0:00.00");
        assert!(
            ranking_key(SHARED_PLACES_DAY - 1, 7826, 1)
                < ranking_key(SHARED_PLACES_DAY - 1, 7831, 2)
        );
        assert!(
            ranking_key(SHARED_PLACES_DAY - 1, 7830, 1)
                < ranking_key(SHARED_PLACES_DAY - 1, 7830, 2)
        );
        assert!(rounded_time(i64::MAX) > 0);
    }
}
