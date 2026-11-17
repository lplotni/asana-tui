//! `asn search` output.

use super::terminal_safe;
use crate::api::search::FoundTask;

/// One tab-separated line per task: `gid  completed-mark  name`.
pub fn render_text(tasks: &[FoundTask]) -> String {
    tasks
        .iter()
        .map(|t| {
            let mark = if t.completed { "✓" } else { " " };
            format!(
                "{}\t{mark}\t{}\n",
                terminal_safe(&t.gid),
                terminal_safe(&t.name)
            )
        })
        .collect()
}

/// The tasks as a JSON array.
pub fn render_json(tasks: &[FoundTask]) -> String {
    serde_json::to_string_pretty(tasks).expect("tasks always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::search::FoundTask;

    fn found(gid: &str, name: &str, completed: bool) -> FoundTask {
        FoundTask {
            gid: gid.into(),
            name: name.into(),
            completed,
        }
    }

    #[test]
    fn one_tab_separated_line_per_task_with_a_completed_mark() {
        let text = render_text(&[
            found("11", "Open one", false),
            found("12", "Done one", true),
        ]);

        assert_eq!(text, "11\t \tOpen one\n12\t✓\tDone one\n");
    }

    #[test]
    fn task_names_cannot_break_the_lines() {
        let text = render_text(&[found("11", "a\tb\nc\u{1b}[2J", false)]);

        assert_eq!(text.lines().count(), 1);
        assert!(!text.trim_end().chars().any(|c| c.is_control() && c != '\t'));
    }

    #[test]
    fn json_is_an_array_of_the_tasks() {
        let json: serde_json::Value =
            serde_json::from_str(&render_json(&[found("11", "Open one", false)])).unwrap();

        assert_eq!(json[0]["gid"], "11");
        assert_eq!(json[0]["completed"], false);
    }
}
