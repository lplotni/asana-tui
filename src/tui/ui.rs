//! Rendering only. Every style comes from the [`Theme`]; no colors here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Padding, Paragraph,
};

use super::app::{App, Detail, Focus, Sizes};
use super::detail;
use super::format::{describe, relative_time};
use super::keys::{BINDINGS, Cmd, Context, Origin, sequence_label};
use super::picker::{Picker, Stage};
use super::theme::{Role, Theme};
use crate::api::notifications::Notification;

/// Lines per Inbox item.
const ITEM_HEIGHT: u16 = 2;

/// The help overlay's lines: every binding, generated from the keymap.
pub fn help_lines(theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (context, heading) in [
        (Context::Normal, "Inbox and task"),
        (Context::Input, "Text fields and pickers"),
    ] {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::styled(heading, theme.style(Role::Title)));
        // One entry per command, listing all of its key sequences.
        let mut seen: Vec<Cmd> = Vec::new();
        for binding in BINDINGS.iter().filter(|b| b.context == context) {
            if seen.contains(&binding.cmd) {
                continue;
            }
            seen.push(binding.cmd);
            let same: Vec<_> = BINDINGS
                .iter()
                .filter(|b| b.context == context && b.cmd == binding.cmd)
                .collect();
            let keys: Vec<String> = same.iter().map(|b| sequence_label(b.keys)).collect();
            let asana = same
                .iter()
                .any(|b| matches!(b.origin, Origin::Asana | Origin::VimAndAsana));
            let mut spans = vec![
                Span::styled(
                    format!("{:<12} ", keys.join(", ")),
                    theme.style(Role::PickerMatch),
                ),
                Span::styled(binding.help.to_owned(), theme.style(Role::Text)),
            ];
            if asana {
                spans.push(Span::styled("  · Asana", theme.style(Role::TextMuted)));
            }
            lines.push(Line::from(spans));
        }
    }
    lines
}

/// The help overlay over the whole body, in two columns.
fn render_help(frame: &mut Frame, over: Rect, theme: &Theme) {
    let area = over;
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Thick)
        .border_style(theme.style(Role::BorderFocused))
        .title(Span::styled(" Keys ", theme.style(Role::Title)))
        .title_bottom(Span::styled(" any key closes ", theme.style(Role::Keyhint)))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block.style(theme.style(Role::Text)), area);

    let lines = help_lines(theme);
    let rows = usize::from(inner.height.max(1));
    let columns = if inner.width >= 80 { 2 } else { 1 };
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(inner);
    let split = if columns == 2 {
        lines.len().div_ceil(2).min(rows)
    } else {
        lines.len()
    };
    let (first, second) = lines.split_at(split.min(lines.len()));
    let first_area = if columns == 2 { left } else { inner };
    frame.render_widget(Paragraph::new(first.to_vec()), first_area);
    if columns == 2 {
        frame.render_widget(Paragraph::new(second.to_vec()), right);
    }
}

