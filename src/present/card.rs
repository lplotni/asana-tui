//! A board card's short texts: who it's assigned to and when it's due.

use super::format::{is_before_today, short_date};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::api::projects::BoardTask;

/// The assignee's initials: the first letters of their first and last
/// name, or "—" when unassigned.
pub fn initials(task: &BoardTask) -> String {
    let words: Vec<&str> = task
        .assignee
        .as_ref()
        .map(|a| a.name.split_whitespace().collect())
        .unwrap_or_default();
    let first_letter = |word: &str| word.chars().next().into_iter().flat_map(char::to_uppercase);
    match words.as_slice() {
        [] => "—".to_owned(),
        [only] => first_letter(only).collect(),
        [first, .., last] => first_letter(first).chain(first_letter(last)).collect(),
    }
}

/// `text` cut to `width` terminal cells, ending in "…" when it was cut.
pub fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut kept = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        kept.push(c);
        used += w;
    }
    if width > 0 {
        kept.push('…');
    }
    kept
}

/// When a card is due, as shown on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    /// "Fri 20 Nov", as in the task pane.
    pub text: String,
    /// Past due. Board cards are open tasks, so this is never "done late".
    pub overdue: bool,
}

pub fn due(task: &BoardTask, now: i64) -> Option<Due> {
    let date = task.due_at.as_deref().or(task.due_on.as_deref())?;
    Some(Due {
        text: short_date(date, now),
        overdue: is_before_today(date, now),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tasks::Named;
    use crate::present::format::parse_timestamp;

    fn task(assignee: Option<&str>, due_on: Option<&str>, due_at: Option<&str>) -> BoardTask {
        BoardTask {
            custom_fields: Vec::new(),
            gid: "1".into(),
            name: "Task".into(),
            assignee: assignee.map(|name| Named {
                gid: "u".into(),
                name: name.into(),
            }),
            due_on: due_on.map(Into::into),
            due_at: due_at.map(Into::into),
            memberships: Vec::new(),
        }
    }

    fn now() -> i64 {
        parse_timestamp("2026-11-16T13:00:00.000Z").unwrap()
    }

    #[test]
    fn initials_are_the_first_letters_of_the_first_and_last_name() {
        assert_eq!(initials(&task(Some("Ada King Lovelace"), None, None)), "AL");
    }

    #[test]
    fn a_single_name_gives_one_initial() {
        assert_eq!(initials(&task(Some("ada"), None, None)), "A");
    }

    #[test]
    fn an_unassigned_task_shows_a_dash() {
        assert_eq!(initials(&task(None, None, None)), "—");
        assert_eq!(initials(&task(Some("  "), None, None)), "—");
    }

    #[test]
    fn the_due_date_is_short_and_not_overdue_when_in_the_future() {
        let due = due(&task(None, Some("2026-11-20"), None), now()).unwrap();

        assert_eq!(due.text, "Fri 20 Nov");
        assert!(!due.overdue);
    }

    #[test]
    fn a_past_due_date_is_overdue() {
        let due = due(&task(None, Some("2026-11-02"), None), now()).unwrap();

        assert_eq!(due.text, "Mon 2 Nov");
        assert!(due.overdue);
    }

    #[test]
    fn a_due_time_wins_over_the_due_day() {
        let due = due(
            &task(None, Some("2026-11-20"), Some("2026-11-21T09:00:00.000Z")),
            now(),
        )
        .unwrap();

        assert_eq!(due.text, "Sat 21 Nov");
    }

    #[test]
    fn text_that_fits_is_unchanged() {
        assert_eq!(truncate("Cookie banner", 13), "Cookie banner");
    }

    #[test]
    fn longer_text_ends_with_an_ellipsis_within_the_width() {
        assert_eq!(truncate("Cookie banner", 8), "Cookie …");
    }

    #[test]
    fn wide_characters_count_as_two_cells() {
        assert_eq!(truncate("日本語のタスク", 7), "日本語…");
    }

    #[test]
    fn no_room_gives_nothing() {
        assert_eq!(truncate("Cookie", 0), "");
    }

    #[test]
    fn no_due_date_gives_none() {
        assert_eq!(due(&task(None, None, None), now()), None);
    }
}
