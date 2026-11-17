//! Timestamps. The database keeps the Python backend's format so existing data stays readable and
//! text ordering stays correct: `YYYY-MM-DD HH:MM:SS.ffffff`, UTC, no offset.

use chrono::{DateTime, NaiveDateTime, Utc};

pub const DB_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.6f";

/// Now, formatted for the database.
pub fn now_db() -> String {
    to_db(Utc::now())
}

pub fn to_db(t: DateTime<Utc>) -> String {
    t.format(DB_FORMAT).to_string()
}

/// A naive database timestamp read by sqlx, as UTC.
pub fn utc(t: NaiveDateTime) -> DateTime<Utc> {
    t.and_utc()
}

pub fn utc_opt(t: Option<NaiveDateTime>) -> Option<DateTime<Utc>> {
    t.map(utc)
}

/// Parse a stored timestamp; accepts the database format and RFC 3339.
pub fn parse(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(t) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f") {
        return Some(t.and_utc());
    }
    DateTime::parse_from_rfc3339(s).ok().map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_python_format() {
        let s = "2026-11-16 21:31:06.138621";
        let t = parse(s).unwrap();
        assert_eq!(to_db(t), s);
        assert_eq!(serde_json::to_string(&t).unwrap(), "\"2026-11-16T21:31:06.138621Z\"");
    }
}
