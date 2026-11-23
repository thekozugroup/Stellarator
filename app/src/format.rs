//! Number and date formatting, as frontend/src/lib/format.ts does it. Never render raw numbers,
//! floats or dates: every screen goes through these.
//!
//! The system's locale comes from nsh (`locale()`, `locale_settings()`): the digit grouping and the
//! decimal mark follow the language (1,284.5 in English, 1.284,5 in German, 1 284,5 in French),
//! and times follow the 12- or 24-hour setting. Words ("5 minutes ago", month names) stay
//! English: nsh 0.6.1 has no CLDR data for Rust code, and the app's text is English. Dates are in
//! the computer's time zone on the desktops and in UTC on the web.
//!
//! The functions without `_in` read the locale of the engine they run on; the `_in` forms take a
//! [`Style`], for tests and for code outside an engine.

use chrono::{DateTime, Datelike, Timelike, Utc};

/// A gray dash for a missing value (DESIGN.md, "Do and don't").
pub const DASH: &str = "–";

/// How numbers and times are written: separators and the hour cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    /// Between groups of three digits.
    pub group: &'static str,
    /// Between the whole part and the fraction.
    pub decimal: &'static str,
    /// 12-hour times ("9:14 PM"); else 24-hour ("21:14").
    pub twelve_hour: bool,
}

impl Style {
    /// English (United States): 1,284.5 and 9:14 PM.
    pub const EN_US: Style = Style { group: ",", decimal: ".", twelve_hour: true };

    /// The style of a BCP 47 language tag (`de-DE`, `fr`, `en-GB`), with its hour cycle.
    pub fn of(tag: &str, twelve_hour: bool) -> Style {
        let language = tag.split(['-', '_']).next().unwrap_or("en").to_ascii_lowercase();
        let (group, decimal) = match language.as_str() {
            // Swiss German: an apostrophe groups.
            _ if tag.replace('_', "-").eq_ignore_ascii_case("de-CH") => ("’", "."),
            // A dot groups, a comma marks the fraction.
            "de" | "nl" | "da" | "id" | "it" | "es" | "pt" | "tr" | "el" | "ro" | "sl" | "hr" | "sr" | "vi" => (".", ","),
            // A narrow no-break space groups, a comma marks the fraction.
            "fr" | "nb" | "no" | "sv" | "fi" | "cs" | "sk" | "pl" | "ru" | "uk" | "hu" | "bg" | "et" | "lt" | "lv" => {
                ("\u{202f}", ",")
            }
            _ => (",", "."),
        };
        Style { group, decimal, twelve_hour }
    }

    /// The style of the system the engine runs on (`locale()` and its hour cycle).
    pub fn system() -> Style {
        use nsh_core::prelude::*;
        let settings = untracked(locale_settings);
        let twelve = matches!(settings.hour_cycle, nsh_core::platform::HourCycle::H11 | nsh_core::platform::HourCycle::H12);
        Style::of(untracked(locale).as_str(), twelve)
    }
}

/// The current style, or English outside an engine (unit tests).
fn style() -> Style {
    if nsh_core::prelude::try_os().is_some() { Style::system() } else { Style::EN_US }
}

/// 1,284
pub fn int(n: i64) -> String {
    int_in(n, style())
}

pub fn int_in(n: i64, s: Style) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 * 3 + 1);
    if n < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push_str(s.group);
        }
        out.push(c);
    }
    out
}

/// `int` of an optional value: the dash when missing.
pub fn int_opt(n: Option<i64>) -> String {
    n.map(int).unwrap_or_else(|| DASH.into())
}

/// 12.5: one decimal.
pub fn dec1(n: f64) -> String {
    dec1_in(n, style())
}

