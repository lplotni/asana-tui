//! A project board: columns of cards, or the same columns split into lanes
//! by a single-select field (F26); the selected card, scrolling and
//! horizontal paging when not every column fits. Pure state; the
//! [`App`](super::app::App) feeds it keys and sizes, and the UI draws it.
//!
//! Both layouts are a grid of lanes × columns: the columns layout is one
//! lane holding the board's columns, so navigation is written once.

use crate::api::projects::BoardTask;
use crate::api::tasks::{CustomField, Named};
use crate::core::lanes::Lane;
use crate::core::{Board, Column};

/// The narrowest a column is drawn, in cells.
pub const MIN_COLUMN_WIDTH: usize = 28;

/// A completed card's animation (spec F23), in frames of 40 ms: the name is
/// struck through during the first [`STRIKE_FRAMES`], then the card shows
/// only its name line, and it leaves after [`COMPLETE_FRAMES`].
pub const STRIKE_FRAMES: u8 = 6;
pub const COMPLETE_FRAMES: u8 = 10;

/// Lines of a lane's header in the lanes grid.
pub const LANE_HEADER_LINES: usize = 1;
/// Lines of a card.
pub const CARD_LINES: usize = 2;

/// How the board is laid out: its columns, or lanes by a single-select field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layout {
    Columns,
    Lanes { field: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoardView {
    board: Board,
    layout: Layout,
    /// The lanes on screen; one lane (the board's columns) in the columns
    /// layout.
    grid: Vec<Lane>,
    /// The selected lane.
    lane: usize,
    /// The selected column.
    column: usize,
    /// The selected row, clamped to the selected cell's cards.
    row: usize,
    /// First visible card of each column (columns layout).
    scrolls: Vec<usize>,
    /// First visible line of the grid (lanes layout).
    grid_scroll: usize,
    /// First visible column.
    first: usize,
    /// Width available to the columns, in cells.
    width: usize,
    /// Cards that fit in a column.
    card_rows: usize,
    /// A completed card playing its animation: its GID and frame.
    completing: Option<(String, u8)>,
    /// A reload that arrived during the animation, shown when it ends.
    held: Option<Box<Board>>,
}

impl BoardView {
    pub fn new(board: Board) -> BoardView {
        let mut view = BoardView {
            scrolls: vec![0; board.columns.len()],
            grid_scroll: 0,
            board,
            layout: Layout::Columns,
            grid: Vec::new(),
            lane: 0,
            column: 0,
            row: 0,
            first: 0,
            width: 0,
            card_rows: 1,
            completing: None,
            held: None,
        };
        view.rebuild();
        view
    }

    /// Goes to the next layout: columns, then lanes by each single-select
    /// field (one named Initiative first, then the project's order), then
    /// columns again. The selected task stays selected. `false` (and no
    /// change) when the board has no single-select field.
    pub fn cycle_layout(&mut self) -> bool {
        let mut fields: Vec<&str> = self.board.fields.iter().map(|f| f.gid.as_str()).collect();
        if fields.is_empty() {
            return false;
        }
        fields.sort_by_key(|gid| {
            let name = self
                .board
                .fields
                .iter()
                .find(|f| f.gid == *gid)
                .map(|f| f.name.trim());
            !name.is_some_and(|n| n.eq_ignore_ascii_case("initiative"))
        });
        let next = match &self.layout {
            Layout::Columns => Some(fields[0]),
            Layout::Lanes { field } => fields
                .iter()
                .position(|f| f == field)
                .and_then(|i| fields.get(i + 1))
                .copied(),
        };
        let layout = next.map_or(Layout::Columns, |field| Layout::Lanes {
            field: field.to_owned(),
        });
        self.set_layout(layout);
        true
    }

    /// Shows `layout`, keeping the selected task selected; without one, the
    /// first lane with a card in the selected column.
    fn set_layout(&mut self, layout: Layout) {
        let task = self.selected_task().map(|t| t.gid.clone());
        self.layout = layout;
        self.grid_scroll = 0;
        self.rebuild();
        if task.as_deref().is_some_and(|gid| self.select(gid)) {
            return;
        }
        self.lane = self.lane_with_cards(0..self.grid.len(), false).unwrap_or(0);
        self.set_row(0);
    }

    /// The field the lanes are by (lanes layout), as GID and name.
    pub fn lane_field(&self) -> Option<Named> {
        let Layout::Lanes { field } = &self.layout else {
            return None;
        };
        self.board
            .fields
            .iter()
            .find(|f| &f.gid == field)
            .map(|f| Named {
                gid: f.gid.clone(),
                name: f.name.clone(),
            })
    }

    /// The name of the field the lanes are by (lanes layout).
    pub fn lane_field_name(&self) -> Option<&str> {
        let Layout::Lanes { field } = &self.layout else {
            return None;
        };
        self.board
            .fields
            .iter()
            .find(|f| &f.gid == field)
            .map(|f| f.name.as_str())
    }

    /// First visible line of the lanes grid (lanes layout).
    pub fn grid_scroll(&self) -> usize {
        self.grid_scroll
    }

    /// How many cards tall lane `lane` is: its fullest cell, at least one.
    pub fn lane_cards(&self, lane: usize) -> usize {
        self.grid
            .get(lane)
            .and_then(|l| l.columns.iter().map(|c| c.tasks.len()).max())
            .unwrap_or(0)
            .max(1)
    }

    /// The first line of lane `lane` in the grid.
    pub fn lane_top(&self, lane: usize) -> usize {
        (0..lane)
            .map(|l| LANE_HEADER_LINES + self.lane_cards(l) * CARD_LINES)
            .sum()
    }

    /// The card playing its completion animation, and its frame.
    pub fn completing(&self) -> Option<(&str, u8)> {
        self.completing
            .as_ref()
            .map(|(gid, frame)| (gid.as_str(), *frame))
    }

    /// Starts card `gid`'s completion animation. `false` if it isn't here.
    pub fn start_completing(&mut self, gid: &str) -> bool {
        let here = self
            .board
            .columns
            .iter()
            .any(|c| c.tasks.iter().any(|t| t.gid == gid));
        if here {
            self.finish_completing();
            self.completing = Some((gid.to_owned(), 0));
        }
        here
    }

    /// The next frame. After the last one the card leaves, and a reload held
    /// back meanwhile is shown. Returns whether the animation goes on.
    pub fn advance(&mut self) -> bool {
        let Some((_, frame)) = &mut self.completing else {
            return false;
        };
        *frame += 1;
        if *frame < COMPLETE_FRAMES {
            return true;
        }
        self.finish_completing();
        false
    }

    /// Stops card `gid`'s animation and keeps the card (its task was
    /// reopened, or completing it failed).
    pub fn cancel_completing(&mut self, gid: &str) {
        if self.completing().is_some_and(|(g, _)| g == gid) {
            self.completing = None;
            if let Some(board) = self.held.take() {
                self.replace(*board);
            }
        }
    }

    /// Ends a running animation at once: the card leaves.
    fn finish_completing(&mut self) {
        if let Some((gid, _)) = self.completing.take() {
            self.remove(&gid);
            if let Some(board) = self.held.take() {
                self.replace(*board);
            }
        }
    }

    pub fn project(&self) -> &Named {
        &self.board.project
    }

    /// The board's columns, as in the columns layout.
    pub fn columns(&self) -> &[Column] {
        &self.board.columns
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// The lanes on screen (one lane in the columns layout).
    pub fn lanes(&self) -> &[Lane] {
        &self.grid
    }

    pub fn lane(&self) -> usize {
        self.lane
    }

    pub fn column(&self) -> usize {
        self.column
    }

    pub fn row(&self) -> usize {
        self.row
    }

    /// First visible card of column `index` (columns layout).
    pub fn scroll(&self, index: usize) -> usize {
        self.scrolls.get(index).copied().unwrap_or(0)
    }

    pub fn selected_task(&self) -> Option<&BoardTask> {
        self.cell(self.lane, self.column).get(self.row)
    }

    /// The selected card's single-select fields, as the board knows them
    /// (the project's fields and the card's values, F24), shaped like a
    /// task's own so `F` works before the card's detail loads.
    pub fn selected_fields(&self) -> Option<Vec<CustomField>> {
        let task = self.selected_task()?;
        Some(
            self.board
                .fields
                .iter()
                .map(|field| {
                    let current = task.option_of(&field.gid);
                    CustomField {
                        gid: field.gid.clone(),
                        name: field.name.clone(),
                        display_value: None,
                        resource_subtype: "enum".into(),
                        enum_value: field
                            .options
                            .iter()
                            .find(|o| current == Some(o.gid.as_str()))
                            .map(|o| Named {
                                gid: o.gid.clone(),
                                name: o.name.clone(),
                            }),
                        enum_options: field.options.clone(),
                    }
                })
                .collect(),
        )
    }

    /// Sets the space the columns get: `width` cells across, `card_rows`
    /// cards down. Keeps the selection in view.
    pub fn resize(&mut self, width: usize, card_rows: usize) {
        self.width = width;
        self.card_rows = card_rows.max(1);
        self.follow();
    }

    /// The columns on screen: as many [`MIN_COLUMN_WIDTH`]-wide columns as
    /// fit (at least one), always including the selected one.
    pub fn visible_columns(&self) -> std::ops::Range<usize> {
        let count = self.fitting_columns().min(self.column_count());
        self.first..self.first + count
    }

    /// How many columns are hidden to the left and to the right.
    pub fn hidden(&self) -> (usize, usize) {
        let visible = self.visible_columns();
        (visible.start, self.column_count() - visible.end)
    }

    /// Down `n` cards: within the cell, then on into the same column of the
    /// next lane that has a card there.
    pub fn down(&mut self, n: usize) {
        for _ in 0..n {
            if self.row + 1 < self.cell(self.lane, self.column).len() {
                self.row += 1;
            } else if let Some(lane) = self.lane_with_cards(self.lane + 1..self.grid.len(), false) {
                self.lane = lane;
                self.row = 0;
            } else {
                break;
            }
        }
        self.set_row(self.row);
    }

    /// Up `n` cards: within the cell, then on into the last card of the same
    /// column of the previous lane that has one.
    pub fn up(&mut self, n: usize) {
        for _ in 0..n {
            if self.row > 0 {
                self.row -= 1;
            } else if let Some(lane) = self.lane_with_cards(0..self.lane, true) {
                self.lane = lane;
                self.row = self.cell(lane, self.column).len().saturating_sub(1);
            } else {
                break;
            }
        }
        self.set_row(self.row);
    }

    /// The first card of the column, in the first lane that has one.
    pub fn top(&mut self) {
        if let Some(lane) = self.lane_with_cards(0..self.grid.len(), false) {
            self.lane = lane;
        }
        self.set_row(0);
    }

    /// The last card of the column, in the last lane that has one.
    pub fn bottom(&mut self) {
        if let Some(lane) = self.lane_with_cards(0..self.grid.len(), true) {
            self.lane = lane;
        }
        self.set_row(usize::MAX);
    }

    pub fn right(&mut self, n: usize) {
        let last = self.column_count().saturating_sub(1);
        self.column = self.column.saturating_add(n).min(last);
        self.set_row(self.row);
    }

    pub fn left(&mut self, n: usize) {
        self.column = self.column.saturating_sub(n);
        self.set_row(self.row);
    }

    /// Selects task `gid` and scrolls it into view. Returns whether it's on
    /// the board; if not, the selection stays.
    pub fn select(&mut self, gid: &str) -> bool {
        let found = self.position_of(gid);
        self.select_position(found)
    }

    /// Selects the first card in grid order (lane by lane, column by column,
    /// top to bottom) whose name contains `query`, ignoring case. Returns
    /// whether one did; if not, the selection stays.
    pub fn first_match(&mut self, query: &str) -> bool {
        let first = self.matches(query).into_iter().next();
        self.select_position(first)
    }

    /// Selects the next (or, not `forward`, previous) matching card after
    /// the selected one, wrapping around the board.
    pub fn next_match(&mut self, query: &str, forward: bool) -> bool {
        let matches = self.matches(query);
        let here = (self.lane, self.column, self.row);
        let next = if forward {
            matches.iter().find(|&&m| m > here).or(matches.first())
        } else {
            matches.iter().rev().find(|&&m| m < here).or(matches.last())
        };
        self.select_position(next.copied())
    }

    /// (lane, column, row) of every card whose name contains `query`, in
    /// grid order.
    fn matches(&self, query: &str) -> Vec<(usize, usize, usize)> {
        let query = query.to_lowercase();
        self.positions()
            .filter(|&(l, c, r)| {
                self.grid[l].columns[c].tasks[r]
                    .name
                    .to_lowercase()
                    .contains(&query)
            })
            .collect()
    }

    /// Every card's (lane, column, row), in grid order.
    fn positions(&self) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
        self.grid.iter().enumerate().flat_map(|(l, lane)| {
            lane.columns
                .iter()
                .enumerate()
                .flat_map(move |(c, column)| (0..column.tasks.len()).map(move |r| (l, c, r)))
        })
    }

    fn position_of(&self, gid: &str) -> Option<(usize, usize, usize)> {
        self.positions()
            .find(|&(l, c, r)| self.grid[l].columns[c].tasks[r].gid == gid)
    }

    fn select_position(&mut self, position: Option<(usize, usize, usize)>) -> bool {
        let Some((lane, column, row)) = position else {
            return false;
        };
        self.lane = lane;
        self.column = column;
        self.set_row(row);
        true
    }

    /// Takes card `gid` off the board (e.g. once completed). Another selected
    /// card stays selected; if `gid` was the selected one, the selection
    /// keeps its place, clamped. Returns whether the card was there.
    pub fn remove(&mut self, gid: &str) -> bool {
        let kept = self
            .selected_task()
            .map(|t| t.gid.clone())
            .filter(|selected| selected != gid);
        let mut removed = false;
        for column in &mut self.board.columns {
            let before = column.tasks.len();
            column.tasks.retain(|t| t.gid != gid);
            removed |= column.tasks.len() != before;
        }
        self.rebuild();
        if !kept.is_some_and(|selected| self.select(&selected)) {
            self.set_row(self.row);
        }
        removed
    }

    /// Shows a reloaded board. The selection stays on the same task wherever
    /// it is now; if it's gone, on the same position of the same section
    /// (or column), clamped.
    pub fn replace(&mut self, board: Board) {
        if self.completing.is_some() {
            self.held = Some(Box::new(board));
            return;
        }
        let task = self.selected_task().map(|t| t.gid.clone());
        let section = self
            .board
            .columns
            .get(self.column)
            .and_then(|c| c.section.as_ref())
            .map(|s| s.gid.clone());
        let option = self
            .grid
            .get(self.lane)
            .and_then(|l| l.option.as_ref())
            .map(|o| o.gid.clone());
        let (lane, column, row) = (self.lane, self.column, self.row);
        self.scrolls = vec![0; board.columns.len()];
        self.board = board;
        self.rebuild();
        if task.as_deref().is_some_and(|gid| self.select(gid)) {
            return;
        }
        self.lane = option
            .and_then(|gid| {
                self.grid
                    .iter()
                    .position(|l| l.option.as_ref().is_some_and(|o| o.gid == gid))
            })
            .unwrap_or(lane)
            .min(self.grid.len().saturating_sub(1));
        self.column = section
            .and_then(|gid| {
                self.board
                    .columns
                    .iter()
                    .position(|c| c.section.as_ref().is_some_and(|s| s.gid == gid))
            })
            .unwrap_or(column)
            .min(self.column_count().saturating_sub(1));
        self.set_row(row);
    }

    /// Recomputes the lanes from the board and the layout. A lane field the
    /// board no longer has means the columns layout again.
    fn rebuild(&mut self) {
        if let Layout::Lanes { field } = &self.layout
            && !self.board.fields.iter().any(|f| &f.gid == field)
        {
            self.layout = Layout::Columns;
        }
        self.grid = match &self.layout {
            Layout::Columns => Vec::new(),
            Layout::Lanes { field } => crate::core::lanes::lanes(&self.board, field),
        };
        if self.grid.is_empty() {
            // The columns layout, or lanes of a board without cards.
            self.grid = vec![Lane {
                option: None,
                columns: self.board.columns.clone(),
            }];
        }
        self.lane = self.lane.min(self.grid.len() - 1);
    }

    /// The cards of `column` in `lane`.
    fn cell(&self, lane: usize, column: usize) -> &[BoardTask] {
        self.grid
            .get(lane)
            .and_then(|l| l.columns.get(column))
            .map_or(&[], |c| c.tasks.as_slice())
    }

    /// The first (or, `last`, the last) lane in `lanes` with a card in the
    /// selected column.
    fn lane_with_cards(&self, lanes: std::ops::Range<usize>, last: bool) -> Option<usize> {
        let mut with = lanes.filter(|&l| !self.cell(l, self.column).is_empty());
        if last { with.next_back() } else { with.next() }
    }

    fn column_count(&self) -> usize {
        self.board.columns.len()
    }

    fn fitting_columns(&self) -> usize {
        (self.width / MIN_COLUMN_WIDTH).max(1)
    }

    /// Selects `row` of the current cell, clamped, and scrolls to it.
    fn set_row(&mut self, row: usize) {
        let cards = self.cell(self.lane, self.column).len();
        self.row = row.min(cards.saturating_sub(1));
        self.follow();
    }

    /// Scrolls to the selected card, and pages the columns to the selected
    /// column.
    fn follow(&mut self) {
        match self.layout {
            Layout::Columns => {
                if let Some(scroll) = self.scrolls.get_mut(self.column) {
                    *scroll = keep_visible(*scroll, self.row, self.card_rows);
                }
            }
            Layout::Lanes { .. } => {
                // The selected card's lines, with its lane's header when it's
                // the lane's first card.
                let card = self.lane_top(self.lane) + LANE_HEADER_LINES + self.row * CARD_LINES;
                let start = if self.row == 0 {
                    card - LANE_HEADER_LINES
                } else {
                    card
                };
                let end = card + CARD_LINES;
                let window = self.card_rows * CARD_LINES;
                if end > self.grid_scroll + window {
                    self.grid_scroll = end.saturating_sub(window);
                }
                if start < self.grid_scroll {
                    self.grid_scroll = start;
                }
                // Never further than needed to show the grid's end.
                let total = self.lane_top(self.grid.len());
                self.grid_scroll = self.grid_scroll.min(total.saturating_sub(window));
            }
        }
        let count = self.fitting_columns().min(self.column_count()).max(1);
        self.first = keep_visible(self.first, self.column, count)
            .min(self.column_count().saturating_sub(count));
    }
}

/// The offset of a window `size` long that starts at `offset` if possible
/// and still contains `index`.
fn keep_visible(offset: usize, index: usize, size: usize) -> usize {
    if index < offset {
        index
    } else if index >= offset + size {
        index + 1 - size
    } else {
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(gid: &str, name: &str) -> Named {
        Named {
            gid: gid.into(),
            name: name.into(),
        }
    }

    fn card(gid: &str) -> BoardTask {
        BoardTask {
            custom_fields: Vec::new(),
            gid: gid.into(),
            name: format!("Task {gid}"),
            assignee: None,
            due_on: None,
            due_at: None,
            memberships: Vec::new(),
        }
    }

    /// A board with one column per entry of `sizes`, holding that many cards.
    /// Column `c` is section `s{c}`; its cards are `c{c}-{row}`.
    fn board(sizes: &[usize]) -> Board {
        Board {
            project: named("p", "Project"),
            fields: Vec::new(),
            columns: sizes
                .iter()
                .enumerate()
                .map(|(c, &n)| Column {
                    section: Some(named(&format!("s{c}"), &format!("Section {c}"))),
                    tasks: (0..n).map(|row| card(&format!("c{c}-{row}"))).collect(),
                })
                .collect(),
        }
    }

    fn view(sizes: &[usize]) -> BoardView {
        BoardView::new(board(sizes))
    }

    fn selected(view: &BoardView) -> Option<&str> {
        view.selected_task().map(|t| t.gid.as_str())
    }

    #[test]
    fn starts_on_the_first_card_of_the_first_column() {
        assert_eq!(selected(&view(&[2, 3])), Some("c0-0"));
    }

    #[test]
    fn down_and_up_stay_within_the_column() {
        let mut v = view(&[3, 3]);

        v.down(1);
        assert_eq!(selected(&v), Some("c0-1"));
        v.down(10);
        assert_eq!(selected(&v), Some("c0-2"));
        v.up(1);
        assert_eq!(selected(&v), Some("c0-1"));
        v.up(10);
        assert_eq!(selected(&v), Some("c0-0"));
    }

    #[test]
    fn top_and_bottom_go_to_the_ends_of_the_column() {
        let mut v = view(&[4]);

        v.bottom();
        assert_eq!(selected(&v), Some("c0-3"));
        v.top();
        assert_eq!(selected(&v), Some("c0-0"));
    }

    #[test]
    fn right_and_left_keep_the_row_clamped_to_the_new_column() {
        let mut v = view(&[5, 2, 5]);
        v.down(3);

        v.right(1);
        assert_eq!(selected(&v), Some("c1-1"));
        v.right(1);
        assert_eq!(selected(&v), Some("c2-1"));
        v.left(2);
        assert_eq!(selected(&v), Some("c0-1"));
    }

    #[test]
    fn left_and_right_stop_at_the_first_and_last_column() {
        let mut v = view(&[1, 1]);

        v.left(1);
        assert_eq!(v.column(), 0);
        v.right(5);
        assert_eq!(v.column(), 1);
    }

    #[test]
    fn an_empty_column_can_be_selected_and_has_no_card() {
        let mut v = view(&[2, 0, 2]);

        v.right(1);
        assert_eq!(v.column(), 1);
        assert_eq!(selected(&v), None);
        v.down(1);
        assert_eq!(selected(&v), None);
        v.right(1);
        assert_eq!(selected(&v), Some("c2-0"));
    }

    #[test]
    fn a_board_without_columns_never_panics() {
        let mut v = view(&[]);

        v.down(1);
        v.right(1);
        v.bottom();
        v.resize(120, 10);

        assert_eq!(selected(&v), None);
        assert_eq!(v.visible_columns(), 0..0);
    }

    #[test]
    fn every_column_is_shown_when_all_fit() {
        let mut v = view(&[1; 5]);

        v.resize(200, 10);

        assert_eq!(v.visible_columns(), 0..5);
        assert_eq!(v.hidden(), (0, 0));
    }

    #[test]
    fn as_many_columns_as_fit_are_shown_and_the_rest_counted_as_hidden() {
        let mut v = view(&[1; 10]);

        v.resize(120, 10);

        assert_eq!(v.visible_columns(), 0..4);
        assert_eq!(v.hidden(), (0, 6));
    }

    #[test]
    fn moving_past_the_last_visible_column_pages_to_keep_it_in_view() {
        let mut v = view(&[1; 10]);
        v.resize(80, 10);

        v.right(2);

        assert_eq!(v.visible_columns(), 1..3);
        assert_eq!(v.hidden(), (1, 7));
        v.left(2);
        assert_eq!(v.visible_columns(), 0..2);
    }

    #[test]
    fn a_terminal_narrower_than_two_columns_shows_one() {
        let mut v = view(&[1; 3]);

        v.resize(40, 10);

        assert_eq!(v.visible_columns(), 0..1);
    }

    #[test]
    fn narrowing_the_terminal_keeps_the_selected_column_visible() {
        let mut v = view(&[1; 6]);
        v.resize(200, 10);
        v.right(5);

        v.resize(80, 10);

        assert!(v.visible_columns().contains(&5));
    }

    #[test]
    fn each_column_scrolls_to_keep_its_selected_card_visible() {
        let mut v = view(&[10, 10]);
        v.resize(120, 3);

        v.down(5);
        assert_eq!(v.scroll(0), 3);
        v.right(1);
        assert_eq!(v.scroll(0), 3);
        assert_eq!(v.scroll(1), 3);
        v.top();
        assert_eq!(v.scroll(1), 0);
    }

    #[test]
    fn removing_the_selected_card_selects_the_one_that_moves_up() {
        let mut v = view(&[3]);
        v.down(1);

        assert!(v.remove("c0-1"));

        assert_eq!(selected(&v), Some("c0-2"));
    }

    #[test]
    fn removing_the_last_card_of_a_column_selects_the_one_above() {
        let mut v = view(&[2]);
        v.bottom();

        assert!(v.remove("c0-1"));

        assert_eq!(selected(&v), Some("c0-0"));
    }

    #[test]
    fn removing_a_card_above_the_selected_one_keeps_the_selection_on_its_card() {
        let mut v = view(&[3]);
        v.down(1);

        assert!(v.remove("c0-0"));

        assert_eq!(selected(&v), Some("c0-1"));
    }

    #[test]
    fn a_completed_card_leaving_keeps_the_selection_on_the_card_moved_to() {
        let mut v = view(&[3]);
        assert!(v.start_completing("c0-0"));
        v.down(1);

        while v.advance() {}

        assert_eq!(selected(&v), Some("c0-1"));
    }

    #[test]
    fn removing_a_card_that_is_not_on_the_board_changes_nothing() {
        let mut v = view(&[2]);

        assert!(!v.remove("elsewhere"));

        assert_eq!(v.columns()[0].tasks.len(), 2);
    }

    #[test]
    fn selecting_a_task_moves_to_its_column_and_row_and_into_view() {
        let mut v = view(&[1, 1, 1, 6]);
        v.resize(80, 2);

        assert!(v.select("c3-4"));

        assert_eq!(selected(&v), Some("c3-4"));
        assert!(v.visible_columns().contains(&3));
        assert_eq!(v.scroll(3), 3);
    }

    #[test]
    fn selecting_a_task_that_is_not_on_the_board_changes_nothing() {
        let mut v = view(&[2, 2]);
        v.right(1);

        assert!(!v.select("elsewhere"));

        assert_eq!(selected(&v), Some("c1-0"));
    }

    /// Columns of cards named by `names`, so searches have something to find.
    fn named_view(columns: &[&[&str]]) -> BoardView {
        let mut board = board(&columns.iter().map(|c| c.len()).collect::<Vec<_>>());
        for (column, names) in board.columns.iter_mut().zip(columns) {
            for (task, name) in column.tasks.iter_mut().zip(names.iter()) {
                task.name = (*name).to_owned();
            }
        }
        BoardView::new(board)
    }

    #[test]
    fn the_first_match_is_found_in_board_order_ignoring_case() {
        let mut v = named_view(&[&["Logo", "Fonts"], &["Font sizes", "Cookie banner"]]);
        v.right(1);

        assert!(v.first_match("FONT"));

        assert_eq!(selected(&v), Some("c0-1"));
    }

    #[test]
    fn the_next_match_goes_column_by_column_and_wraps() {
        let mut v = named_view(&[&["Logo", "Fonts"], &["Font sizes"], &["Webfont"]]);
        v.first_match("font");

        assert!(v.next_match("font", true));
        assert_eq!(selected(&v), Some("c1-0"));
        assert!(v.next_match("font", true));
        assert_eq!(selected(&v), Some("c2-0"));
        assert!(v.next_match("font", true));
        assert_eq!(selected(&v), Some("c0-1"));
    }

    #[test]
    fn the_previous_match_goes_backwards_and_wraps() {
        let mut v = named_view(&[&["Fonts"], &["Logo"], &["Webfont"]]);
        v.first_match("font");

        assert!(v.next_match("font", false));

        assert_eq!(selected(&v), Some("c2-0"));
    }

    #[test]
    fn no_match_leaves_the_selection_where_it_is() {
        let mut v = named_view(&[&["Logo"], &["Fonts"]]);
        v.right(1);

        assert!(!v.first_match("banner"));
        assert!(!v.next_match("banner", true));

        assert_eq!(selected(&v), Some("c1-0"));
    }

    #[test]
    fn replacing_the_board_keeps_the_selected_task_even_in_another_column() {
        let mut v = view(&[3, 3]);
        v.down(2);
        assert_eq!(v.project().name, "Project");
        let mut moved = board(&[2, 3]);
        let task = card("c0-2");
        moved.columns[1].tasks.insert(1, task);

        v.replace(moved);

        assert_eq!(selected(&v), Some("c0-2"));
        assert_eq!(v.column(), 1);
    }

    #[test]
    fn replacing_the_board_clamps_when_the_selected_task_is_gone() {
        let mut v = view(&[2, 4]);
        v.right(1);
        v.bottom();

        v.replace(board(&[2, 2]));

        assert_eq!(v.column(), 1);
        assert_eq!(selected(&v), Some("c1-1"));
    }

    #[test]
    fn replacing_the_board_with_fewer_columns_clamps_the_column() {
        let mut v = view(&[1, 1, 1]);
        v.right(2);

        v.replace(board(&[1]));

        assert_eq!(selected(&v), Some("c0-0"));
    }

    // --- Completion animation (F23) ------------------------------------------

    #[test]
    fn a_completing_card_stays_until_its_last_frame() {
        let mut v = view(&[3]);
        assert!(v.start_completing("c0-0"));

        for _ in 1..COMPLETE_FRAMES {
            assert!(v.advance());
            assert_eq!(selected(&v), Some("c0-0"));
        }
        assert!(!v.advance(), "the last frame ends it");

        assert_eq!(v.completing(), None);
        assert_eq!(selected(&v), Some("c0-1"));
        assert!(!v.advance());
    }

    #[test]
    fn a_card_that_isnt_on_the_board_doesnt_animate() {
        let mut v = view(&[2]);

        assert!(!v.start_completing("elsewhere"));
        assert_eq!(v.completing(), None);
    }

    #[test]
    fn a_reload_during_the_animation_is_shown_when_it_ends() {
        let mut v = view(&[3]);
        v.start_completing("c0-0");
        let mut reloaded = board(&[3]);
        reloaded.columns[0].tasks.remove(0);
        reloaded.columns[0].tasks.push(card("new"));

        v.replace(reloaded);
        assert_eq!(v.columns()[0].tasks.len(), 3, "held back");
        while v.advance() {}

        let gids: Vec<&str> = v.columns()[0]
            .tasks
            .iter()
            .map(|t| t.gid.as_str())
            .collect();
        assert_eq!(gids, ["c0-1", "c0-2", "new"]);
    }

    #[test]
    fn cancelling_keeps_the_card_and_shows_a_held_reload() {
        let mut v = view(&[2]);
        v.start_completing("c0-0");
        v.advance();
        v.replace(board(&[2]));

        v.cancel_completing("c0-0");

        assert_eq!(v.completing(), None);
        assert_eq!(selected(&v), Some("c0-0"));
        assert!(!v.advance());
    }

    #[test]
    fn completing_another_card_finishes_the_first_at_once() {
        let mut v = view(&[3]);
        v.start_completing("c0-0");

        v.start_completing("c0-1");

        assert_eq!(v.completing(), Some(("c0-1", 0)));
        assert!(v.columns()[0].tasks.iter().all(|t| t.gid != "c0-0"));
    }

    // --- Lanes (F26) ---

    use crate::api::projects::{FieldValue, OptionRef, SelectField, SelectOption};

    /// A card with Initiative (`f1`) set to `option`, if any.
    fn lane_card(gid: &str, option: Option<&str>) -> BoardTask {
        BoardTask {
            custom_fields: vec![FieldValue {
                gid: "f1".into(),
                enum_value: option.map(|o| OptionRef { gid: o.into() }),
            }],
            ..card(gid)
        }
    }

    fn select_field(gid: &str, name: &str, options: &[(&str, &str)]) -> SelectField {
        SelectField {
            gid: gid.into(),
            name: name.into(),
            options: options
                .iter()
                .map(|(gid, name)| SelectOption {
                    gid: (*gid).into(),
                    name: (*name).into(),
                    enabled: true,
                })
                .collect(),
        }
    }

    /// Priority (f2, no values) listed before Initiative (f1: o1 Checkout,
    /// o2 Onboarding). By Initiative: Checkout has c | d, Onboarding a | —,
    /// none b | e.
    fn lane_board() -> Board {
        Board {
            project: named("p", "Project"),
            fields: vec![
                select_field("f2", "Priority", &[("p1", "High")]),
                select_field(
                    "f1",
                    "Initiative",
                    &[("o1", "Checkout"), ("o2", "Onboarding")],
                ),
            ],
            columns: vec![
                Column {
                    section: Some(named("s0", "Backlog")),
                    tasks: vec![
                        lane_card("a", Some("o2")),
                        lane_card("b", None),
                        lane_card("c", Some("o1")),
                    ],
                },
                Column {
                    section: Some(named("s1", "Done")),
                    tasks: vec![lane_card("d", Some("o1")), lane_card("e", None)],
                },
            ],
        }
    }

    /// The lane board in lanes by Initiative.
    fn lanes_view() -> BoardView {
        let mut v = BoardView::new(lane_board());
        assert!(v.cycle_layout());
        v
    }

    #[test]
    fn v_cycles_columns_then_initiative_first_then_the_other_fields() {
        let mut v = BoardView::new(lane_board());

        let mut seen = vec![v.layout().clone()];
        for _ in 0..3 {
            assert!(v.cycle_layout());
            seen.push(v.layout().clone());
        }

        let lanes = |field: &str| Layout::Lanes {
            field: field.into(),
        };
        assert_eq!(
            seen,
            [Layout::Columns, lanes("f1"), lanes("f2"), Layout::Columns]
        );
    }

    #[test]
    fn a_board_without_single_select_fields_stays_in_columns() {
        let mut v = view(&[2]);

        assert!(!v.cycle_layout());
        assert_eq!(v.layout(), &Layout::Columns);
    }

    #[test]
    fn lanes_follow_the_option_order_then_cards_without_a_value() {
        let v = lanes_view();

        let options: Vec<_> = v
            .lanes()
            .iter()
            .map(|l| l.option.as_ref().map(|o| o.name.as_str()))
            .collect();
        assert_eq!(options, [Some("Checkout"), Some("Onboarding"), None]);
    }

    #[test]
    fn switching_layouts_keeps_the_selected_task() {
        let mut v = BoardView::new(lane_board());
        v.select("a");

        v.cycle_layout();
        assert_eq!((selected(&v), v.lane()), (Some("a"), 1));
        v.select("e");
        v.cycle_layout();
        v.cycle_layout();
        assert_eq!(v.layout(), &Layout::Columns);
        assert_eq!(selected(&v), Some("e"));
    }

    #[test]
    fn down_goes_on_into_the_same_column_of_the_next_lane_with_a_card() {
        let mut v = lanes_view();
        v.select("c");

        v.down(1);
        assert_eq!(selected(&v), Some("a"));
        v.down(1);
        assert_eq!(selected(&v), Some("b"));
        v.down(1);
        assert_eq!(selected(&v), Some("b"));

        v.select("d");
        v.down(1);
        assert_eq!(selected(&v), Some("e"), "Onboarding has no card in Done");
    }

    #[test]
    fn up_goes_back_to_the_last_card_of_the_previous_lane_with_one() {
        let mut v = lanes_view();
        v.select("e");

        v.up(1);
        assert_eq!(selected(&v), Some("d"));
        v.up(1);
        assert_eq!(selected(&v), Some("d"));
    }

    #[test]
    fn left_and_right_stay_in_the_lane_even_on_an_empty_cell() {
        let mut v = lanes_view();
        v.select("a");

        v.right(1);
        assert_eq!((selected(&v), v.lane(), v.column()), (None, 1, 1));
        v.down(1);
        assert_eq!(selected(&v), Some("e"));
    }

    #[test]
    fn top_and_bottom_go_to_the_ends_of_the_column_across_lanes() {
        let mut v = lanes_view();
        v.select("a");

        v.bottom();
        assert_eq!(selected(&v), Some("b"));
        v.top();
        assert_eq!(selected(&v), Some("c"));
    }

    #[test]
    fn a_reload_that_moves_the_card_to_another_lane_keeps_it_selected() {
        let mut v = lanes_view();
        v.select("a");
        let mut board = lane_board();
        board.columns[0].tasks[0] = lane_card("a", Some("o1"));

        v.replace(board);

        assert_eq!((selected(&v), v.lane()), (Some("a"), 0));
    }

    #[test]
    fn a_reload_without_the_lane_field_goes_back_to_columns() {
        let mut v = lanes_view();
        v.select("e");
        let mut board = lane_board();
        board.fields.retain(|f| f.gid != "f1");

        v.replace(board);

        assert_eq!(v.layout(), &Layout::Columns);
        assert_eq!(selected(&v), Some("e"));
    }

    #[test]
    fn removing_the_only_card_of_a_lane_drops_the_lane() {
        let mut v = lanes_view();
        v.select("a");

        v.remove("a");

        assert_eq!(v.lanes().len(), 2);
        assert!(selected(&v).is_some());
    }

    #[test]
    fn search_goes_lane_by_lane_in_grid_order_and_wraps() {
        let mut v = lanes_view();
        v.select("c");

        let mut order = Vec::new();
        for _ in 0..5 {
            v.next_match("task", true);
            order.push(selected(&v).unwrap().to_owned());
        }

        assert_eq!(order, ["d", "a", "b", "e", "c"]);
    }

    #[test]
    fn the_grid_scrolls_to_keep_the_selected_card_and_its_lane_header_visible() {
        // Each lane: a header line and one card of two lines.
        let mut v = lanes_view();
        v.resize(100, 2);

        v.select("b");
        assert_eq!(v.grid_scroll(), 5, "lines 6–8 in a 4-line window");
        v.top();
        assert_eq!(v.grid_scroll(), 0);
    }

    #[test]
    fn growing_the_window_scrolls_back_up_when_the_whole_grid_fits() {
        let mut v = lanes_view();
        v.resize(100, 1);
        v.select("b");
        assert!(v.grid_scroll() > 0);

        v.resize(100, 10);

        assert_eq!(v.grid_scroll(), 0, "9 lines fit in 20");
    }
}
