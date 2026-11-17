//! Rendering only. Every style comes from the [`Theme`]; no colors here.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Padding, Paragraph,
};

use super::app::{App, BoardState, Detail, Editor, Focus, IMAGE_ROWS, ImageState, Screen, Sizes};
use super::board::BoardView;
use super::keys::{BINDINGS, Cmd, Context, Origin, sequence_label};
use super::picker::{Picker, Stage};
use super::theme::{Role, Theme};
use crate::api::notifications::Notification;
use crate::present::format::{describe, relative_time};
use crate::present::{card, detail};

/// Lines per Inbox item.
const ITEM_HEIGHT: u16 = 2;
/// Lines per board card.
const CARD_HEIGHT: u16 = 2;

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

/// Clears `area` and draws an overlay's frame (thick border, `title`, an
/// optional `footer`); returns the area inside it.
fn draw_overlay(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    footer: Option<&str>,
    theme: &Theme,
) -> Rect {
    let mut block = Block::new()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Thick)
        .border_style(theme.style(Role::BorderFocused))
        .title(Span::styled(title.to_owned(), theme.style(Role::Title)))
        .padding(Padding::horizontal(1))
        .style(theme.style(Role::Text));
    if let Some(footer) = footer {
        block = block.title_bottom(Span::styled(footer.to_owned(), theme.style(Role::Keyhint)));
    }
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    inner
}

/// A `width` × `height` rectangle in the middle of `area`, shrunk to fit.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let (width, height) = (width.min(area.width), height.min(area.height));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

