//! The keymap (spec F6): one table of bindings, and a parser that turns key
//! presses into commands, handling counts (`5j`), sequences (`gg`, `Tab c`,
//! `Ctrl-w l`) and text input. The help overlay is generated from [`BINDINGS`].

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// A key as the keymap sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Enter,
    Esc,
    Tab,
    Backspace,
    Up,
    Down,
}

/// Where keys are being interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// Lists and panes: vim motions and commands.
    Normal,
    /// Text fields (picker query, `/` search, `:` command line): printable
    /// keys are typed, only control keys are commands.
    Input,
}

/// What a binding does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    MoveDown,
    MoveUp,
    Top,
    /// `G`, or line N with a count.
    Bottom,
    HalfPageDown,
    HalfPageUp,
    FocusLeft,
    FocusRight,
    Open,
    Back,
    Search,
    NextMatch,
    PrevMatch,
    Visual,
    MarkRead,
    MarkUnread,
    ToggleRead,
    Comment,
    EditDescription,
    AddToProject,
    GoToInbox,
    OpenInBrowser,
    ToggleFilter,
    Refresh,
    CommandLine,
    Quit,
    Help,
    // Input context.
    Submit,
    Cancel,
    DeleteChar,
    Next,
    Prev,
}

/// Where a binding comes from, shown in the help overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Vim,
    Asana,
    VimAndAsana,
    Asn,
}

pub struct Binding {
    pub context: Context,
    pub keys: &'static [Key],
    pub cmd: Cmd,
    pub origin: Origin,
    pub help: &'static str,
}

const fn bind(
    context: Context,
    keys: &'static [Key],
    cmd: Cmd,
    origin: Origin,
    help: &'static str,
) -> Binding {
    Binding {
        context,
        keys,
        cmd,
        origin,
        help,
    }
}

use Context::{Input, Normal};
use Key::{Backspace, Char, Ctrl, Down, Enter, Esc, Tab, Up};
use Origin::{Asana, Asn, Vim, VimAndAsana};

pub const BINDINGS: &[Binding] = &[
    bind(Normal, &[Char('j')], Cmd::MoveDown, VimAndAsana, "down"),
    bind(Normal, &[Char('k')], Cmd::MoveUp, VimAndAsana, "up"),
    bind(Normal, &[Char('g'), Char('g')], Cmd::Top, Vim, "top"),
    bind(
        Normal,
        &[Char('G')],
        Cmd::Bottom,
        Vim,
        "bottom, or line N with a count",
    ),
    bind(
        Normal,
        &[Ctrl('d')],
        Cmd::HalfPageDown,
        Vim,
        "half page down",
    ),
    bind(Normal, &[Ctrl('u')], Cmd::HalfPageUp, Vim, "half page up"),
    bind(Normal, &[Char('h')], Cmd::FocusLeft, Vim, "focus left pane"),
    bind(
        Normal,
        &[Char('l')],
        Cmd::FocusRight,
        Vim,
        "focus right pane",
    ),
    bind(
        Normal,
        &[Ctrl('w'), Char('h')],
        Cmd::FocusLeft,
        Vim,
        "focus left pane",
    ),
    bind(
        Normal,
        &[Ctrl('w'), Char('l')],
        Cmd::FocusRight,
        Vim,
        "focus right pane",
    ),
    bind(Normal, &[Enter], Cmd::Open, Asn, "open / focus detail"),
    bind(Normal, &[Esc], Cmd::Back, VimAndAsana, "back"),
    bind(Normal, &[Char('/')], Cmd::Search, Vim, "search"),
    bind(Normal, &[Char('n')], Cmd::NextMatch, Vim, "next match"),
    bind(Normal, &[Char('N')], Cmd::PrevMatch, Vim, "previous match"),
    bind(Normal, &[Char('V')], Cmd::Visual, Vim, "visual line mode"),
    bind(
        Normal,
        &[Char('e')],
        Cmd::MarkRead,
        Asana,
        "mark read (\"archive\"; stays in the web Inbox as read)",
    ),
    bind(
        Normal,
        &[Char('u')],
        Cmd::MarkUnread,
        Asana,
        "mark unread (\"move to Inbox\")",
    ),
    bind(Normal, &[Char('x')], Cmd::ToggleRead, Asn, "toggle read"),
    bind(Normal, &[Char('c')], Cmd::Comment, Asn, "comment"),
    bind(Normal, &[Tab, Char('c')], Cmd::Comment, Asana, "comment"),
    bind(
        Normal,
        &[Tab, Char('e')],
        Cmd::EditDescription,
        Asn,
        "edit description",
    ),
    bind(
        Normal,
        &[Char('p')],
        Cmd::AddToProject,
        Asn,
        "add to project",
    ),
    bind(
        Normal,
        &[Tab, Char('p')],
        Cmd::AddToProject,
        Asana,
        "add to project",
    ),
    bind(
        Normal,
        &[Tab, Char('i')],
        Cmd::GoToInbox,
        Asana,
        "go to Inbox",
    ),
    bind(
        Normal,
        &[Char('o')],
        Cmd::OpenInBrowser,
        Asn,
        "open in browser",
    ),
    bind(
        Normal,
        &[Char('f')],
        Cmd::ToggleFilter,
        Asn,
        "unread only / all",
    ),
    bind(Normal, &[Char('R')], Cmd::Refresh, Asn, "refresh"),
    bind(Normal, &[Char(':')], Cmd::CommandLine, Vim, "command line"),
    bind(
        Normal,
        &[Char('q')],
        Cmd::Quit,
        Vim,
        "quit (back outside the Inbox)",
    ),
    bind(Normal, &[Ctrl('c')], Cmd::Quit, Asn, "quit"),
    bind(Normal, &[Char('?')], Cmd::Help, Asn, "help"),
    bind(Input, &[Enter], Cmd::Submit, Asn, "confirm"),
    bind(Input, &[Esc], Cmd::Cancel, Asn, "cancel"),
    bind(Input, &[Ctrl('c')], Cmd::Cancel, Asn, "cancel"),
    bind(
        Input,
        &[Backspace],
        Cmd::DeleteChar,
        Asn,
        "delete character",
    ),
    bind(Input, &[Ctrl('n')], Cmd::Next, Asn, "next item"),
    bind(Input, &[Down], Cmd::Next, Asn, "next item"),
    bind(Input, &[Ctrl('p')], Cmd::Prev, Asn, "previous item"),
    bind(Input, &[Up], Cmd::Prev, Asn, "previous item"),
];