pub fn dec1_in(n: f64, s: Style) -> String {
    if !n.is_finite() {
        return DASH.into();
    }
    let tenths = js_round(n * 10.0) as i64;
    let whole = int_in(tenths / 10, s);
    let sign = if tenths < 0 && tenths / 10 == 0 { "-" } else { "" };
    format!("{sign}{whole}{}{}", s.decimal, (tenths % 10).abs())
}

/// 12.7k: only for capacity lines and tight spots, never in tables.
pub fn short(n: f64) -> String {
    short_in(n, style())
}

pub fn short_in(n: f64, s: Style) -> String {
    if !n.is_finite() {
        return DASH.into();
    }
    let a = n.abs();
    let (value, unit) = if a >= 1e12 {
        (n / 1e12, "T")
    } else if a >= 1e9 {
        (n / 1e9, "B")
    } else if a >= 1e6 {
        (n / 1e6, "M")
    } else if a >= 1e3 {
        (n / 1e3, "K")
    } else {
        (n, "")
    };
    // At most one decimal, none when it is zero (Intl's compact notation).
    let tenths = js_round(value * 10.0) as i64;
    let body = if tenths % 10 == 0 { int_in(tenths / 10, s) } else { dec1_in(tenths as f64 / 10.0, s) };
    format!("{body}{unit}")
}

/// "1 prompt", "1,284 prompts"
pub fn count(n: i64, one: &str, many: &str) -> String {
    format!("{} {}", int(n), if n == 1 { one } else { many })
}

/// 42.0% of `whole`; the dash when `whole` is 0.
pub fn pct(part: f64, whole: f64) -> String {
    if whole == 0.0 {
        return DASH.into();
    }
    format!("{}%", dec1(part / whole * 100.0))
}

/// "820 ms", "4.2 s", "3 min 12 s", "2 h 14 min", "3 d 4 h"
pub fn duration(ms: Option<f64>) -> String {
    duration_in(ms, style())
}

pub fn duration_in(ms: Option<f64>, st: Style) -> String {
    let Some(ms) = ms.filter(|m| m.is_finite()) else { return DASH.into() };
    if ms < 1000.0 {
        return format!("{} ms", int_in(js_round(ms) as i64, st));
    }
    let s = ms / 1000.0;
    if s < 60.0 {
        return format!("{} s", dec1_in(s, st));
    }
    let m = (s / 60.0).floor() as i64;
    if m < 60 {
        return format!("{m} min {} s", js_round(s % 60.0) as i64);
    }
    let h = m / 60;
    if h < 48 {
        return format!("{h} h {} min", m % 60);
    }
    format!("{} d {} h", h / 24, h % 24)
}

/// "just now", "5 minutes ago", "yesterday", then a date ("Nov 16, 2026") after a week, as the web
/// UI's `relative` (Intl.RelativeTimeFormat with numeric "auto").
pub fn relative(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let diff = (then - now).num_milliseconds() as f64 / 1000.0;
    let abs = diff.abs();
    if abs < 45.0 {
        return "just now".into();
    }
    let (value, unit) = if abs < 3600.0 {
        (js_round(diff / 60.0) as i64, "minute")
    } else if abs < 86_400.0 {
        (js_round(diff / 3600.0) as i64, "hour")
    } else if abs < 7.0 * 86_400.0 {
        (js_round(diff / 86_400.0) as i64, "day")
    } else {
        return date(then);
    };
    match (value, unit) {
        (0, "minute") => "this minute".into(),
        (0, "hour") => "this hour".into(),
        (0, _) => "today".into(),
        (-1, "day") => "yesterday".into(),
        (1, "day") => "tomorrow".into(),
        (v, u) if v < 0 => format!("{} {}{} ago", -v, u, if v == -1 { "" } else { "s" }),
        (v, u) => format!("in {v} {u}{}", if v == 1 { "" } else { "s" }),
    }
}

