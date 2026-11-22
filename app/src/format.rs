//! Number and date formatting, as frontend/src/lib/format.ts does it (in English; the web UI
//! follows the browser's locale, which the native app does not read yet).

use chrono::{DateTime, Utc};

/// A gray dash for a missing value (DESIGN.md, "Do and don't").
pub const DASH: &str = "–";

/// 1,284: an integer with thousands separators.
pub fn int(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if n < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "just now", "5 minutes ago", "yesterday", then a date ("Nov 16, 2026") after a week, as the
/// web UI's `relative` (Intl.RelativeTimeFormat with numeric "auto").
pub fn relative(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let diff = (then - now).num_milliseconds() as f64 / 1000.0;
    let abs = diff.abs();
    if abs < 45.0 {
        return "just now".into();
    }
    let (value, unit) = if abs < 3600.0 {
        ((diff / 60.0).round() as i64, "minute")
    } else if abs < 86_400.0 {
        ((diff / 3600.0).round() as i64, "hour")
    } else if abs < 7.0 * 86_400.0 {
        ((diff / 86_400.0).round() as i64, "day")
    } else {
        return then.format("%b %-d, %Y").to_string();
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

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;

    #[test]
    fn integers_group_by_thousands() {
        assert_eq!(int(0), "0");
        assert_eq!(int(999), "999");
        assert_eq!(int(1284), "1,284");
        assert_eq!(int(1_000_000), "1,000,000");
        assert_eq!(int(-4200), "-4,200");
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
        assert_eq!(relative(now - Duration::days(8), now), "Nov 14, 2026");
    }
}