/// The keys that work right now, as (keys, what they do), most useful first.
pub fn key_hints(app: &App) -> Vec<(String, &'static str)> {
    let key = |context: Context, cmd: Cmd| -> String {
        BINDINGS
            .iter()
            .find(|b| b.context == context && b.cmd == cmd)
            .map(|b| sequence_label(b.keys))
            .unwrap_or_default()
    };
    let normal = |cmd| key(Context::Normal, cmd);
    let input = |cmd| key(Context::Input, cmd);
    let pair = |a: String, b: String| format!("{a}/{b}");
    let lit = |text: &str| text.to_owned();

    if app.help {
        return vec![(lit("any key"), "close")];
    }
    if app.confirm.is_some() {
        return vec![(lit("y"), "yes"), (lit("any other key"), "no")];
    }
    if let Some(picker) = &app.picker {
        let mut hints = Vec::new();
        if picker.stage == Stage::Project {
            hints.push((lit("type"), "to search"));
        }
        hints.extend([
            (pair(input(Cmd::Next), input(Cmd::Prev)), "move"),
            (input(Cmd::Submit), "choose"),
            (input(Cmd::Cancel), "cancel"),
        ]);
        return hints;
    }
    if app.command.is_some() {
        return vec![
            (input(Cmd::Submit), "run"),
            (input(Cmd::Cancel), "cancel"),
            (lit("q, refresh, unread, read-all"), "commands"),
        ];
    }
    if app.search.as_ref().is_some_and(|s| s.editing) {
        return vec![
            (lit("type"), "to filter"),
            (input(Cmd::Submit), "keep"),
            (input(Cmd::Cancel), "clear"),
        ];
    }
    let moves = pair(normal(Cmd::MoveDown), normal(Cmd::MoveUp));
    if app.visual_anchor.is_some() {
        return vec![
            (moves, "extend"),
            (normal(Cmd::MarkRead), "read"),
            (normal(Cmd::MarkUnread), "unread"),
            (normal(Cmd::ToggleRead), "toggle"),
            (normal(Cmd::Back), "cancel"),
        ];
    }
    if app.focus == Focus::Detail {
        return vec![
            (moves, "scroll"),
            (normal(Cmd::Back), "back"),
            (normal(Cmd::Comment), "comment"),
            (normal(Cmd::EditDescription), "edit description"),
            (normal(Cmd::AddToProject), "project"),
            (normal(Cmd::OpenInBrowser), "open in browser"),
            (normal(Cmd::MarkRead), "read"),
            (normal(Cmd::Help), "help"),
        ];
    }
    let mut hints = vec![(moves, "move")];
    if app.search.is_some() {
        hints.push((
            pair(normal(Cmd::NextMatch), normal(Cmd::PrevMatch)),
            "next/previous match",
        ));
        hints.push((normal(Cmd::Back), "clear search"));
    }
    hints.extend([
        (normal(Cmd::Open), "open"),
        (normal(Cmd::MarkRead), "read"),
        (normal(Cmd::MarkUnread), "unread"),
        (normal(Cmd::Visual), "select"),
        (normal(Cmd::Comment), "comment"),
        (normal(Cmd::AddToProject), "project"),
        (normal(Cmd::Search), "search"),
        (normal(Cmd::ToggleFilter), "filter"),
        (normal(Cmd::Refresh), "refresh"),
        (normal(Cmd::Help), "help"),
        (normal(Cmd::Quit), "quit"),
    ]);
    hints
}