/// The help overlay, centered: two columns when they fit, else one.
fn render_help(frame: &mut Frame, over: Rect, theme: &Theme) {
    let lines = help_lines(theme);
    let line_width = lines.iter().map(Line::width).max().unwrap_or(0) as u16;
    // Borders and padding take 4 columns; two columns add a 2-column gap.
    let two_columns = over.width >= 2 * line_width + 6;
    let (width, height) = if two_columns {
        (2 * line_width + 6, lines.len().div_ceil(2) as u16 + 2)
    } else {
        (line_width + 4, lines.len() as u16 + 2)
    };
    let area = centered(over, width, height);
    let inner = draw_overlay(frame, area, " Keys ", Some(" any key closes "), theme);

    if two_columns {
        let [left, _, right] = Layout::horizontal([
            Constraint::Length(line_width),
            Constraint::Length(2),
            Constraint::Length(line_width),
        ])
        .areas(inner);
        let (first, second) = lines.split_at(lines.len().div_ceil(2));
        frame.render_widget(Paragraph::new(first.to_vec()), left);
        frame.render_widget(Paragraph::new(second.to_vec()), right);
    } else {
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// Colors role-tagged lines with `theme`.
fn styled(lines: Vec<detail::TextLine>, theme: &Theme) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|segments| {
            Line::from(
                segments
                    .into_iter()
                    .map(|s| Span::styled(s.text, theme.style(s.role)))
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
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
    if let Some(editor) = &app.editor {
        return vec![
            (lit("Ctrl-s"), editor.kind.verb()),
            (lit("Esc"), "cancel"),
            (lit("Ctrl-o"), "open in $EDITOR"),
        ];
    }
    if app.confirm.is_some() {
        return vec![(lit("y"), "yes"), (lit("any other key"), "no")];
    }
    if let Some(picker) = &app.picker {
        let mut hints = Vec::new();
        if picker.has_query() {
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
    if app.screen == Screen::Board && app.focus == Focus::Detail {
        return vec![
            (moves, "scroll"),
            (normal(Cmd::Back), "back"),
            (normal(Cmd::Comment), "comment"),
            (normal(Cmd::EditDescription), "edit description"),
            (normal(Cmd::AddToProject), "project"),
            (normal(Cmd::MoveToSection), "move"),
            (normal(Cmd::OpenInBrowser), "open in browser"),
            (normal(Cmd::Help), "help"),
        ];
    }
    if app.screen == Screen::Board {
        return vec![
            (moves, "move"),
            (normal(Cmd::Open), "open"),
            (
                pair(normal(Cmd::FocusLeft), normal(Cmd::FocusRight)),
                "column",
            ),
            (pair(normal(Cmd::Top), normal(Cmd::Bottom)), "top/bottom"),
            (normal(Cmd::Refresh), "refresh"),
            (normal(Cmd::Help), "help"),
            (normal(Cmd::Quit), "quit"),
        ];
    }
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
            (normal(Cmd::MoveToSection), "move"),
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
        (normal(Cmd::SearchTasks), "search tasks"),
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

/// What fits in a terminal of `size`: Inbox items, the detail pane's text
/// area and the board. Must match the layout in [`render`].
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
        board_width: usize::from(body.width),
        board_card_rows: usize::from((body.height.saturating_sub(2) / CARD_HEIGHT).max(1)),
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

/// An image attachment fully in view: where the image layer should draw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownImage {
    pub attachment_gid: String,
    pub area: Rect,
}

/// Draws the screen. Returns the loaded images that are fully in view and
/// not covered by an overlay; drawing them is up to [`super::images`].
pub fn render(frame: &mut Frame, app: &App, theme: &Theme, now: i64) -> Vec<ShownImage> {
    let (header, body, status, hint_line) = split_screen(frame.area());
    if app.screen == Screen::Board {
        render_board_header(frame, header, app, theme);
        // An open card takes the whole body, like the task pane on a
        // narrow terminal.
        let mut images = Vec::new();
        if app.focus == Focus::Detail {
            images = render_detail(frame, body, app, theme, now);
        } else {
            render_board(frame, body, app, theme, now);
        }
        render_status(frame, status, app, theme);
        render_hints(frame, hint_line, app, theme);
        render_overlays(frame, app, theme);
        if app.overlay_open() {
            images.clear();
        }
        return images;
    }
    let (list, detail) = split_body(body);

    render_header(frame, header, app, theme);
    let narrow = body.width < NARROW;
    if !narrow || app.focus == Focus::List {
        render_list(frame, list, app, theme, now);
    }
    let mut images = Vec::new();
    if !narrow || app.focus == Focus::Detail {
        images = render_detail(frame, detail, app, theme, now);
    }
    render_status(frame, status, app, theme);
    render_hints(frame, hint_line, app, theme);
    render_overlays(frame, app, theme);
    if app.overlay_open() {
        images.clear();
    }
    images
}

/// Help, picker and editor, drawn over whatever screen is shown.
fn render_overlays(frame: &mut Frame, app: &App, theme: &Theme) {
    let screen = frame.area();
    if app.help {
        render_help(frame, screen, theme);
    }
    if let Some(picker) = &app.picker {
        render_picker(frame, screen, picker, theme);
    }
    if let Some(editor) = &app.editor {
        render_editor(frame, screen, editor, app, theme);
    }
}

/// " asn  Project · N open tasks" with the hidden columns at the right.
fn render_board_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let view = app.board_view();
    let title = match view {
        Some(view) => {
            let open: usize = view.columns().iter().map(|c| c.tasks.len()).sum();
            format!(" {} · {open} open", view.project().name)
        }
        None => " Board".to_owned(),
    };
    let line = Line::from(vec![
        Span::styled(" asn ", theme.style(Role::Title)),
        Span::styled(title, theme.style(Role::Text)),
    ]);
    let right = match view.map(BoardView::hidden) {
        Some((0, 0)) | None => String::new(),
        Some((left, right)) => {
            let left = if left > 0 {
                format!("‹ {left}")
            } else {
                String::new()
            };
            let right = if right > 0 {
                format!("{right} ›")
            } else {
                String::new()
            };
            format!("{left}  {right} ")
        }
    };
    let right = Span::styled(right, theme.style(Role::Keyhint));
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right.width() as u16)])
            .areas(area);
    let style = theme.style(Role::Text);
    frame.render_widget(Paragraph::new(line).style(style), left_area);
    frame.render_widget(Paragraph::new(right).style(style), right_area);
}

/// The board's columns, or why there are none yet.
fn render_board(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, now: i64) {
    let message = match app.board.as_ref().map(|b| &b.state) {
        Some(BoardState::Loaded(view)) => {
            render_columns(frame, area, view, theme, now);
            return;
        }
        Some(BoardState::Failed(message)) => {
            vec![
                Line::styled(
                    "The board couldn't be loaded",
                    theme.style(Role::ToastError),
                ),
                Line::styled(message.clone(), theme.style(Role::TextMuted)),
            ]
        }
        Some(BoardState::Loading) | None => {
            vec![Line::styled("Loading board…", theme.style(Role::TextMuted))]
        }
    };
    let block = pane("Board", true, theme).padding(Padding::horizontal(1));
    frame.render_widget(Paragraph::new(message).block(block), area);
}

fn render_columns(frame: &mut Frame, area: Rect, view: &BoardView, theme: &Theme, now: i64) {
    let visible = view.visible_columns();
    let count = visible.len() as u32;
    let areas = Layout::horizontal((0..count).map(|_| Constraint::Ratio(1, count))).split(area);
    for (index, column_area) in visible.zip(areas.iter()) {
        let column = &view.columns()[index];
        let name = column
            .section
            .as_ref()
            .map_or("(no section)", |s| s.name.as_str());
        let focused = index == view.column();
        let block = pane(&format!("{name} ({})", column.tasks.len()), focused, theme);
        if column.tasks.is_empty() {
            let block = block.padding(Padding::horizontal(1));
            let text = Paragraph::new(Span::styled("no tasks", theme.style(Role::TextMuted)));
            frame.render_widget(text.block(block), *column_area);
            continue;
        }
        // Inside the borders, after the one-cell selection marker.
        let text_width = usize::from(column_area.width.saturating_sub(3));
        let items: Vec<ListItem> = column
            .tasks
            .iter()
            .map(|task| card_item(task, text_width, theme, now))
            .collect();
        let list = List::new(items)
            .block(block)
            .highlight_style(theme.style(Role::Selection))
            .highlight_symbol("▌")
            .highlight_spacing(HighlightSpacing::Always);
        let mut state = ListState::default()
            .with_offset(view.scroll(index))
            .with_selected(focused.then_some(view.row()));
        frame.render_stateful_widget(list, *column_area, &mut state);
    }
}

/// A card: the task name, then the assignee's initials and the due date.
fn card_item<'a>(
    task: &crate::api::projects::BoardTask,
    width: usize,
    theme: &Theme,
    now: i64,
) -> ListItem<'a> {
    let name = Line::styled(card::truncate(&task.name, width), theme.style(Role::Text));
    let mut meta = vec![Span::styled(
        card::initials(task),
        theme.style(Role::TaskMeta),
    )];
    if let Some(due) = card::due(task, now) {
        meta.push(Span::styled(" · ", theme.style(Role::TextMuted)));
        if due.overdue {
            meta.push(Span::styled(due.text, theme.style(Role::ToastError)));
            meta.push(Span::styled("!", theme.style(Role::ToastError)));
        } else {
            meta.push(Span::styled(due.text, theme.style(Role::TaskMeta)));
        }
    }
    ListItem::new(vec![name, Line::from(meta)])
}

