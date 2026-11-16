//! When the streak's last call comes: about three hours before the daily changes (midnight UTC,
//! `model::calendar`), in the evening of the player's own day if that would be at night.
//!
//! The daily changes at a different hour of each time zone's day: 01:00 in Berlin in winter and
//! 02:00 in summer, 19:00 in New York, 09:00 in Tokyo. Three hours before is fine in most places
//! (22:00 in Berlin's winter, 16:00 in New York, 13:00 in Los Angeles), but in some it is at
//! night: 23:00 in Berlin's summer, 06:00 in Tokyo. Nobody gets woken up for a puzzle, so a last
//! call in the quiet hours (23:00 to 08:00 local time) comes at 21:00 of the evening before the
//! change instead (Tokyo: 21:00, twelve hours before; Berlin in summer: 21:00, five hours before).
//!
//! The time zone is the IANA name the player's browser or app reported (`Europe/Berlin`), with
//! its daylight saving rules from the time zone database bundled with `jiff`. Without one (or an
//! unknown one) it's UTC: 21:00, three hours before midnight.

use jiff::Timestamp;
use jiff::civil::Time;
use jiff::tz::TimeZone;

use crate::model::MS_PER_DAY;

/// How long before the change the last call comes.
pub const LEAD_MS: i64 = 3 * 3_600_000;
/// The quiet hours, local time: from 23:00 to 08:00 nobody gets a last call.
const QUIET_FROM: i8 = 23;
const QUIET_UNTIL: i8 = 8;
/// Where a last call in the quiet hours goes: 21:00 of the evening before the change.
const EVENING: i8 = 21;

/// The longest IANA time zone names are about 30 characters.
const MAX_ZONE_LEN: usize = 64;

/// The time zone of an IANA name, `None` for one the database doesn't know.
pub fn zone(name: &str) -> Option<TimeZone> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_ZONE_LEN {
        return None;
    }
    TimeZone::get(name).ok()
}

/// The name as stored, if it is a time zone: what a client reports goes through here.
pub fn valid_zone(name: Option<&str>) -> Option<String> {
    name.filter(|n| zone(n).is_some())
        .map(|n| n.trim().to_string())
}

fn local_hour(ms: i64, tz: &TimeZone) -> Option<i8> {
    Some(
        Timestamp::from_millisecond(ms)
            .ok()?
            .to_zoned_datetime(tz.clone())
            .hour(),
    )
}

/// Whether a moment is in the quiet hours of `tz`.
pub fn quiet(ms: i64, tz: &TimeZone) -> bool {
    local_hour(ms, tz).is_some_and(|h| !(QUIET_UNTIL..QUIET_FROM).contains(&h))
}

/// When the last call for the daily of `day` is due, in ms since the epoch.
///
/// For a player in the time zone `zone_name`: [`LEAD_MS`] before the change, or 21:00 of the
/// evening before the change if that is in the quiet hours.
pub fn last_call_at(day: i64, zone_name: Option<&str>) -> i64 {
    let tz = zone_name.and_then(zone).unwrap_or(TimeZone::UTC);
    let change = (day + 1) * MS_PER_DAY;
    let lead = change - LEAD_MS;
    if !quiet(lead, &tz) {
        return lead;
    }
    evening_before(change, &tz)
        // Never before the daily came out
        .map_or(lead, |evening| evening.max(day * MS_PER_DAY))
}

/// 21:00 local time on the day before the local date of `change`.
fn evening_before(change: i64, tz: &TimeZone) -> Option<i64> {
    let local = Timestamp::from_millisecond(change)
        .ok()?
        .to_zoned_datetime(tz.clone());
    let date = local.date().yesterday().ok()?;
    // A gap (a clock jumping forward over 21:00) takes the later side, see `to_zoned_datetime`
    let evening = date
        .to_datetime(Time::constant(EVENING, 0, 0, 0))
        .to_zoned_datetime(tz.clone())
        .ok()?;
    Some(evening.timestamp().as_millisecond())
}

/// For the tests and the docs: the local time of a moment as `HH:MM`.
#[cfg(test)]
pub fn local_time(ms: i64, zone_name: &str) -> String {
    let local = Timestamp::from_millisecond(ms)
        .unwrap()
        .to_zoned_datetime(zone(zone_name).unwrap());
    format!("{:02}:{:02}", local.hour(), local.minute())
}

