//! Formats of text fields: the text a field shows, the value it stores,
//! and the standard form scripts (`AFNumber_Format` and the like) that
//! apply the format in a viewer as the user types.
//!
//! Scripts are built from enums, integers and the two strings validation
//! whitelists (`pattern` and `currency`), written as escaped JavaScript
//! string literals. The field's value never goes into a script.

use std::fmt::Write as _;

use crate::date::{is_digits, parse_local, parse_local_time, LocalDateTime};
use crate::schema::{FieldFormat, FormatKind, NegativeStyle, Separators};

/// Most `decimals`.
pub(crate) const MAX_DECIMALS: u8 = 10;
/// Longest `currency`, in characters.
pub(crate) const MAX_CURRENCY: usize = 8;
/// Longest `pattern`, in characters.
pub(crate) const MAX_PATTERN: usize = 64;
/// Most digits before, and after, the decimal point of a number value.
const MAX_DIGITS: usize = 15;
/// Characters a pattern shows as they are.
const LITERALS: &[char] = &[' ', '.', ',', '/', ':', '-'];

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// What a formatted field shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shown {
    pub text: String,
    /// In red: a negative number in a red style.
    pub red: bool,
}

/// The text field `value` shows under format `f`; `None` when the value
/// does not fit the format (validation reports it).
pub(crate) fn display(f: &FieldFormat, value: &str) -> Option<Shown> {
    let plain = |text: String| Some(Shown { text, red: false });
    match f.kind {
        FormatKind::Number | FormatKind::Percent => number(f, value),
        FormatKind::Date => plain(dated(f.pattern.as_deref()?, parse_local(value)?)),
        FormatKind::Time => {
            let t = parse_local_time(value).or_else(|| parse_local(value))?;
            plain(dated(f.pattern.as_deref()?, t))
        }
        FormatKind::Zip => plain(digits(value, 5)?.to_string()),
        FormatKind::ZipPlus4 => {
            let d = digits(value, 9)?;
            plain(format!("{}-{}", &d[..5], &d[5..]))
        }
        FormatKind::Phone => {
            let d = digits(value, 10)?;
            plain(format!("({}) {}-{}", &d[..3], &d[3..6], &d[6..]))
        }
        FormatKind::Ssn => {
            let d = digits(value, 9)?;
            plain(format!("{}-{}-{}", &d[..3], &d[3..5], &d[5..]))
        }
    }
}

/// The value field `value` stores under format `f` (its `/V`): the number
/// itself for numbers, which the viewer formats, and the shown text for
/// dates and times, as viewers store them.
pub(crate) fn stored(f: &FieldFormat, value: &str) -> Option<String> {
    let shown = display(f, value)?;
    Some(match f.kind {
        FormatKind::Date | FormatKind::Time => shown.text,
        _ => value.to_string(),
    })
}

/// The keystroke and format scripts of format `f`: the form scripts that
/// check what the user types and format it when the field loses focus.
pub(crate) fn scripts(f: &FieldFormat) -> (String, String) {
    let decimals = f.decimals.unwrap_or(2).min(MAX_DECIMALS);
    let separators = match f.separators.unwrap_or_default() {
        Separators::CommaDot => 0,
        Separators::Dot => 1,
        Separators::DotComma => 2,
        Separators::Comma => 3,
    };
    let (name, ex, args) = match f.kind {
        FormatKind::Number => {
            let negative = match f.negative.unwrap_or_default() {
                NegativeStyle::Minus => 0,
                NegativeStyle::Red => 1,
                NegativeStyle::Parens => 2,
                NegativeStyle::ParensRed => 3,
            };
            let currency = js_string_literal(f.currency.as_deref().unwrap_or(""));
            let before = f.currency_before.unwrap_or(true);
            (
                "AFNumber",
                "",
                format!("{decimals}, {separators}, {negative}, 0, {currency}, {before}"),
            )
        }
        FormatKind::Percent => ("AFPercent", "", format!("{decimals}, {separators}")),
        FormatKind::Date | FormatKind::Time => (
            if f.kind == FormatKind::Date {
                "AFDate"
            } else {
                "AFTime"
            },
            "Ex",
            js_string_literal(f.pattern.as_deref().unwrap_or("")),
        ),
        FormatKind::Zip => ("AFSpecial", "", "0".into()),
        FormatKind::ZipPlus4 => ("AFSpecial", "", "1".into()),
        FormatKind::Phone => ("AFSpecial", "", "2".into()),
        FormatKind::Ssn => ("AFSpecial", "", "3".into()),
    };
    (
        format!("{name}_Keystroke{ex}({args});"),
        format!("{name}_Format{ex}({args});"),
    )
}

