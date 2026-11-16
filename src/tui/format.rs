//! Display helpers: relative times and readable notification actions.

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

#[cfg(test)]
mod tests {
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
}
