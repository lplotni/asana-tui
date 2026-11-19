//! A project board: columns of cards, the selected card, vertical scroll per
//! column and horizontal paging when not every column fits. Pure state; the
//! [`App`](super::app::App) feeds it keys and sizes, and the UI draws it.

use crate::api::projects::BoardTask;
use crate::api::tasks::Named;
use crate::core::{Board, Column};

/// The narrowest a column is drawn, in cells.
pub const MIN_COLUMN_WIDTH: usize = 28;

/// A completed card's animation (spec F23), in frames of 40 ms: the name is
/// struck through during the first [`STRIKE_FRAMES`], then the card shows
/// only its name line, and it leaves after [`COMPLETE_FRAMES`].
pub const STRIKE_FRAMES: u8 = 6;
pub const COMPLETE_FRAMES: u8 = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct BoardView {
    project: Named,
    columns: Vec<Column>,
    /// The selected column.
    column: usize,
    /// The selected row, clamped to the selected column's cards.
    row: usize,
    /// First visible card of each column.
    scrolls: Vec<usize>,
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
        BoardView {
            project: board.project,
            scrolls: vec![0; board.columns.len()],
            columns: board.columns,
            column: 0,
            row: 0,
            first: 0,
            width: 0,
            card_rows: 1,
            completing: None,
            held: None,
        }
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
        &self.project
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn column(&self) -> usize {
        self.column
    }

    pub fn row(&self) -> usize {
        self.row
    }

    /// First visible card of column `index`.
    pub fn scroll(&self, index: usize) -> usize {
        self.scrolls.get(index).copied().unwrap_or(0)
    }

    pub fn selected_task(&self) -> Option<&BoardTask> {
        self.columns.get(self.column)?.tasks.get(self.row)
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
        let count = self.fitting_columns().min(self.columns.len());
        self.first..self.first + count
    }

    /// How many columns are hidden to the left and to the right.
    pub fn hidden(&self) -> (usize, usize) {
        let visible = self.visible_columns();
        (visible.start, self.columns.len() - visible.end)
    }

    pub fn down(&mut self, n: usize) {
        self.set_row(self.row.saturating_add(n));
    }

    pub fn up(&mut self, n: usize) {
        self.set_row(self.row.saturating_sub(n));
    }

    pub fn top(&mut self) {
        self.set_row(0);
    }

    pub fn bottom(&mut self) {
        self.set_row(usize::MAX);
    }

    pub fn right(&mut self, n: usize) {
        let last = self.columns.len().saturating_sub(1);
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
        let found = self.columns.iter().enumerate().find_map(|(c, column)| {
            let row = column.tasks.iter().position(|t| t.gid == gid)?;
            Some((c, row))
        });
        self.select_position(found)
    }

    /// Selects the first card in board order (column by column, top to
    /// bottom) whose name contains `query`, ignoring case. Returns whether
    /// one did; if not, the selection stays.
    pub fn first_match(&mut self, query: &str) -> bool {
        let first = self.matches(query).into_iter().next();
        self.select_position(first)
    }

    /// Selects the next (or, not `forward`, previous) matching card after
    /// the selected one, wrapping around the board.
    pub fn next_match(&mut self, query: &str, forward: bool) -> bool {
        let matches = self.matches(query);
        let here = (self.column, self.row);
        let next = if forward {
            matches.iter().find(|&&m| m > here).or(matches.first())
        } else {
            matches.iter().rev().find(|&&m| m < here).or(matches.last())
        };
        self.select_position(next.copied())
    }

    /// (column, row) of every card whose name contains `query`, in board
    /// order.
    fn matches(&self, query: &str) -> Vec<(usize, usize)> {
        let query = query.to_lowercase();
        self.columns
            .iter()
            .enumerate()
            .flat_map(|(c, column)| {
                column
                    .tasks
                    .iter()
                    .enumerate()
                    .filter(|(_, task)| task.name.to_lowercase().contains(&query))
                    .map(move |(row, _)| (c, row))
            })
            .collect()
    }

    fn select_position(&mut self, position: Option<(usize, usize)>) -> bool {
        let Some((column, row)) = position else {
            return false;
        };
        self.column = column;
        self.set_row(row);
        true
    }

    /// Takes card `gid` off the board (e.g. once completed). The selection
    /// keeps its row, clamped. Returns whether the card was there.
    pub fn remove(&mut self, gid: &str) -> bool {
        let mut removed = false;
        for column in &mut self.columns {
            let before = column.tasks.len();
            column.tasks.retain(|t| t.gid != gid);
            removed |= column.tasks.len() != before;
        }
        self.set_row(self.row);
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
            .columns
            .get(self.column)
            .and_then(|c| c.section.as_ref())
            .map(|s| s.gid.clone());
        let found = task.and_then(|gid| {
            board.columns.iter().enumerate().find_map(|(c, column)| {
                let row = column.tasks.iter().position(|t| t.gid == gid)?;
                Some((c, row))
            })
        });
        let (column, row) = found.unwrap_or_else(|| {
            let column = section
                .and_then(|gid| {
                    board
                        .columns
                        .iter()
                        .position(|c| c.section.as_ref().is_some_and(|s| s.gid == gid))
                })
                .unwrap_or(self.column);
            (column, self.row)
        });
        self.scrolls = vec![0; board.columns.len()];
        self.project = board.project;
        self.columns = board.columns;
        self.column = column.min(self.columns.len().saturating_sub(1));
        self.set_row(row);
    }

    fn fitting_columns(&self) -> usize {
        (self.width / MIN_COLUMN_WIDTH).max(1)
    }

    /// Selects `row` of the current column, clamped, and scrolls to it.
    fn set_row(&mut self, row: usize) {
        let cards = self.columns.get(self.column).map_or(0, |c| c.tasks.len());
        self.row = row.min(cards.saturating_sub(1));
        self.follow();
    }

    /// Scrolls the selected column to its selected card, and pages the
    /// columns to the selected column.
    fn follow(&mut self) {
        if let Some(scroll) = self.scrolls.get_mut(self.column) {
            *scroll = keep_visible(*scroll, self.row, self.card_rows);
        }
        let count = self.fitting_columns().min(self.columns.len()).max(1);
        self.first = keep_visible(self.first, self.column, count)
            .min(self.columns.len().saturating_sub(count));
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
}