/// `relative` of an optional time: the dash when missing.
pub fn relative_opt(then: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    then.map(|t| relative(t, now)).unwrap_or_else(|| DASH.into())
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// A time in the zone dates are shown in: the computer's on the desktops, UTC on the web.
fn local(t: DateTime<Utc>) -> chrono::NaiveDateTime {
    #[cfg(not(target_arch = "wasm32"))]
    {
        t.with_timezone(&chrono::Local).naive_local()
    }
    #[cfg(target_arch = "wasm32")]
    {
        t.naive_utc()
    }
}

/// "Nov 16, 2026"
pub fn date(t: DateTime<Utc>) -> String {
    let d = local(t);
    format!("{} {}, {}", MONTHS[d.month0() as usize], d.day(), d.year())
}

/// "Nov 16, 9:14 PM"
pub fn date_time(t: DateTime<Utc>) -> String {
    date_time_in(local(t), style())
}

pub fn date_time_in(d: chrono::NaiveDateTime, s: Style) -> String {
    format!("{} {}, {}", MONTHS[d.month0() as usize], d.day(), clock(d.hour(), d.minute(), None, s))
}

/// "9:14:05 PM"
pub fn time(t: DateTime<Utc>) -> String {
    let d = local(t);
    clock(d.hour(), d.minute(), Some(d.second()), style())
}

/// "9:14 PM": the time without seconds (a chart's axis).
pub fn time_hm(t: DateTime<Utc>) -> String {
    let d = local(t);
    clock(d.hour(), d.minute(), None, style())
}

/// "Nov 16": the date without the year (a chart's axis).
pub fn month_day(t: DateTime<Utc>) -> String {
    let d = local(t);
    format!("{} {}", MONTHS[d.month0() as usize], d.day())
}

/// "Nov 16, 9:14:05 PM" (a chart's readout across days).
pub fn date_time_seconds(t: DateTime<Utc>) -> String {
    let d = local(t);
    format!("{} {}, {}", MONTHS[d.month0() as usize], d.day(), clock(d.hour(), d.minute(), Some(d.second()), style()))
}

/// The calendar day `t` falls on where dates are shown, to tell whether two times are on one day.
pub fn local_day(t: DateTime<Utc>) -> chrono::NaiveDate {
    local(t).date()
}

/// "9:14 PM" or "21:14", with seconds when given.
fn clock(hour: u32, minute: u32, second: Option<u32>, s: Style) -> String {
    let seconds = second.map(|v| format!(":{v:02}")).unwrap_or_default();
    if s.twelve_hour {
        let h = if hour % 12 == 0 { 12 } else { hour % 12 };
        format!("{h}:{minute:02}{seconds} {}", if hour < 12 { "AM" } else { "PM" })
    } else {
        format!("{hour:02}:{minute:02}{seconds}")
    }
}

/// A file size: "2.3 MB".
pub fn bytes(n: u64) -> String {
    if n < 1024 {
        return format!("{} B", int(n as i64));
    }
    let units = ["KB", "MB", "GB"];
    let mut v = n as f64 / 1024.0;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{} {}", dec1(v), units[i])
}

/// One line of at most `max` characters: whitespace collapsed, an ellipsis when cut.
pub fn one_line(s: &str, max: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max {
        let kept: String = flat.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    } else {
        flat
    }
}

/// Keep the start and the end: "Qwen/Qwen3-235B-A22B…Instruct-FP8".
pub fn middle(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_owned();
    }
    let keep = max - 1;
    let head = (keep as f64 * 0.6).ceil() as usize;
    let tail = keep - head;
    let start: String = chars[..head].iter().collect();
    let end: String = chars[chars.len() - tail..].iter().collect();
    format!("{start}…{end}")
}