/// The hint line: as many of the current context's hints as fit.
/// `? help` is always kept, at the end, so every other key can be looked up.
fn render_hints(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let width_of = |keys: &str, label: &str| keys.chars().count() + 1 + label.chars().count() + 3;
    let (help, others): (Vec<_>, Vec<_>) = key_hints(app)
        .into_iter()
        .partition(|(_, label)| *label == "help");
    let reserved: usize = help.iter().map(|(k, l)| width_of(k, l)).sum();
    let mut width = 1 + reserved;
    let mut shown = Vec::new();
    for (keys, label) in others {
        let needed = width_of(&keys, label);
        if width + needed > usize::from(area.width) {
            break;
        }
        width += needed;
        shown.push((keys, label));
    }
    shown.extend(help);
    let mut spans = vec![Span::raw(" ")];
    for (keys, label) in shown {
        spans.push(Span::styled(keys, theme.style(Role::PickerMatch)));
        spans.push(Span::styled(
            format!(" {label}   "),
            theme.style(Role::Keyhint),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.style(Role::Text)),
        area,
    );
}

/// What fits in a terminal of `size`: Inbox items and the detail pane's text
/// area. Must match the layout in [`render`].
pub fn sizes(size: Rect) -> Sizes {
    let (_, body, _, _) = split_screen(size);
    let (list, detail) = split_body(body);
    // List: top and bottom border. Detail: borders plus one column of padding
    // on each side.
    let list_inner = list.height.saturating_sub(2);
    Sizes {
        list_rows: usize::from((list_inner / ITEM_HEIGHT).max(1)),
        detail_width: usize::from(detail.width.saturating_sub(4)),
        detail_rows: usize::from(detail.height.saturating_sub(2)),
    }
}

/// Header, body, status bar and the key-hint line.
fn split_screen(area: Rect) -> (Rect, Rect, Rect, Rect) {
    let [header, body, status, hints] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    (header, body, status, hints)
}

/// Below this width only the focused pane is shown, at full width.
const NARROW: u16 = 100;

fn split_body(body: Rect) -> (Rect, Rect) {
    if body.width < NARROW {
        return (body, body);
    }
    let [list, detail] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(body);
    (list, detail)
}

pub fn render(frame: &mut Frame, app: &App, theme: &Theme, now: i64) {
    let (header, body, status, hint_line) = split_screen(frame.area());
    let (list, detail) = split_body(body);

    render_header(frame, header, app, theme);
    let narrow = body.width < NARROW;
    if !narrow || app.focus == Focus::List {
        render_list(frame, list, app, theme, now);
    }
    if !narrow || app.focus == Focus::Detail {
        render_detail(frame, detail, app, theme, now);
    }
    render_status(frame, status, app, theme);
    render_hints(frame, hint_line, app, theme);
    if app.help {
        render_help(frame, body, theme);
    }
    if let Some(picker) = &app.picker {
        render_picker(frame, detail, picker, theme);
    }
}

/// The project/section picker, drawn over the task pane.
fn render_picker(frame: &mut Frame, over: Rect, picker: &Picker, theme: &Theme) {
    let width = over.width.saturating_sub(4).min(56);
    let height = over.height.saturating_sub(2).min(14);
    let area = Rect::new(over.x + (over.width - width) / 2, over.y + 2, width, height);
    let title = match &picker.stage {
        Stage::Project => " Add to project ".to_owned(),
        Stage::Section { project } => format!(" Section in {} ", project.name),
    };
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Thick)
        .border_style(theme.style(Role::BorderFocused))
        .title(Span::styled(title, theme.style(Role::Title)))
        .padding(Padding::horizontal(1));

    let mut lines = Vec::new();
    if picker.stage == Stage::Project {
        lines.push(Line::from(vec![
            Span::styled("> ", theme.style(Role::PickerMatch)),
            Span::styled(picker.query.clone(), theme.style(Role::Text)),
            Span::styled("▏", theme.style(Role::PickerMatch)),
        ]));
        lines.push(Line::default());
    }
    if picker.loading && picker.items.is_empty() {
        lines.push(Line::styled("Searching…", theme.style(Role::TextMuted)));
    } else if picker.items.is_empty() {
        lines.push(Line::styled(
            "No matching projects",
            theme.style(Role::TextMuted),
        ));
    }
    for (index, item) in picker.items.iter().enumerate() {
        let style = if index == picker.selected {
            theme.style(Role::Selection)
        } else {
            theme.style(Role::Text)
        };
        let marker = if index == picker.selected { "▌" } else { " " };
        lines.push(Line::styled(format!("{marker}{}", item.name), style));
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(theme.style(Role::Text)),
        area,
    );
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
    if let Some(search) = app.search.as_ref().filter(|s| !s.editing) {
        right.insert_str(0, &format!("/{}  (Esc clears)  ", search.query));
    }
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
    let block = pane("Inbox", app.focus == Focus::List, theme);
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

/// The selected notification's task: its details once loaded, otherwise
/// what's known so far.
fn render_detail(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, now: i64) {
    let block = pane("Task", app.focus == Focus::Detail, theme).padding(Padding::horizontal(1));
    let width = usize::from(area.width.saturating_sub(4));
    let muted = theme.style(Role::TextMuted);
    let Some(n) = app.selected() else {
        frame.render_widget(block, area);
        return;
    };
    let name = n.work_objects.first().map_or("-", |o| o.name.as_str());
    let title = || Line::styled(name.to_owned(), theme.style(Role::Title));
    let lines = match (app.selected_task_gid(), app.selected_detail()) {
        (Some(_), Some(Detail::Loaded(detail))) => detail::lines(detail, theme, width, now),
        (Some(_), Some(Detail::Failed(message))) => vec![
            title(),
            Line::default(),
            Line::styled(
                format!("Couldn't load the task: {message}"),
                theme.style(Role::ToastError),
            ),
        ],
        (Some(_), _) => vec![
            title(),
            Line::default(),
            Line::styled("Loading task…", muted),
        ],
        (None, _) => {
            let kind = n
                .work_objects
                .first()
                .map_or("item", |o| o.resource_type.as_str());
            vec![
                title(),
                Line::default(),
                Line::styled(
                    format!("This notification is about a {kind}; asn shows task details only."),
                    muted,
                ),
            ]
        }
    };
    let scroll = u16::try_from(app.detail_scroll).unwrap_or(u16::MAX);
    frame.render_widget(Paragraph::new(lines).block(block).scroll((scroll, 0)), area);
}

fn render_status(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if let Some(confirm) = &app.confirm {
        let line = Line::from(vec![
            Span::styled(" CONFIRM ", theme.style(Role::StatusbarMode)),
            Span::styled(
                format!(" {}", confirm.question),
                theme.style(Role::ToastError),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(theme.style(Role::Statusbar)),
            area,
        );
        return;
    }
    // While typing a `:` command or `/` query, the bar is the input line.
    let input = match (&app.command, &app.search) {
        (Some(command), _) => Some((" COMMAND ", format!(":{command}"))),
        (None, Some(search)) if search.editing => Some((" SEARCH ", format!("/{}", search.query))),
        _ => None,
    };
    if let Some((mode, text)) = input {
        let line = Line::from(vec![
            Span::styled(mode, theme.style(Role::StatusbarMode)),
            Span::raw(format!(" {text}")),
            Span::styled("▏", theme.style(Role::PickerMatch)),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(theme.style(Role::Statusbar)),
            area,
        );
        return;
    }
    let mode = if app.picker.is_some() {
        " PICKER "
    } else if app.visual_anchor.is_some() {
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
    let style = theme.style(Role::Statusbar);
    frame.render_widget(Paragraph::new(Line::from(left)).style(style), area);
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
        app.update(Msg::Resize(sizes(Rect::new(0, 0, 100, 24))));
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
    fn sizes_count_two_line_items_and_the_detail_text_area() {
        // 24 rows − header − status bar − hint line − 2 border rows = 19 rows = 9 items.
        let s = sizes(Rect::new(0, 0, 100, 24));
        assert_eq!(s.list_rows, 9);
        // The detail pane is 55 of 100 columns, minus borders and padding.
        assert_eq!((s.detail_width, s.detail_rows), (51, 19));
        assert_eq!(sizes(Rect::new(0, 0, 100, 3)).list_rows, 1);
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

    fn with_detail(app: &mut App) {
        let gid = app.selected_task_gid().unwrap().to_owned();
        let read = |name: &str| -> serde_json::Value {
            serde_json::from_str(
                &std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap(),
            )
            .unwrap()
        };
        let task = serde_json::from_value(read("task_full.json")["data"].clone()).unwrap();
        let stories: Vec<crate::api::tasks::TaskStory> =
            serde_json::from_value(read("stories_thread.json")["data"].clone()).unwrap();
        let (comments, activity): (Vec<_>, Vec<_>) = stories
            .into_iter()
            .partition(crate::api::tasks::TaskStory::is_comment);
        let detail = crate::core::TaskDetail {
            task,
            comments,
            activity_count: activity.len(),
        };
        app.update(Msg::DetailDue { gid: gid.clone() });
        app.update(Msg::DetailLoaded {
            gid,
            result: Ok(Box::new(detail)),
        });
    }

    #[test]
    fn task_detail_screen() {
        let mut app = app_with(fixture("notifications_page.json"));
        with_detail(&mut app);

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }

    #[test]
    fn task_detail_scrolled_to_the_comments() {
        let mut app = app_with(fixture("notifications_page.json"));
        with_detail(&mut app);
        press(&mut app, "lG");

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }

    #[test]
    fn the_focused_pane_has_the_heavy_border() {
        let theme = terminal_theme();
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "l");

        let terminal = draw(&app, &theme);

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 1)].symbol(), "┌", "list is no longer focused");
        assert_eq!(buffer[(45, 1)].symbol(), "┏", "detail is focused");
    }

    fn open_picker(app: &mut App) {
        with_detail(app);
        press(app, "p");
        press(app, "web");
        let seq = app.picker.as_ref().unwrap().search_seq;
        app.update(Msg::ProjectsFound {
            seq,
            result: Ok(vec![
                crate::api::tasks::Named {
                    gid: "1".into(),
                    name: "Webshop".into(),
                },
                crate::api::tasks::Named {
                    gid: "2".into(),
                    name: "Web analytics".into(),
                },
            ]),
        });
    }

    #[test]
    fn project_picker() {
        let mut app = app_with(fixture("notifications_page.json"));
        open_picker(&mut app);

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }

    #[test]
    fn section_picker() {
        let mut app = app_with(fixture("notifications_page.json"));
        open_picker(&mut app);
        app.update(Msg::Key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        )));
        app.update(Msg::SectionsLoaded {
            project_gid: "1".into(),
            result: Ok(vec![crate::api::tasks::Named {
                gid: "3".into(),
                name: "In progress".into(),
            }]),
        });

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }

    #[test]
    fn typing_a_search() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "/bravo");

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }

    #[test]
    fn a_kept_search_shows_in_the_header() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "/bravo");
        app.update(Msg::Key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        )));

        let terminal = draw(&app, &terminal_theme());

        let header: String = (0..100)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_owned())
            .collect();
        assert!(header.contains("/bravo"), "{header}");
    }

    #[test]
    fn typing_a_command() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, ":read-all");

        let terminal = draw(&app, &terminal_theme());

        let status: String = (0..100)
            .map(|x| terminal.backend().buffer()[(x, 22)].symbol().to_owned())
            .collect();
        assert!(status.starts_with(" COMMAND  :read-all▏"), "{status}");
    }

    fn draw_at(app: &mut App, width: u16) -> Terminal<TestBackend> {
        app.update(Msg::Resize(sizes(Rect::new(0, 0, width, 24))));
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal
            .draw(|f| render(f, app, &terminal_theme(), NOW))
            .unwrap();
        terminal
    }

    #[test]
    fn help_lists_every_key_binding_once_per_command() {
        use crate::tui::keys::{BINDINGS, sequence_label};
        let lines = help_lines(&terminal_theme());
        let text: String = lines.iter().map(|l| detail::plain(l) + "\n").collect();

        for binding in BINDINGS {
            assert!(
                text.contains(&sequence_label(binding.keys)),
                "{:?} missing",
                binding.keys
            );
        }
        let mut commands: Vec<_> = BINDINGS.iter().map(|b| (b.context, b.cmd)).collect();
        commands.dedup();
        commands.sort_by_key(|(context, cmd)| format!("{context:?}{cmd:?}"));
        commands.dedup();
        let entries = lines.iter().filter(|l| l.spans.len() >= 2).count();
        assert_eq!(entries, commands.len());
    }

    #[test]
    fn help_marks_asana_web_keys() {
        let text: String = help_lines(&terminal_theme())
            .iter()
            .map(|l| detail::plain(l) + "\n")
            .collect();

        let e_line = text
            .lines()
            .find(|l| l.trim_start().starts_with("e "))
            .unwrap();
        assert!(e_line.contains("Asana"), "{e_line}");
    }

    #[test]
    fn help_overlay() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "?");

        insta::assert_snapshot!(draw(&app, &terminal_theme()).backend());
    }

    #[test]
    fn narrow_terminals_show_only_the_list() {
        let mut app = app_with(fixture("notifications_page.json"));

        insta::assert_snapshot!(draw_at(&mut app, 80).backend());
    }

    #[test]
    fn narrow_terminals_show_only_the_task_when_it_has_focus() {
        let mut app = app_with(fixture("notifications_page.json"));
        with_detail(&mut app);
        press(&mut app, "l");

        insta::assert_snapshot!(draw_at(&mut app, 80).backend());
    }

    #[test]
    fn wide_terminals_show_both_panes() {
        let mut app = app_with(fixture("notifications_page.json"));
        with_detail(&mut app);

        insta::assert_snapshot!(draw_at(&mut app, 120).backend());
    }

    #[test]
    fn narrow_sizes_give_the_task_the_full_width() {
        let s = sizes(Rect::new(0, 0, 80, 24));

        assert_eq!(s.detail_width, 76);
        assert_eq!(s.list_rows, 9);
    }

    #[test]
    fn resizing_across_the_threshold_keeps_selection_and_focus() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "jjl");

        draw_at(&mut app, 80);
        draw_at(&mut app, 120);

        assert_eq!(app.cursor, 2);
        assert_eq!(app.focus, crate::tui::app::Focus::Detail);
    }

    #[test]
    fn the_formatting_question_is_asked_in_the_status_bar() {
        let mut app = app_with(fixture("notifications_page.json"));
        with_detail(&mut app);
        app.update(Msg::Key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Tab,
            ratatui::crossterm::event::KeyModifiers::NONE,
        )));
        press(&mut app, "e");

        let terminal = draw(&app, &terminal_theme());

        let status: String = (0..100)
            .map(|x| terminal.backend().buffer()[(x, 22)].symbol().to_owned())
            .collect();
        assert!(
            status.starts_with(" CONFIRM  This will remove formatting. Continue? (y/N)"),
            "{status}"
        );
    }

    // --- Context-dependent key hints ----------------------------------------

    fn hint_text(app: &App) -> String {
        key_hints(app)
            .iter()
            .map(|(keys, label)| format!("{keys} {label}"))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn key_event(code: ratatui::crossterm::event::KeyCode) -> Msg {
        Msg::Key(ratatui::crossterm::event::KeyEvent::new(
            code,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ))
    }

    #[test]
    fn the_list_hints_its_main_actions() {
        let app = app_with(fixture("notifications_page.json"));

        let hints = hint_text(&app);

        for expected in [
            "j/k move",
            "Enter open",
            "e read",
            "u unread",
            "V select",
            "c comment",
            "p project",
            "/ search",
            "? help",
            "q quit",
        ] {
            assert!(hints.contains(expected), "missing {expected:?} in {hints}");
        }
    }

    #[test]
    fn the_task_pane_hints_scrolling_and_task_actions() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "l");

        let hints = hint_text(&app);

        for expected in [
            "j/k scroll",
            "Esc back",
            "c comment",
            "Tab e edit description",
            "p project",
            "o open in browser",
        ] {
            assert!(hints.contains(expected), "missing {expected:?} in {hints}");
        }
        assert!(!hints.contains("V select"));
    }

    #[test]
    fn visual_mode_hints_only_what_applies_to_a_range() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "V");

        let hints = hint_text(&app);

        assert!(
            hints.contains("e read") && hints.contains("x toggle") && hints.contains("Esc cancel"),
            "{hints}"
        );
        assert!(!hints.contains("comment"), "{hints}");
    }

    #[test]
    fn a_kept_search_adds_match_navigation() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "/bravo");
        app.update(key_event(ratatui::crossterm::event::KeyCode::Enter));

        let hints = hint_text(&app);

        assert!(
            hints.contains("n/N next/previous match") && hints.contains("Esc clear search"),
            "{hints}"
        );
    }

    #[test]
    fn text_inputs_hint_how_to_finish() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "/b");
        assert!(hint_text(&app).contains("Enter keep") && hint_text(&app).contains("Esc clear"));

        app.update(key_event(ratatui::crossterm::event::KeyCode::Esc));
        press(&mut app, ":");
        let hints = hint_text(&app);
        assert!(
            hints.contains("Enter run")
                && hints.contains("Esc cancel")
                && hints.contains("read-all"),
            "{hints}"
        );
    }

    #[test]
    fn the_picker_hints_moving_and_choosing() {
        let mut app = app_with(fixture("notifications_page.json"));
        open_picker(&mut app);

        let hints = hint_text(&app);

        assert!(
            hints.contains("Ctrl-n/Ctrl-p move")
                && hints.contains("Enter choose")
                && hints.contains("Esc cancel"),
            "{hints}"
        );
    }

    #[test]
    fn the_confirm_question_and_help_hint_their_keys() {
        let mut app = app_with(fixture("notifications_page.json"));
        with_detail(&mut app);
        app.update(key_event(ratatui::crossterm::event::KeyCode::Tab));
        press(&mut app, "e");
        assert!(hint_text(&app).contains("y yes"), "{}", hint_text(&app));

        press(&mut app, "n");
        press(&mut app, "?");
        assert_eq!(hint_text(&app), "any key close");
    }

    #[test]
    fn hinted_keys_are_real_bindings() {
        use crate::tui::keys::{BINDINGS, sequence_label};
        let labels: Vec<String> = BINDINGS.iter().map(|b| sequence_label(b.keys)).collect();
        let mut app = app_with(fixture("notifications_page.json"));
        let mut all = key_hints(&app);
        press(&mut app, "l");
        all.extend(key_hints(&app));

        for (keys, _) in all {
            let parts: Vec<&str> = if keys == "/" {
                vec!["/"]
            } else {
                keys.split('/').collect()
            };
            for key in parts {
                assert!(labels.iter().any(|l| l == key), "{key:?} is not bound");
            }
        }
    }

    #[test]
    fn the_hint_line_stays_visible_while_a_message_is_shown() {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, "o");
        assert!(app.toast.is_none() || app.toast.is_some());
        app.update(Msg::Marked {
            gids: vec![app.notifications[0].gid.clone()],
            read: true,
            result: Err("Could not reach Asana".into()),
        });

        let terminal = draw(&app, &terminal_theme());

        let row = |y: u16| -> String {
            (0..100)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol().to_owned())
                .collect()
        };
        assert!(row(22).contains("Could not reach Asana"), "{}", row(22));
        assert!(row(23).contains("? help"), "{}", row(23));
    }
}
