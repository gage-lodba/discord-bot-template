//! Timestamps in the ISO8601 format Discord uses.
//!
//! Discord returns and accepts timestamps as ISO8601 strings, for example
//! `2015-07-17T17:00:00.000000+00:00`. This module stores a timestamp as Unix
//! milliseconds, formats it back to that shape and parses incoming strings.
//!
//! Reference: `docs/reference.md` (ISO8601 Date/Time).

use std::fmt;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An instant, stored as milliseconds since the Unix epoch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    unix_millis: i64,
}

impl Timestamp {
    /// Creates a timestamp from milliseconds since the Unix epoch.
    #[must_use]
    pub const fn from_unix_millis(unix_millis: i64) -> Self {
        Self { unix_millis }
    }

    /// Creates a timestamp from seconds since the Unix epoch.
    #[must_use]
    pub const fn from_unix_seconds(unix_seconds: i64) -> Self {
        Self {
            unix_millis: unix_seconds * 1000,
        }
    }

    /// Returns the current instant.
    ///
    /// A clock before the Unix epoch is treated as the epoch itself: a Discord
    /// timestamp is never negative, and panicking in a log-formatting helper
    /// would be worse than an obviously wrong value.
    #[must_use]
    pub fn now() -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            // Elapsed milliseconds since 1970 stay far below `i64::MAX`; the
            // saturating fallback keeps this helper panic-free.
            .map_or(0, |elapsed| {
                i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
            });

        Self::from_unix_millis(millis)
    }

    /// Returns milliseconds since the Unix epoch.
    #[must_use]
    pub const fn unix_millis(self) -> i64 {
        self.unix_millis
    }

    /// Returns whole seconds since the Unix epoch, rounding towards zero.
    #[must_use]
    pub const fn unix_seconds(self) -> i64 {
        self.unix_millis / 1000
    }

    /// Splits the timestamp into a date and a time of day in UTC.
    fn civil(self) -> (i64, u32, u32, u32, u32, u32, u32) {
        let millis_of_day = self.unix_millis.rem_euclid(86_400_000);
        let days = (self.unix_millis - millis_of_day) / 86_400_000;
        let (year, month, day) = civil_from_days(days);

        // `rem_euclid` constrains the value to one day, so it always fits `u32`.
        let millis_of_day = u32::try_from(millis_of_day).unwrap_or_default();
        let hour = millis_of_day / 3_600_000;
        let minute = (millis_of_day % 3_600_000) / 60_000;
        let second = (millis_of_day % 60_000) / 1000;
        let millisecond = millis_of_day % 1000;

        (year, month, day, hour, minute, second, millisecond)
    }
}

impl fmt::Display for Timestamp {
    /// Formats as ISO8601 in UTC with millisecond precision.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (year, month, day, hour, minute, second, millisecond) = self.civil();

        write!(
            f,
            "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millisecond:03}+00:00"
        )
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TimestampVisitor;