/// JavaScript's `Math.round`: halves go up (towards +∞), so -0.5 becomes -0, not -1 as Rust's
/// `round` gives. Ports of the web UI's arithmetic use it so they give the same numbers.
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, NaiveDate, TimeZone};

    use super::*;

    #[test]
    fn integers_group_by_thousands_in_the_locale() {
        assert_eq!(int_in(0, Style::EN_US), "0");
        assert_eq!(int_in(999, Style::EN_US), "999");
        assert_eq!(int_in(1284, Style::EN_US), "1,284");
        assert_eq!(int_in(1_000_000, Style::EN_US), "1,000,000");
        assert_eq!(int_in(-4200, Style::EN_US), "-4,200");
        assert_eq!(int_in(1_234_567, Style::of("de-DE", false)), "1.234.567");
        assert_eq!(int_in(1_234_567, Style::of("fr-FR", false)), "1\u{202f}234\u{202f}567");
        assert_eq!(int_in(1_234_567, Style::of("de-CH", false)), "1’234’567");
    }

    #[test]
    fn decimals_and_compact_numbers() {
        assert_eq!(dec1_in(12.47, Style::EN_US), "12.5");
        assert_eq!(dec1_in(1234.0, Style::EN_US), "1,234.0");
        assert_eq!(dec1_in(1234.56, Style::of("de", false)), "1.234,6");
        assert_eq!(dec1_in(-0.25, Style::EN_US), "-0.2");
        assert_eq!(short_in(12_700.0, Style::EN_US), "12.7K");
        assert_eq!(short_in(4_000_000.0, Style::EN_US), "4M");
        assert_eq!(short_in(950.0, Style::EN_US), "950");
    }

    #[test]
    fn durations_read_like_the_web_ui() {
        assert_eq!(duration_in(Some(820.0), Style::EN_US), "820 ms");
        assert_eq!(duration_in(Some(4200.0), Style::EN_US), "4.2 s");
        assert_eq!(duration_in(Some(192_000.0), Style::EN_US), "3 min 12 s");
        assert_eq!(duration_in(Some(8_040_000.0), Style::EN_US), "2 h 14 min");
        assert_eq!(duration_in(Some(3.0 * 86_400_000.0 + 4.0 * 3_600_000.0), Style::EN_US), "3 d 4 h");
        assert_eq!(duration_in(None, Style::EN_US), DASH);
    }

    #[test]
    fn times_read_like_the_web_ui() {
        let now = Utc.with_ymd_and_hms(2026, 11, 22, 12, 0, 0).unwrap();
        assert_eq!(relative(now - Duration::seconds(10), now), "just now");
        assert_eq!(relative(now - Duration::minutes(5), now), "5 minutes ago");
        assert_eq!(relative(now - Duration::minutes(1), now), "1 minute ago");
        assert_eq!(relative(now - Duration::hours(3), now), "3 hours ago");
        assert_eq!(relative(now - Duration::hours(30), now), "yesterday");
        assert_eq!(relative(now - Duration::days(3), now), "3 days ago");
        assert_eq!(relative(now + Duration::minutes(5), now), "in 5 minutes");
    }

    #[test]
    fn clocks_follow_the_hour_cycle() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap().and_hms_opt(21, 14, 5).unwrap();
        assert_eq!(date_time_in(d, Style::EN_US), "Nov 16, 9:14 PM");
        assert_eq!(date_time_in(d, Style::of("de-DE", false)), "Nov 16, 21:14");
        assert_eq!(clock(0, 5, Some(9), Style::EN_US), "12:05:09 AM");
    }

    #[test]
    fn text_shortening() {
        // As format.ts: 19 characters kept, ceil(19 * 0.6) = 12 before the ellipsis.
        assert_eq!(middle("Qwen/Qwen3-235B-A22B-Instruct-FP8", 20), "Qwen/Qwen3-2…uct-FP8");
        assert_eq!(middle("short", 20), "short");
        assert_eq!(one_line("a\n  b\tc", 140), "a b c");
        assert_eq!(one_line("abcdef", 4), "abc…");
        assert_eq!(js_round(-0.5), 0.0);
        assert_eq!(js_round(2.5), 3.0);
    }
}
