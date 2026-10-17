//! Minimal RFC 3339 parsing for `creationDate` (no extra dependency needed).

/// A date and time with its offset from UTC, as `creationDate` holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Timestamp {
    pub local: LocalDateTime,
    /// Offset from UTC in minutes.
    pub offset_minutes: i16,
}

impl Timestamp {
    /// The offset from UTC as PDF and XMP dates end: `Z`, or the sign,
    /// hours, `sep` and minutes (`+02'00` with `'`, `+02:00` with `:`).
    pub fn zone(&self, sep: char) -> String {
        let off = self.offset_minutes;
        if off == 0 {
            return "Z".to_string();
        }
        let sign = if off < 0 { '-' } else { '+' };
        let a = off.unsigned_abs();
        format!("{sign}{:02}{sep}{:02}", a / 60, a % 60)
    }
}

/// A date and time without an offset, as a form field value holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct LocalDateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)`. Fractional seconds are
/// accepted and dropped (PDF dates have one-second resolution).
pub(crate) fn parse_rfc3339(s: &str) -> Option<Timestamp> {
    let date = s.get(..10)?;
    let rest = s.get(10..)?.strip_prefix(['T', 't', ' '])?;
    let (clock, mut zone) = (rest.get(..8)?, rest.get(8..)?);
    let mut local = parse_date(date)?;
    let (hour, minute, second) = parse_clock(clock)?;
    (local.hour, local.minute) = (hour, minute);
    // Leap second: PDF dates cannot express it.
    local.second = second.min(59);
    if let Some(frac) = zone.strip_prefix('.') {
        let n = frac.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            return None;
        }
        zone = &frac[n..];
    }
    let offset_minutes = match zone.as_bytes().first()? {
        b'Z' | b'z' if zone.len() == 1 => 0,
        sign @ (b'+' | b'-') if zone.len() == 6 && zone.as_bytes()[3] == b':' => {
            let h = digits(&zone[1..3], 2)?;
            let m = digits(&zone[4..6], 2)?;
            if h > 23 || m > 59 {
                return None;
            }
            let v = (h * 60 + m) as i16;
            if *sign == b'-' {
                -v
            } else {
                v
            }
        }
        _ => return None,
    };
    Some(Timestamp {
        local,
        offset_minutes,
    })
}

/// Days in `month` of `year` (Gregorian); 0 for no month.
fn days_in_month(year: u32, month: u32) -> u32 {
    let leap = (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    }
}

/// Whether `s` is exactly `len` ASCII digits.
pub(crate) fn is_digits(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_digit())
}

/// The digits of `s` as a number, when `s` is exactly `len` (at most 9)
/// ASCII digits.
fn digits(s: &str, len: usize) -> Option<u32> {
    is_digits(s, len).then(|| s.bytes().fold(0, |a, d| a * 10 + u32::from(d - b'0')))
}

/// Parses `YYYY-MM-DD`, a real day; the time is left at zero.
fn parse_date(s: &str) -> Option<LocalDateTime> {
    let mut parts = s.split('-');
    let year = digits(parts.next()?, 4)?;
    let month = digits(parts.next()?, 2)?;
    let day = digits(parts.next()?, 2)?;
    if parts.next().is_some() || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some(LocalDateTime {
        year: year as u16,
        month: month as u8,
        day: day as u8,
        ..LocalDateTime::default()
    })
}

/// Parses `HH:MM[:SS]`, with a second up to 60 (a leap second).
fn parse_clock(s: &str) -> Option<(u8, u8, u8)> {
    let mut parts = s.split(':');
    let hour = digits(parts.next()?, 2)?;
    let minute = digits(parts.next()?, 2)?;
    let second = match parts.next() {
        Some(p) => digits(p, 2)?,
        None => 0,
    };
    if parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some((hour as u8, minute as u8, second as u8))
}

/// Parses `HH:MM[:SS]` into the time of `t`.
fn parse_time_into(s: &str, t: &mut LocalDateTime) -> Option<()> {
    let (hour, minute, second) = parse_clock(s).filter(|c| c.2 < 60)?;
    (t.hour, t.minute, t.second) = (hour, minute, second);
    Some(())
}

/// Parses `YYYY-MM-DD[THH:MM[:SS]]`, a date in years 1 to 9999 and an
/// optional time.
pub(crate) fn parse_local(s: &str) -> Option<LocalDateTime> {
    let (date, time) = match s.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };
    let mut t = parse_date(date).filter(|t| t.year > 0)?;
    if let Some(time) = time {
        parse_time_into(time, &mut t)?;
    }
    Some(t)
}

/// Parses `HH:MM[:SS]`, a time of day; the date is left at zero.
pub(crate) fn parse_local_time(s: &str) -> Option<LocalDateTime> {
    let mut t = LocalDateTime::default();
    parse_time_into(s, &mut t)?;
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_local_dates_and_times() {
        let t = parse_local("2024-02-29T13:05").unwrap();
        assert_eq!(
            (t.year, t.month, t.day, t.hour, t.minute, t.second),
            (2024, 2, 29, 13, 5, 0)
        );
        assert_eq!(parse_local("2026-10-08").unwrap().hour, 0);
        assert_eq!(parse_local_time("23:59:59").unwrap().second, 59);
        for s in [
            "",
            "2023-02-29",
            "0000-01-01",
            "2026-1-01",
            "2026-10-08T",
            "2026-10-08T24:00",
            "2026-10-08T10:60",
            "2026-10-08T10:00:00Z",
            "2026-10-08 10:00",
            "+2026-10-08",
        ] {
            assert!(parse_local(s).is_none(), "{s}");
        }
        for s in ["", "1:00", "10:00:00:00", "10-00", "10:0a"] {
            assert!(parse_local_time(s).is_none(), "{s}");
        }
    }

    #[test]
    fn parses_valid() {
        let t = parse_rfc3339("2026-10-08T00:00:00Z").unwrap();
        let d = t.local;
        assert_eq!((d.year, d.month, d.day, t.offset_minutes), (2026, 10, 8, 0));
        let t = parse_rfc3339("2024-02-29T13:45:30.123+03:00").unwrap();
        let d = t.local;
        assert_eq!(
            (d.hour, d.minute, d.second, t.offset_minutes),
            (13, 45, 30, 180)
        );
        // A leap second is the second before it; a space or `t` may
        // separate the date and time.
        assert_eq!(
            parse_rfc3339("2016-12-31 23:59:60z").unwrap().local.second,
            59
        );
        assert!(parse_rfc3339("0000-01-01t00:00:00Z").is_some());
        assert_eq!(
            parse_rfc3339("2026-01-01T00:00:00-05:30")
                .unwrap()
                .offset_minutes,
            -330
        );
    }

    #[test]
    fn rejects_invalid() {
        for s in [
            "",
            "2026-10-08",
            "2026-10-08T00:00:00",
            "2023-02-29T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-10-08T24:00:00Z",
            "2026-10-08T00:00:00.Z",
            "2026-10-08T00:00:00+0300",
            "2026-10-08T00:00:00Zjunk",
            "2026-10-08T00:00:61Z",
            "2026-10-08T00:00:00.123",
            "2026-10-08T00:00:00+24:00",
            "2026-10-08T00:00:00+03:60",
            "2026-10-08T00:00:00+03:00junk",
            "2026-10-08X00:00:00Z",
            "2026-10-08T00:00Z",
            "2026-10-08T00:00:00é",
            "2026-1é-08T00:00:00Z",
        ] {
            assert!(parse_rfc3339(s).is_none(), "{s}");
        }
    }
}
