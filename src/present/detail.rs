//! The task view's content: everything the web task pane shows that the API
//! gives us, as wrapped lines of role-tagged text. The TUI colors them;
//! `asn task` prints them as plain text.

use unicode_width::UnicodeWidthStr;

use super::Role;
use super::format::{is_before_today, relative_time, short_date};
use crate::core::TaskDetail;

/// A run of text with one role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub role: Role,
}

impl Segment {
    pub fn new(text: impl Into<String>, role: Role) -> Segment {
        Segment {
            text: text.into(),
            role,
        }
    }
}

/// One display line.
pub type TextLine = Vec<Segment>;

/// The detail pane for `detail`, wrapped to `width` columns.
pub fn lines(detail: &TaskDetail, width: usize, now: i64) -> Vec<TextLine> {
    let task = &detail.task;
    let width = width.max(10);
    let mut out: Vec<TextLine> = Vec::new();
    let push = |text: &str, role: Role, out: &mut Vec<TextLine>| {
        for line in wrap(text, width) {
            out.push(vec![Segment::new(line, role)]);
        }
    };

    push(&task.name, Role::Title, &mut out);
    out.extend(status_lines(detail, width, now));
    for membership in &task.memberships {
        let place = match &membership.section {
            Some(section) => format!("{} › {}", membership.project.name, section.name),
            None => membership.project.name.clone(),
        };
        push(&place, Role::TaskMeta, &mut out);
    }
    if !task.tags.is_empty() {
        let tags: Vec<&str> = task.tags.iter().map(|t| t.name.as_str()).collect();
        push(
            &format!("Tags: {}", tags.join(", ")),
            Role::TaskMeta,
            &mut out,
        );
    }
    if let Some(parent) = &task.parent {
        push(
            &format!("Parent: {}", parent.name),
            Role::TaskMeta,
            &mut out,
        );
    }
    if task.num_subtasks > 0 {
        let text = format!("Subtasks: {} (o opens the task)", task.num_subtasks);
        push(&text, Role::TaskMeta, &mut out);
    }
    for field in &task.custom_fields {
        let value = field.display_value.as_deref().unwrap_or("—");
        push(&format!("{}: {value}", field.name), Role::Text, &mut out);
    }

    out.push(Vec::new());
    push("Description", Role::Title, &mut out);
    if task.notes.trim().is_empty() {
        push("No description", Role::TextMuted, &mut out);
    } else {
        push(task.notes.trim_end(), Role::Text, &mut out);
    }

    out.push(Vec::new());
    let mut header = vec![Segment::new(
        format!("Comments ({})", detail.comments.len()),
        Role::Title,
    )];
    if detail.activity_count > 0 {
        header.push(Segment::new(
            format!(" · {} updates not shown", detail.activity_count),
            Role::TextMuted,
        ));
    }
    out.push(header);
    for comment in &detail.comments {
        out.push(Vec::new());
        let author = comment
            .created_by
            .as_ref()
            .map_or("Asana", |a| a.name.as_str());
        out.push(vec![
            Segment::new(author.to_owned(), Role::CommentAuthor),
            Segment::new(
                format!(" · {}", relative_time(&comment.created_at, now)),
                Role::Timestamp,
            ),
        ]);
        let text = comment.text.as_deref().unwrap_or("").trim_end();
        for line in wrap(text, width.saturating_sub(2)) {
            out.push(vec![Segment::new(format!("  {line}"), Role::Text)]);
        }
    }
    out
}

