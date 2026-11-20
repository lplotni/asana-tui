//! Swimlanes (F25, F26): a board's columns split by the value of one of its
//! single-select fields. Pure, so the CLI and the TUI group the same way.

use super::resolve::{self, Unresolved};
use super::{Board, Column};
use crate::api::projects::{SelectField, SelectOption};
use crate::api::tasks::Named;

/// The cards of one option of the lane field, in the board's columns.
/// `option` is `None` for the lane of cards without a value.
#[derive(Debug, Clone, PartialEq)]
pub struct Lane {
    pub option: Option<Named>,
    pub columns: Vec<Column>,
}

/// One lane per option of the field `field_gid` that has cards, in the
/// field's option order, then the lane of cards without a (known) value.
/// Every lane has every column of the board, in board order, so lanes line
/// up; cards keep their board order within a column.
pub fn lanes(board: &Board, field_gid: &str) -> Vec<Lane> {
    let options: Vec<Named> = board
        .field(field_gid)
        .map(|f| f.options.iter().map(SelectOption::named).collect())
        .unwrap_or_default();
    // The lane a card belongs to: its option's lane, or the last one (none).
    let lane_of = |value: Option<&str>| {
        value
            .and_then(|gid| options.iter().position(|o| o.gid == gid))
            .unwrap_or(options.len())
    };
    let mut lanes: Vec<Lane> = options
        .iter()
        .cloned()
        .map(Some)
        .chain(std::iter::once(None))
        .map(|option| Lane {
            option,
            columns: board
                .columns
                .iter()
                .map(|c| Column {
                    section: c.section.clone(),
                    tasks: Vec::new(),
                })
                .collect(),
        })
        .collect();
    for (index, column) in board.columns.iter().enumerate() {
        for task in &column.tasks {
            lanes[lane_of(task.option_of(field_gid))].columns[index]
                .tasks
                .push(task.clone());
        }
    }
    lanes.retain(|lane| lane.columns.iter().any(|c| !c.tasks.is_empty()));
    lanes
}

/// The board's single-select field `arg` names: its GID, or its name (exact
/// ignoring case, else the only field containing it, as in F19).
pub fn lane_field<'a>(board: &'a Board, arg: &str) -> Result<&'a SelectField, Unresolved> {
    resolve::pick_local_by("field", arg, &board.fields, SelectField::named)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::projects::{BoardTask, FieldValue, OptionRef, SelectOption};

    const FIELD: &str = "1200000000000061";

    fn named(gid: &str, name: &str) -> Named {
        Named {
            gid: gid.into(),
            name: name.into(),
        }
    }

    fn option(gid: &str, name: &str) -> SelectOption {
        SelectOption {
            gid: gid.into(),
            name: name.into(),
            enabled: true,
        }
    }

    /// A card with `value` for the field (an option GID, or none).
    fn card(gid: &str, value: Option<&str>) -> BoardTask {
        BoardTask {
            gid: gid.into(),
            name: format!("Card {gid}"),
            assignee: None,
            due_on: None,
            due_at: None,
            memberships: Vec::new(),
            custom_fields: vec![FieldValue {
                gid: FIELD.into(),
                enum_value: value.map(|v| OptionRef { gid: v.into() }),
            }],
        }
    }

    fn initiative() -> SelectField {
        SelectField {
            gid: FIELD.into(),
            name: "Initiative".into(),
            options: vec![
                option("o1", "Checkout"),
                option("o2", "Onboarding"),
                option("o3", "Legacy"),
            ],
        }
    }

    /// Two columns: Backlog (a, b, c) and Done (d, e).
    fn board() -> Board {
        Board {
            project: named("p", "Web"),
            columns: vec![
                Column {
                    section: Some(named("s1", "Backlog")),
                    tasks: vec![
                        card("a", Some("o2")),
                        card("b", None),
                        card("c", Some("o1")),
                    ],
                },
                Column {
                    section: Some(named("s2", "Done")),
                    tasks: vec![card("d", Some("o1")), card("e", Some("o2"))],
                },
            ],
            fields: vec![initiative()],
        }
    }

    /// Each lane as (option name, [cards per column]).
    fn shape(lanes: &[Lane]) -> Vec<(&str, Vec<Vec<&str>>)> {
        lanes
            .iter()
            .map(|lane| {
                (
                    lane.option.as_ref().map_or("—", |o| o.name.as_str()),
                    lane.columns
                        .iter()
                        .map(|c| c.tasks.iter().map(|t| t.gid.as_str()).collect())
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn lanes_follow_the_option_order_then_cards_without_a_value() {
        let lanes = lanes(&board(), FIELD);

        assert_eq!(
            shape(&lanes),
            [
                ("Checkout", vec![vec!["c"], vec!["d"]]),
                ("Onboarding", vec![vec!["a"], vec!["e"]]),
                ("—", vec![vec!["b"], vec![]]),
            ]
        );
    }

    #[test]
    fn every_lane_has_every_column_in_board_order() {
        let lanes = lanes(&board(), FIELD);

        for lane in &lanes {
            let sections: Vec<_> = lane
                .columns
                .iter()
                .map(|c| c.section.as_ref().map(|s| s.name.as_str()))
                .collect();
            assert_eq!(sections, [Some("Backlog"), Some("Done")]);
        }
    }

    #[test]
    fn options_without_cards_get_no_lane() {
        let lanes = lanes(&board(), FIELD);

        assert!(
            lanes
                .iter()
                .all(|l| l.option.as_ref().is_none_or(|o| o.name != "Legacy"))
        );
    }

    #[test]
    fn a_value_that_is_not_an_option_of_the_field_counts_as_none() {
        let mut board = board();
        board.columns[0].tasks = vec![card("x", Some("o9"))];
        board.columns[1].tasks.clear();

        let lanes = lanes(&board, FIELD);

        assert_eq!(shape(&lanes), [("—", vec![vec!["x"], vec![]])]);
    }

    #[test]
    fn an_empty_board_has_no_lanes() {
        let mut board = board();
        for column in &mut board.columns {
            column.tasks.clear();
        }

        assert!(lanes(&board, FIELD).is_empty());
    }

    #[test]
    fn the_lane_field_is_found_by_name_in_any_case_or_by_gid() {
        let board = board();

        assert_eq!(lane_field(&board, "initiative").unwrap().gid, FIELD);
        assert_eq!(lane_field(&board, FIELD).unwrap().gid, FIELD);
    }

    #[test]
    fn an_unknown_lane_field_lists_the_boards_fields() {
        let mut board = board();
        board.fields.push(SelectField {
            gid: "1200000000000065".into(),
            name: "Priority".into(),
            options: Vec::new(),
        });

        let unresolved = lane_field(&board, "Team").unwrap_err();

        assert_eq!(unresolved.kind, "field");
        let names: Vec<_> = unresolved
            .candidates
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, ["Initiative", "Priority"]);
    }
}