/// The comment or description editor, centered.
fn render_editor(frame: &mut Frame, over: Rect, editor: &Editor, app: &App, theme: &Theme) {
    let area = centered(
        over,
        over.width.saturating_sub(4).min(80),
        over.height.saturating_sub(2).min(18),
    );
    let title = editor.kind.title(app.shown_task_name().unwrap_or("task"));
    let footer = format!(
        " Ctrl-s {} · Esc cancel · Ctrl-o $EDITOR ",
        editor.kind.verb()
    );
    let inner = draw_overlay(frame, area, &title, Some(&footer), theme);
    frame.render_widget(&editor.textarea, inner);
}

fn render_picker(frame: &mut Frame, over: Rect, picker: &Picker, theme: &Theme) {
    let area = centered(
        over,
        over.width.saturating_sub(4).min(56),
        over.height.saturating_sub(2).min(14),
    );
    let height = area.height;
    let title = match &picker.stage {
        Stage::Project => " Add to project ".to_owned(),
        Stage::Section { project } => format!(" Section in {} ", project.name),
        Stage::Task => " Search tasks ".to_owned(),
        Stage::MoveProject => " Move within which project? ".to_owned(),
        Stage::MoveSection { project, .. } => format!(" Move to a section of {} ", project.name),
    };
    let mut lines = Vec::new();
    if picker.has_query() {
        lines.push(Line::from(vec![
            Span::styled("> ", theme.style(Role::PickerMatch)),
            Span::styled(picker.query.clone(), theme.style(Role::Text)),
            Span::styled("▏", theme.style(Role::PickerMatch)),
        ]));
        lines.push(Line::default());
    }
    let muted = theme.style(Role::TextMuted);
    if picker.loading && picker.items.is_empty() {
        lines.push(Line::styled("Searching…", muted));
    } else if picker.stage == Stage::Task && picker.query.trim().is_empty() {
        lines.push(Line::styled("Type to search tasks", muted));
    } else if picker.items.is_empty() {
        let message = match picker.stage {
            Stage::Task => "No matching tasks",
            Stage::MoveSection { .. } => "This project has no sections",
            _ => "No matching projects",
        };
        lines.push(Line::styled(message, muted));
    }
    // Rows left for items inside the borders; scroll to keep the selection in.
    let rows = usize::from(height.saturating_sub(2)).saturating_sub(lines.len());
    let first = (picker.selected + 1).saturating_sub(rows.max(1));
    for (index, item) in picker.items.iter().enumerate().skip(first).take(rows) {
        let style = if index == picker.selected {
            theme.style(Role::Selection)
        } else {
            theme.style(Role::Text)
        };
        let marker = if index == picker.selected { "▌" } else { " " };
        let done = if item.checked { "✓ " } else { "" };
        lines.push(Line::styled(format!("{marker}{done}{}", item.name), style));
    }
    let inner = draw_overlay(frame, area, &title, None, theme);
    frame.render_widget(Paragraph::new(lines), inner);
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
fn render_detail(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    now: i64,
) -> Vec<ShownImage> {
    let block = pane("Task", app.focus == Focus::Detail, theme).padding(Padding::horizontal(1));
    let inner = block.inner(area);
    let width = usize::from(area.width.saturating_sub(4));
    let muted = theme.style(Role::TextMuted);
    if app.shown_task_name().is_none() {
        frame.render_widget(block, area);
        return Vec::new();
    }
    let mut slots = Vec::new();
    let name = app.shown_task_name().unwrap_or("-");
    let title = || Line::styled(name.to_owned(), theme.style(Role::Title));
    let lines = match (app.selected_task_gid(), app.selected_detail()) {
        (Some(_), Some(Detail::Loaded(detail))) => {
            let (lines, image_slots) = detail::lines_with_images(detail, width, now, IMAGE_ROWS);
            slots = image_slots;
            styled(lines, theme)
        }
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
            let kind = app
                .selected()
                .and_then(|n| n.work_objects.first())
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
    image_areas(frame, inner, app, &slots, theme)
}

/// For each image slot in view: the area to draw a loaded image in, or a
/// note in the slot's first visible row (loading, failed, cut off).
fn image_areas(
    frame: &mut Frame,
    inner: Rect,
    app: &App,
    slots: &[detail::ImageSlot],
    theme: &Theme,
) -> Vec<ShownImage> {
    let (top, height) = (app.detail_scroll, usize::from(inner.height));
    let mut shown = Vec::new();
    for slot in slots {
        let (start, end) = (slot.line, slot.line + IMAGE_ROWS);
        if end <= top || start >= top + height {
            continue;
        }
        let first_row = inner.y + u16::try_from(start.saturating_sub(top)).unwrap_or(0);
        let fully_visible = start >= top && end <= top + height;
        let note = match app.images.get(&slot.attachment_gid) {
            Some(ImageState::Loaded(_)) if fully_visible => {
                shown.push(ShownImage {
                    attachment_gid: slot.attachment_gid.clone(),
                    area: Rect::new(inner.x, first_row, inner.width, IMAGE_ROWS as u16),
                });
                continue;
            }
            Some(ImageState::Loaded(_)) => {
                ("Scroll to see the whole image".to_owned(), Role::TextMuted)
            }
            Some(ImageState::Loading) => ("Loading image…".to_owned(), Role::TextMuted),
            Some(ImageState::Failed(message)) => (
                format!("Couldn't load the image: {message}"),
                Role::ToastError,
            ),
            None => continue,
        };
        let row = Rect::new(inner.x, first_row, inner.width, 1);
        frame.render_widget(Line::styled(note.0, theme.style(note.1)), row);
    }
    shown
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
    let on_board = app.screen == Screen::Board;
    // Position in the list on screen: the Inbox, or the board's column.
    let position = if on_board {
        app.board_view().and_then(|view| {
            let cards = view.columns().get(view.column())?.tasks.len();
            (cards > 0).then(|| (view.row() + 1, cards))
        })
    } else {
        (app.visible_len() > 0).then(|| (app.cursor + 1, app.visible_len()))
    };
    if let Some((at, of)) = position {
        left.push(Span::raw(format!(" {at}/{of}")));
    }
    if let Some(pending) = app.keys.pending() {
        left.push(Span::raw(format!("  {pending}")));
    }
    let loading = if on_board {
        matches!(
            app.board.as_ref().map(|b| &b.state),
            Some(BoardState::Loading)
        )
    } else {
        app.loading
    };
    if loading {
        left.push(Span::raw("  Loading…"));
    }
    if let Some(error) = app.error.as_ref().filter(|_| !on_board) {
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

    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }
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
        draw_sized(app, theme, 100, 24)
    }

    fn draw_sized(app: &App, theme: &Theme, width: u16, height: u16) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| {
                render(f, app, theme, NOW);
            })
            .unwrap();
        terminal
    }

    fn terminal_theme() -> Theme {
        Theme::builtin("terminal").unwrap()
    }

    #[test]
    fn the_board_gets_the_whole_body_and_two_rows_per_card() {
        // 24 rows: header, status and hint lines leave 21; borders leave 19.
        let sizes = sizes(Rect::new(0, 0, 120, 24));

        assert_eq!(sizes.board_width, 120);
        assert_eq!(sizes.board_card_rows, 9);
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

    // --- Board screen -----------------------------------------------------

    fn card(
        gid: &str,
        name: &str,
        assignee: Option<&str>,
        due_on: Option<&str>,
    ) -> crate::api::projects::BoardTask {
        crate::api::projects::BoardTask {
            gid: gid.into(),
            name: name.into(),
            assignee: assignee.map(|name| crate::api::tasks::Named {
                gid: "u".into(),
                name: name.into(),
            }),
            due_on: due_on.map(Into::into),
            due_at: None,
            memberships: Vec::new(),
        }
    }

    fn column(
        gid: &str,
        name: &str,
        tasks: Vec<crate::api::projects::BoardTask>,
    ) -> crate::core::Column {
        crate::core::Column {
            section: Some(crate::api::tasks::Named {
                gid: gid.into(),
                name: name.into(),
            }),
            tasks,
        }
    }

    /// Five sections; "Review" is empty. NOW is Mon 16 Nov 2026.
    fn sample_board() -> crate::core::Board {
        crate::core::Board {
            project: crate::api::tasks::Named {
                gid: "42".into(),
                name: "Website relaunch".into(),
            },
            columns: vec![
                column(
                    "s1",
                    "Backlog",
                    vec![
                        card("t1", "Pick a font pairing for headings", None, None),
                        card(
                            "t2",
                            "Audit image sizes",
                            Some("Grace Hopper"),
                            Some("2026-11-27"),
                        ),
                    ],
                ),
                column(
                    "s2",
                    "In progress",
                    vec![
                        card(
                            "t3",
                            "Rewrite the pricing page copy so it fits on one screen",
                            Some("Ada Lovelace"),
                            Some("2026-11-12"),
                        ),
                        card(
                            "t4",
                            "Cookie banner",
                            Some("Alan Turing"),
                            Some("2026-11-20"),
                        ),
                    ],
                ),
                column("s3", "Review", Vec::new()),
                column(
                    "s4",
                    "Done this week",
                    vec![card("t5", "Favicon", Some("Grace Hopper"), None)],
                ),
                column("s5", "Later", vec![card("t6", "Dark mode", None, None)]),
            ],
        }
    }

    fn board_app() -> App {
        let (mut app, _) = App::new_on_board("42".into());
        app.update(Msg::BoardLoaded {
            project_gid: "42".into(),
            result: Ok(sample_board()),
        });
        app
    }

    #[test]
    fn board_screen() {
        let mut app = board_app();

        insta::assert_snapshot!(draw_at(&mut app, 120).backend());
    }

    #[test]
    fn board_paged_to_the_right_shows_hidden_columns_on_both_sides() {
        let mut app = board_app();
        app.update(Msg::Resize(sizes(Rect::new(0, 0, 80, 24))));
        press(&mut app, "lll");

        insta::assert_snapshot!(draw_at(&mut app, 80).backend());
    }

    #[test]
    fn board_on_a_terminal_too_narrow_for_two_columns() {
        let mut app = board_app();

        insta::assert_snapshot!(draw_at(&mut app, 50).backend());
    }

    #[test]
    fn board_loading_screen() {
        let (mut app, _) = App::new_on_board("42".into());

        insta::assert_snapshot!(draw_at(&mut app, 80).backend());
    }

    #[test]
    fn board_error_screen() {
        let (mut app, _) = App::new_on_board("42".into());
        app.update(Msg::BoardLoaded {
            project_gid: "42".into(),
            result: Err("Asana API error 404: project: Not a recognized ID".into()),
        });

        insta::assert_snapshot!(draw_at(&mut app, 80).backend());
    }

    #[test]
    fn an_overdue_card_is_marked_with_more_than_color() {
        let mut app = board_app();

        let terminal = draw_at(&mut app, 120);

        assert_eq!(
            cell_style(&terminal, "!"),
            terminal_theme().style(Role::ToastError)
        );
    }

    #[test]
    fn no_color_board_has_no_colors_and_a_reversed_selection() {
        let mut app = board_app();
        app.update(Msg::Resize(sizes(Rect::new(0, 0, 100, 24))));

        let terminal = draw(&app, &Theme::no_color());

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

    #[test]
    fn the_board_status_shows_the_card_position_not_the_inbox_behind_it() {
        // The Inbox is still loading and failed once; neither is the board's.
        let mut app = board_app();
        app.error = Some("Inbox failed".into());
        app.update(Msg::Resize(sizes(Rect::new(0, 0, 120, 24))));
        press(&mut app, "lj");

        let terminal = draw_at(&mut app, 120);
        let buffer = terminal.backend().buffer();
        let status: String = (0..120)
            .map(|x| buffer[(x, 22)].symbol().to_owned())
            .collect();

        assert!(status.contains(" 2/2"), "{status:?}");
        assert!(!status.contains("Loading"), "{status:?}");
        assert!(!status.contains("Inbox failed"), "{status:?}");
    }

    #[test]
    fn an_open_card_fills_the_board() {
        let mut app = board_app();
        app.update(Msg::Resize(sizes(Rect::new(0, 0, 120, 24))));
        with_detail(&mut app);
        app.update(Msg::Key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        )));

        insta::assert_snapshot!(draw_at(&mut app, 120).backend());
    }

    #[test]
    fn an_open_cards_hints_offer_task_actions_but_not_read_state() {
        let mut app = board_app();
        app.update(Msg::Key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        )));

        let labels: Vec<&str> = key_hints(&app).into_iter().map(|(_, l)| l).collect();

        assert!(labels.contains(&"back"), "{labels:?}");
        assert!(labels.contains(&"move"), "{labels:?}");
        assert!(!labels.contains(&"read"), "{labels:?}");
    }

    #[test]
    fn the_board_hint_line_offers_columns_not_inbox_actions() {
        let app = board_app();

        let labels: Vec<&str> = key_hints(&app).into_iter().map(|(_, l)| l).collect();

        assert!(labels.contains(&"column"), "{labels:?}");
        assert!(labels.contains(&"refresh"), "{labels:?}");
        assert!(!labels.contains(&"read"), "{labels:?}");
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
            attachments: Vec::new(),
            comments_error: None,
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
            .draw(|f| {
                render(f, app, &terminal_theme(), NOW);
            })
            .unwrap();
        terminal
    }

    #[test]
    fn help_lists_every_key_binding_once_per_command() {
        use crate::tui::keys::{BINDINGS, sequence_label};
        let lines = help_lines(&terminal_theme());
        let text: String = lines.iter().map(|l| line_text(l) + "\n").collect();

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
            .map(|l| line_text(l) + "\n")
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

    // --- Images ---------------------------------------------------------------

    const IMAGE: &str = "1200000000000301";

    /// The selected task has one image attachment, scrolled so that its
    /// space starts `offset` rows below the top of the pane.
    fn app_with_image(state: Option<crate::tui::app::ImageState>, offset: isize) -> App {
        let mut app = app_with(fixture("notifications_page.json"));
        let gid = app.selected_task_gid().unwrap().to_owned();
        let text = std::fs::read_to_string("tests/fixtures/attachments.json").unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let task = std::fs::read_to_string("tests/fixtures/task_full.json").unwrap();
        let task: serde_json::Value = serde_json::from_str(&task).unwrap();
        let detail = crate::core::TaskDetail {
            task: serde_json::from_value(task["data"].clone()).unwrap(),
            comments: Vec::new(),
            activity_count: 0,
            attachments: serde_json::from_value(value["data"].clone()).unwrap(),
            comments_error: None,
        };
        app.update(Msg::DetailDue { gid: gid.clone() });
        app.update(Msg::DetailLoaded {
            gid,
            result: Ok(Box::new(detail.clone())),
        });
        app.images.clear();
        if let Some(state) = state {
            app.images.insert(IMAGE.into(), state);
        }
        let (_, slots) = detail::lines_with_images(
            &detail,
            app.sizes.detail_width,
            NOW,
            crate::tui::app::IMAGE_ROWS,
        );
        app.detail_scroll = (slots[0].line as isize - offset).max(0) as usize;
        app
    }

    fn draw_images(app: &App) -> (Terminal<TestBackend>, Vec<ShownImage>) {
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        let mut shown = Vec::new();
        terminal
            .draw(|f| shown = render(f, app, &terminal_theme(), NOW))
            .unwrap();
        (terminal, shown)
    }

    fn screen_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn loaded_image() -> crate::tui::app::ImageState {
        crate::tui::app::ImageState::Loaded(std::sync::Arc::new(image::DynamicImage::new_rgba8(
            4, 4,
        )))
    }

    #[test]
    fn a_loaded_image_in_full_view_is_handed_out_with_its_area() {
        let app = app_with_image(Some(loaded_image()), 2);

        let (_, shown) = draw_images(&app);

        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].attachment_gid, IMAGE);
        assert_eq!(
            usize::from(shown[0].area.height),
            crate::tui::app::IMAGE_ROWS
        );
        // Pane border + header row, then two rows of text above the image.
        assert_eq!(shown[0].area.y, 1 + 1 + 2);
    }

    #[test]
    fn a_partly_scrolled_out_image_is_not_drawn() {
        let app = app_with_image(Some(loaded_image()), -1);

        let (terminal, shown) = draw_images(&app);

        assert!(shown.is_empty());
        assert!(screen_text(&terminal).contains("Scroll to see the whole image"));
    }

    #[test]
    fn an_image_being_downloaded_shows_a_placeholder() {
        let app = app_with_image(Some(crate::tui::app::ImageState::Loading), 2);

        let (terminal, shown) = draw_images(&app);

        assert!(shown.is_empty());
        assert!(screen_text(&terminal).contains("Loading image…"));
    }

    #[test]
    fn a_failed_image_says_why() {
        let app = app_with_image(
            Some(crate::tui::app::ImageState::Failed("HTTP 403".into())),
            2,
        );

        let (terminal, _) = draw_images(&app);

        assert!(screen_text(&terminal).contains("Couldn't load the image: HTTP 403"));
    }

    #[test]
    fn overlays_hide_images() {
        let mut app = app_with_image(Some(loaded_image()), 2);
        app.help = true;

        let (_, shown) = draw_images(&app);

        assert!(shown.is_empty());
    }

    // --- Task search ------------------------------------------------------------

    fn app_searching(query: &str, results: usize) -> App {
        let mut app = app_with(fixture("notifications_page.json"));
        let mut picker = Picker::tasks();
        let seq = picker.set_query(query.into());
        let found = (0..results)
            .map(|i| crate::api::search::FoundTask {
                gid: i.to_string(),
                name: format!("Task number {i}"),
                completed: false,
            })
            .collect();
        picker.accept_tasks(seq, found);
        app.picker = Some(picker);
        app
    }

    #[test]
    fn the_task_search_overlay_asks_for_words_first() {
        let app = app_searching("", 0);

        let text = screen_text(&draw(&app, &terminal_theme()));

        assert!(text.contains("Search tasks"), "{text}");
        assert!(text.contains("Type to search tasks"));
    }

    #[test]
    fn completed_tasks_in_the_results_are_marked() {
        let mut app = app_searching("task", 2);
        app.picker.as_mut().unwrap().items[1].checked = true;

        let text = screen_text(&draw(&app, &terminal_theme()));

        assert!(text.contains("✓ Task number 1"), "{text}");
        assert!(!text.contains("✓ Task number 0"));
    }

    #[test]
    fn the_task_search_overlay_says_when_nothing_matched() {
        let app = app_searching("zzz", 0);

        assert!(screen_text(&draw(&app, &terminal_theme())).contains("No matching tasks"));
    }

    #[test]
    fn the_picker_scrolls_to_keep_the_selection_visible() {
        let mut app = app_searching("task", 30);
        app.picker.as_mut().unwrap().move_by(25);

        let text = screen_text(&draw(&app, &terminal_theme()));

        assert!(text.contains("Task number 25"), "{text}");
        assert!(!text.contains("Task number 0 "));
    }

    #[test]
    fn a_found_task_shows_its_name_while_loading() {
        let mut app = app_searching("task", 1);
        app.update(key_event(ratatui::crossterm::event::KeyCode::Enter));

        let text = screen_text(&draw(&app, &terminal_theme()));

        assert!(text.contains("Task number 0"), "{text}");
        assert!(text.contains("Loading task…"));
    }

    // --- Overlays ---------------------------------------------------------------

    #[test]
    fn overlays_are_centered_and_never_larger_than_the_screen() {
        let screen = Rect::new(0, 0, 100, 24);

        assert_eq!(centered(screen, 60, 10), Rect::new(20, 7, 60, 10));
        assert_eq!(centered(screen, 200, 50), screen);
        assert_eq!(
            centered(Rect::new(10, 5, 40, 10), 20, 4),
            Rect::new(20, 8, 20, 4)
        );
    }

    /// Checks that the overlay titled `title` is centered on the screen.
    fn assert_centered(terminal: &Terminal<TestBackend>, title: &str) {
        let buffer = terminal.backend().buffer();
        let (w, h) = (buffer.area.width, buffer.area.height);
        let row = |y: u16| -> Vec<&str> { (0..w).map(|x| buffer[(x, y)].symbol()).collect() };
        let (top, left) = (0..h)
            .find_map(|y| {
                let cells = row(y);
                let text: String = cells.concat();
                let at = text.find(title)?;
                // Column of the title's first character (cells are 1 wide here).
                let column = text[..at].chars().count() as u16;
                Some((y, column - 1))
            })
            .unwrap_or_else(|| panic!("no overlay titled {title:?}"));
        assert_eq!(buffer[(left, top)].symbol(), "┏");
        let right = (left..w)
            .find(|&x| buffer[(x, top)].symbol() == "┓")
            .unwrap();
        let bottom = (top..h)
            .find(|&y| buffer[(left, y)].symbol() == "┗")
            .unwrap();
        let (width, height) = (right - left + 1, bottom - top + 1);
        assert!(
            left.abs_diff(w - left - width) <= 1,
            "not centered horizontally: x={left} w={width}"
        );
        assert!(
            top.abs_diff(h - top - height) <= 1,
            "not centered vertically: y={top} h={height}"
        );
    }

    fn app_editing_a_comment(text: &str) -> App {
        let mut app = app_with(fixture("notifications_page.json"));
        press(&mut app, &format!("c{text}"));
        app
    }

    #[test]
    fn the_comment_editor_is_a_centered_overlay_with_the_text() {
        let app = app_editing_a_comment("Ship it");

        let terminal = draw(&app, &terminal_theme());
        let text = screen_text(&terminal);

        assert!(text.contains("Comment"), "{text}");
        assert!(text.contains("Ship it"));
        assert!(text.contains("Ctrl-s"));
        assert_centered(&terminal, " Comment on ");
    }

    #[test]
    fn the_editor_hints_say_how_to_save_cancel_and_use_editor() {
        let app = app_editing_a_comment("x");

        let hints: Vec<&str> = key_hints(&app).into_iter().map(|(_, what)| what).collect();

        assert_eq!(hints, ["post", "cancel", "open in $EDITOR"]);
    }

    #[test]
    fn the_picker_is_centered() {
        let app = app_searching("task", 3);

        assert_centered(&draw(&app, &terminal_theme()), " Search tasks ");
    }

    #[test]
    fn the_help_is_centered_and_sized_to_its_keys() {
        let mut app = app_with(fixture("notifications_page.json"));
        app.help = true;

        let terminal = draw_sized(&app, &terminal_theme(), 160, 40);

        assert_centered(&terminal, " Keys ");
        assert!(screen_text(&terminal).contains("search tasks in the workspace"));
    }

    #[test]
    fn the_section_picker_for_a_move_marks_the_current_section() {
        let mut app = app_with(fixture("notifications_page.json"));
        let mut picker = Picker::move_to_section(
            "t".into(),
            crate::api::tasks::Named {
                gid: "p".into(),
                name: "Webshop".into(),
            },
            Some("s2".into()),
        );
        picker.accept_sections(
            "p",
            vec![
                crate::api::tasks::Named {
                    gid: "s1".into(),
                    name: "To do".into(),
                },
                crate::api::tasks::Named {
                    gid: "s2".into(),
                    name: "Doing".into(),
                },
            ],
        );
        app.picker = Some(picker);

        let terminal = draw(&app, &terminal_theme());
        let text = screen_text(&terminal);

        assert!(text.contains("Move to a section of Webshop"), "{text}");
        assert!(text.contains("✓ Doing"));
        assert!(!text.contains("✓ To do"));
        assert!(!text.contains("No section"));
        assert_centered(&terminal, " Move to a section of Webshop ");
    }
}
