//! `asn inbox` output.

use super::terminal_safe;
use crate::api::notifications::Notification;

/// One tab-separated line per notification:
/// `gid  unread-marker  created_at  actor  subtype  object-name`.
pub fn render_text(notifications: &[Notification]) -> String {
    notifications
        .iter()
        .map(|n| {
            let marker = if n.is_read { " " } else { "●" };
            let actor = n.created_by.as_ref().map_or("Asana", |a| a.name.as_str());
            let object = n.work_objects.first().map_or("-", |o| o.name.as_str());
            let fields = [
                n.gid.as_str(),
                marker,
                n.created_at.as_str(),
                actor,
                n.story_resource_subtype.as_str(),
                object,
            ];
            let safe: Vec<String> = fields.iter().map(|f| terminal_safe(f)).collect();
            safe.join("\t") + "\n"
        })
        .collect()
}

/// The notifications as Asana returned them, as a JSON array.
pub fn render_json(notifications: &[Notification]) -> String {
    serde_json::to_string_pretty(notifications).expect("notifications always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge_cases() -> Vec<Notification> {
        let text = std::fs::read_to_string("tests/fixtures/notifications_edge_cases.json").unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        serde_json::from_value(value["data"].clone()).unwrap()
    }

    #[test]
    fn one_tab_separated_line_per_notification_starting_with_the_gid() {
        let text = render_text(&edge_cases());
        let lines: Vec<&str> = text.lines().collect();

        assert_eq!(lines.len(), 4);
        assert_eq!(
            lines[2],
            "1300000000000003\t●\t2026-11-14T08:00:00.000Z\tGrace Hopper\tcomment_added\tSynthetic goal"
        );
    }

    #[test]
    fn read_items_have_a_blank_marker_and_system_items_say_asana() {
        let text = render_text(&edge_cases());
        let lines: Vec<&str> = text.lines().collect();

        assert!(
            lines[0].contains("\t●\t") && lines[0].contains("\tAsana\t"),
            "{}",
            lines[0]
        );
        assert!(
            lines[1].starts_with("1300000000000002\t \t"),
            "{}",
            lines[1]
        );
    }

    #[test]
    fn notification_without_objects_shows_a_dash() {
        let text = render_text(&edge_cases());

        assert!(text.lines().nth(3).unwrap().ends_with("\tcomment_added\t-"));
    }

    #[test]
    fn untrusted_names_cannot_inject_escapes_or_break_the_columns() {
        let mut notifications = edge_cases();
        notifications[2].created_by.as_mut().unwrap().name = "Eve\u{1b}]0;pwned\u{7}".into();
        notifications[2].work_objects[0].name = "two\tcolumns\nand a line".into();

        let text = render_text(&notifications);

        assert!(!text.contains('\u{1b}'));
        assert_eq!(text.lines().count(), 4);
        assert!(text.lines().all(|l| l.split('\t').count() == 6), "{text:?}");
    }
}
