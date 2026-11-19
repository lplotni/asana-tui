//! Renders the README screenshots: the real TUI with made-up data, as SVG.
//!
//!     cargo run --example screenshot > docs/screenshot.svg
//!     cargo run --example screenshot -- board > docs/board.svg
//!     cargo run --example screenshot -- lanes > docs/lanes.svg

use asn::api::notifications::{Actor, Notification, Story, WorkObject};
use asn::api::projects::{BoardTask, FieldValue, OptionRef, SelectField, SelectOption};
use asn::api::tasks::{CustomField, Membership, Named, Task, TaskStory};
use asn::core::{Board, Column, TaskDetail};
use asn::present::format;
use asn::tui::app::{App, Msg};
use asn::tui::theme::{Role, Theme};
use asn::tui::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

const WIDTH: u16 = 100;
const HEIGHT: u16 = 24;
const NOW: &str = "2026-11-16T13:00:00.000Z";

const CELL_W: f32 = 8.4;
const CELL_H: f32 = 16.8;
const FONT_SIZE: f32 = 14.0;
const PAD: f32 = 16.0;

fn main() {
    let theme = Theme::builtin("dark").expect("built-in theme");
    let now = format::parse_timestamp(NOW).expect("valid timestamp");

    let app = match std::env::args().nth(1).as_deref() {
        Some("board") => board_app(),
        Some("lanes") => lanes_app(),
        _ => inbox_app(),
    };

    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("test backend");
    terminal
        .draw(|frame| {
            ui::render(frame, &app, &theme, now);
        })
        .expect("draw");
    print!("{}", svg(terminal.backend().buffer(), &theme));
}

/// The Inbox with the first task's details.
fn inbox_app() -> App {
    let (mut app, _) = App::new();
    app.update(Msg::Resize(ui::sizes(Rect::new(0, 0, WIDTH, HEIGHT))));
    app.update(Msg::InboxLoaded(Ok((demo_inbox(), None))));
    let gid = app.selected_task_gid().expect("a task").to_owned();
    app.update(Msg::DetailDue { gid: gid.clone() });
    app.update(Msg::DetailLoaded {
        gid,
        result: Ok(Box::new(demo_task())),
    });
    app
}

/// A project board, as opened with `asn --project`.
fn board_app() -> App {
    let (mut app, _) = App::new_on_board("5000000000000000".into());
    app.update(Msg::Resize(ui::sizes(Rect::new(0, 0, WIDTH, HEIGHT))));
    app.update(Msg::BoardLoaded {
        project_gid: "5000000000000000".into(),
        result: Ok(demo_board()),
    });
    app
}

/// The project board in lanes by Initiative, as after pressing `v`.
fn lanes_app() -> App {
    let (mut app, _) = App::new_on_board("5000000000000000".into());
    app.update(Msg::Resize(ui::sizes(Rect::new(0, 0, WIDTH, HEIGHT))));
    app.update(Msg::BoardLoaded {
        project_gid: "5000000000000000".into(),
        result: Ok(demo_lanes_board()),
    });
    app.update(Msg::Key(ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Char('v'),
        ratatui::crossterm::event::KeyModifiers::NONE,
    )));
    app
}

/// The demo board with a fictional Initiative field: Performance, Brand,
/// and two cards without one.
fn demo_lanes_board() -> Board {
    let option = |gid: &str, name: &str| SelectOption {
        gid: gid.into(),
        name: name.into(),
        enabled: true,
    };
    let mut board = demo_board();
    board.fields = vec![SelectField {
        gid: "5000000000000010".into(),
        name: "Initiative".into(),
        options: vec![
            option("5000000000000011", "Performance"),
            option("5000000000000012", "Brand"),
        ],
    }];
    for column in &mut board.columns {
        for task in &mut column.tasks {
            let value = match task.gid.as_str() {
                "52" | "55" | "58" => Some("5000000000000011"),
                "51" | "56" | "57" => Some("5000000000000012"),
                _ => None,
            };
            task.custom_fields = vec![FieldValue {
                gid: "5000000000000010".into(),
                enum_value: value.map(|v| OptionRef { gid: v.into() }),
            }];
        }
    }
    board
}