        impl Visitor<'_> for TimestampVisitor {
            type Value = Timestamp;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an ISO8601 timestamp such as 2015-07-17T17:00:00.000000+00:00")
            }

            fn visit_str<E: DeError>(self, value: &str) -> Result<Self::Value, E> {
                parse(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(TimestampVisitor)
    }
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.fraction](Z|+HH:MM|-HH:MM)`.
fn parse(text: &str) -> Result<Timestamp, String> {
    let invalid = || format!("invalid ISO8601 timestamp {text:?}");

    let (date, time) = text.split_once('T').ok_or_else(invalid)?;

    let mut date_parts = date.splitn(3, '-');
    let year: i64 = next_part(&mut date_parts, invalid)?
        .parse()
        .map_err(|_| invalid())?;
    let month: u32 = next_part(&mut date_parts, invalid)?
        .parse()
        .map_err(|_| invalid())?;
    let day: u32 = next_part(&mut date_parts, invalid)?
        .parse()
        .map_err(|_| invalid())?;

    let (time, offset_minutes) = split_offset(time).ok_or_else(invalid)?;

    let mut time_parts = time.splitn(3, ':');
    let hour: u32 = next_part(&mut time_parts, invalid)?
        .parse()
        .map_err(|_| invalid())?;
    let minute: u32 = next_part(&mut time_parts, invalid)?
        .parse()
        .map_err(|_| invalid())?;
    let (second, fraction) = split_fraction(next_part(&mut time_parts, invalid)?);

    let second: u32 = second.parse().map_err(|_| invalid())?;
    let millisecond = fraction_to_millis(fraction).ok_or_else(invalid)?;

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
        || !(-1439..=1439).contains(&offset_minutes)
    {
        return Err(invalid());
    }

    let days = days_from_civil(year, month, day);
    let local_millis = days * 86_400_000
        + i64::from(hour) * 3_600_000
        + i64::from(minute) * 60_000
        + i64::from(second) * 1000
        + i64::from(millisecond);

    Ok(Timestamp::from_unix_millis(
        local_millis - i64::from(offset_minutes) * 60_000,
    ))
}

fn next_part<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    invalid: impl Fn() -> String,
) -> Result<&'a str, String> {
    parts.next().ok_or_else(invalid)
}

/// Splits `HH:MM:SS...` from its timezone suffix, returning the local time and
/// the offset in minutes from UTC.
fn split_offset(time: &str) -> Option<(&str, i32)> {
    if let Some(time) = time.strip_suffix('Z') {
        return Some((time, 0));
    }

    for sign in ['+', '-'] {
        if let Some(index) = time.rfind(sign) {
            let local = &time[..index];
            let offset = &time[index + 1..];
            let (hours, minutes) = offset.split_once(':')?;
            let hours: i32 = hours.parse().ok()?;
            let minutes: i32 = minutes.parse().ok()?;
            let total = hours * 60 + minutes;

            return Some((local, if sign == '-' { -total } else { total }));
        }
    }

    None
}

/// Splits `SS` from an optional `.fraction` suffix.
fn split_fraction(time: &str) -> (&str, Option<&str>) {
    time.split_once('.')
        .map_or((time, None), |(seconds, fraction)| {
            (seconds, Some(fraction))
        })
}

/// Converts a fractional-second string to milliseconds, truncating sub-millisecond
/// digits. `None` means a non-numeric or overly long fraction.
fn fraction_to_millis(fraction: Option<&str>) -> Option<u32> {
    let Some(fraction) = fraction else {
        return Some(0);
    };

    if fraction.is_empty() || fraction.len() > 9 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let mut millis = 0u32;

    // `fraction` holds at most 9 ASCII digits (checked above), most significant
    // first; the first three are the millisecond digits.
    for (digit, place) in fraction.bytes().take(3).zip([100u32, 10, 1]) {
        millis += u32::from(digit - b'0') * place;
    }

    Some(millis)
}

/// Days since the Unix epoch for a civil date.
///
/// Howard Hinnant's `days_from_civil` algorithm, which is exact for proleptic
/// Gregorian dates.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

/// The inverse of [`days_from_civil`].
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };

    (
        if month <= 2 { year + 1 } else { year },
        // `month_index` is in `0..=11` and the computed day in `1..=31`, so
        // both always fit a `u32`.
        u32::try_from(month).unwrap_or_default(),
        u32::try_from(day).unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_unix_epoch() {
        let timestamp = Timestamp::from_unix_millis(0);

        assert_eq!(timestamp.to_string(), "1970-01-01T00:00:00.000+00:00");
    }

    #[test]
    fn formats_a_known_instant() {
        // 2015-01-01T00:00:00Z, the Discord epoch.
        let timestamp = Timestamp::from_unix_millis(1_420_070_400_000);

        assert_eq!(timestamp.to_string(), "2015-01-01T00:00:00.000+00:00");
    }

    #[test]
    fn handles_a_leap_day() {
        let timestamp = Timestamp::from_unix_millis(951_782_400_000);

        assert_eq!(timestamp.to_string(), "2000-02-29T00:00:00.000+00:00");
    }

    #[test]
    fn parses_the_documented_shape_with_microseconds() {
        let timestamp: Timestamp =
            serde_json::from_str("\"2015-07-17T17:00:00.000000+00:00\"").expect("parsed");

        assert_eq!(timestamp.unix_millis(), 1_437_152_400_000);
        assert_eq!(timestamp.to_string(), "2015-07-17T17:00:00.000+00:00");
    }

    #[test]
    fn parses_a_zulu_suffix() {
        let timestamp: Timestamp =
            serde_json::from_str("\"1970-01-01T00:00:01Z\"").expect("parsed");

        assert_eq!(timestamp.unix_millis(), 1000);
    }

    #[test]
    fn parses_a_non_utc_offset() {
        // 1970-01-01T01:00:00+01:00 is the Unix epoch.
        let timestamp: Timestamp =
            serde_json::from_str("\"1970-01-01T01:00:00+01:00\"").expect("parsed");

        assert_eq!(timestamp.unix_millis(), 0);
    }

    #[test]
    fn rejects_garbage() {
        assert!(serde_json::from_str::<Timestamp>("\"not a timestamp\"").is_err());
        assert!(serde_json::from_str::<Timestamp>("\"1970-13-01T00:00:00Z\"").is_err());
        assert!(serde_json::from_str::<Timestamp>("\"1970-01-01T00:00:00+00:00\"").is_ok());
    }

    #[test]
    fn round_trips_through_json() {
        let timestamp = Timestamp::now();
        let json = serde_json::to_string(&timestamp).expect("serialisable");
        let parsed: Timestamp = serde_json::from_str(&json).expect("deserialisable");

        assert_eq!(parsed, timestamp);
    }

    #[test]
    fn unix_seconds_truncates() {
        let timestamp = Timestamp::from_unix_millis(1234);

        assert_eq!(timestamp.unix_seconds(), 1);
    }

    #[test]
    fn from_unix_seconds_converts_to_milliseconds() {
        assert_eq!(
            Timestamp::from_unix_seconds(1_437_152_400).unix_millis(),
            1_437_152_400_000
        );
        assert_eq!(Timestamp::from_unix_seconds(0).unix_millis(), 0);
    }

    #[test]
    fn parses_the_documented_invite_timestamp() {
        // docs/invite.md shows `"created_at": "2025-01-08T12:00:00.000000+00:00"`.
        let timestamp: Timestamp =
            serde_json::from_str("\"2025-01-08T12:00:00.000000+00:00\"").expect("parsed");

        assert_eq!(timestamp.unix_millis(), 1_736_337_600_000);
    }

    #[test]
    fn parses_millisecond_precision_and_truncates_finer_digits() {
        let millis: Timestamp =
            serde_json::from_str("\"2015-07-17T17:00:00.123+00:00\"").expect("parsed");
        assert_eq!(millis.unix_millis(), 1_437_152_400_123);

        // Fractional seconds are positional: `.1` is 100ms, `.05` is 50ms.
        let tenth: Timestamp =
            serde_json::from_str("\"2015-07-17T17:00:00.1+00:00\"").expect("parsed");
        assert_eq!(tenth.unix_millis(), 1_437_152_400_100);

        let hundredths: Timestamp =
            serde_json::from_str("\"2015-07-17T17:00:00.05+00:00\"").expect("parsed");
        assert_eq!(hundredths.unix_millis(), 1_437_152_400_050);

        // Sub-millisecond digits are truncated, not rounded.
        let nanos: Timestamp =
            serde_json::from_str("\"2015-07-17T17:00:00.123456789+00:00\"").expect("parsed");
        assert_eq!(nanos.unix_millis(), 1_437_152_400_123);
    }

    #[test]
    fn parses_a_negative_offset() {
        let timestamp: Timestamp =
            serde_json::from_str("\"1970-01-01T00:00:00-05:00\"").expect("parsed");

        assert_eq!(timestamp.unix_millis(), 18_000_000);
    }

    #[test]
    fn rejects_fractions_and_offsets_it_cannot_represent() {
        // A non-numeric or over-long fraction is not an ISO8601 instant.
        assert!(serde_json::from_str::<Timestamp>("\"2015-07-17T17:00:00.12x+00:00\"").is_err());
        assert!(
            serde_json::from_str::<Timestamp>("\"2015-07-17T17:00:00.1234567890+00:00\"").is_err()
        );
        assert!(serde_json::from_str::<Timestamp>("\"2015-07-17T17:00:00.+00:00\"").is_err());
        // Discord timestamps always carry a timezone offset.
        assert!(serde_json::from_str::<Timestamp>("\"2015-07-17T17:00:00\"").is_err());
        assert!(serde_json::from_str::<Timestamp>("\"2015-07-17T17:00:00+25:00\"").is_err());
    }
}
