//! The board screen's drawing: its header line, the columns or lanes
//! layout, and the cards with their completion animation (F23, F26).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, HighlightSpacing, List, ListItem, ListState, Padding, Paragraph};

use super::pane;
use crate::api::projects::BoardTask;
use crate::present::card;
use crate::tui::app::{App, BoardState, is_local};
use crate::tui::board::{BoardView, CARD_LINES, STRIKE_FRAMES};
use crate::tui::theme::{Role, Theme};

/// " asn  Project · N open tasks" with the hidden columns at the right.
pub(super) fn render_board_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let view = app.board_view();
    let title = match view {
        Some(view) => {
            let open: usize = view.columns().iter().map(|c| c.tasks.len()).sum();
            match view.lane_field() {
                Some(field) => format!(
                    " {} · {open} open · lanes: {}",
                    view.project().name,
                    field.name
                ),
                None => format!(" {} · {open} open", view.project().name),
            }
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
pub(super) fn render_board(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, now: i64) {
    let message = match app.board().map(|b| &b.state) {
        Some(BoardState::Loaded(view)) => {
            match view.lane_field() {
                Some(field) => render_lanes(frame, area, view, &field.name, theme, now),
                None => render_columns(frame, area, view, theme, now),
            }
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

/// The visible columns side by side, each with its area and its pane: the
/// section's name and card count, focused when it's the selected column.
fn column_panes(area: Rect, view: &BoardView, theme: &Theme) -> Vec<(usize, Rect, Block<'static>)> {
    let visible = view.visible_columns();
    let count = visible.len() as u32;
    let areas = Layout::horizontal((0..count).map(|_| Constraint::Ratio(1, count))).split(area);
    visible
        .zip(areas.iter().copied())
        .map(|(index, area)| {
            let column = &view.columns()[index];
            let name = card::section_name(column.section.as_ref());
            let title = format!("{name} ({})", column.tasks.len());
            (index, area, pane(&title, index == view.column(), theme))
        })
        .collect()
}

fn render_columns(frame: &mut Frame, area: Rect, view: &BoardView, theme: &Theme, now: i64) {
    for (index, column_area, block) in column_panes(area, view, theme) {
        let column = &view.columns()[index];
        if column.tasks.is_empty() {
            let block = block.padding(Padding::horizontal(1));
            let text = Paragraph::new(Span::styled("no tasks", theme.style(Role::TextMuted)));
            frame.render_widget(text.block(block), column_area);
            continue;
        }
        // Inside the borders, after the one-cell selection marker.
        let text_width = usize::from(column_area.width.saturating_sub(3));
        let items: Vec<ListItem> = column
            .tasks
            .iter()
            .map(|task| ListItem::new(card_or_frame(view, task, text_width, theme, now)))
            .collect();
        let list = List::new(items)
            .block(block)
            .highlight_style(theme.style(Role::Selection))
            .highlight_symbol("▌")
            .highlight_spacing(HighlightSpacing::Always);
        let mut state = ListState::default()
            .with_offset(view.scroll(index))
            .with_selected((index == view.column()).then_some(view.row()));
        frame.render_stateful_widget(list, column_area, &mut state);
    }
}

/// The board in lanes (F26): each visible section a bordered column, and
/// in it, lane by lane, a header line (the lane's name and card count in
/// the first visible column, a border line in the others) over the lane's
/// cards. Lanes are as tall as their fullest cell, so they line up across
/// columns, and the grid scrolls as one.
fn render_lanes(
    frame: &mut Frame,
    area: Rect,
    view: &BoardView,
    field: &str,
    theme: &Theme,
    now: i64,
) {
    let first = view.visible_columns().start;
    for (index, column_area, block) in column_panes(area, view, theme) {
        let inner = block.inner(column_area);
        frame.render_widget(block, column_area);
        let focused = index == view.column();
        let width = usize::from(inner.width);
        // After the one-cell selection marker.
        let text_width = width.saturating_sub(1);
        let mut lines: Vec<Line> = Vec::new();
        for (l, lane) in view.lanes().iter().enumerate() {
            if index == first {
                let label = card::lane_name(lane.option.as_ref(), field);
                let cards: usize = lane.columns.iter().map(|c| c.tasks.len()).sum();
                lines.push(Line::styled(
                    card::truncate(&format!("{label} ({cards})"), width),
                    theme.style(Role::Title),
                ));
            } else {
                lines.push(Line::styled("─".repeat(width), theme.style(Role::Border)));
            }
            let here = focused && l == view.lane();
            let cell = &lane.columns[index].tasks;
            let mut cell_lines: Vec<Line> = Vec::new();
            for (row, task) in cell.iter().enumerate() {
                let selected = here && row == view.row();
                cell_lines.extend(
                    card_or_frame(view, task, text_width, theme, now)
                        .into_iter()
                        .map(|line| marked(line, selected, width, theme)),
                );
            }
            if cell.is_empty() && here {
                let none = Line::styled("no tasks", theme.style(Role::TextMuted));
                cell_lines.push(marked(none, true, width, theme));
            }
            cell_lines.resize(view.lane_cards(l) * CARD_LINES, Line::default());
            lines.extend(cell_lines);
        }
        let shown: Vec<Line> = lines
            .into_iter()
            .skip(view.grid_scroll())
            .take(usize::from(inner.height))
            .collect();
        frame.render_widget(Paragraph::new(shown), inner);
    }
}

/// Card `task`: its completion animation's frame while that plays, `creating…`
/// while Asana creates it (F32b), else the card.
fn card_or_frame<'a>(
    view: &BoardView,
    task: &BoardTask,
    width: usize,
    theme: &Theme,
    now: i64,
) -> Vec<Line<'a>> {
    if is_local(&task.gid) {
        let name = Line::styled(card::truncate(&task.name, width), theme.style(Role::Text));
        return vec![
            name,
            Line::styled("creating…", theme.style(Role::TextMuted)),
        ];
    }
    match view.completing() {
        Some((gid, frame)) if gid == task.gid => completing_card_lines(task, width, theme, frame),
        _ => card_lines(task, width, theme, now),
    }
}

/// A card line behind the one-cell selection marker (`▌` when `selected`),
/// filled to `width` and in the selection style when selected.
fn marked<'a>(line: Line<'a>, selected: bool, width: usize, theme: &Theme) -> Line<'a> {
    let marker = if selected { "▌" } else { " " };
    let mut spans = vec![Span::raw(marker)];
    spans.extend(line.spans);
    let mut line = Line::from(spans);
    if selected {
        let fill = width.saturating_sub(line.width());
        line.spans.push(Span::raw(" ".repeat(fill)));
        line = line.patch_style(theme.style(Role::Selection));
    }
    line
}

/// A card: the task name, then the assignee's initials and the due date.
fn card_lines<'a>(task: &BoardTask, width: usize, theme: &Theme, now: i64) -> Vec<Line<'a>> {
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
    vec![name, Line::from(meta)]
}

/// A completed card's animation frame (F23): the name struck through from
/// the left with `✓ done` below it, then the name line alone.
fn completing_card_lines<'a>(
    task: &BoardTask,
    width: usize,
    theme: &Theme,
    frame: u8,
) -> Vec<Line<'a>> {
    let name = card::truncate(&task.name, width);
    let crossed = |role| theme.style(role).add_modifier(Modifier::CROSSED_OUT);
    if frame >= STRIKE_FRAMES {
        return vec![Line::from(Span::styled(name, crossed(Role::TextMuted)))];
    }
    let characters = characters(&name);
    let struck = (characters.len() * usize::from(frame + 1)).div_ceil(usize::from(STRIKE_FRAMES));
    let split: usize = characters[..struck].iter().map(|c| c.len()).sum();
    // A band of colours that moves along with the strike.
    let mut spans: Vec<Span> = characters[..struck]
        .iter()
        .enumerate()
        .map(|(i, c)| Span::styled((*c).to_owned(), crossed(celebrate(i + usize::from(frame)))))
        .collect();
    spans.push(Span::styled(
        name[split..].to_owned(),
        theme.style(Role::Text),
    ));
    let done = Line::styled("✓ done", theme.style(celebrate(usize::from(frame))));
    vec![Line::from(spans), done]
}