/// A fictional project: four sections, so one is paged off to the right.
fn demo_board() -> Board {
    let named = |gid: &str, name: &str| Named {
        gid: gid.into(),
        name: name.into(),
    };
    let card = |gid: &str, name: &str, assignee: Option<&str>, due: Option<&str>| BoardTask {
        custom_fields: Vec::new(),
        gid: gid.into(),
        name: name.into(),
        assignee: assignee.map(|a| named("2", a)),
        due_on: due.map(Into::into),
        due_at: None,
        memberships: Vec::new(),
    };
    let column = |gid: &str, name: &str, tasks: Vec<BoardTask>| Column {
        section: Some(named(gid, name)),
        tasks,
    };
    Board {
        fields: Vec::new(),
        project: named("5000000000000000", "Website relaunch"),
        columns: vec![
            column(
                "5000000000000001",
                "Backlog",
                vec![
                    card("51", "Pick a font pairing for headings", None, None),
                    card(
                        "52",
                        "Audit image sizes",
                        Some("Grace Hopper"),
                        Some("2026-11-27"),
                    ),
                    card("53", "Dark mode", None, None),
                    card("54", "Localize the footer", Some("Alan Turing"), None),
                ],
            ),
            column(
                "5000000000000002",
                "In progress",
                vec![
                    card(
                        "55",
                        "Rewrite the pricing page copy",
                        Some("Ada Lovelace"),
                        Some("2026-11-12"),
                    ),
                    card(
                        "56",
                        "Cookie banner",
                        Some("Alan Turing"),
                        Some("2026-11-20"),
                    ),
                ],
            ),
            column(
                "5000000000000003",
                "Review",
                vec![card(
                    "57",
                    "New hero illustration",
                    Some("Grace Hopper"),
                    Some("2026-11-18"),
                )],
            ),
            column(
                "5000000000000004",
                "Done this week",
                vec![card("58", "Favicon", Some("Ada Lovelace"), None)],
            ),
        ],
    }
}

/// The selected task: fictional, like everything else here.
fn demo_task() -> TaskDetail {
    let named = |gid: &str, name: &str| Named {
        gid: gid.into(),
        name: name.into(),
    };
    let comment = |gid: &str, who: &str, when: &str, text: &str| TaskStory {
        gid: gid.into(),
        kind: "comment".into(),
        resource_subtype: Some("comment_added".into()),
        html_text: None,
        text: Some(text.into()),
        created_at: when.into(),
        created_by: Some(named("2", who)),
        reaction_summary: Vec::new(),
    };
    TaskDetail {
        task: Task {
            gid: "4000000000000000".into(),
            name: "Fix checkout redirect loop".into(),
            completed: false,
            completed_at: None,
            assignee: Some(named("1", "Sam Chen")),
            due_on: Some("2026-11-18".into()),
            due_at: None,
            start_on: None,
            notes: "Users on Safari get stuck between /cart and /checkout after logging in.\n\n\
                    Repro: log out, add an item, log in on the checkout page."
                .into(),
            html_notes: None,
            permalink_url: "https://app.asana.com/".into(),
            modified_at: None,
            memberships: vec![Membership {
                project: named("5", "Webshop"),
                section: Some(named("6", "In progress")),
            }],
            tags: vec![named("7", "bug")],
            custom_fields: vec![CustomField {
                gid: "8".into(),
                name: "Priority".into(),
                display_value: Some("High".into()),
                resource_subtype: String::new(),
                enum_value: None,
                enum_options: Vec::new(),
            }],
            parent: None,
            num_subtasks: 0,
        },
        comments: vec![
            comment(
                "9",
                "Sam Chen",
                "2026-11-15T10:00:00.000Z",
                "Can reproduce on iOS too.",
            ),
            comment(
                "10",
                "Alex Rivera",
                "2026-11-16T12:58:00.000Z",
                "Looks like the session cookie is SameSite=Strict. PR incoming.",
            ),
        ],
        activity_count: 6,
        attachments: Vec::new(),
        comments_error: None,
    }
}

