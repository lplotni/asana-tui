//! The TUI state machine: `update(msg)` changes state and returns effects for
//! the event loop to run. No I/O happens here.

use ratatui::crossterm::event::KeyEvent;

use super::keys::{Action, Cmd, Context, KeyParser, Outcome};
use crate::api::notifications::Notification;

/// Something that happened: a key press or the result of an effect.
#[derive(Debug)]
pub enum Msg {
    Key(KeyEvent),
    InboxLoaded(Result<Vec<Notification>, String>),
    /// How many Inbox items fit on screen (sent on start and resize).
    Resize {
        list_rows: usize,
    },
}

/// Work for the event loop.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    LoadInbox,
}

#[derive(Debug, Default)]
pub struct App {
    pub notifications: Vec<Notification>,
    pub cursor: usize,
    /// Index of the first visible item.
    pub scroll: usize,
    pub list_rows: usize,
    pub loading: bool,
    pub error: Option<String>,
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
        (app, vec![Effect::LoadInbox])
    }

    pub fn update(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Key(event) => match self.keys.feed(Context::Normal, event) {
                Outcome::Action(action) => return self.run(action),
                Outcome::Insert(_) | Outcome::Pending | Outcome::Ignored => {}
            },
            Msg::InboxLoaded(Ok(notifications)) => {
                self.notifications = notifications;
                self.loading = false;
                self.error = None;
                self.move_to(self.cursor);
            }
            Msg::InboxLoaded(Err(message)) => {
                self.loading = false;
                self.error = Some(message);
            }
            Msg::Resize { list_rows } => {
                self.list_rows = list_rows.max(1);
                self.move_to(self.cursor);
            }
        }
        Vec::new()
    }

    pub fn selected(&self) -> Option<&Notification> {
        self.notifications.get(self.cursor)
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
            Cmd::Quit => self.should_quit = true,
            _ => {}
        }
        Vec::new()
    }

    /// Moves the cursor to `index` (clamped) and scrolls it into view.
    fn move_to(&mut self, index: usize) {
        self.cursor = index.min(self.notifications.len().saturating_sub(1));
        let rows = self.list_rows.max(1);
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + rows {
            self.scroll = self.cursor + 1 - rows;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    fn notifications(n: usize) -> Vec<Notification> {
        let text = std::fs::read_to_string("tests/fixtures/notifications_page.json").unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let page: Vec<Notification> = serde_json::from_value(value["data"].clone()).unwrap();
        page.into_iter().cycle().take(n).collect()
    }

    fn loaded(n: usize, list_rows: usize) -> App {
        let (mut app, _) = App::new();
        app.update(Msg::Resize { list_rows });
        app.update(Msg::InboxLoaded(Ok(notifications(n))));
        app
    }

    fn keys(app: &mut App, typed: &str) {
        for c in typed.chars() {
            app.update(Msg::Key(KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::NONE,
            )));
        }
    }

    fn ctrl(app: &mut App, c: char) {
        app.update(Msg::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::CONTROL,
        )));
    }

    #[test]
    fn starts_loading_the_inbox() {
        let (app, effects) = App::new();

        assert!(app.loading);
        assert_eq!(effects, [Effect::LoadInbox]);
    }

    #[test]
    fn loaded_inbox_selects_the_first_item() {
        let app = loaded(10, 5);

        assert!(!app.loading);
        assert_eq!(app.cursor, 0);
        assert_eq!(app.selected().unwrap().gid, app.notifications[0].gid);
    }

    #[test]
    fn load_error_is_shown_and_loading_stops() {
        let (mut app, _) = App::new();

        app.update(Msg::InboxLoaded(Err("Could not reach Asana".into())));

        assert!(!app.loading);
        assert_eq!(app.error.as_deref(), Some("Could not reach Asana"));
    }

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

        ctrl(&mut app, 'd');
        assert_eq!(app.cursor, 5);
        ctrl(&mut app, 'u');
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
        ctrl(&mut app, 'c');
        assert!(app.should_quit);
    }

    #[test]
    fn a_reload_keeps_the_cursor_in_range() {
        let mut app = loaded(10, 5);
        keys(&mut app, "G");

        app.update(Msg::InboxLoaded(Ok(notifications(4))));

        assert_eq!(app.cursor, 3);
    }
}
