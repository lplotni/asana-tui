//! Renders the README screenshot: the real TUI with made-up data, as SVG.
//!
//!     cargo run --example screenshot > docs/screenshot.svg

use asn::api::notifications::{Actor, Notification, Story, WorkObject};
use asn::tui::app::{App, Msg};
use asn::tui::theme::{Role, Theme};
use asn::tui::{format, ui};
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

    let (mut app, _) = App::new();
    app.update(Msg::Resize {
        list_rows: ui::list_rows(Rect::new(0, 0, WIDTH, HEIGHT)),
    });
    app.update(Msg::InboxLoaded(Ok((demo_inbox(), None))));

    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("test backend");
    terminal
        .draw(|frame| ui::render(frame, &app, &theme, now))
        .expect("draw");
    print!("{}", svg(terminal.backend().buffer(), &theme));
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