/// Actor (`None` = Asana), subtype, task, read, when, comment.
type DemoItem = (
    Option<&'static str>,
    &'static str,
    &'static str,
    bool,
    &'static str,
    Option<&'static str>,
);

/// Fictional people and tasks; nothing here comes from a real Inbox.
fn demo_inbox() -> Vec<Notification> {
    let items: [DemoItem; 9] = [
        (
            Some("Alex Rivera"),
            "comment_added",
            "Fix checkout redirect loop",
            false,
            "12:58:00",
            Some(
                "Looks like the session cookie is SameSite=Strict.\nPR incoming in a few minutes.",
            ),
        ),
        (
            Some("Sam Chen"),
            "assigned",
            "Migrate product catalog to the new CMS",
            false,
            "12:01:00",
            None,
        ),
        (
            None,
            "overdue_tasks_reminder",
            "Update CSP headers",
            false,
            "10:00:00",
            None,
        ),
        (
            Some("Priya Patel"),
            "comment_added",
            "Q4 launch checklist",
            false,
            "09:12:00",
            Some("Added the rollout dates for all regions."),
        ),
        (
            Some("Jordan Lee"),
            "added_to_project",
            "Cart drawer accessibility audit",
            true,
            "2026-11-15",
            None,
        ),
        (
            Some("Mo Okafor"),
            "marked_complete",
            "Bump payment SDK to v5",
            true,
            "2026-11-13",
            None,
        ),
        (
            Some("Sam Chen"),
            "story_reaction_added",
            "Quarterly planning notes",
            true,
            "2026-11-10",
            None,
        ),
        (
            Some("Alex Rivera"),
            "comment_added",
            "Remove legacy feature flags",
            true,
            "2026-11-08",
            Some("Done."),
        ),
        (
            Some("Priya Patel"),
            "assigned",
            "Write onboarding guide",
            true,
            "2026-11-02",
            None,
        ),
    ];
    items
        .into_iter()
        .enumerate()
        .map(|(i, (actor, subtype, task, is_read, when, comment))| {
            let created_at = if when.contains(':') {
                format!("2026-11-16T{when}.000Z")
            } else {
                format!("{when}T09:00:00.000Z")
            };
            Notification {
                gid: format!("10000000000000{i:02}"),
                created_at,
                created_by: actor.map(|name| Actor {
                    gid: format!("20000000000000{i:02}"),
                    name: name.into(),
                    resource_type: "user".into(),
                }),
                is_read,
                story: Some(Story {
                    gid: format!("30000000000000{i:02}"),
                    text: comment.map(Into::into),
                    html_text: None,
                }),
                story_resource_subtype: subtype.into(),
                priority_classification: "unclassified".into(),
                work_objects: vec![WorkObject {
                    gid: format!("40000000000000{i:02}"),
                    name: task.into(),
                    resource_type: "task".into(),
                }],
            }
        })
        .collect()
}

