//! Display helpers: relative times, readable notification actions, and
//! which characters are unsafe to show.

/// A bidi override or isolate (U+202A–202E, U+2066–2069): it would reorder
/// what the terminal shows after it. The CLI's `terminal_safe` and the TUI's
/// render pass both drop these.
pub fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// Seconds since the Unix epoch for Asana's `2026-11-16T12:43:23.527Z`
/// timestamps (always UTC). `None` for anything else.
pub fn parse_timestamp(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    let well_formed = b.len() >= 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && text.ends_with('Z');
    if !well_formed {
        return None;
    }
    let num = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// "now", "5m", "3h", "2d" or "4w" between `created_at` and `now`.
pub fn relative_time(created_at: &str, now: i64) -> String {
    let Some(created) = parse_timestamp(created_at) else {
        return String::new();
    };
    let seconds = (now - created).max(0);
    match seconds {
        s if s < 60 => "now".to_owned(),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s if s < 7 * 86_400 => format!("{}d", s / 86_400),
        s => format!("{}w", s / (7 * 86_400)),
    }
}

/// `now` in Asana's timestamp format, e.g. `2026-11-16T13:00:00.000Z`.
pub fn timestamp(now: i64) -> String {
    let days = now.div_euclid(86_400);
    let seconds = now.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

/// (year, month, day) for days since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`, the inverse of [`days_from_civil`]).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `2026-11-04` → "Wed 4 Nov" in the year of `now`, else "Wed 31 Dec 2025".
/// Unparseable input is returned unchanged.
pub fn short_date(date: &str, now: i64) -> String {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let Some((year, month, day)) = parse_date(date) else {
        return date.to_owned();
    };
    let days = days_from_civil(year, month, day);
    let weekday = WEEKDAYS[(days + 4).rem_euclid(7) as usize];
    let month_name = MONTHS[(month - 1) as usize];
    let today = (now + local_offset(now)).div_euclid(86_400);
    let same_year = days_from_civil(year, 1, 1) <= today && today < days_from_civil(year + 1, 1, 1);
    if same_year {
        format!("{weekday} {day} {month_name}")
    } else {
        format!("{weekday} {day} {month_name} {year}")
    }
}

/// Whether the `YYYY-MM-DD` day `date` lies before the local day of `now`
/// when the local time is `offset` seconds east of UTC.
pub fn is_before_today_at(date: &str, now: i64, offset: i64) -> bool {
    let today = (now + offset).div_euclid(86_400);
    parse_date(date).is_some_and(|(y, m, d)| days_from_civil(y, m, d) < today)
}

/// Whether the `YYYY-MM-DD` day `date` lies before today (local time).
pub fn is_before_today(date: &str, now: i64) -> bool {
    is_before_today_at(date, now, local_offset(now))
}

/// `YYYY-MM-DD` (or the date part of a timestamp) as (year, month, day).
fn parse_date(text: &str) -> Option<(i64, i64, i64)> {
    let b = text.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then_some((year, month, day))
}

/// Seconds east of UTC for local time at `now` (from the system's time zone).
pub fn local_offset(now: i64) -> i64 {
    let time: libc::time_t = now as libc::time_t;
    // SAFETY: `localtime_r` only writes into the zeroed `tm` we own; a null
    // result (conversion failed) falls back to UTC.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&time, &mut tm).is_null() {
            0
        } else {
            // `tm_gmtoff` is 32-bit on some targets.
            #[allow(clippy::useless_conversion)]
            i64::from(tm.tm_gmtoff)
        }
    }
}

/// What happened, as read in the Inbox: `comment_added` → "commented".
/// Unknown subtypes are shown with spaces instead of underscores.
pub fn describe(subtype: &str) -> String {
    match subtype {
        "comment_added" => "commented",
        "assigned" => "assigned you",
        "story_reaction_added" => "reacted",
        "added_to_project" => "added to project",
        "marked_complete" => "completed",
        "overdue_tasks_reminder" => "overdue reminder",
        other => return other.replace('_', " "),
    }
    .to_owned()
}

