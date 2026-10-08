//! Minimal RFC 3339 parsing for `creationDate` (no extra dependency needed).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Timestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    /// Offset from UTC in minutes.
    pub offset_minutes: i16,
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)`. Fractional seconds are
/// accepted and dropped (PDF dates have one-second resolution).
pub(crate) fn parse_rfc3339(s: &str) -> Option<Timestamp> {
    let b = s.as_bytes();
    if b.len() < 20 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = b.get(r)?;
        if !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        Some(part.iter().fold(0u32, |a, d| a * 10 + u32::from(d - b'0')))
    };
    let sep = |i: usize, chars: &[u8]| b.get(i).is_some_and(|c| chars.contains(c));
    if !(sep(4, b"-") && sep(7, b"-") && sep(10, b"Tt ") && sep(13, b":") && sep(16, b":")) {
        return None;
    }
    let year = num(0..4)?;
    let month = num(5..7)?;
    let day = num(8..10)?;
    let hour = num(11..13)?;
    let minute = num(14..16)?;
    let second = num(17..19)?;

    let mut i = 19;
    if b[i] == b'.' {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
    }
    let offset_minutes: i32 = match b.get(i)? {
        b'Z' | b'z' if i + 1 == b.len() => 0,
        sign @ (b'+' | b'-') if i + 6 == b.len() && b[i + 3] == b':' => {
            let h = num(i + 1..i + 3)?;
            let m = num(i + 4..i + 6)?;
            if h > 23 || m > 59 {
                return None;
            }
            let v = (h * 60 + m) as i32;
            if *sign == b'-' {
                -v
            } else {
                v
            }
        }
        _ => return None,
    };

    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if day == 0 || day > days_in_month || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some(Timestamp {
        year: year as u16,
        month: month as u8,
        day: day as u8,
        hour: hour as u8,
        minute: minute as u8,
        // Leap second: PDF dates cannot express it.
        second: second.min(59) as u8,
        offset_minutes: offset_minutes as i16,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid() {
        let t = parse_rfc3339("2026-10-08T00:00:00Z").unwrap();
        assert_eq!((t.year, t.month, t.day, t.offset_minutes), (2026, 10, 8, 0));
        let t = parse_rfc3339("2024-02-29T13:45:30.123+03:00").unwrap();
        assert_eq!(
            (t.hour, t.minute, t.second, t.offset_minutes),
            (13, 45, 30, 180)
        );
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
        ] {
            assert!(parse_rfc3339(s).is_none(), "{s}");
        }
    }
}