/// The buffer as SVG: a background rect per styled run, then the text, each
/// run stretched to its exact cell width so box-drawing lines stay aligned.
fn svg(buffer: &Buffer, theme: &Theme) -> String {
    let base = theme.style(Role::Text);
    let default_fg = hex(base.fg, "#ebdbb2");
    let default_bg = hex(base.bg, "#282828");
    let width = f32::from(buffer.area.width) * CELL_W + 2.0 * PAD;
    let height = f32::from(buffer.area.height) * CELL_H + 2.0 * PAD;

    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.1}\" height=\"{height:.1}\" \
         viewBox=\"0 0 {width:.1} {height:.1}\" font-family=\"ui-monospace, SFMono-Regular, Menlo, \
         Consolas, 'DejaVu Sans Mono', monospace\" font-size=\"{FONT_SIZE}\">\n\
         <rect width=\"100%\" height=\"100%\" rx=\"10\" fill=\"{default_bg}\"/>\n"
    );
    // Box-drawing characters become lines: fonts lack (or mis-size) them.
    let mut lines = String::new();
    for y in 0..buffer.area.height {
        let mut x = 0;
        while x < buffer.area.width {
            let first = &buffer[(x, y)];
            let key = (first.fg, first.bg, first.modifier.contains(Modifier::BOLD));
            let mut text = String::new();
            let start = x;
            while x < buffer.area.width {
                let cell = &buffer[(x, y)];
                if (cell.fg, cell.bg, cell.modifier.contains(Modifier::BOLD)) != key {
                    break;
                }
                let symbol = cell.symbol();
                match box_segments(symbol) {
                    Some((segments, heavy)) => {
                        let left = PAD + f32::from(x) * CELL_W;
                        let top = PAD + f32::from(y) * CELL_H;
                        lines +=
                            &box_path(left, top, segments, heavy, &hex(Some(cell.fg), &default_fg));
                        text.push(' ');
                    }
                    None => text.push_str(symbol),
                }
                x += 1;
            }
            let (fg, bg, bold) = key;
            let left = PAD + f32::from(start) * CELL_W;
            let top = PAD + f32::from(y) * CELL_H;
            let run_width = f32::from(x - start) * CELL_W;
            if bg != Color::Reset {
                out += &format!(
                    "<rect x=\"{left:.1}\" y=\"{top:.1}\" width=\"{run_width:.1}\" height=\"{CELL_H}\" fill=\"{}\"/>\n",
                    hex(Some(bg), &default_bg)
                );
            }
            if !text.trim().is_empty() {
                out += &format!(
                    "<text x=\"{left:.1}\" y=\"{:.1}\" fill=\"{}\"{} textLength=\"{run_width:.1}\" \
                     lengthAdjust=\"spacingAndGlyphs\" xml:space=\"preserve\">{}</text>\n",
                    top + CELL_H * 0.78,
                    hex(Some(fg), &default_fg),
                    if bold { " font-weight=\"bold\"" } else { "" },
                    escape(&text)
                );
            }
        }
    }
    out + &lines + "</svg>\n"
}

/// Which half-segments from the cell centre a box-drawing character has
/// (left, right, up, down), and whether it is heavy.
fn box_segments(symbol: &str) -> Option<([bool; 4], bool)> {
    let (segments, heavy) = match symbol {
        "─" => ([true, true, false, false], false),
        "━" => ([true, true, false, false], true),
        "│" => ([false, false, true, true], false),
        "┃" => ([false, false, true, true], true),
        "┌" => ([false, true, false, true], false),
        "┏" => ([false, true, false, true], true),
        "┐" => ([true, false, false, true], false),
        "┓" => ([true, false, false, true], true),
        "└" => ([false, true, true, false], false),
        "┗" => ([false, true, true, false], true),
        "┘" => ([true, false, true, false], false),
        "┛" => ([true, false, true, false], true),
        _ => return None,
    };
    Some((segments, heavy))
}

fn box_path(left: f32, top: f32, [l, r, u, d]: [bool; 4], heavy: bool, color: &str) -> String {
    let (cx, cy) = (left + CELL_W / 2.0, top + CELL_H / 2.0);
    let mut d_attr = String::new();
    for (on, x, y) in [
        (l, left, cy),
        (r, left + CELL_W, cy),
        (u, cx, top),
        (d, cx, top + CELL_H),
    ] {
        if on {
            d_attr += &format!("M{cx:.1} {cy:.1}L{x:.1} {y:.1}");
        }
    }
    let width = if heavy { 2.4 } else { 1.2 };
    format!(
        "<path d=\"{d_attr}\" stroke=\"{color}\" stroke-width=\"{width}\" stroke-linecap=\"square\"/>\n"
    )
}

fn hex(color: Option<Color>, default: &str) -> String {
    match color {
        Some(Color::Rgb(r, g, b)) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => default.to_owned(),
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
