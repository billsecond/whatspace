//! Dates as the owner reads them: in their own time zone, year first.

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// The system's time zone, or UTC when it can't be found.
pub fn local_zone() -> TimeZone {
    TimeZone::try_system().unwrap_or(TimeZone::UTC)
}

/// Now, in seconds since 1970.
pub fn now() -> i64 {
    Timestamp::now().as_second()
}

/// `secs` (since 1970) as "2026-11-04 15:03" in `tz`; empty when unknown (0) or out of range.
pub fn format_time(secs: i64, tz: &TimeZone) -> String {
    if secs == 0 {
        return String::new();
    }
    match Timestamp::from_second(secs) {
        Ok(ts) => ts.to_zoned(tz.clone()).strftime("%Y-%m-%d %H:%M").to_string(),
        Err(_) => String::new(),
    }
}

/// `secs` as an RFC 3339 UTC time ("2026-11-04T15:03:00Z") for exports; empty when unknown.
pub fn format_utc(secs: i64) -> String {
    if secs == 0 {
        return String::new();
    }
    Timestamp::from_second(secs).map(|ts| ts.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_dates() {
        let utc = TimeZone::UTC;
        assert_eq!(format_time(0, &utc), "");
        assert_eq!(format_time(1_762_268_580, &utc), "2025-11-04 15:03");
        assert_eq!(format_utc(1_762_268_580), "2025-11-04T15:03:00Z");
        assert_eq!(format_time(i64::MAX, &utc), "", "out of range is blank, not a crash");
        assert_eq!(format_time(-86_400, &utc), "1969-12-31 00:00");
        assert!(now() > 1_700_000_000);
        let _ = local_zone();
    }
}
