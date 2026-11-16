//! Rendering only. Every style comes from the [`Theme`]; no colors here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, HighlightSpacing, List, ListItem, ListState, Padding, Paragraph, Wrap,
};

use super::app::App;
use super::format::{describe, relative_time};
use super::theme::{Role, Theme};
use crate::api::notifications::Notification;

/// Lines per Inbox item.
const ITEM_HEIGHT: u16 = 2;

/// How many Inbox items fit in a terminal of `size`.
pub fn list_rows(size: Rect) -> usize {
    // Header and status bar, plus the list's top and bottom border.
    let inner = size.height.saturating_sub(4);
    usize::from((inner / ITEM_HEIGHT).max(1))
}

pub fn render(frame: &mut Frame, app: &App, theme: &Theme, now: i64) {
    let [header, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let [list, detail] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(body);

    render_header(frame, header, app, theme);
    render_list(frame, list, app, theme, now);
    render_detail(frame, detail, app, theme, now);
    render_status(frame, status, app, theme);
}

fn render_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut title = " Inbox".to_owned();
    if !app.loading || !app.notifications.is_empty() {
        let unread = app.notifications.iter().filter(|n| !n.is_read).count();
        title.push_str(&format!(" · {unread} unread"));
    }
    let line = Line::from(vec![
        Span::styled(" asn ", theme.style(Role::Title)),
        Span::styled(title, theme.style(Role::Text)),
    ]);
    let filter = if app.unread_only {
        "unread only"
    } else {
        "all"
    };
    let mut right = format!("[f] {filter} ");
    if app.loading_more {
        right.insert_str(0, "loading more…  ");
    }
    let right = Span::styled(right, theme.style(Role::Keyhint));
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right.width() as u16)])
            .areas(area);
    let style = theme.style(Role::Text);
    frame.render_widget(Paragraph::new(line).style(style), left_area);
    frame.render_widget(Paragraph::new(right).style(style), right_area);
}

fn pane(title: &str, focused: bool, theme: &Theme) -> Block<'static> {
    let (border, border_type) = if focused {
        (
            theme.style(Role::BorderFocused),
            ratatui::widgets::BorderType::Thick,
        )
    } else {
        (
            theme.style(Role::Border),
            ratatui::widgets::BorderType::Plain,
        )
    };
    Block::new()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(border)
        .title(Span::styled(format!(" {title} "), theme.style(Role::Title)))
}

fn render_list(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, now: i64) {
    let block = pane("Inbox", true, theme);
    let message = if app.loading && app.notifications.is_empty() {
        Some("Loading Inbox…")
    } else if app.error.is_some() && app.notifications.is_empty() {
        Some("The Inbox couldn't be loaded")
    } else if app.notifications.is_empty() {
        Some("Nothing in your Inbox")
    } else if app.visible_len() == 0 {
        Some("No unread notifications · f shows all")
    } else {
        None
    };
    if let Some(message) = message {
        let block = block.padding(Padding::horizontal(1));
        let text = Paragraph::new(Span::styled(message, theme.style(Role::TextMuted))).block(block);
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = app
        .visible()
        .enumerate()
        .map(|(index, n)| {
            let item = item(n, theme, now);
            if app.in_visual(index) {
                item.style(theme.style(Role::VisualSelection))
            } else {
                item
            }
        })
        .collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(theme.style(Role::Selection))
        .highlight_symbol("▌")
        .highlight_spacing(HighlightSpacing::Always);
    let mut state = ListState::default()
        .with_offset(app.scroll)
        .with_selected(Some(app.cursor));
    frame.render_stateful_widget(list, area, &mut state);
}

fn item<'a>(n: &'a Notification, theme: &Theme, now: i64) -> ListItem<'a> {
    let dot = if n.is_read {
        Span::raw(" ")
    } else {
        Span::styled("●", theme.style(Role::UnreadMarker))
    };
    let actor = n.created_by.as_ref().map_or("Asana", |a| a.name.as_str());
    let header = Line::from(vec![
        dot,
        Span::raw(" "),
        Span::styled(
            format!("{:<4}", relative_time(&n.created_at, now)),
            theme.style(Role::Timestamp),
        ),
        Span::styled(actor, theme.style(Role::Actor)),
        Span::raw("  "),
        Span::styled(
            describe(&n.story_resource_subtype),
            theme.style(Role::TextMuted),
        ),
    ]);
    let name_style = if n.is_read {
        Role::Text
    } else {
        Role::UnreadText
    };
    let object = n.work_objects.first().map_or("-", |o| o.name.as_str());
    let name = Line::from(vec![
        Span::raw("      "),
        Span::styled(object, theme.style(name_style)),
    ]);
    ListItem::new(vec![header, name])
}