/// `s` as a JavaScript string literal in double quotes: `\`, `"` and `'`
/// escaped, and everything outside printable ASCII (U+2028 and U+2029
/// included) as `\uXXXX`.
pub(crate) fn js_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' | '"' | '\'' => {
                out.push('\\');
                out.push(c);
            }
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0; 2];
                for u in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{u:04x}");
                }
            }
        }
    }
    out.push('"');
    out
}

/// Whether `currency` may be a currency symbol: at most [`MAX_CURRENCY`]
/// characters, without control characters, quotes or backslashes.
pub(crate) fn currency_ok(currency: &str) -> bool {
    currency.chars().count() <= MAX_CURRENCY
        && !currency
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '\'' | '\\' | '\u{2028}' | '\u{2029}'))
}

/// Checks a date or time `pattern`: runs of the letters of
/// [`dated`] (only the time ones for a time) and the characters of
/// [`LITERALS`]. The error says what is wrong.
pub(crate) fn check_pattern(pattern: &str, kind: FormatKind) -> Result<(), String> {
    if pattern.is_empty() || pattern.chars().count() > MAX_PATTERN {
        return Err(format!("a pattern is 1 to {MAX_PATTERN} characters"));
    }
    let mut fields = 0;
    for (letter, n) in runs(pattern) {
        if LITERALS.contains(&letter) {
            continue;
        }
        let ok = match letter {
            'd' | 'm' => (1..=4).contains(&n),
            'y' => n == 2 || n == 4,
            'H' | 'h' | 'M' | 's' | 't' => (1..=2).contains(&n),
            _ => false,
        };
        if !ok {
            return Err(format!(
                "`{}` is not part of a pattern: use d, dd, ddd, dddd, m, mm, mmm, mmmm, yy, yyyy, H, HH, h, hh, M, MM, s, ss, t, tt and the characters ` .,/:-`",
                String::from(letter).repeat(n)
            ));
        }
        if kind == FormatKind::Time && matches!(letter, 'd' | 'm' | 'y') {
            return Err("a time pattern shows only H, h, M, s and t".into());
        }
        fields += 1;
    }
    if fields == 0 {
        return Err("a pattern shows at least one part of the date or time".into());
    }
    Ok(())
}

/// `s` split into runs of one character: `("dd", "/")` as `('d', 2)`,
/// `('/', 1)`.
fn runs(s: &str) -> Vec<(char, usize)> {
    let mut out: Vec<(char, usize)> = Vec::new();
    for c in s.chars() {
        match out.last_mut() {
            Some((last, n)) if *last == c && !LITERALS.contains(&c) => *n += 1,
            _ => out.push((c, 1)),
        }
    }
    out
}