/// A byte count as "999 B", "121 KB", "2.3 MB" or "3.0 GB" (1024-based).
pub fn file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{} KB", (b / KB).round())
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.1} GB", b / (KB * KB * KB))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_sizes_are_human_readable() {
        assert_eq!(file_size(0), "0 B");
        assert_eq!(file_size(999), "999 B");
        assert_eq!(file_size(1024), "1 KB");
        assert_eq!(file_size(123_456), "121 KB");
        assert_eq!(file_size(2_411_724), "2.3 MB");
        assert_eq!(file_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    use super::*;

    #[test]
    fn parses_asanas_timestamps_to_epoch_seconds() {
        assert_eq!(parse_timestamp("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            parse_timestamp("2026-11-16T12:43:23.527Z"),
            Some(1_794_833_003)
        );
        assert_eq!(parse_timestamp("2024-02-29T23:59:59Z"), Some(1_709_251_199));
    }

    #[test]
    fn rejects_other_formats() {
        assert_eq!(parse_timestamp("yesterday"), None);
        assert_eq!(parse_timestamp("2026-11-16 12:43:23"), None);
        assert_eq!(parse_timestamp(""), None);
    }

    #[test]
    fn relative_time_picks_the_largest_whole_unit() {
        let created = "2026-11-16T12:00:00.000Z";
        let at = |offset: i64| relative_time(created, parse_timestamp(created).unwrap() + offset);

        assert_eq!(at(30), "now");
        assert_eq!(at(5 * 60), "5m");
        assert_eq!(at(3 * 3600 + 59), "3h");
        assert_eq!(at(2 * 86_400), "2d");
        assert_eq!(at(15 * 86_400), "2w");
    }

    #[test]
    fn relative_time_of_an_unparseable_timestamp_is_empty() {
        assert_eq!(relative_time("garbage", 0), "");
    }

    #[test]
    fn describes_known_and_unknown_subtypes() {
        assert_eq!(describe("comment_added"), "commented");
        assert_eq!(describe("assigned"), "assigned you");
        assert_eq!(describe("story_reaction_added"), "reacted");
        assert_eq!(describe("some_future_subtype"), "some future subtype");
    }

    #[test]
    fn short_date_shows_weekday_day_and_month() {
        let now = parse_timestamp("2026-11-16T13:00:00.000Z").unwrap();

        assert_eq!(short_date("2026-11-04", now), "Wed 4 Nov");
        assert_eq!(short_date("2025-12-31", now), "Wed 31 Dec 2025");
        assert_eq!(short_date("someday", now), "someday");
    }

    #[test]
    fn is_before_today_compares_whole_days() {
        let now = parse_timestamp("2026-11-16T13:00:00.000Z").unwrap();

        assert!(is_before_today("2026-11-15", now));
        assert!(!is_before_today("2026-11-16", now));
        assert!(!is_before_today("2026-11-20", now));
    }

    #[test]
    fn timestamp_round_trips_with_parse_timestamp() {
        for text in [
            "2026-11-16T13:00:00.000Z",
            "2024-02-29T23:59:59.000Z",
            "1970-01-01T00:00:00.000Z",
        ] {
            assert_eq!(timestamp(parse_timestamp(text).unwrap()), text);
        }
    }

    #[test]
    fn overdue_uses_the_local_date_not_utc() {
        // 2026-11-16 23:30 UTC.
        let now = parse_timestamp("2026-11-16T23:30:00.000Z").unwrap();

        assert!(
            is_before_today_at("2026-11-16", now, 2 * 3600),
            "already the 17th at UTC+2"
        );
        assert!(
            !is_before_today_at("2026-11-16", now, -8 * 3600),
            "still the 16th at UTC-8"
        );
        assert!(!is_before_today_at("2026-11-17", now, 2 * 3600));
    }

    #[test]
    fn the_local_offset_is_a_plausible_time_zone() {
        let offset = local_offset(parse_timestamp("2026-11-16T12:00:00.000Z").unwrap());

        assert!((-14 * 3600..=14 * 3600).contains(&offset), "{offset}");
    }
}
