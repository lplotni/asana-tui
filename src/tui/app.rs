//! The TUI state machine: `update(msg)` changes state and returns effects for
//! the event loop to run. No I/O happens here.

use ratatui::crossterm::event::KeyEvent;

use super::keys::{Action, Cmd, Context, KeyParser, Outcome};
use crate::api::notifications::Notification;

/// Notifications fetched on start and the minimum fetched on refresh.
pub const INITIAL_LOAD: usize = 100;
/// Load the next page when the cursor is this close to the end.
const LOAD_MORE_MARGIN: usize = 10;

/// Notifications plus the offset to continue from (`None` at the end).
pub type Loaded = Result<(Vec<Notification>, Option<String>), String>;

/// Something that happened: a key press, a timer, or the result of an effect.
#[derive(Debug)]
pub enum Msg {
    Key(KeyEvent),
    /// How many Inbox items fit on screen (sent on start and resize).
    Resize {
        list_rows: usize,
    },
    /// The 60-second refresh timer.
    Tick,
    /// The Inbox from the first page (start or refresh).
    InboxLoaded(Loaded),
    /// More notifications after the current ones.
    MoreLoaded(Loaded),
    /// Result of marking `gids` read (or unread): the GIDs Asana couldn't update.
    Marked {
        gids: Vec<String>,
        read: bool,
        result: Result<Vec<String>, String>,
    },
}

/// Work for the event loop.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    /// Load the Inbox from the first page until `target` notifications.
    LoadInbox {
        target: usize,
    },
    /// Load notifications after `offset`.
    LoadMore {
        offset: String,
    },
    MarkRead {
        gids: Vec<String>,
        read: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub text: String,
    pub is_error: bool,
}

#[derive(Debug, Default)]
pub struct App {
    /// Everything loaded, newest first.
    pub notifications: Vec<Notification>,
    /// Show only unread notifications. Off by default, matching the web Inbox.
    pub unread_only: bool,
    /// Indices into `notifications` that pass the filter.
    visible: Vec<usize>,
    /// Position in the visible list.
    pub cursor: usize,
    /// Index of the first visible row.
    pub scroll: usize,
    pub list_rows: usize,
    /// Where visual mode started, as a visible position.
    pub visual_anchor: Option<usize>,
    pub loading: bool,
    pub loading_more: bool,
    next_offset: Option<String>,
    /// A failed "load more" already triggered a reload from the first page.
    recovering: bool,
    /// Why the Inbox couldn't be loaded (shown when there is nothing to show).
    pub error: Option<String>,
    pub toast: Option<Toast>,
    pub should_quit: bool,
    pub keys: KeyParser,
}

impl App {
    /// A new app that starts loading the Inbox.
    pub fn new() -> (App, Vec<Effect>) {
        let app = App {
            loading: true,
            ..App::default()
        };
        (
            app,
            vec![Effect::LoadInbox {
                target: INITIAL_LOAD,
            }],
        )
    }