/// `t` shown by `pattern` (checked), with English month and day names.
fn dated(pattern: &str, t: LocalDateTime) -> String {
    let mut out = String::new();
    let month = usize::from(t.month.clamp(1, 12) - 1);
    let hour12 = match t.hour % 12 {
        0 => 12,
        h => h,
    };
    for (letter, n) in runs(pattern) {
        let part = match (letter, n) {
            ('d', 1) => t.day.to_string(),
            ('d', 2) => format!("{:02}", t.day),
            ('d', 3) => DAYS[weekday(t)][..3].to_string(),
            ('d', _) => DAYS[weekday(t)].to_string(),
            ('m', 1) => t.month.to_string(),
            ('m', 2) => format!("{:02}", t.month),
            ('m', 3) => MONTHS[month][..3].to_string(),
            ('m', _) => MONTHS[month].to_string(),
            ('y', 2) => format!("{:02}", t.year % 100),
            ('y', _) => format!("{:04}", t.year),
            ('H', 1) => t.hour.to_string(),
            ('H', _) => format!("{:02}", t.hour),
            ('h', 1) => hour12.to_string(),
            ('h', _) => format!("{hour12:02}"),
            ('M', 1) => t.minute.to_string(),
            ('M', _) => format!("{:02}", t.minute),
            ('s', 1) => t.second.to_string(),
            ('s', _) => format!("{:02}", t.second),
            ('t', 1) => (if t.hour < 12 { "a" } else { "p" }).to_string(),
            ('t', _) => (if t.hour < 12 { "am" } else { "pm" }).to_string(),
            (c, n) => String::from(c).repeat(n),
        };
        out.push_str(&part);
    }
    out
}

/// The day of the week of `t`'s date, 0 for Sunday (Gregorian).
fn weekday(t: LocalDateTime) -> usize {
    const OFFSETS: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let month = u32::from(t.month.clamp(1, 12));
    let year = u32::from(t.year) + 10_000 - u32::from(month < 3);
    let day = u32::from(t.day);
    ((year + year / 4 - year / 100 + year / 400 + OFFSETS[month as usize - 1] + day) % 7) as usize
}

/// `value` when it is exactly `n` ASCII digits.
fn digits(value: &str, n: usize) -> Option<&str> {
    is_digits(value, n).then_some(value)
}

/// A number value: `-?\d{1,15}(\.\d{1,15})?`, as digits.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Decimal {
    negative: bool,
    int: Vec<u8>,
    frac: Vec<u8>,
}

fn parse_number(value: &str) -> Option<Decimal> {
    let (negative, rest) = match value.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, value),
    };
    let (int, frac) = match rest.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (rest, None),
    };
    let ok = |s: &str| (1..=MAX_DIGITS).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit());
    if !ok(int) || frac.is_some_and(|f| !ok(f)) {
        return None;
    }
    Some(Decimal {
        negative,
        int: int.bytes().collect(),
        frac: frac.unwrap_or("").bytes().collect(),
    })
}

impl Decimal {
    /// Times 100: the decimal point two digits to the right.
    fn percent(mut self) -> Self {
        let mut frac = std::mem::take(&mut self.frac);
        frac.resize(frac.len().max(2), b'0');
        self.int.extend(frac.drain(..2));
        self.frac = frac;
        self
    }

    /// Rounded to `n` decimals, half away from zero, without leading zeros
    /// and never negative zero.
    fn round(mut self, n: usize) -> Self {
        let up = self.frac.get(n).is_some_and(|&d| d >= b'5');
        self.frac.resize(n, b'0');
        let mut all = std::mem::take(&mut self.int);
        all.append(&mut self.frac);
        if up {
            let mut carry = true;
            for d in all.iter_mut().rev() {
                if *d == b'9' {
                    *d = b'0';
                } else {
                    *d += 1;
                    carry = false;
                    break;
                }
            }
            if carry {
                all.insert(0, b'1');
            }
        }
        let split = all.len() - n;
        self.frac = all.split_off(split);
        let zeros = all.iter().take_while(|&&d| d == b'0').count();
        all.drain(..zeros.min(all.len() - 1));
        self.int = all;
        if self.int.iter().chain(&self.frac).all(|&d| d == b'0') {
            self.negative = false;
        }
        self
    }
}