/// A complete command, with the count typed before it (`5j`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Action {
    pub cmd: Cmd,
    pub count: Option<usize>,
}

/// The result of feeding one key to the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Action(Action),
    /// A printable character typed into a text field.
    Insert(char),
    /// Part of a count or sequence; see [`KeyParser::pending`].
    Pending,
    /// Not bound, or a pending sequence was cancelled.
    Ignored,
}

#[derive(Debug, Default)]
pub struct KeyParser {
    count: Option<usize>,
    sequence: Vec<Key>,
}

impl KeyParser {
    pub fn feed(&mut self, context: Context, event: KeyEvent) -> Outcome {
        if event.kind == KeyEventKind::Release {
            return Outcome::Ignored;
        }
        let Some(key) = Key::from_event(event) else {
            return Outcome::Ignored;
        };

        if context == Context::Input {
            return match lookup(context, &[key]) {
                Some(cmd) => Outcome::Action(Action { cmd, count: None }),
                None => match key {
                    Key::Char(c) => Outcome::Insert(c),
                    _ => Outcome::Ignored,
                },
            };
        }

        let is_pending = self.count.is_some() || !self.sequence.is_empty();
        if key == Key::Esc && is_pending {
            self.reset();
            return Outcome::Ignored;
        }
        if self.sequence.is_empty()
            && let Key::Char(c @ '0'..='9') = key
            && (c != '0' || self.count.is_some())
        {
            let digit = c.to_digit(10).expect("ascii digit") as usize;
            self.count = Some(
                self.count
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(digit),
            );
            return Outcome::Pending;
        }

        self.sequence.push(key);
        if let Some(cmd) = lookup(context, &self.sequence) {
            let count = self.count;
            self.reset();
            return Outcome::Action(Action { cmd, count });
        }
        let is_prefix = BINDINGS.iter().any(|b| {
            b.context == context
                && b.keys.len() > self.sequence.len()
                && b.keys.starts_with(&self.sequence)
        });
        if is_prefix {
            Outcome::Pending
        } else {
            self.reset();
            Outcome::Ignored
        }
    }

    /// What has been typed so far, for the status bar (`12`, `g`, `Tab-`).
    pub fn pending(&self) -> Option<String> {
        if self.count.is_none() && self.sequence.is_empty() {
            return None;
        }
        let mut shown = self.count.map(|c| c.to_string()).unwrap_or_default();
        for key in &self.sequence {
            shown.push_str(&key.label());
        }
        Some(shown)
    }

    fn reset(&mut self) {
        self.count = None;
        self.sequence.clear();
    }
}

fn lookup(context: Context, keys: &[Key]) -> Option<Cmd> {
    BINDINGS
        .iter()
        .find(|b| b.context == context && b.keys == keys)
        .map(|b| b.cmd)
}

