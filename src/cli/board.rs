//! `asn board` output.

use super::tsv_line;
use crate::core::Board;

/// Each section (`gid  name`) followed by its tasks, one indented
/// tab-separated line each: `gid  name  assignee  due_on`. The column of
/// tasks without a known section has an empty gid.
pub fn render_text(board: &Board) -> String {
    let mut text = String::new();
    for column in &board.columns {
        text += &match &column.section {
            Some(section) => tsv_line(&[&section.gid, &section.name]),
            None => tsv_line(&["", "(no section)"]),
        };
        for task in &column.tasks {
            let assignee = task.assignee.as_ref().map_or("", |a| a.name.as_str());
            let due = task.due_on.as_deref().unwrap_or("");
            text += &tsv_line(&["", &task.gid, &task.name, assignee, due]);
        }
    }
    text
}

/// `{"sections": [...], "tasks": [...]}`: the project's sections and its
/// open tasks in board order, each task with its memberships.
pub fn render_json(board: &Board) -> String {
    let sections: Vec<_> = board
        .columns
        .iter()
        .filter_map(|c| c.section.as_ref())
        .collect();
    let tasks: Vec<_> = board.columns.iter().flat_map(|c| &c.tasks).collect();
    serde_json::to_string_pretty(&serde_json::json!({ "sections": sections, "tasks": tasks }))
        .expect("a board always serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::projects::BoardTask;
    use crate::api::tasks::Named;
    use crate::core::Column;

    fn named(gid: &str, name: &str) -> Named {
        Named {
            gid: gid.into(),
            name: name.into(),
        }
    }

    fn card(gid: &str, name: &str, assignee: Option<&str>, due_on: Option<&str>) -> BoardTask {
        BoardTask {
            gid: gid.into(),
            name: name.into(),
            assignee: assignee.map(|a| named("u", a)),
            due_on: due_on.map(Into::into),
            due_at: None,
            memberships: Vec::new(),
        }
    }

    fn board() -> Board {
        Board {
            columns: vec![
                Column {
                    section: Some(named("s1", "To do")),
                    tasks: vec![
                        card("t1", "Write spec", Some("Ada Lovelace"), Some("2026-11-20")),
                        card("t2", "Plan", None, None),
                    ],
                },
                Column {
                    section: Some(named("s2", "Done")),
                    tasks: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn each_section_is_followed_by_its_tasks_as_indented_tab_separated_lines() {
        let text = render_text(&board());

        assert_eq!(
            text,
            "s1\tTo do\n\
             \tt1\tWrite spec\tAda Lovelace\t2026-11-20\n\
             \tt2\tPlan\t\t\n\
             s2\tDone\n"
        );
    }

    #[test]
    fn the_no_section_column_has_an_empty_gid() {
        let board = Board {
            columns: vec![Column {
                section: None,
                tasks: vec![card("t1", "Loose", None, None)],
            }],
        };

        assert_eq!(render_text(&board), "\t(no section)\n\tt1\tLoose\t\t\n");
    }

    #[test]
    fn task_names_cannot_break_the_lines() {
        let board = Board {
            columns: vec![Column {
                section: Some(named("s1", "A\nB")),
                tasks: vec![card("t1", "a\tb\nc\u{1b}[2J", None, None)],
            }],
        };

        let text = render_text(&board);

        assert_eq!(text.lines().count(), 2);
        assert!(
            !text
                .chars()
                .any(|c| c.is_control() && c != '\t' && c != '\n')
        );
    }

    #[test]
    fn json_lists_the_sections_and_every_task_in_board_order() {
        let json: serde_json::Value = serde_json::from_str(&render_json(&board())).unwrap();

        assert_eq!(
            json["sections"],
            serde_json::json!([
                { "gid": "s1", "name": "To do" },
                { "gid": "s2", "name": "Done" }
            ])
        );
        let gids: Vec<&str> = json["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["gid"].as_str().unwrap())
            .collect();
        assert_eq!(gids, ["t1", "t2"]);
        assert_eq!(json["tasks"][0]["assignee"]["name"], "Ada Lovelace");
    }

    #[test]
    fn json_has_no_section_entry_for_the_no_section_column() {
        let board = Board {
            columns: vec![Column {
                section: None,
                tasks: vec![card("t1", "Loose", None, None)],
            }],
        };

        let json: serde_json::Value = serde_json::from_str(&render_json(&board)).unwrap();

        assert_eq!(json["sections"], serde_json::json!([]));
        assert_eq!(json["tasks"][0]["gid"], "t1");
    }
}