/// A number or percent value shown under `f`.
fn number(f: &FieldFormat, value: &str) -> Option<Shown> {
    let mut d = parse_number(value)?;
    if f.kind == FormatKind::Percent {
        d = d.percent();
    }
    let n = usize::from(f.decimals.unwrap_or(2).min(MAX_DECIMALS));
    let d = d.round(n);
    let (group, point) = match f.separators.unwrap_or_default() {
        Separators::CommaDot => (Some(','), '.'),
        Separators::Dot => (None, '.'),
        Separators::DotComma => (Some('.'), ','),
        Separators::Comma => (None, ','),
    };
    let mut text = String::new();
    for (i, &digit) in d.int.iter().enumerate() {
        let left = d.int.len() - i;
        if i > 0 && left % 3 == 0 {
            text.extend(group);
        }
        text.push(char::from(digit));
    }
    if n > 0 {
        text.push(point);
        text.extend(d.frac.iter().map(|&b| char::from(b)));
    }
    if f.kind == FormatKind::Percent {
        text.push('%');
        if d.negative {
            text.insert(0, '-');
        }
        return Some(Shown { text, red: false });
    }
    if let Some(c) = f.currency.as_deref().filter(|c| !c.is_empty()) {
        if f.currency_before.unwrap_or(true) {
            text.insert_str(0, c);
        } else {
            text.push_str(c);
        }
    }
    if !d.negative {
        return Some(Shown { text, red: false });
    }
    let style = f.negative.unwrap_or_default();
    let text = match style {
        NegativeStyle::Minus => format!("-{text}"),
        NegativeStyle::Red => text,
        NegativeStyle::Parens | NegativeStyle::ParensRed => format!("({text})"),
    };
    Some(Shown {
        text,
        red: matches!(style, NegativeStyle::Red | NegativeStyle::ParensRed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(json: &str) -> FieldFormat {
        serde_json::from_str(json).unwrap()
    }

    fn shown(json: &str, value: &str) -> String {
        display(&fmt(json), value).unwrap().text
    }

    #[test]
    fn numbers_round_half_away_from_zero() {
        let n = r#"{"type":"number"}"#;
        for (value, want) in [
            ("0", "0.00"),
            ("1234.5", "1,234.50"),
            ("1234567.891", "1,234,567.89"),
            ("0.005", "0.01"),
            ("0.0049", "0.00"),
            ("-0.005", "-0.01"),
            ("-0.004", "0.00"),
            ("999.995", "1,000.00"),
            ("999999999999999.999", "1,000,000,000,000,000.00"),
            ("007", "7.00"),
        ] {
            assert_eq!(shown(n, value), want, "{value}");
        }
        assert_eq!(shown(r#"{"type":"number","decimals":0}"#, "2.5"), "3");
        assert_eq!(shown(r#"{"type":"number","decimals":0}"#, "-2.5"), "-3");
        assert_eq!(
            shown(r#"{"type":"number","decimals":10}"#, "1.5"),
            "1.5000000000"
        );
    }

    #[test]
    fn numbers_take_separators_negative_styles_and_currencies() {
        let v = "-1234.5";
        for (sep, want) in [
            ("1,234.56", "-1,234.50"),
            ("1234.56", "-1234.50"),
            ("1.234,56", "-1.234,50"),
            ("1234,56", "-1234,50"),
        ] {
            let f = format!(r#"{{"type":"number","separators":"{sep}"}}"#);
            assert_eq!(shown(&f, v), want);
        }
        for (style, want, red) in [
            ("minus", "-$1,234.50", false),
            ("red", "$1,234.50", true),
            ("parens", "($1,234.50)", false),
            ("parensRed", "($1,234.50)", true),
        ] {
            let f = fmt(&format!(
                r#"{{"type":"number","negative":"{style}","currency":"$"}}"#
            ));
            let s = display(&f, v).unwrap();
            assert_eq!((s.text.as_str(), s.red), (want, red), "{style}");
            // Positive numbers are never red.
            assert!(!display(&f, "1").unwrap().red);
        }
        assert_eq!(
            shown(
                r#"{"type":"number","currency":" €","currencyBefore":false,"separators":"1.234,56"}"#,
                "1234.5"
            ),
            "1.234,50 €"
        );
    }

    #[test]
    fn percents_are_hundredths() {
        let p = r#"{"type":"percent"}"#;
        for (value, want) in [
            ("0.125", "12.50%"),
            ("1", "100.00%"),
            ("0.5", "50.00%"),
            ("-0.00125", "-0.13%"),
            ("12", "1,200.00%"),
        ] {
            assert_eq!(shown(p, value), want, "{value}");
        }
        assert_eq!(
            shown(
                r#"{"type":"percent","decimals":0,"separators":"1234,56"}"#,
                "12.345"
            ),
            "1235%"
        );
    }

    #[test]
    fn dates_and_times_show_every_token() {
        let date = |pattern: &str, value: &str| {
            shown(
                &format!(r#"{{"type":"date","pattern":"{pattern}"}}"#),
                value,
            )
        };
        let v = "2026-03-05T07:08:09";
        for (pattern, want) in [
            ("d/m/yy", "5/3/26"),
            ("dd.mm.yyyy", "05.03.2026"),
            ("ddd, mmm d", "Thu, Mar 5"),
            ("dddd mmmm d, yyyy", "Thursday March 5, 2026"),
            ("H:M:s", "7:8:9"),
            ("HH:MM:ss", "07:08:09"),
            ("h:MM t", "7:08 a"),
            ("hh:MM tt", "07:08 am"),
        ] {
            assert_eq!(date(pattern, v), want, "{pattern}");
        }
        assert_eq!(date("h tt", "2026-03-05T00:30"), "12 am");
        assert_eq!(date("h tt", "2026-03-05T12:30"), "12 pm");
        assert_eq!(date("h tt", "2026-03-05T23:30"), "11 pm");
        assert_eq!(date("dddd", "2000-01-01"), "Saturday");
        assert_eq!(date("dddd", "2024-02-29"), "Thursday");
        let time = r#"{"type":"time","pattern":"HH:MM"}"#;
        assert_eq!(shown(time, "18:30"), "18:30");
        assert_eq!(shown(time, "2026-01-01T09:15"), "09:15");
        // Dates and times are stored as shown; numbers as they are.
        assert_eq!(stored(&fmt(time), "18:30:00").unwrap(), "18:30");
        assert_eq!(
            stored(&fmt(r#"{"type":"number"}"#), "-1.5").unwrap(),
            "-1.5"
        );
    }

    #[test]
    fn special_formats_mask_digits() {
        for (kind, value, want) in [
            ("zip", "02134", "02134"),
            ("zipPlus4", "021341234", "02134-1234"),
            ("phone", "5551234567", "(555) 123-4567"),
            ("ssn", "123456789", "123-45-6789"),
        ] {
            assert_eq!(shown(&format!(r#"{{"type":"{kind}"}}"#), value), want);
        }
    }

    #[test]
    fn values_that_do_not_fit_are_rejected() {
        for (f, value) in [
            (r#"{"type":"number"}"#, ""),
            (r#"{"type":"number"}"#, "1e5"),
            (r#"{"type":"number"}"#, "+1"),
            (r#"{"type":"number"}"#, ".5"),
            (r#"{"type":"number"}"#, "5."),
            (r#"{"type":"number"}"#, "1,000"),
            (r#"{"type":"number"}"#, "1234567890123456"),
            (r#"{"type":"number"}"#, "1.1234567890123456"),
            (r#"{"type":"percent"}"#, "--1"),
            (r#"{"type":"date","pattern":"dd"}"#, "2026-02-30"),
            (r#"{"type":"date","pattern":"dd"}"#, "18:30"),
            (r#"{"type":"time","pattern":"HH"}"#, "25:00"),
            (r#"{"type":"zip"}"#, "1234"),
            (r#"{"type":"zipPlus4"}"#, "02134-1234"),
            (r#"{"type":"phone"}"#, "(555) 123-4567"),
            (r#"{"type":"ssn"}"#, "12345678a"),
        ] {
            assert!(display(&fmt(f), value).is_none(), "{f} {value}");
        }
    }

    #[test]
    fn patterns_are_whitelisted() {
        for p in [
            "dd/mm/yyyy",
            "d-m-yy",
            "H:MM",
            "hh:MM tt",
            "dddd, mmmm d",
            "yyyy.mm.dd",
        ] {
            assert_eq!(check_pattern(p, FormatKind::Date), Ok(()), "{p}");
        }
        assert_eq!(check_pattern("HH:MM:ss tt", FormatKind::Time), Ok(()));
        for p in [
            "",
            "yyy",
            "ddddd",
            "HHH",
            "x",
            "dd\"",
            "dd'",
            "dd\\",
            "dd(",
            "\");app.alert(1);//",
            " / ",
            &"d".repeat(65),
        ] {
            assert!(check_pattern(p, FormatKind::Date).is_err(), "{p}");
        }
        assert!(check_pattern("dd HH", FormatKind::Time).is_err());
    }

    #[test]
    fn currencies_have_no_quotes_or_controls() {
        for c in ["$", "€", " EUR", "CHF ", "12345678"] {
            assert!(currency_ok(c), "{c}");
        }
        for c in ["\"", "'", "\\", "\n", "\u{2028}", "123456789"] {
            assert!(!currency_ok(c), "{c:?}");
        }
    }

    #[test]
    fn scripts_call_the_standard_functions() {
        let s = |json: &str| scripts(&fmt(json));
        assert_eq!(
            s(
                r#"{"type":"number","decimals":0,"separators":"1.234,56","negative":"parensRed","currency":"€","currencyBefore":false}"#
            ),
            (
                r#"AFNumber_Keystroke(0, 2, 3, 0, "\u20ac", false);"#.to_string(),
                r#"AFNumber_Format(0, 2, 3, 0, "\u20ac", false);"#.to_string()
            )
        );
        assert_eq!(
            s(r#"{"type":"number"}"#).1,
            r#"AFNumber_Format(2, 0, 0, 0, "", true);"#
        );
        assert_eq!(s(r#"{"type":"percent"}"#).0, "AFPercent_Keystroke(2, 0);");
        assert_eq!(
            s(r#"{"type":"date","pattern":"dd/mm/yyyy"}"#),
            (
                r#"AFDate_KeystrokeEx("dd/mm/yyyy");"#.to_string(),
                r#"AFDate_FormatEx("dd/mm/yyyy");"#.to_string()
            )
        );
        assert_eq!(
            s(r#"{"type":"time","pattern":"HH:MM"}"#).1,
            r#"AFTime_FormatEx("HH:MM");"#
        );
        assert_eq!(s(r#"{"type":"phone"}"#).1, "AFSpecial_Format(2);");
        // The defaults React gives a format written with the other key
        // names: no decimals, no thousands separator.
        let other = r#"{"type":"number","decimals":0,"separators":"1234.56"}"#;
        assert_eq!(s(other).1, r#"AFNumber_Format(0, 1, 0, 0, "", true);"#);
        assert_eq!(shown(other, "1250"), "1250");
        assert_eq!(shown(other, "-1250.5"), "-1251");
    }

    #[test]
    fn string_literals_escape_what_could_end_them() {
        assert_eq!(js_string_literal("a\"b'c\\d"), r#""a\"b\'c\\d""#);
        assert_eq!(
            js_string_literal("\");app.alert(1);//"),
            r#""\");app.alert(1);//""#
        );
        assert_eq!(
            js_string_literal("\n\u{2028}\u{2029}é😀"),
            r#""\u000a\u2028\u2029\u00e9\ud83d\ude00""#
        );
    }
}