impl Key {
    fn from_event(event: KeyEvent) -> Option<Key> {
        let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        Some(match event.code {
            KeyCode::Char(c) if ctrl => Key::Ctrl(c.to_ascii_lowercase()),
            KeyCode::Char(c) => Key::Char(c),
            KeyCode::Enter => Key::Enter,
            KeyCode::Esc => Key::Esc,
            KeyCode::Tab => Key::Tab,
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Up => Key::Up,
            KeyCode::Down => Key::Down,
            _ => return None,
        })
    }

    /// How the key is written in the status bar and help overlay.
    pub fn label(&self) -> String {
        match self {
            Key::Char(c) => c.to_string(),
            Key::Ctrl(c) => format!("Ctrl-{c} "),
            Key::Enter => "Enter".into(),
            Key::Esc => "Esc".into(),
            Key::Tab => "Tab-".into(),
            Key::Backspace => "Backspace".into(),
            Key::Up => "Up".into(),
            Key::Down => "Down".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ch(c: char) -> KeyEvent {
        key(KeyCode::Char(c))
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn action(cmd: Cmd, count: Option<usize>) -> Outcome {
        Outcome::Action(Action { cmd, count })
    }

    fn feed_all(parser: &mut KeyParser, context: Context, keys: &[KeyEvent]) -> Vec<Outcome> {
        keys.iter().map(|k| parser.feed(context, *k)).collect()
    }

    #[test]
    fn single_key_without_count() {
        assert_eq!(
            KeyParser::default().feed(Normal, ch('j')),
            action(Cmd::MoveDown, None)
        );
    }

    #[test]
    fn count_prefix_applies_to_the_next_command() {
        let mut p = KeyParser::default();

        let outcomes = feed_all(&mut p, Normal, &[ch('5'), ch('j')]);

        assert_eq!(outcomes, [Outcome::Pending, action(Cmd::MoveDown, Some(5))]);
    }

    #[test]
    fn multi_digit_count_with_capital_g_goes_to_a_line() {
        let mut p = KeyParser::default();

        let outcomes = feed_all(&mut p, Normal, &[ch('1'), ch('2'), ch('G')]);

        assert_eq!(outcomes.last(), Some(&action(Cmd::Bottom, Some(12))));
    }

    #[test]
    fn zero_continues_a_count_but_is_not_a_count_on_its_own() {
        let mut p = KeyParser::default();

        assert_eq!(p.feed(Normal, ch('0')), Outcome::Ignored);
        let outcomes = feed_all(&mut p, Normal, &[ch('1'), ch('0'), ch('j')]);
        assert_eq!(outcomes.last(), Some(&action(Cmd::MoveDown, Some(10))));
    }

    #[test]
    fn gg_is_a_two_key_sequence() {
        let mut p = KeyParser::default();

        let outcomes = feed_all(&mut p, Normal, &[ch('g'), ch('g')]);

        assert_eq!(outcomes, [Outcome::Pending, action(Cmd::Top, None)]);
    }

    #[test]
    fn ctrl_w_then_l_focuses_right() {
        let mut p = KeyParser::default();

        let outcomes = feed_all(&mut p, Normal, &[ctrl('w'), ch('l')]);

        assert_eq!(outcomes.last(), Some(&action(Cmd::FocusRight, None)));
    }

    #[test]
    fn tab_prefix_sequences() {
        let mut p = KeyParser::default();

        assert_eq!(
            feed_all(&mut p, Normal, &[key(KeyCode::Tab), ch('c')])[1],
            action(Cmd::Comment, None)
        );
        assert_eq!(
            feed_all(&mut p, Normal, &[key(KeyCode::Tab), ch('e')])[1],
            action(Cmd::EditDescription, None)
        );
    }

    #[test]
    fn esc_cancels_a_pending_sequence_without_acting() {
        let mut p = KeyParser::default();

        let outcomes = feed_all(
            &mut p,
            Normal,
            &[key(KeyCode::Tab), key(KeyCode::Esc), ch('j')],
        );

        assert_eq!(
            outcomes,
            [
                Outcome::Pending,
                Outcome::Ignored,
                action(Cmd::MoveDown, None)
            ]
        );
    }

    #[test]
    fn esc_cancels_a_pending_count() {
        let mut p = KeyParser::default();

        let outcomes = feed_all(&mut p, Normal, &[ch('5'), key(KeyCode::Esc), ch('j')]);

        assert_eq!(outcomes[2], action(Cmd::MoveDown, None));
    }

    #[test]
    fn unbound_key_after_a_prefix_cancels_without_acting() {
        let mut p = KeyParser::default();

        let outcomes = feed_all(&mut p, Normal, &[key(KeyCode::Tab), ch('z'), ch('j')]);

        assert_eq!(
            outcomes,
            [
                Outcome::Pending,
                Outcome::Ignored,
                action(Cmd::MoveDown, None)
            ]
        );
    }

    #[test]
    fn esc_with_nothing_pending_is_back() {
        assert_eq!(
            KeyParser::default().feed(Normal, key(KeyCode::Esc)),
            action(Cmd::Back, None)
        );
    }

    #[test]
    fn pending_shows_counts_and_prefixes() {
        let mut p = KeyParser::default();
        assert_eq!(p.pending(), None);

        feed_all(&mut p, Normal, &[ch('1'), ch('2')]);
        assert_eq!(p.pending().as_deref(), Some("12"));

        p.feed(Normal, key(KeyCode::Esc));
        p.feed(Normal, key(KeyCode::Tab));
        assert_eq!(p.pending().as_deref(), Some("Tab-"));

        p.feed(Normal, key(KeyCode::Esc));
        p.feed(Normal, ch('g'));
        assert_eq!(p.pending().as_deref(), Some("g"));
    }

    #[test]
    fn input_context_types_letters_and_digits() {
        let mut p = KeyParser::default();

        let outcomes = feed_all(&mut p, Input, &[ch('j'), ch('5'), ch('G')]);

        assert_eq!(
            outcomes,
            [
                Outcome::Insert('j'),
                Outcome::Insert('5'),
                Outcome::Insert('G')
            ]
        );
    }

    #[test]
    fn input_context_control_keys_are_commands() {
        let mut p = KeyParser::default();

        assert_eq!(p.feed(Input, ctrl('n')), action(Cmd::Next, None));
        assert_eq!(p.feed(Input, key(KeyCode::Up)), action(Cmd::Prev, None));
        assert_eq!(
            p.feed(Input, key(KeyCode::Enter)),
            action(Cmd::Submit, None)
        );
        assert_eq!(p.feed(Input, key(KeyCode::Esc)), action(Cmd::Cancel, None));
        assert_eq!(
            p.feed(Input, key(KeyCode::Backspace)),
            action(Cmd::DeleteChar, None)
        );
    }

    #[test]
    fn key_releases_are_ignored() {
        let mut release = ch('j');
        release.kind = KeyEventKind::Release;

        assert_eq!(KeyParser::default().feed(Normal, release), Outcome::Ignored);
    }

    #[test]
    fn no_two_bindings_share_or_shadow_a_sequence_in_one_context() {
        for (i, a) in BINDINGS.iter().enumerate() {
            for b in &BINDINGS[i + 1..] {
                if a.context != b.context {
                    continue;
                }
                let shorter = a.keys.len().min(b.keys.len());
                assert_ne!(
                    a.keys[..shorter],
                    b.keys[..shorter],
                    "{:?} and {:?} collide in {:?}",
                    a.keys,
                    b.keys,
                    a.context
                );
            }
        }
    }

    #[test]
    fn every_spec_f6_key_is_bound() {
        let normal: &[(&[Key], Cmd)] = &[
            (&[Char('j')], Cmd::MoveDown),
            (&[Char('k')], Cmd::MoveUp),
            (&[Char('g'), Char('g')], Cmd::Top),
            (&[Char('G')], Cmd::Bottom),
            (&[Ctrl('d')], Cmd::HalfPageDown),
            (&[Ctrl('u')], Cmd::HalfPageUp),
            (&[Char('h')], Cmd::FocusLeft),
            (&[Char('l')], Cmd::FocusRight),
            (&[Ctrl('w'), Char('h')], Cmd::FocusLeft),
            (&[Ctrl('w'), Char('l')], Cmd::FocusRight),
            (&[Enter], Cmd::Open),
            (&[Esc], Cmd::Back),
            (&[Char('/')], Cmd::Search),
            (&[Char('n')], Cmd::NextMatch),
            (&[Char('N')], Cmd::PrevMatch),
            (&[Char('V')], Cmd::Visual),
            (&[Char('e')], Cmd::MarkRead),
            (&[Char('u')], Cmd::MarkUnread),
            (&[Char('x')], Cmd::ToggleRead),
            (&[Char('c')], Cmd::Comment),
            (&[Tab, Char('c')], Cmd::Comment),
            (&[Tab, Char('e')], Cmd::EditDescription),
            (&[Char('p')], Cmd::AddToProject),
            (&[Tab, Char('p')], Cmd::AddToProject),
            (&[Tab, Char('i')], Cmd::GoToInbox),
            (&[Char('o')], Cmd::OpenInBrowser),
            (&[Char('f')], Cmd::ToggleFilter),
            (&[Char('R')], Cmd::Refresh),
            (&[Char(':')], Cmd::CommandLine),
            (&[Char('q')], Cmd::Quit),
            (&[Char('?')], Cmd::Help),
        ];
        for (keys, cmd) in normal {
            assert!(
                BINDINGS
                    .iter()
                    .any(|b| b.context == Normal && b.keys == *keys && b.cmd == *cmd),
                "{keys:?} → {cmd:?} missing"
            );
        }
    }
}
