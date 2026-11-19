//! The search-as-you-type picker: "add to project" (search projects, then
//! choose a section), "move to section" (choose one of the task's projects,
//! then a section), workspace task search, and opening a project's board. Pure state; the
//! [`App`](super::app::App) turns its decisions into effects.

use crate::api::search::FoundTask;
use crate::api::tasks::Named;

/// How long typing must pause before the project search runs.
pub const SEARCH_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(200);

/// One row of the picker. A `None` gid is the "No section" choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub gid: Option<String>,
    pub name: String,
    /// Drawn with ✓: a completed task, or the section a task is in.
    pub checked: bool,
    /// What it is, drawn after the name: "person", "project" or "task".
    pub note: Option<&'static str>,
}

impl From<Named> for Item {
    fn from(named: Named) -> Item {
        Item {
            gid: Some(named.gid),
            name: named.name,
            checked: false,
            note: None,
        }
    }
}

impl Item {
    /// The project, section or task this row stands for. Only "No section"
    /// has no gid, and it's never chosen as one of those.
    pub fn into_named(self) -> Named {
        Named {
            gid: self.gid.unwrap_or_default(),
            name: self.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    Project,
    Section {
        project: Named,
    },
    /// Finding a task in the workspace to show in the task pane.
    Task,
    /// Choosing which of the task's projects to move it within.
    MoveProject,
    /// Choosing the section of `project` to move the task to; `current` is
    /// the section it's in now.
    MoveSection {
        project: Named,
        current: Option<String>,
    },
    /// Choosing a project to open as a board: the boards opened this
    /// session while the query is empty, else projects found.
    Board,
    /// Choosing which of the task's projects to open as a board, on it.
    TaskBoard,
    /// Choosing whom (or which project or task) to @-mention in the editor.
    Mention,
    /// Choosing whom to assign the task to: "Me" and "Unassigned" while the
    /// query is empty, else people found.
    Assign,
    /// Choosing which of the task's single-select fields to set (F27).
    SetFieldChoice,
    /// Choosing the option of `field` to set, or "(none)" (no gid).
    SetFieldOption {
        field: Named,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    /// The task being added to a project (empty when searching tasks).
    pub task_gid: String,
    pub stage: Stage,
    pub query: String,
    pub items: Vec<Item>,
    pub selected: usize,
    pub loading: bool,
    /// Increases with every query change; only the latest search counts.
    pub search_seq: u64,
    /// Projects the task is already in; never offered.
    exclude: Vec<String>,
    /// Boards opened this session, offered while the query is empty.
    recents: Vec<Named>,
}

impl Picker {
    pub fn new(task_gid: String, exclude: Vec<String>) -> Picker {
        Picker {
            task_gid,
            stage: Stage::Project,
            query: String::new(),
            items: Vec::new(),
            selected: 0,
            loading: true,
            search_seq: 0,
            exclude,
            recents: Vec::new(),
        }
    }

    /// Opening a board: `recents` (most recent first) until a query is typed.
    pub fn boards(recents: Vec<Named>) -> Picker {
        let mut picker = Picker {
            stage: Stage::Board,
            recents,
            ..Picker::new(String::new(), Vec::new())
        };
        picker.show_recents();
        picker
    }

    /// Assigning `task_gid`: "Me" and "Unassigned" until a name is typed.
    pub fn assignees(task_gid: String) -> Picker {
        let mut picker = Picker {
            stage: Stage::Assign,
            loading: false,
            ..Picker::new(task_gid, Vec::new())
        };
        picker.show_assign_defaults();
        picker
    }

    /// "Me" (gid `me`, which Asana understands) and "Unassigned" (no gid).
    pub fn show_assign_defaults(&mut self) {
        let item = |gid: Option<&str>, name: &str| Item {
            gid: gid.map(str::to_owned),
            name: name.to_owned(),
            checked: false,
            note: None,
        };
        self.items = vec![item(Some("me"), "Me"), item(None, "Unassigned")];
        self.selected = 0;
        self.loading = false;
    }

    /// Takes the people found by search `seq`, unless a newer one was
    /// started.
    pub fn accept_people(&mut self, seq: u64, people: Vec<Named>) -> bool {
        if self.stage != Stage::Assign || seq != self.search_seq {
            return false;
        }
        self.items = people.into_iter().map(Item::from).collect();
        self.selected = 0;
        self.loading = false;
        true
    }

    /// @-mentioning in the editor: waits for the first letters of a name.
    pub fn mentions() -> Picker {
        Picker {
            stage: Stage::Mention,
            loading: false,
            ..Picker::new(String::new(), Vec::new())
        }
    }

    /// Takes the people, projects and tasks found by search `seq`, unless a
    /// newer one was started.
    pub fn accept_mentions(
        &mut self,
        seq: u64,
        targets: Vec<crate::core::mention::Target>,
    ) -> bool {
        if self.stage != Stage::Mention || seq != self.search_seq {
            return false;
        }
        self.items = targets
            .into_iter()
            .map(|t| Item {
                gid: Some(t.gid),
                name: t.name,
                checked: false,
                note: Some(t.kind),
            })
            .collect();
        self.selected = 0;
        self.loading = false;
        true
    }

    /// Opening the board of `task_gid`: choose among `projects` (its own).
    pub fn task_board(task_gid: String, projects: Vec<Named>) -> Picker {
        Picker {
            stage: Stage::TaskBoard,
            loading: false,
            items: projects.into_iter().map(Item::from).collect(),
            ..Picker::new(task_gid, Vec::new())
        }
    }

    /// Whether there are recent boards to show for an empty query.
    pub fn has_recents(&self) -> bool {
        !self.recents.is_empty()
    }

    /// Lists the recent boards (for an empty query).
    pub fn show_recents(&mut self) {
        self.items = self.recents.iter().cloned().map(Item::from).collect();
        self.selected = 0;
        self.loading = false;
    }

    /// Workspace task search, waiting for the first words.
    pub fn tasks() -> Picker {
        Picker {
            stage: Stage::Task,
            loading: false,
            ..Picker::new(String::new(), Vec::new())
        }
    }

    /// Moving `task_gid`: choose among `projects` (the task's own).
    pub fn move_within(task_gid: String, projects: Vec<Named>) -> Picker {
        Picker {
            stage: Stage::MoveProject,
            loading: false,
            items: projects.into_iter().map(Item::from).collect(),
            ..Picker::new(task_gid, Vec::new())
        }
    }

    /// Moving `task_gid` within `project`, from section `current`; the
    /// sections are loading.
    pub fn move_to_section(task_gid: String, project: Named, current: Option<String>) -> Picker {
        Picker {
            stage: Stage::MoveSection { project, current },
            ..Picker::new(task_gid, Vec::new())
        }
    }

    /// Setting one of `task_gid`'s single-select fields: choose among
    /// `fields` (each named `Field: current option`).
    pub fn fields(task_gid: String, fields: Vec<Item>) -> Picker {
        Picker {
            stage: Stage::SetFieldChoice,
            loading: false,
            items: fields,
            ..Picker::new(task_gid, Vec::new())
        }
    }

    /// Setting `field` of `task_gid`: "(none)" (no gid), then `options`.
    pub fn field_options(task_gid: String, field: Named, options: Vec<Item>) -> Picker {
        let none = Item {
            gid: None,
            name: "(none)".into(),
            checked: !options.iter().any(|o| o.checked),
            note: None,
        };
        Picker {
            stage: Stage::SetFieldOption { field },
            loading: false,
            items: std::iter::once(none).chain(options).collect(),
            ..Picker::new(task_gid, Vec::new())
        }
    }

    /// Whether the query is typed (rather than a list chosen from).
    pub fn has_query(&self) -> bool {
        matches!(
            self.stage,
            Stage::Project | Stage::Task | Stage::Board | Stage::Mention | Stage::Assign
        )
    }

    /// Changes the query; returns the new search number to schedule.
    pub fn set_query(&mut self, query: String) -> u64 {
        self.query = query;
        self.search_seq += 1;
        self.search_seq
    }

    /// Takes the results of search `seq`, unless a newer one was started.
    pub fn accept_projects(&mut self, seq: u64, projects: Vec<Named>) -> bool {
        if !matches!(self.stage, Stage::Project | Stage::Board) || seq != self.search_seq {
            return false;
        }
        self.items = projects
            .into_iter()
            .filter(|p| !self.exclude.contains(&p.gid))
            .map(Item::from)
            .collect();
        self.selected = 0;
        self.loading = false;
        true
    }

    /// Takes the tasks found by search `seq`, unless a newer one was started.
    pub fn accept_tasks(&mut self, seq: u64, tasks: Vec<FoundTask>) -> bool {
        if self.stage != Stage::Task || seq != self.search_seq {
            return false;
        }
        self.items = tasks
            .into_iter()
            .map(|t| Item {
                gid: Some(t.gid),
                name: t.name,
                checked: t.completed,
                note: None,
            })
            .collect();
        self.selected = 0;
        self.loading = false;
        true
    }

    /// Moves to choosing a section of `project`.
    pub fn choose_project(&mut self, project: Named) {
        self.stage = Stage::Section { project };
        self.items.clear();
        self.selected = 0;
        self.loading = true;
    }

    /// Takes the sections of `project_gid`. Adding to a project offers
    /// "No section" first; moving marks the section the task is in.
    pub fn accept_sections(&mut self, project_gid: &str, sections: Vec<Named>) -> bool {
        match &self.stage {
            Stage::Section { project } if project.gid == project_gid => {}
            Stage::MoveSection { project, current } if project.gid == project_gid => {
                self.items = sections
                    .into_iter()
                    .map(|s| Item {
                        checked: current.as_ref() == Some(&s.gid),
                        gid: Some(s.gid),
                        name: s.name,
                        note: None,
                    })
                    .collect();
                self.selected = 0;
                self.loading = false;
                return true;
            }
            _ => return false,
        }
        self.items = std::iter::once(Item {
            gid: None,
            name: "No section".into(),
            checked: false,
            note: None,
        })
        .chain(sections.into_iter().map(Item::from))
        .collect();
        self.selected = 0;
        self.loading = false;
        true
    }

    pub fn move_by(&mut self, delta: isize) {
        let last = self.items.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    pub fn current(&self) -> Option<&Item> {
        self.items.get(self.selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, Effect, Msg, Sizes};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn named(gid: &str, name: &str) -> Named {
        Named {
            gid: gid.into(),
            name: name.into(),
        }
    }

    /// An app with one unread notification about task "t0", whose details
    /// say it's already in project "p-in".
    fn app() -> App {
        let text = std::fs::read_to_string("tests/fixtures/notifications_page.json").unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut n: crate::api::notifications::Notification =
            serde_json::from_value(value["data"][0].clone()).unwrap();
        n.work_objects[0].gid = "t0".into();
        let task_text = std::fs::read_to_string("tests/fixtures/task_full.json").unwrap();
        let mut task: crate::api::tasks::Task = serde_json::from_value(
            serde_json::from_str::<serde_json::Value>(&task_text).unwrap()["data"].clone(),
        )
        .unwrap();
        task.memberships = vec![crate::api::tasks::Membership {
            project: named("p-in", "Already"),
            section: None,
        }];

        let (mut app, _) = App::new();
        app.update(Msg::Resize(Sizes {
            list_rows: 10,
            detail_width: 60,
            detail_rows: 20,
            ..Sizes::default()
        }));
        app.update(Msg::InboxLoaded(Ok((vec![n], None))));
        app.update(Msg::DetailDue { gid: "t0".into() });
        app.update(Msg::DetailLoaded {
            gid: "t0".into(),
            result: Ok(Box::new(crate::core::TaskDetail {
                task,
                comments: vec![],
                activity_count: 0,
                attachments: Vec::new(),
                comments_error: None,
            })),
        });
        app
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Vec<Effect> {
        app.update(Msg::Key(KeyEvent::new(code, modifiers)))
    }

    fn typed(app: &mut App, text: &str) -> Vec<Effect> {
        text.chars()
            .flat_map(|c| press(app, KeyCode::Char(c), KeyModifiers::NONE))
            .collect()
    }

    fn found(app: &mut App, seq: u64, projects: Vec<Named>) {
        app.update(Msg::ProjectsFound {
            seq,
            result: Ok(projects),
        });
    }

    #[test]
    fn p_opens_the_picker_and_schedules_a_search_for_recent_projects() {
        let mut app = app();

        let effects = typed(&mut app, "p");

        assert!(app.picker.is_some());
        assert_eq!(effects, [Effect::ScheduleSearch { seq: 1 }]);
    }

    #[test]
    fn typing_quickly_searches_once_for_the_final_query() {
        let mut app = app();
        typed(&mut app, "p");

        let scheduled = typed(&mut app, "web");
        let searches: Vec<Effect> = (1..=4)
            .flat_map(|seq| app.update(Msg::SearchDue { seq }))
            .collect();

        assert_eq!(scheduled.len(), 3, "one schedule per keystroke");
        assert_eq!(
            searches,
            [Effect::SearchProjects {
                query: "web".into(),
                seq: 4
            }]
        );
        assert!(app.picker.as_ref().unwrap().loading);
    }

    #[test]
    fn letters_are_typed_into_the_query_not_used_as_commands() {
        let mut app = app();
        typed(&mut app, "p");

        typed(&mut app, "jkq");

        assert_eq!(app.picker.as_ref().unwrap().query, "jkq");
        assert!(!app.should_quit);
    }

    #[test]
    fn a_late_response_for_an_older_query_is_ignored() {
        let mut app = app();
        typed(&mut app, "pwe");
        typed(&mut app, "b");
        found(&mut app, 4, vec![named("1", "Webshop")]);

        found(&mut app, 3, vec![named("2", "Wedding planning")]);

        let names: Vec<_> = app
            .picker
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|i| i.name.clone())
            .collect();
        assert_eq!(names, ["Webshop"]);
    }

    #[test]
    fn projects_the_task_is_already_in_are_never_offered() {
        let mut app = app();
        typed(&mut app, "p");

        found(
            &mut app,
            1,
            vec![named("p-in", "Already"), named("p-new", "New")],
        );

        let gids: Vec<_> = app
            .picker
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|i| i.gid.clone())
            .collect();
        assert_eq!(gids, [Some("p-new".to_string())]);
    }

    #[test]
    fn ctrl_n_ctrl_p_and_arrows_move_the_selection() {
        let mut app = app();
        typed(&mut app, "p");
        found(
            &mut app,
            1,
            vec![named("1", "A"), named("2", "B"), named("3", "C")],
        );

        press(&mut app, KeyCode::Char('n'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.picker.as_ref().unwrap().selected, 2);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.picker.as_ref().unwrap().selected, 2, "stops at the end");
        press(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.picker.as_ref().unwrap().selected, 0);
    }

    #[test]
    fn enter_on_a_project_loads_its_sections() {
        let mut app = app();
        typed(&mut app, "p");
        found(&mut app, 1, vec![named("42", "Webshop")]);

        let effects = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!(
            effects,
            [Effect::LoadSections {
                project_gid: "42".into()
            }]
        );
        assert_eq!(
            app.picker.as_ref().unwrap().stage,
            Stage::Section {
                project: named("42", "Webshop")
            }
        );
    }

    #[test]
    fn sections_are_offered_after_no_section() {
        let mut app = app();
        typed(&mut app, "p");
        found(&mut app, 1, vec![named("42", "Webshop")]);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

        app.update(Msg::SectionsLoaded {
            project_gid: "42".into(),
            result: Ok(vec![named("43", "To do")]),
        });

        let items = &app.picker.as_ref().unwrap().items;
        assert_eq!(
            items[0],
            Item {
                gid: None,
                name: "No section".into(),
                checked: false,
                note: None,
            }
        );
        assert_eq!(items[1].gid.as_deref(), Some("43"));
    }

    #[test]
    fn choosing_a_section_adds_the_task_and_shows_the_project_at_once() {
        let mut app = app();
        typed(&mut app, "p");
        found(&mut app, 1, vec![named("42", "Webshop")]);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        app.update(Msg::SectionsLoaded {
            project_gid: "42".into(),
            result: Ok(vec![named("43", "To do")]),
        });
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);

        let effects = press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!(
            effects,
            [Effect::AddToProject {
                task_gid: "t0".into(),
                project: named("42", "Webshop"),
                section: Some(named("43", "To do")),
            }]
        );
        assert!(app.picker.is_none());
        let memberships = match app.selected_detail() {
            Some(crate::tui::app::Detail::Loaded(d)) => d.task.memberships.clone(),
            _ => panic!("detail missing"),
        };
        assert_eq!(memberships.len(), 2);
        assert_eq!(memberships[1].project.gid, "42");
    }

    #[test]
    fn a_failed_add_removes_the_project_again_and_says_so() {
        let mut app = app();
        typed(&mut app, "p");
        found(&mut app, 1, vec![named("42", "Webshop")]);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        app.update(Msg::SectionsLoaded {
            project_gid: "42".into(),
            result: Ok(vec![]),
        });
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

        app.update(Msg::AddedToProject {
            task_gid: "t0".into(),
            project_gid: "42".into(),
            result: Err("Forbidden".into()),
        });

        let memberships = match app.selected_detail() {
            Some(crate::tui::app::Detail::Loaded(d)) => d.task.memberships.clone(),
            _ => panic!("detail missing"),
        };
        assert_eq!(memberships.len(), 1);
        assert!(app.toast.as_ref().unwrap().is_error);
    }

    #[test]
    fn esc_closes_the_picker() {
        let mut app = app();
        typed(&mut app, "p");

        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

        assert!(app.picker.is_none());
    }

    #[test]
    fn backspace_edits_the_query_and_searches_again() {
        let mut app = app();
        typed(&mut app, "pab");

        let effects = press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);

        assert_eq!(app.picker.as_ref().unwrap().query, "a");
        assert_eq!(effects, [Effect::ScheduleSearch { seq: 4 }]);
    }
}