    pub fn update(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Key(event) => {
                self.toast = None;
                match self.keys.feed(Context::Normal, event) {
                    Outcome::Action(action) => self.run(action),
                    Outcome::Insert(_) | Outcome::Pending | Outcome::Ignored => Vec::new(),
                }
            }
            Msg::Resize { list_rows } => {
                self.list_rows = list_rows.max(1);
                self.move_to(self.cursor);
                self.load_more_if_needed()
            }
            Msg::Tick => self.refresh(),
            Msg::InboxLoaded(Ok((notifications, next))) => {
                self.loading = false;
                self.recovering = false;
                self.error = None;
                self.next_offset = next;
                self.visual_anchor = None;
                self.replace_keeping_selection(|app| app.notifications = notifications);
                self.load_more_if_needed()
            }
            Msg::InboxLoaded(Err(message)) => {
                self.loading = false;
                if self.notifications.is_empty() {
                    self.error = Some(message);
                } else {
                    self.toast = Some(Toast::error(format!("Refresh failed: {message}")));
                }
                Vec::new()
            }
            Msg::MoreLoaded(Ok((notifications, next))) => {
                self.loading_more = false;
                self.next_offset = next;
                self.notifications.extend(notifications);
                self.replace_keeping_selection(|_| {});
                self.load_more_if_needed()
            }
            Msg::MoreLoaded(Err(message)) => {
                self.loading_more = false;
                if self.recovering {
                    self.toast = Some(Toast::error(format!("Loading more failed: {message}")));
                    return Vec::new();
                }
                // Most likely an expired offset (they last 15 minutes).
                self.recovering = true;
                self.next_offset = None;
                self.loading = true;
                vec![Effect::LoadInbox {
                    target: self.notifications.len().max(INITIAL_LOAD),
                }]
            }
            Msg::Marked { gids, read, result } => {
                let (failed, toast) = match result {
                    Ok(failed) if failed.is_empty() => return Vec::new(),
                    Ok(failed) => {
                        let text =
                            format!("Asana couldn't update {} notification(s)", failed.len());
                        (failed, text)
                    }
                    Err(message) => (gids, format!("Couldn't update: {message}")),
                };
                self.replace_keeping_selection(|app| app.set_read(&failed, !read));
                self.toast = Some(Toast::error(toast));
                Vec::new()
            }
        }
    }

    /// The notifications that pass the filter, in display order.
    pub fn visible(&self) -> impl Iterator<Item = &Notification> {
        self.visible.iter().map(|&i| &self.notifications[i])
    }

    pub fn visible_len(&self) -> usize {
        self.visible.len()
    }

    pub fn selected(&self) -> Option<&Notification> {
        self.visible
            .get(self.cursor)
            .map(|&i| &self.notifications[i])
    }

    /// Whether visible position `index` is inside the visual selection.
    pub fn in_visual(&self, index: usize) -> bool {
        self.visual_anchor.is_some_and(|anchor| {
            let (start, end) = (anchor.min(self.cursor), anchor.max(self.cursor));
            (start..=end).contains(&index)
        })
    }

    fn run(&mut self, Action { cmd, count }: Action) -> Vec<Effect> {
        let n = count.unwrap_or(1);
        let half_page = (self.list_rows / 2).max(1);
        match cmd {
            Cmd::MoveDown => self.move_to(self.cursor.saturating_add(n)),
            Cmd::MoveUp => self.move_to(self.cursor.saturating_sub(n)),
            Cmd::Top => self.move_to(0),
            Cmd::Bottom => self.move_to(count.map_or(usize::MAX, |line| line.saturating_sub(1))),
            Cmd::HalfPageDown => self.move_to(self.cursor.saturating_add(half_page)),
            Cmd::HalfPageUp => self.move_to(self.cursor.saturating_sub(half_page)),
            Cmd::Visual => {
                self.visual_anchor = match self.visual_anchor {
                    Some(_) => None,
                    None => (!self.visible.is_empty()).then_some(self.cursor),
                };
            }
            Cmd::Back => self.visual_anchor = None,
            Cmd::MarkRead => return self.mark(Some(true)),
            Cmd::MarkUnread => return self.mark(Some(false)),
            Cmd::ToggleRead => return self.mark(None),
            Cmd::ToggleFilter => {
                self.visual_anchor = None;
                self.replace_keeping_selection(|app| app.unread_only = !app.unread_only);
            }
            Cmd::Refresh => return self.refresh(),
            Cmd::Quit => self.should_quit = true,
            _ => {}
        }
        self.load_more_if_needed()
    }

    /// Marks the selection (visual range or cursor) read, unread, or (`None`)
    /// read unless all of it is already read. Updates the list right away.
    fn mark(&mut self, read: Option<bool>) -> Vec<Effect> {
        let range = match self.visual_anchor.take() {
            Some(anchor) => anchor.min(self.cursor)..=anchor.max(self.cursor),
            None => self.cursor..=self.cursor,
        };
        let targets: Vec<usize> = range
            .clone()
            .filter_map(|pos| self.visible.get(pos).copied())
            .collect();
        let read = read.unwrap_or_else(|| targets.iter().any(|&i| !self.notifications[i].is_read));
        let gids: Vec<String> = targets
            .iter()
            .filter(|&&i| self.notifications[i].is_read != read)
            .map(|&i| self.notifications[i].gid.clone())
            .collect();
        if gids.is_empty() {
            return Vec::new();
        }
        self.set_read(&gids, read);
        self.rebuild_visible();
        // Rows marked read disappear in unread-only mode; land on what follows.
        self.move_to(*range.start());
        let mut effects = vec![Effect::MarkRead { gids, read }];
        effects.extend(self.load_more_if_needed());
        effects
    }

    fn set_read(&mut self, gids: &[String], read: bool) {
        for n in &mut self.notifications {
            if gids.contains(&n.gid) {
                n.is_read = read;
            }
        }
    }

    fn refresh(&mut self) -> Vec<Effect> {
        if self.loading {
            return Vec::new();
        }
        self.loading = true;
        vec![Effect::LoadInbox {
            target: self.notifications.len().max(INITIAL_LOAD),
        }]
    }

    fn load_more_if_needed(&mut self) -> Vec<Effect> {
        let near_end = self.cursor + LOAD_MORE_MARGIN >= self.visible.len();
        match &self.next_offset {
            Some(offset) if near_end && !self.loading && !self.loading_more => {
                self.loading_more = true;
                vec![Effect::LoadMore {
                    offset: offset.clone(),
                }]
            }
            _ => Vec::new(),
        }
    }

    /// Applies `change`, then keeps the cursor on the same notification or,
    /// if it's gone, on the one now at the same position.
    fn replace_keeping_selection(&mut self, change: impl FnOnce(&mut App)) {
        let selected = self.selected().map(|n| n.gid.clone());
        change(self);
        self.rebuild_visible();
        let position = selected
            .and_then(|gid| self.visible().position(|n| n.gid == gid))
            .unwrap_or(self.cursor);
        self.move_to(position);
    }

    fn rebuild_visible(&mut self) {
        let unread_only = self.unread_only;
        self.visible = (0..self.notifications.len())
            .filter(|&i| !unread_only || !self.notifications[i].is_read)
            .collect();
    }

    /// Moves the cursor to `index` (clamped) and scrolls it into view.
    fn move_to(&mut self, index: usize) {
        self.cursor = index.min(self.visible.len().saturating_sub(1));
        let rows = self.list_rows.max(1);
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + rows {
            self.scroll = self.cursor + 1 - rows;
        }
    }
}