/// How long before the change a last call comes.
#[cfg(test)]
pub fn hours_before_change(day: i64, zone_name: &str) -> f64 {
    let at = last_call_at(day, Some(zone_name));
    let before = jiff::SignedDuration::from_millis((day + 1) * MS_PER_DAY - at);
    before.as_secs_f64() / 3600.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::days_from_civil;

    /// A winter day (no daylight saving time in the north) and a summer day.
    fn winter() -> i64 {
        days_from_civil(2027, 1, 14)
    }
    fn summer() -> i64 {
        days_from_civil(2027, 7, 14)
    }

    #[test]
    fn three_hours_before_the_change_where_that_is_awake() {
        for (zone, day, local) in [
            ("America/New_York", winter(), "16:00"),
            ("America/New_York", summer(), "17:00"),
            ("America/Los_Angeles", winter(), "13:00"),
            ("America/Los_Angeles", summer(), "14:00"),
            ("Europe/Berlin", winter(), "22:00"),
            ("Europe/London", summer(), "22:00"),
            ("Europe/London", winter(), "21:00"),
            ("UTC", winter(), "21:00"),
            ("Pacific/Honolulu", summer(), "11:00"),
            // UTC+14: the change is at 14:00 of the next local day
            ("Pacific/Kiritimati", winter(), "11:00"),
            // Sydney's summer is in January, UTC+11: the change is at 11:00, three hours before
            // is 08:00, just after the quiet hours
            ("Australia/Sydney", winter(), "08:00"),
        ] {
            let at = last_call_at(day, Some(zone));
            assert_eq!(at, (day + 1) * MS_PER_DAY - LEAD_MS, "{zone}");
            assert_eq!(local_time(at, zone), local, "{zone}");
        }
    }

    #[test]
    fn at_night_it_comes_the_evening_before() {
        for (zone, day, hours) in [
            // The change at 09:00, three hours before is 06:00: 21:00 the evening before
            ("Asia/Tokyo", winter(), 12.0),
            ("Asia/Tokyo", summer(), 12.0),
            // Berlin in summer: the change at 02:00, 23:00 is quiet already
            ("Europe/Berlin", summer(), 5.0),
            // Brisbane has no daylight saving time: the change at 10:00, 07:00 is quiet
            ("Australia/Brisbane", winter(), 13.0),
            // Sydney's winter, UTC+10 like Brisbane
            ("Australia/Sydney", summer(), 13.0),
            // India, UTC+5:30: the change at 05:30
            ("Asia/Kolkata", winter(), 8.5),
            ("Europe/Moscow", winter(), 6.0),
        ] {
            let at = last_call_at(day, Some(zone));
            assert_eq!(local_time(at, zone), "21:00", "{zone}");
            assert!(
                (hours_before_change(day, zone) - hours).abs() < 1e-9,
                "{zone}: {}",
                hours_before_change(day, zone)
            );
            // Always while the daily of the day is out
            assert!(
                at >= day * MS_PER_DAY && at < (day + 1) * MS_PER_DAY,
                "{zone}"
            );
        }
    }

    /// Berlin is in daylight saving time from the last Sunday of March to the last Sunday of
    /// October: three hours before is 22:00 in winter (fine) and 23:00 in summer (quiet).
    #[test]
    fn daylight_saving_time_moves_it() {
        // 2027: summer time from 28 March to 31 October
        let before = days_from_civil(2027, 3, 27);
        let after = days_from_civil(2027, 3, 28);
        assert_eq!(
            local_time(last_call_at(before, Some("Europe/Berlin")), "Europe/Berlin"),
            "22:00"
        );
        assert_eq!(
            local_time(last_call_at(after, Some("Europe/Berlin")), "Europe/Berlin"),
            "21:00"
        );
        assert!((hours_before_change(after, "Europe/Berlin") - 5.0).abs() < 1e-9);
        let back = days_from_civil(2027, 10, 31);
        assert_eq!(
            local_time(last_call_at(back, Some("Europe/Berlin")), "Europe/Berlin"),
            "22:00"
        );
        // New York changes on other days, and stays awake either way
        let spring = days_from_civil(2027, 3, 14);
        assert_eq!(
            local_time(
                last_call_at(spring, Some("America/New_York")),
                "America/New_York"
            ),
            "17:00"
        );
    }

    #[test]
    fn without_a_time_zone_it_is_utc() {
        let day = winter();
        let utc = (day + 1) * MS_PER_DAY - LEAD_MS;
        assert_eq!(last_call_at(day, None), utc);
        assert_eq!(last_call_at(day, Some("Mars/Olympus_Mons")), utc);
        assert_eq!(last_call_at(day, Some("")), utc);
    }

    #[test]
    fn only_real_zones_are_kept() {
        assert_eq!(
            valid_zone(Some(" Europe/Berlin ")).as_deref(),
            Some("Europe/Berlin")
        );
        assert_eq!(
            valid_zone(Some("Asia/Tokyo")).as_deref(),
            Some("Asia/Tokyo")
        );
        assert_eq!(valid_zone(Some("Nowhere/Land")), None);
        assert_eq!(valid_zone(Some(&"A".repeat(200))), None);
        assert_eq!(valid_zone(None), None);
    }

    #[test]
    fn quiet_hours_are_23_to_8() {
        let tz = zone("UTC").unwrap();
        let day = winter() * MS_PER_DAY;
        let at = |h: i64, m: i64| day + h * 3_600_000 + m * 60_000;
        assert!(quiet(at(23, 0), &tz));
        assert!(quiet(at(0, 0), &tz));
        assert!(quiet(at(7, 59), &tz));
        assert!(!quiet(at(8, 0), &tz));
        assert!(!quiet(at(22, 59), &tz));
    }
}
