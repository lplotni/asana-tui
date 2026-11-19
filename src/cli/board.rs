//! `asn board` output.

use super::tsv_line;
use crate::core::lanes::Lane;
use crate::core::{Board, Column};

/// Each section (`gid  name`) followed by its tasks, one indented
/// tab-separated line each: `gid  name  assignee  due_on`. The column of
/// tasks without a known section has an empty gid.
pub fn render_text(board: &Board) -> String {
    columns_text(&board.columns, "")
}

/// Sections and their tasks, every line starting with `indent` (tabs, so
/// it reads as empty leading fields).
fn columns_text(columns: &[Column], indent: &str) -> String {
    let mut text = String::new();
    for column in columns {
        text += indent;
        text += &match &column.section {
            Some(section) => tsv_line(&[&section.gid, &section.name]),
            None => tsv_line(&["", "(no section)"]),
        };
        for task in &column.tasks {
            let assignee = task.assignee.as_ref().map_or("", |a| a.name.as_str());
            let due = task.due_on.as_deref().unwrap_or("");
            text += indent;
            text += &tsv_line(&["", &task.gid, &task.name, assignee, due]);
        }
    }
    text
}

/// `asn board --lanes`: per lane, `option-gid  option-name` (an empty gid
/// and `(no <Field>)` for cards without a value), then its sections and
/// tasks as in [`render_text`], one level deeper.
pub fn render_lanes(lanes: &[Lane], field: &str) -> String {
    let mut text = String::new();
    for lane in lanes {
        text += &match &lane.option {
            Some(option) => tsv_line(&[&option.gid, &option.name]),
            None => tsv_line(&["", &format!("(no {field})")]),
        };
        text += &columns_text(&lane.columns, "\t");
    }
    text
}

/// `{"sections": [...], "tasks": [...]}`: the project's sections and its
/// open tasks in board order, each task with its memberships.
pub fn render_json(board: &Board, lanes: Option<&[Lane]>) -> String {
    let sections: Vec<_> = board
        .columns
        .iter()
        .filter_map(|c| c.section.as_ref())
        .collect();
    let tasks: Vec<_> = board.columns.iter().flat_map(|c| &c.tasks).collect();
    let mut json = serde_json::json!({ "sections": sections, "tasks": tasks });
    if let Some(lanes) = lanes {
        json["lanes"] = lanes
            .iter()
            .map(|lane| {
                let sections: Vec<_> = lane
                    .columns
                    .iter()
                    .map(|c| {
                        let tasks: Vec<_> = c.tasks.iter().map(|t| t.gid.as_str()).collect();
                        serde_json::json!({
                            "section": c.section.as_ref().map(|s| s.gid.as_str()),
                            "tasks": tasks,
                        })
                    })
                    .collect();
                serde_json::json!({ "option": lane.option, "sections": sections })
            })
            .collect();
    }
    serde_json::to_string_pretty(&json).expect("a board always serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::projects::BoardTask;
    use crate::api::tasks::Named;
    use crate::core::lanes::Lane;

    fn named(gid: &str, name: &str) -> Named {
        Named {
            gid: gid.into(),
            name: name.into(),
        }
    }

    fn card(gid: &str, name: &str, assignee: Option<&str>, due_on: Option<&str>) -> BoardTask {
        BoardTask {
            custom_fields: Vec::new(),
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
            fields: Vec::new(),
            project: named("p", "Project"),
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

    /// The test board in lanes: "Checkout" has t1 in To do; the no-value
    /// lane has t2 in To do.
    fn lanes() -> Vec<Lane> {
        let mut board = board();
        let mut checkout = board.columns.clone();
        checkout[0].tasks.truncate(1);
        board.columns[0].tasks.remove(0);
        vec![
            Lane {
                option: Some(named("o1", "Checkout")),
                columns: checkout,
            },
            Lane {
                option: None,
                columns: board.columns,
            },
        ]
    }

    #[test]
    fn lanes_nest_each_lanes_sections_and_tasks_one_level_deeper() {
        let text = render_lanes(&lanes(), "Initiative");

        assert_eq!(
            text,
            "o1\tCheckout\n\
             \ts1\tTo do\n\
             \t\tt1\tWrite spec\tAda Lovelace\t2026-11-20\n\
             \ts2\tDone\n\
             \t(no Initiative)\n\
             \ts1\tTo do\n\
             \t\tt2\tPlan\t\t\n\
             \ts2\tDone\n"
        );
    }

    #[test]
    fn json_with_lanes_lists_each_lanes_option_and_task_gids_per_section() {
        let json: serde_json::Value =
            serde_json::from_str(&render_json(&board(), Some(&lanes()))).unwrap();

        assert_eq!(
            json["lanes"],
            serde_json::json!([
                { "option": { "gid": "o1", "name": "Checkout" }, "sections": [
                    { "section": "s1", "tasks": ["t1"] },
                    { "section": "s2", "tasks": [] }
                ] },
                { "option": null, "sections": [
                    { "section": "s1", "tasks": ["t2"] },
                    { "section": "s2", "tasks": [] }
                ] }
            ])
        );
    }

    #[test]
    fn json_without_lanes_has_no_lanes_key() {
        let json: serde_json::Value = serde_json::from_str(&render_json(&board(), None)).unwrap();

        assert!(json.get("lanes").is_none(), "{json}");
    }

    #[test]
    fn the_no_section_column_has_an_empty_gid() {
        let board = Board {
            fields: Vec::new(),
            project: named("p", "Project"),
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
            fields: Vec::new(),
            project: named("p", "Project"),
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
    fn json_tasks_include_their_single_select_values() {
        let mut board = board();
        board.columns[0].tasks[0].custom_fields = vec![crate::api::projects::FieldValue {
            gid: "1200000000000061".into(),
            enum_value: Some(crate::api::projects::OptionRef {
                gid: "1200000000000062".into(),
            }),
        }];

        let json: serde_json::Value = serde_json::from_str(&render_json(&board, None)).unwrap();

        assert_eq!(
            json["tasks"][0]["custom_fields"],
            serde_json::json!([
                { "gid": "1200000000000061", "enum_value": { "gid": "1200000000000062" } }
            ])
        );
    }

    #[test]
    fn json_lists_the_sections_and_every_task_in_board_order() {
        let json: serde_json::Value = serde_json::from_str(&render_json(&board(), None)).unwrap();

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
            fields: Vec::new(),
            project: named("p", "Project"),
            columns: vec![Column {
                section: None,
                tasks: vec![card("t1", "Loose", None, None)],
            }],
        };

        let json: serde_json::Value = serde_json::from_str(&render_json(&board, None)).unwrap();

        assert_eq!(json["sections"], serde_json::json!([]));
        assert_eq!(json["tasks"][0]["gid"], "t1");
    }
}
