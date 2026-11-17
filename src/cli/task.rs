//! `asn task` output: the detail pane's content without styles, or JSON.

use super::terminal_safe;
use crate::core::TaskDetail;
use crate::present::detail;

/// Width used for text output.
const TEXT_WIDTH: usize = 80;

pub fn render_text(detail: &TaskDetail, now: i64) -> String {
    detail::lines(detail, TEXT_WIDTH, now)
        .iter()
        .map(|line| terminal_safe(&detail::plain(line)) + "\n")
        .collect()
}

pub fn render_json(detail: &TaskDetail) -> String {
    serde_json::to_string_pretty(detail).expect("task details always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tasks::TaskStory;

    fn detail() -> TaskDetail {
        let read = |name: &str| -> serde_json::Value {
            serde_json::from_str(
                &std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap(),
            )
            .unwrap()
        };
        let task = serde_json::from_value(read("task_full.json")["data"].clone()).unwrap();
        let stories: Vec<TaskStory> =
            serde_json::from_value(read("stories_thread.json")["data"].clone()).unwrap();
        let (comments, activity): (Vec<_>, Vec<_>) =
            stories.into_iter().partition(TaskStory::is_comment);
        TaskDetail {
            task,
            comments,
            activity_count: activity.len(),
            attachments: Vec::new(),
        }
    }

    #[test]
    fn text_is_the_detail_pane_without_styles() {
        let d = detail();

        let text = render_text(&d, 1_794_834_000);

        assert!(text.starts_with(&format!("{}\n", d.task.name)));
        assert!(text.contains("Due Wed 4 Nov (overdue)"));
        assert!(text.contains(&format!("Comments ({})", d.comments.len())));
        assert!(
            text.lines()
                .all(|l| unicode_width::UnicodeWidthStr::width(l) <= TEXT_WIDTH)
        );
    }

    #[test]
    fn json_has_the_task_comments_and_activity_count() {
        let d = detail();

        let json: serde_json::Value = serde_json::from_str(&render_json(&d)).unwrap();

        assert_eq!(json["task"]["name"], d.task.name);
        assert_eq!(json["comments"].as_array().unwrap().len(), d.comments.len());
        assert_eq!(json["activity_count"], d.activity_count);
    }

    #[test]
    fn untrusted_comments_cannot_inject_escapes() {
        let mut d = detail();
        d.comments[0].text = Some("hello \u{1b}]52;c;aGk=\u{7}\u{1b}[2J".into());
        d.task.name = "\u{1b}[31mred\u{1b}[0m".into();

        let text = render_text(&d, 1_794_834_000);

        assert!(!text.contains('\u{1b}'), "{text:?}");
        assert!(!text.chars().any(|c| c.is_control() && c != '\n'));
    }
}