/// "☐ Incomplete · Ada · Due Wed 4 Nov (overdue)", breaking between the
/// `·`-separated parts when it doesn't fit in `width`.
fn status_lines(detail: &TaskDetail, width: usize, now: i64) -> Vec<TextLine> {
    let task = &detail.task;
    let mut parts: Vec<Vec<Segment>> = vec![vec![if task.completed {
        Segment::new("✓ Completed", Role::ToastSuccess)
    } else {
        Segment::new("☐ Incomplete", Role::TaskMeta)
    }]];
    let assignee = task
        .assignee
        .as_ref()
        .map_or("Unassigned", |a| a.name.as_str());
    parts.push(vec![Segment::new(assignee.to_owned(), Role::TaskMeta)]);

    let due = task.due_at.as_deref().or(task.due_on.as_deref());
    let due_text = due.map(|d| {
        let date = short_date(d, now);
        // due_at is a UTC timestamp; show its time too.
        match d.get(11..16) {
            Some(time) if d.len() > 10 => format!("{date} {time} UTC"),
            _ => date,
        }
    });
    let dates = match (task.start_on.as_deref(), due_text) {
        (Some(start), Some(due)) => Some(format!("{} – {due}", short_date(start, now))),
        (None, Some(due)) => Some(format!("Due {due}")),
        (Some(start), None) => Some(format!("Starts {}", short_date(start, now))),
        (None, None) => None,
    };
    if let Some(dates) = dates {
        let mut part = vec![Segment::new(dates, Role::TaskMeta)];
        if !task.completed && due.is_some_and(|d| is_before_today(d, now)) {
            part.push(Segment::new(" (overdue)", Role::ToastError));
        }
        parts.push(part);
    }

    let mut lines: Vec<Vec<Segment>> = vec![Vec::new()];
    for part in parts {
        let part_width: usize = part.iter().map(|s| s.text.width()).sum();
        let current = lines.last_mut().expect("at least one line");
        let current_width: usize = current.iter().map(|s| s.text.width()).sum();
        if current.is_empty() {
            current.extend(part);
        } else if current_width + 3 + part_width <= width {
            current.push(Segment::new(" · ", Role::TaskMeta));
            current.extend(part);
        } else {
            lines.push(part);
        }
    }
    lines
}

/// Word-wraps `text` to `width` columns, keeping blank lines. Words longer
/// than `width` are split.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        // Leading indentation (e.g. code) is kept on every wrapped line.
        let words_start = paragraph.len() - paragraph.trim_start().len();
        let indent = paragraph[..words_start].replace('\t', "    ");
        let room = width.saturating_sub(indent.width()).max(1);
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            for piece in split_long(word, room) {
                let needed = if line.is_empty() {
                    piece.width()
                } else {
                    line.width() + 1 + piece.width()
                };
                if needed > room && !line.is_empty() {
                    out.push(format!("{indent}{}", std::mem::take(&mut line)));
                }
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(&piece);
            }
        }
        out.push(if line.is_empty() {
            line
        } else {
            format!("{indent}{line}")
        });
    }
    out
}

/// `word` cut into pieces no wider than `width`.
fn split_long(word: &str, width: usize) -> Vec<String> {
    if word.width() <= width {
        return vec![word.to_owned()];
    }
    let mut pieces = Vec::new();
    let mut piece = String::new();
    for c in word.chars() {
        if piece.width() + c.to_string().width() > width {
            pieces.push(std::mem::take(&mut piece));
        }
        piece.push(c);
    }
    pieces.push(piece);
    pieces
}