/// `text` split into the characters a reader sees: a zero-width code point
/// (a combining mark, a variation selector, a zero-width joiner) stays with
/// the one before it, and so does the code point after a joiner, so accents
/// and joined emoji are struck and coloured whole.
fn characters(text: &str) -> Vec<&str> {
    use unicode_width::UnicodeWidthChar;
    let mut characters: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut joined = false;
    for (i, c) in text.char_indices() {
        let attaches = joined || c.width() == Some(0);
        if i > 0 && !attaches {
            characters.push(&text[start..i]);
            start = i;
        }
        joined = c == '\u{200d}';
    }
    if start < text.len() {
        characters.push(&text[start..]);
    }
    characters
}

/// The `n`th colour of the completion animation, cycling.
pub(super) fn celebrate(n: usize) -> Role {
    [
        Role::Celebrate1,
        Role::Celebrate2,
        Role::Celebrate3,
        Role::Celebrate4,
    ][n % 4]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(name: &str) -> crate::api::projects::BoardTask {
        crate::api::projects::BoardTask {
            custom_fields: Vec::new(),
            gid: "1200000000000001".into(),
            name: name.into(),
            assignee: None,
            due_on: None,
            due_at: None,
            memberships: Vec::new(),
        }
    }

    #[test]
    fn the_strike_colours_whole_characters_not_their_parts() {
        // A ZWJ emoji, then e with a combining acute accent.
        let name = "👩\u{200d}💻 e\u{301}x";
        let lines = completing_card_lines(&task(name), 40, &Theme::no_color(), STRIKE_FRAMES - 1);

        let parts: Vec<&str> = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(parts, ["👩\u{200d}💻", " ", "e\u{301}", "x", ""]);
    }

    #[test]
    fn the_strike_advances_by_whole_characters() {
        let name = "e\u{301}e\u{301}e\u{301}";
        let lines = completing_card_lines(&task(name), 40, &Theme::no_color(), 0);

        let struck: String = lines[0].spans[..lines[0].spans.len() - 1]
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(struck, "e\u{301}");
    }
}