/// Until the task detail pane exists (T9), shows what the notification says.
fn render_detail(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, now: i64) {
    let block = pane("Task", false, theme).padding(Padding::horizontal(1));
    let Some(n) = app.selected() else {
        frame.render_widget(block, area);
        return;
    };
    let actor = n.created_by.as_ref().map_or("Asana", |a| a.name.as_str());
    let object = n.work_objects.first().map_or("-", |o| o.name.as_str());
    let mut lines = vec![
        Line::styled(object, theme.style(Role::Title)),
        Line::default(),
        Line::from(vec![
            Span::styled(actor, theme.style(Role::Actor)),
            Span::styled(
                format!(" {}", describe(&n.story_resource_subtype)),
                theme.style(Role::Text),
            ),
            Span::styled(
                format!(" · {}", relative_time(&n.created_at, now)),
                theme.style(Role::Timestamp),
            ),
        ]),
    ];
    if let Some(text) = n.story.as_ref().and_then(|s| s.text.as_deref()) {
        lines.push(Line::default());
        lines.extend(
            text.lines()
                .map(|l| Line::styled(l.to_owned(), theme.style(Role::Text))),
        );
    }
    let detail = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(detail, area);
}

fn render_status(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mode = if app.visual_anchor.is_some() {
        " VISUAL "
    } else {
        " NORMAL "
    };
    let mut left = vec![Span::styled(mode, theme.style(Role::StatusbarMode))];
    if app.visible_len() > 0 {
        left.push(Span::raw(format!(
            " {}/{}",
            app.cursor + 1,
            app.visible_len()
        )));
    }
    if let Some(pending) = app.keys.pending() {
        left.push(Span::raw(format!("  {pending}")));
    }
    if app.loading {
        left.push(Span::raw("  Loading…"));
    }
    if let Some(error) = &app.error {
        left.push(Span::styled(
            format!("  ✗ {error}"),
            theme.style(Role::ToastError),
        ));
    }
    if let Some(toast) = &app.toast {
        let (icon, role) = if toast.is_error {
            ("✗", Role::ToastError)
        } else {
            ("✓", Role::ToastSuccess)
        };
        left.push(Span::styled(
            format!("  {icon} {}", toast.text),
            theme.style(role),
        ));
    }
    // A message gets the whole bar; hints return with the next key press.
    let hints = if app.toast.is_some() || app.error.is_some() {
        Span::raw("")
    } else {
        Span::styled(
            "e read  u unread  V visual  R refresh  q quit ",
            theme.style(Role::Keyhint),
        )
    };
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(hints.width() as u16)])
            .areas(area);
    let style = theme.style(Role::Statusbar);
    frame.render_widget(Paragraph::new(Line::from(left)).style(style), left_area);
    frame.render_widget(Paragraph::new(hints).style(style), right_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::Msg;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::{Modifier, Style};

    /// 2026-11-16T13:00:00Z, shortly after the newest fixture notification.
    const NOW: i64 = 1_794_834_000;

    fn fixture(name: &str) -> Vec<Notification> {
        let text = std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        serde_json::from_value(value["data"].clone()).unwrap()
    }

    fn app_with(notifications: Vec<Notification>) -> App {
        let (mut app, _) = App::new();
        app.update(Msg::Resize {
            list_rows: list_rows(Rect::new(0, 0, 100, 24)),
        });
        app.update(Msg::InboxLoaded(Ok((notifications, None))));
        app
    }

    fn draw(app: &App, theme: &Theme) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|f| render(f, app, theme, NOW)).unwrap();
        terminal
    }

    fn terminal_theme() -> Theme {
        Theme::builtin("terminal").unwrap()
    }

    /// The style of the first cell whose symbol is `symbol`. Buffer cells
    /// store "no color" as `Color::Reset`; that is mapped back to `None`.
    fn cell_style(terminal: &Terminal<TestBackend>, symbol: &str) -> Style {
        let buffer = terminal.backend().buffer();
        let cell = buffer
            .content()
            .iter()
            .find(|c| c.symbol() == symbol)
            .expect(symbol);
        let unset =
            |c: Option<ratatui::style::Color>| c.filter(|c| *c != ratatui::style::Color::Reset);
        let mut style = cell.style();
        style.fg = unset(style.fg);
        style.bg = unset(style.bg);
        style.underline_color = unset(style.underline_color);
        style.sub_modifier = Modifier::empty();
        style
    }

    #[test]
    fn list_rows_counts_two_line_items_inside_the_chrome() {
        // 24 rows − header − status bar − 2 border rows = 20 rows = 10 items.
        assert_eq!(list_rows(Rect::new(0, 0, 100, 24)), 10);
        assert_eq!(list_rows(Rect::new(0, 0, 100, 3)), 1);
    }

    #[test]
    fn inbox_screen() {
        let mut notifications = fixture("notifications_page.json");
        notifications.extend(fixture("notifications_edge_cases.json"));
        let app = app_with(notifications);

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }

    #[test]
    fn loading_screen() {
        let (app, _) = App::new();

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }

    #[test]
    fn empty_inbox_screen() {
        insta::assert_snapshot!(draw(&app_with(Vec::new()), &terminal_theme()).backend());
    }

    #[test]
    fn error_screen() {
        let (mut app, _) = App::new();
        app.update(Msg::InboxLoaded(Err(
            "Could not reach Asana: connection failed".into(),
        )));

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }

    #[test]
    fn selection_marker_and_borders_use_theme_roles() {
        let theme = terminal_theme();
        let terminal = draw(&app_with(fixture("notifications_page.json")), &theme);

        assert_eq!(cell_style(&terminal, "▌"), theme.style(Role::Selection));
        assert_eq!(
            cell_style(&terminal, "●").fg,
            theme.style(Role::UnreadMarker).fg
        );
        assert_eq!(
            cell_style(&terminal, "┏").fg,
            theme.style(Role::BorderFocused).fg
        );
        assert_eq!(cell_style(&terminal, "┌").fg, theme.style(Role::Border).fg);
    }

    #[test]
    fn no_color_shows_selection_without_any_color() {
        let terminal = draw(
            &app_with(fixture("notifications_page.json")),
            &Theme::no_color(),
        );

        let selected = cell_style(&terminal, "▌");
        assert!(selected.add_modifier.contains(Modifier::REVERSED));
        let buffer = terminal.backend().buffer();
        assert!(
            buffer.content().iter().all(|c| c
                .style()
                .fg
                .is_none_or(|fg| fg == ratatui::style::Color::Reset)
                && c.style()
                    .bg
                    .is_none_or(|bg| bg == ratatui::style::Color::Reset)),
            "a cell has a color under NO_COLOR"
        );
    }

    fn press(app: &mut App, typed: &str) {
        for c in typed.chars() {
            app.update(Msg::Key(ratatui::crossterm::event::KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Char(c),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )));
        }
    }

    #[test]
    fn visual_mode() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "jVjj");

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }

    #[test]
    fn visual_rows_use_the_visual_selection_role() {
        let theme = terminal_theme();
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "Vj");
        let terminal = draw(&app, &theme);

        // Row 2 is the first item's first line; its first cell after the
        // border is the selection marker column.
        let buffer = terminal.backend().buffer();
        let cell = &buffer[(2, 2)];
        assert_eq!(cell.bg, theme.style(Role::VisualSelection).bg.unwrap());
    }

    #[test]
    fn error_toast_in_the_status_bar() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "e");
        let gid = app.notifications[0].gid.clone();
        app.update(Msg::Marked {
            gids: vec![gid],
            read: true,
            result: Err("Could not reach Asana: connection failed".into()),
        });

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }

    #[test]
    fn all_read_suggests_showing_all() {
        let mut notifications = fixture("notifications_page.json");
        for n in &mut notifications {
            n.is_read = true;
        }

        let mut app = app_with(notifications);
        press(&mut app, "f");

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }
}