impl Toast {
    fn error(text: String) -> Toast {
        Toast {
            text,
            is_error: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    /// `n` notifications with GIDs "0", "1", …; `read` decides each one's state.
    fn inbox(n: usize, read: impl Fn(usize) -> bool) -> Vec<Notification> {
        let text = std::fs::read_to_string("tests/fixtures/notifications_page.json").unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let template: Notification = serde_json::from_value(value["data"][0].clone()).unwrap();
        (0..n)
            .map(|i| Notification {
                gid: i.to_string(),
                is_read: read(i),
                ..template.clone()
            })
            .collect()
    }

    fn loaded_with(notifications: Vec<Notification>, next: Option<&str>, list_rows: usize) -> App {
        let (mut app, _) = App::new();
        app.update(Msg::Resize { list_rows });
        app.update(Msg::InboxLoaded(Ok((
            notifications,
            next.map(str::to_owned),
        ))));
        app
    }

    fn loaded(n: usize, list_rows: usize) -> App {
        loaded_with(inbox(n, |_| false), None, list_rows)
    }

    fn keys(app: &mut App, typed: &str) -> Vec<Effect> {
        typed
            .chars()
            .flat_map(|c| {
                app.update(Msg::Key(KeyEvent::new(
                    KeyCode::Char(c),
                    KeyModifiers::NONE,
                )))
            })
            .collect()
    }

    fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Vec<Effect> {
        app.update(Msg::Key(KeyEvent::new(code, modifiers)))
    }

    fn selected_gid(app: &App) -> &str {
        &app.selected().unwrap().gid
    }

    fn visible_gids(app: &App) -> Vec<&str> {
        app.visible().map(|n| n.gid.as_str()).collect()
    }

    // --- Loading -------------------------------------------------------------

    #[test]
    fn starts_loading_the_first_page_showing_all_like_the_web_inbox() {
        let (app, effects) = App::new();

        assert!(app.loading);
        assert!(!app.unread_only);
        assert_eq!(
            effects,
            [Effect::LoadInbox {
                target: INITIAL_LOAD
            }]
        );
    }

    #[test]
    fn read_and_unread_notifications_are_both_shown_by_default() {
        let app = loaded_with(inbox(4, |i| i % 2 == 0), None, 10);

        assert_eq!(visible_gids(&app), ["0", "1", "2", "3"]);
    }

    #[test]
    fn unread_only_hides_read_notifications() {
        let mut app = loaded_with(inbox(4, |i| i % 2 == 0), None, 10);

        keys(&mut app, "f");

        assert_eq!(visible_gids(&app), ["1", "3"]);
    }

    #[test]
    fn load_error_with_nothing_loaded_is_shown_as_the_error() {
        let (mut app, _) = App::new();

        app.update(Msg::InboxLoaded(Err("Could not reach Asana".into())));

        assert!(!app.loading);
        assert_eq!(app.error.as_deref(), Some("Could not reach Asana"));
    }

    // --- Navigation ----------------------------------------------------------

    #[test]
    fn j_and_k_move_and_stop_at_the_edges() {
        let mut app = loaded(3, 10);

        keys(&mut app, "k");
        assert_eq!(app.cursor, 0);
        keys(&mut app, "jjjj");
        assert_eq!(app.cursor, 2);
        keys(&mut app, "k");
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn counts_multiply_moves_and_clamp() {
        let mut app = loaded(10, 10);

        keys(&mut app, "5j");
        assert_eq!(app.cursor, 5);
        keys(&mut app, "20j");
        assert_eq!(app.cursor, 9);
        keys(&mut app, "3k");
        assert_eq!(app.cursor, 6);
    }

    #[test]
    fn gg_and_capital_g_jump_to_top_bottom_and_line_n() {
        let mut app = loaded(10, 10);

        keys(&mut app, "G");
        assert_eq!(app.cursor, 9);
        keys(&mut app, "gg");
        assert_eq!(app.cursor, 0);
        keys(&mut app, "4G");
        assert_eq!(app.cursor, 3);
        keys(&mut app, "99G");
        assert_eq!(app.cursor, 9);
    }

    #[test]
    fn half_page_moves_half_the_visible_rows() {
        let mut app = loaded(30, 10);

        key(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert_eq!(app.cursor, 5);
        key(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn scrolling_keeps_the_cursor_visible() {
        let mut app = loaded(30, 5);

        keys(&mut app, "7j");
        assert_eq!((app.cursor, app.scroll), (7, 3));
        keys(&mut app, "6k");
        assert_eq!((app.cursor, app.scroll), (1, 1));
        keys(&mut app, "G");
        assert_eq!((app.cursor, app.scroll), (29, 25));
    }

    #[test]
    fn moving_in_an_empty_inbox_does_nothing() {
        let mut app = loaded(0, 5);

        keys(&mut app, "jGk");

        assert_eq!(app.cursor, 0);
        assert!(app.selected().is_none());
    }

    #[test]
    fn q_and_ctrl_c_quit() {
        let mut app = loaded(3, 5);
        keys(&mut app, "q");
        assert!(app.should_quit);

        let mut app = loaded(3, 5);
        key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.should_quit);
    }

    // --- Read state ----------------------------------------------------------

    #[test]
    fn e_marks_read_and_keeps_the_row_like_the_web_inbox() {
        let mut app = loaded(4, 10);
        keys(&mut app, "j");

        let effects = keys(&mut app, "e");

        assert_eq!(
            effects,
            [Effect::MarkRead {
                gids: vec!["1".into()],
                read: true
            }]
        );
        assert_eq!(visible_gids(&app), ["0", "1", "2", "3"]);
        assert_eq!(selected_gid(&app), "1");
        assert!(app.selected().unwrap().is_read);
    }

    #[test]
    fn in_unread_only_mode_e_removes_the_row_and_lands_on_the_next_item() {
        let mut app = loaded(4, 10);
        keys(&mut app, "fj");

        let effects = keys(&mut app, "e");

        assert_eq!(
            effects,
            [Effect::MarkRead {
                gids: vec!["1".into()],
                read: true
            }]
        );
        assert_eq!(visible_gids(&app), ["0", "2", "3"]);
        assert_eq!(selected_gid(&app), "2");
    }

    #[test]
    fn a_failed_mark_restores_the_row_and_shows_an_error_toast() {
        let mut app = loaded(4, 10);
        keys(&mut app, "fje");

        app.update(Msg::Marked {
            gids: vec!["1".into()],
            read: true,
            result: Err("Could not reach Asana".into()),
        });

        assert_eq!(visible_gids(&app), ["0", "1", "2", "3"]);
        assert!(app.toast.as_ref().unwrap().is_error);
        assert_eq!(
            selected_gid(&app),
            "2",
            "the cursor stays on the item it was on"
        );
    }

    #[test]
    fn only_the_gids_asana_could_not_update_are_restored() {
        let mut app = loaded(4, 10);
        keys(&mut app, "fVje");

        app.update(Msg::Marked {
            gids: vec!["0".into(), "1".into()],
            read: true,
            result: Ok(vec!["1".into()]),
        });

        assert_eq!(visible_gids(&app), ["1", "2", "3"]);
        assert!(app.toast.as_ref().unwrap().is_error);
    }

    #[test]
    fn u_marks_unread_and_x_toggles() {
        let mut app = loaded_with(inbox(2, |_| true), None, 10);

        assert_eq!(
            keys(&mut app, "u"),
            [Effect::MarkRead {
                gids: vec!["0".into()],
                read: false
            }]
        );
        assert_eq!(
            keys(&mut app, "x"),
            [Effect::MarkRead {
                gids: vec!["0".into()],
                read: true
            }]
        );
        assert_eq!(
            keys(&mut app, "x"),
            [Effect::MarkRead {
                gids: vec!["0".into()],
                read: false
            }]
        );
    }

    #[test]
    fn marking_without_a_change_sends_nothing() {
        let mut app = loaded(2, 10);

        assert!(keys(&mut app, "u").is_empty());
    }

    #[test]
    fn visual_mode_marks_the_whole_range_in_one_request_and_ends() {
        let mut app = loaded(5, 10);
        keys(&mut app, "fjVjj");
        assert!(app.in_visual(1) && app.in_visual(3) && !app.in_visual(4));

        let effects = keys(&mut app, "e");

        assert_eq!(
            effects,
            [Effect::MarkRead {
                gids: vec!["1".into(), "2".into(), "3".into()],
                read: true
            }]
        );
        assert_eq!(app.visual_anchor, None);
        assert_eq!(visible_gids(&app), ["0", "4"]);
        assert_eq!(selected_gid(&app), "4");
    }

    #[test]
    fn esc_leaves_visual_mode() {
        let mut app = loaded(5, 10);
        keys(&mut app, "Vj");

        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

        assert_eq!(app.visual_anchor, None);
    }

    // --- Filter --------------------------------------------------------------

    #[test]
    fn f_toggles_the_filter_and_keeps_the_selected_notification() {
        let mut app = loaded_with(inbox(6, |i| i % 2 == 0), None, 10);
        keys(&mut app, "3j");
        assert_eq!(selected_gid(&app), "3");

        keys(&mut app, "f");
        assert!(app.unread_only);
        assert_eq!(visible_gids(&app), ["1", "3", "5"]);
        assert_eq!(selected_gid(&app), "3");

        keys(&mut app, "f");
        assert_eq!(selected_gid(&app), "3");
    }

    // --- Refresh -------------------------------------------------------------

    #[test]
    fn r_and_the_timer_refresh_at_least_as_much_as_is_loaded() {
        let mut app = loaded(150, 10);

        assert_eq!(keys(&mut app, "R"), [Effect::LoadInbox { target: 150 }]);
        app.update(Msg::InboxLoaded(Ok((inbox(150, |_| false), None))));
        assert_eq!(app.update(Msg::Tick), [Effect::LoadInbox { target: 150 }]);
    }

    #[test]
    fn no_second_refresh_while_one_is_running() {
        let mut app = loaded(5, 10);
        keys(&mut app, "R");

        assert!(app.update(Msg::Tick).is_empty());
        assert!(keys(&mut app, "R").is_empty());
    }

    #[test]
    fn refresh_keeps_the_cursor_on_the_same_notification() {
        let mut app = loaded(10, 10);
        keys(&mut app, "5j");
        assert_eq!(selected_gid(&app), "5");
        keys(&mut app, "R");
        let mut fresh = inbox(10, |_| false);
        fresh.insert(
            0,
            Notification {
                gid: "new".into(),
                ..fresh[0].clone()
            },
        );

        app.update(Msg::InboxLoaded(Ok((fresh, None))));

        assert_eq!(selected_gid(&app), "5");
        assert_eq!(app.cursor, 6);
    }

    #[test]
    fn refresh_moves_to_the_nearest_neighbour_when_the_item_is_gone() {
        let mut app = loaded(10, 10);
        keys(&mut app, "5j");
        keys(&mut app, "R");
        let fresh: Vec<_> = inbox(10, |_| false)
            .into_iter()
            .filter(|n| n.gid != "5")
            .collect();

        app.update(Msg::InboxLoaded(Ok((fresh, None))));

        assert_eq!(selected_gid(&app), "6");
    }

    // --- Loading more --------------------------------------------------------

    #[test]
    fn nearing_the_end_loads_the_next_page_exactly_once() {
        let mut app = loaded_with(inbox(30, |_| false), Some("p2"), 10);

        assert!(keys(&mut app, "10j").is_empty());
        assert_eq!(
            keys(&mut app, "10j"),
            [Effect::LoadMore {
                offset: "p2".into()
            }]
        );
        assert!(keys(&mut app, "j").is_empty(), "already loading");

        app.update(Msg::MoreLoaded(Ok((
            inbox(40, |_| false).split_off(30),
            Some("p3".into()),
        ))));
        assert_eq!(app.visible_len(), 40);
        assert_eq!(
            keys(&mut app, "G"),
            [Effect::LoadMore {
                offset: "p3".into()
            }]
        );
    }

    #[test]
    fn a_short_inbox_keeps_loading_until_the_cursor_has_room() {
        let (mut app, _) = App::new();
        app.update(Msg::Resize { list_rows: 10 });

        let effects = app.update(Msg::InboxLoaded(Ok((
            inbox(3, |_| false),
            Some("p2".into()),
        ))));

        assert_eq!(
            effects,
            [Effect::LoadMore {
                offset: "p2".into()
            }]
        );
    }

    #[test]
    fn a_failed_load_more_reloads_from_the_first_page_once() {
        let mut app = loaded_with(inbox(5, |_| false), Some("expired"), 10);

        let effects = app.update(Msg::MoreLoaded(Err("offset expired".into())));
        assert_eq!(
            effects,
            [Effect::LoadInbox {
                target: INITIAL_LOAD
            }]
        );

        app.update(Msg::InboxLoaded(Ok((
            inbox(5, |_| false),
            Some("fresh".into()),
        ))));
        let effects = app.update(Msg::MoreLoaded(Err("offset expired".into())));
        assert_eq!(
            effects,
            [Effect::LoadInbox {
                target: INITIAL_LOAD
            }],
            "a successful reload resets recovery"
        );

        let effects = app.update(Msg::MoreLoaded(Err("still failing".into())));
        assert!(effects.is_empty());
        assert!(app.toast.as_ref().unwrap().is_error);
    }

    #[test]
    fn a_key_press_clears_the_toast() {
        let mut app = loaded(3, 10);
        app.toast = Some(Toast {
            text: "x".into(),
            is_error: false,
        });

        keys(&mut app, "j");

        assert_eq!(app.toast, None);
    }
}