/// A line's text without styles.
pub fn plain(line: &TextLine) -> String {
    line.iter().map(|s| s.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tasks::{Task, TaskStory};
    use crate::present::format::parse_timestamp;

    fn now() -> i64 {
        parse_timestamp("2026-11-16T13:00:00.000Z").unwrap()
    }

    fn fixture(name: &str) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap())
            .unwrap()
    }

    fn detail(task_fixture: &str) -> TaskDetail {
        let task: Task = serde_json::from_value(fixture(task_fixture)["data"].clone()).unwrap();
        let stories: Vec<TaskStory> =
            serde_json::from_value(fixture("stories_thread.json")["data"].clone()).unwrap();
        let (comments, activity): (Vec<_>, Vec<_>) =
            stories.into_iter().partition(TaskStory::is_comment);
        TaskDetail {
            task,
            comments,
            activity_count: activity.len(),
        }
    }

    fn text(detail: &TaskDetail, width: usize) -> Vec<String> {
        lines(detail, width, now()).iter().map(plain).collect()
    }

    #[test]
    fn starts_with_the_task_name() {
        let d = detail("task_full.json");

        assert_eq!(text(&d, 60)[0], d.task.name);
    }

    #[test]
    fn status_line_has_completion_assignee_and_an_overdue_due_date() {
        let d = detail("task_full.json");

        assert_eq!(
            text(&d, 60)[1],
            "☐ Incomplete · Donald Knuth · Due Wed 4 Nov (overdue)"
        );
    }

    #[test]
    fn completed_tasks_are_never_overdue() {
        let mut d = detail("task_full_completed.json");
        d.task.due_on = Some("2026-11-04".into());
        d.task.assignee = None;

        assert_eq!(text(&d, 60)[1], "✓ Completed · Unassigned · Due Wed 4 Nov");
    }

    #[test]
    fn start_date_is_shown_as_a_range() {
        let mut d = detail("task_full.json");
        d.task.start_on = Some("2026-11-02".into());
        d.task.due_on = Some("2026-11-20".into());

        assert!(
            text(&d, 80)[1].ends_with("Mon 2 Nov – Fri 20 Nov"),
            "{}",
            text(&d, 80)[1]
        );
    }

    #[test]
    fn lists_every_project_with_its_section() {
        let lines = text(&detail("task_full.json"), 60);

        assert!(lines.contains(&"Stone maple zulu cedar › Maple ember".to_string()));
        assert!(lines.contains(&"Ember zulu › Mike maple".to_string()));
    }

    #[test]
    fn custom_fields_show_a_dash_when_empty() {
        let lines = text(&detail("task_full.json"), 60);

        assert!(lines.contains(&"Quebec river: —".to_string()), "{lines:#?}");
    }

    #[test]
    fn tags_parent_and_subtasks_appear_when_present() {
        let mut d = detail("task_full.json");
        d.task.tags = vec![
            crate::api::tasks::Named {
                gid: "1".into(),
                name: "backend".into(),
            },
            crate::api::tasks::Named {
                gid: "2".into(),
                name: "urgent".into(),
            },
        ];
        d.task.parent = Some(crate::api::tasks::Named {
            gid: "3".into(),
            name: "Launch".into(),
        });
        d.task.num_subtasks = 4;

        let lines = text(&d, 60);

        assert!(lines.contains(&"Tags: backend, urgent".to_string()));
        assert!(lines.contains(&"Parent: Launch".to_string()));
        assert!(lines.contains(&"Subtasks: 4 (o opens the task)".to_string()));
    }

    #[test]
    fn description_is_shown_in_full_and_wrapped() {
        let d = detail("task_full.json");
        let lines = text(&d, 40);

        let start = lines.iter().position(|l| l == "Description").unwrap() + 1;
        let words: Vec<String> = d.task.notes.split_whitespace().map(str::to_owned).collect();
        let shown: Vec<String> = lines[start..]
            .iter()
            .take_while(|l| !l.starts_with("Comments"))
            .flat_map(|l| l.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
            .collect();
        assert_eq!(shown, words);
        assert!(
            lines.iter().all(|l| l.width() <= 40),
            "a line is wider than 40"
        );
    }

    #[test]
    fn empty_description_says_so() {
        let mut d = detail("task_full.json");
        d.task.notes = String::new();

        assert!(text(&d, 60).contains(&"No description".to_string()));
    }

    #[test]
    fn every_comment_is_shown_with_author_and_time() {
        let d = detail("task_full.json");
        let lines = text(&d, 60);

        let header = format!(
            "Comments ({}) · {} updates not shown",
            d.comments.len(),
            d.activity_count
        );
        assert!(lines.contains(&header), "{lines:#?}");
        for c in &d.comments {
            let author = c.created_by.as_ref().unwrap().name.clone();
            let byline = format!("{author} · {}", relative_time(&c.created_at, now()));
            assert!(lines.contains(&byline), "missing {byline}");
        }
    }

    #[test]
    fn wrap_keeps_leading_indentation_on_every_line() {
        assert_eq!(wrap("  let x = 1;", 20), ["  let x = 1;"]);
        assert_eq!(
            wrap("    indented words that wrap", 16),
            ["    indented", "    words that", "    wrap"]
        );
    }

    #[test]
    fn wrap_breaks_on_words_and_keeps_blank_lines() {
        assert_eq!(
            wrap("one two three\n\nfour", 7),
            ["one two", "three", "", "four"]
        );
        assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap("", 10), [""]);
    }
}
