//! The TUI state machine: `update(msg)` changes state and returns effects for
//! the event loop to run. No I/O happens here.

use ratatui::crossterm::event::KeyEvent;

use super::detail;
use super::format;
use super::keys::{Action, Cmd, Context, KeyParser, Outcome};
use super::picker::{self, Picker};
use super::theme::Theme;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::api::notifications::Notification;
use crate::api::tasks::{Membership, Named, TaskStory};
use crate::core::TaskDetail;

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
    /// How much fits on screen (sent on start and resize).
    Resize(Sizes),
    /// The 60-second refresh timer.
    Tick,
    /// The Inbox from the first page (start or refresh).
    InboxLoaded(Loaded),
    /// More notifications after the current ones.
    MoreLoaded(Loaded),
    /// The cursor rested on task `gid` long enough to load its details.
    DetailDue {
        gid: String,
    },
    DetailLoaded {
        gid: String,
        result: Result<Box<TaskDetail>, String>,
    },
    /// The editor closed: the comment text, or `None` if cancelled.
    CommentWritten {
        task_gid: String,
        text: Option<String>,
    },
    CommentPosted {
        task_gid: String,
        local_gid: String,
        result: Result<Box<TaskStory>, String>,
    },
    /// Typing in the project picker paused long enough to search.
    SearchDue {
        seq: u64,
    },
    ProjectsFound {
        seq: u64,
        result: Result<Vec<Named>, String>,
    },
    SectionsLoaded {
        project_gid: String,
        result: Result<Vec<Named>, String>,
    },
    AddedToProject {
        task_gid: String,
        project_gid: String,
        result: Result<(), String>,
    },
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
    /// Send [`Msg::DetailDue`] for `gid` after [`DETAIL_DWELL`].
    ScheduleDetail {
        gid: String,
    },
    LoadDetail {
        gid: String,
    },
    OpenUrl {
        url: String,
    },
    /// Hand the terminal to `$EDITOR`, starting with `initial`, then send
    /// [`Msg::CommentWritten`].
    EditComment {
        task_gid: String,
        initial: String,
    },
    PostComment {
        task_gid: String,
        local_gid: String,
        text: String,
    },
    /// Send [`Msg::SearchDue`] for `seq` after [`picker::SEARCH_DEBOUNCE`].
    ScheduleSearch {
        seq: u64,
    },
    SearchProjects {
        query: String,
        seq: u64,
    },
    LoadSections {
        project_gid: String,
    },
    AddToProject {
        task_gid: String,
        project: Named,
        section: Option<Named>,
    },
}

/// How long the cursor rests on a task before its details load.
pub const DETAIL_DWELL: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    #[default]
    List,
    Detail,
}

/// A task's details as far as they're known.
#[derive(Debug, Clone, PartialEq)]
pub enum Detail {
    Loading,
    Loaded(Box<TaskDetail>),
    Failed(String),
}

/// Space available to the panes, in cells.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Sizes {
    /// Inbox items that fit in the list.
    pub list_rows: usize,
    /// Text columns inside the detail pane.
    pub detail_width: usize,
    /// Text rows inside the detail pane.
    pub detail_rows: usize,
}

/// The `/` filter: the list shows only notifications matching `query`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    pub query: String,
    /// Still typing (keys go to the query) rather than browsing the matches.
    pub editing: bool,
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
    pub sizes: Sizes,
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
    /// The `/` filter, when active.
    pub search: Option<Search>,
    /// The `:` command line being typed, when open.
    pub command: Option<String>,
    /// The "add to project" picker, when open.
    pub picker: Option<Picker>,
    /// Comment text whose post failed, by task, offered again on `c`.
    drafts: HashMap<String, String>,
    /// Numbers comments shown before Asana confirms them.
    next_local_id: u64,
    /// Text of comments being posted, by local GID (kept as a draft if the
    /// post fails).
    posting: HashMap<String, String>,
    /// Read state set locally but not yet confirmed by Asana, by GID.
    pending_marks: HashMap<String, bool>,
    /// Task details by task GID (several notifications can share a task).
    pub details: HashMap<String, Detail>,
    pub focus: Focus,
    /// First visible line of the detail pane.
    pub detail_scroll: usize,
    /// Current Unix time, set by the event loop (for line counts).
    pub now: i64,
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
        let before = self.selected_task_gid().map(str::to_owned);
        let mut effects = self.handle(msg);
        let after = self.selected_task_gid().map(str::to_owned);
        if after != before {
            self.detail_scroll = 0;
            if let Some(gid) = after
                && !self.details.contains_key(&gid)
            {
                effects.push(Effect::ScheduleDetail { gid });
            }
        }
        effects
    }

    fn handle(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Key(event) => {
                self.toast = None;
                if self.picker.is_some() {
                    let outcome = self.keys.feed(Context::Input, event);
                    return self.picker_key(outcome);
                }
                if self.command.is_some() {
                    let outcome = self.keys.feed(Context::Input, event);
                    return self.command_key(outcome);
                }
                if self.search.as_ref().is_some_and(|s| s.editing) {
                    let outcome = self.keys.feed(Context::Input, event);
                    self.search_key(outcome);
                    return self.load_more_if_needed();
                }
                match self.keys.feed(Context::Normal, event) {
                    Outcome::Action(action) => self.run(action),
                    Outcome::Insert(_) | Outcome::Pending | Outcome::Ignored => Vec::new(),
                }
            }
            Msg::Resize(sizes) => {
                self.list_rows = sizes.list_rows.max(1);
                self.sizes = sizes;
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
                self.replace_keeping_selection(|app| {
                    app.notifications = notifications;
                    app.apply_pending_marks();
                });
                let mut effects = self.reload_shown_detail();
                effects.extend(self.load_more_if_needed());
                effects
            }
            Msg::DetailDue { gid } => {
                if self.selected_task_gid() != Some(gid.as_str()) || self.details.contains_key(&gid)
                {
                    return Vec::new();
                }
                self.details.insert(gid.clone(), Detail::Loading);
                vec![Effect::LoadDetail { gid }]
            }
            Msg::DetailLoaded { gid, result } => {
                let detail = match result {
                    Ok(detail) => Detail::Loaded(detail),
                    Err(message) => Detail::Failed(message),
                };
                self.details.insert(gid, detail);
                self.detail_scroll = self.detail_scroll.min(self.detail_max_scroll());
                Vec::new()
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
                self.replace_keeping_selection(App::apply_pending_marks);
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
            Msg::CommentWritten {
                task_gid,
                text: Some(text),
            } => self.show_and_post_comment(task_gid, text),
            Msg::CommentWritten { text: None, .. } => Vec::new(),
            Msg::CommentPosted {
                task_gid,
                local_gid,
                result,
            } => {
                let text = self.posting.remove(&local_gid).unwrap_or_default();
                let comments = match self.details.get_mut(&task_gid) {
                    Some(Detail::Loaded(detail)) => Some(&mut detail.comments),
                    _ => None,
                };
                match result {
                    Ok(story) => {
                        if let Some(comments) = comments
                            && let Some(local) = comments.iter_mut().find(|c| c.gid == local_gid)
                        {
                            *local = *story;
                        }
                        self.toast = Some(Toast {
                            text: "Comment posted".into(),
                            is_error: false,
                        });
                    }
                    Err(message) => {
                        if let Some(comments) = comments {
                            comments.retain(|c| c.gid != local_gid);
                        }
                        self.drafts.insert(task_gid, text);
                        self.toast = Some(Toast::error(format!(
                            "Comment not posted: {message} (c reopens your text)"
                        )));
                    }
                }
                Vec::new()
            }
            Msg::SearchDue { seq } => match &mut self.picker {
                Some(p) if p.stage == picker::Stage::Project && p.search_seq == seq => {
                    p.loading = true;
                    vec![Effect::SearchProjects {
                        query: p.query.clone(),
                        seq,
                    }]
                }
                _ => Vec::new(),
            },
            Msg::ProjectsFound { seq, result } => {
                match (&mut self.picker, result) {
                    (Some(p), Ok(projects)) => {
                        p.accept_projects(seq, projects);
                    }
                    (Some(p), Err(message)) if p.search_seq == seq => {
                        p.loading = false;
                        self.toast =
                            Some(Toast::error(format!("Project search failed: {message}")));
                    }
                    _ => {}
                }
                Vec::new()
            }
            Msg::SectionsLoaded {
                project_gid,
                result,
            } => {
                if let Some(p) = &mut self.picker {
                    match result {
                        Ok(sections) => {
                            p.accept_sections(&project_gid, sections);
                        }
                        Err(message) => {
                            // Adding without a section still works.
                            p.accept_sections(&project_gid, Vec::new());
                            self.toast =
                                Some(Toast::error(format!("Couldn't load sections: {message}")));
                        }
                    }
                }
                Vec::new()
            }
            Msg::AddedToProject {
                task_gid,
                project_gid,
                result,
            } => {
                let memberships = match self.details.get_mut(&task_gid) {
                    Some(Detail::Loaded(detail)) => Some(&mut detail.task.memberships),
                    _ => None,
                };
                match result {
                    Ok(()) => {
                        self.toast = Some(Toast {
                            text: "Added to project".into(),
                            is_error: false,
                        });
                    }
                    Err(message) => {
                        if let Some(memberships) = memberships {
                            memberships.retain(|m| m.project.gid != project_gid);
                        }
                        self.toast = Some(Toast::error(format!("Not added to project: {message}")));
                    }
                }
                Vec::new()
            }
            Msg::Marked { gids, read, result } => {
                for gid in &gids {
                    self.pending_marks.remove(gid);
                }
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

    /// The task the selected notification is about, if it is about a task.
    pub fn selected_task_gid(&self) -> Option<&str> {
        let object = self.selected()?.work_objects.first()?;
        (object.resource_type == "task").then_some(object.gid.as_str())
    }

    /// What's known about the selected task.
    pub fn selected_detail(&self) -> Option<&Detail> {
        self.details.get(self.selected_task_gid()?)
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
        if self.focus == Focus::Detail && self.scroll_detail(cmd, count) {
            return Vec::new();
        }
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
            Cmd::Back if self.visual_anchor.is_some() => self.visual_anchor = None,
            Cmd::Back if self.search.is_some() && self.focus == Focus::List => {
                self.replace_keeping_selection(|app| app.search = None);
            }
            Cmd::Search => {
                self.focus = Focus::List;
                self.search = Some(Search {
                    query: String::new(),
                    editing: true,
                });
            }
            Cmd::NextMatch | Cmd::PrevMatch
                if self.search.is_some() && !self.visible.is_empty() =>
            {
                let len = self.visible.len();
                let step = if cmd == Cmd::NextMatch { 1 } else { len - 1 };
                self.move_to((self.cursor + step) % len);
            }
            Cmd::CommandLine => self.command = Some(String::new()),
            Cmd::Back | Cmd::FocusLeft => self.focus = Focus::List,
            Cmd::FocusRight | Cmd::Open if self.selected().is_some() => self.focus = Focus::Detail,
            Cmd::OpenInBrowser => return self.open_in_browser(),
            Cmd::Comment => return self.start_comment(),
            Cmd::AddToProject => return self.open_picker(),
            Cmd::MarkRead => return self.mark(Some(true)),
            Cmd::MarkUnread => return self.mark(Some(false)),
            Cmd::ToggleRead => return self.mark(None),
            Cmd::ToggleFilter => {
                self.visual_anchor = None;
                self.replace_keeping_selection(|app| app.unread_only = !app.unread_only);
            }
            Cmd::Refresh => return self.refresh(),
            Cmd::Quit if self.visual_anchor.is_some() => self.visual_anchor = None,
            Cmd::Quit if self.focus == Focus::Detail => self.focus = Focus::List,
            Cmd::Quit | Cmd::ForceQuit => self.should_quit = true,
            _ => {}
        }
        self.load_more_if_needed()
    }

    /// Scrolls the detail pane for a motion command. Returns false for
    /// commands that aren't motions.
    fn scroll_detail(&mut self, cmd: Cmd, count: Option<usize>) -> bool {
        let n = count.unwrap_or(1);
        let half_page = (self.sizes.detail_rows / 2).max(1);
        let target = match cmd {
            Cmd::MoveDown => self.detail_scroll.saturating_add(n),
            Cmd::MoveUp => self.detail_scroll.saturating_sub(n),
            Cmd::HalfPageDown => self.detail_scroll.saturating_add(half_page),
            Cmd::HalfPageUp => self.detail_scroll.saturating_sub(half_page),
            Cmd::Top => 0,
            Cmd::Bottom => usize::MAX,
            _ => return false,
        };
        self.detail_scroll = target.min(self.detail_max_scroll());
        true
    }

    /// The last scroll position that still fills the detail pane.
    fn detail_max_scroll(&self) -> usize {
        static COUNTING_THEME: OnceLock<Theme> = OnceLock::new();
        match self.selected_detail() {
            Some(Detail::Loaded(detail)) => {
                let theme = COUNTING_THEME.get_or_init(Theme::no_color);
                detail::lines(detail, theme, self.sizes.detail_width, self.now)
                    .len()
                    .saturating_sub(self.sizes.detail_rows)
            }
            _ => 0,
        }
    }

    /// A key while typing a `/` query: the list filters as the query changes.
    fn search_key(&mut self, outcome: Outcome) {
        let Some(search) = &self.search else {
            return;
        };
        let mut query = search.query.clone();
        match outcome {
            Outcome::Insert(c) => query.push(c),
            Outcome::Action(Action {
                cmd: Cmd::DeleteChar,
                ..
            }) => {
                query.pop();
            }
            Outcome::Action(Action {
                cmd: Cmd::Submit, ..
            }) => {
                if query.is_empty() {
                    self.replace_keeping_selection(|app| app.search = None);
                } else if let Some(search) = &mut self.search {
                    search.editing = false;
                }
                return;
            }
            Outcome::Action(Action {
                cmd: Cmd::Cancel, ..
            }) => {
                self.replace_keeping_selection(|app| app.search = None);
                return;
            }
            _ => return,
        }
        self.replace_keeping_selection(|app| {
            app.search = Some(Search {
                query,
                editing: true,
            });
        });
    }

    /// A key while typing a `:` command.
    fn command_key(&mut self, outcome: Outcome) -> Vec<Effect> {
        let Some(command) = &mut self.command else {
            return Vec::new();
        };
        match outcome {
            Outcome::Insert(c) => command.push(c),
            Outcome::Action(Action {
                cmd: Cmd::DeleteChar,
                ..
            }) => {
                command.pop();
            }
            Outcome::Action(Action {
                cmd: Cmd::Cancel, ..
            }) => self.command = None,
            Outcome::Action(Action {
                cmd: Cmd::Submit, ..
            }) => {
                let command = self.command.take().unwrap_or_default();
                return self.run_command(command.trim());
            }
            _ => {}
        }
        Vec::new()
    }

    /// `:q`, `:refresh`, `:read-all` and `:unread`.
    fn run_command(&mut self, command: &str) -> Vec<Effect> {
        match command {
            "" => Vec::new(),
            "q" | "quit" => {
                self.should_quit = true;
                Vec::new()
            }
            "refresh" => self.refresh(),
            "unread" => {
                self.visual_anchor = None;
                self.replace_keeping_selection(|app| app.unread_only = !app.unread_only);
                Vec::new()
            }
            "read-all" => {
                let gids: Vec<String> = self
                    .notifications
                    .iter()
                    .filter(|n| !n.is_read)
                    .map(|n| n.gid.clone())
                    .collect();
                if gids.is_empty() {
                    return Vec::new();
                }
                self.replace_keeping_selection(|app| app.set_read(&gids, true));
                for gid in &gids {
                    self.pending_marks.insert(gid.clone(), true);
                }
                self.toast = Some(Toast {
                    text: format!("Marking {} notification(s) read", gids.len()),
                    is_error: false,
                });
                vec![Effect::MarkRead { gids, read: true }]
            }
            other => {
                self.toast = Some(Toast::error(format!("Unknown command: {other}")));
                Vec::new()
            }
        }
    }

    /// Opens the project picker for the selected task, hiding the projects
    /// it's already in.
    fn open_picker(&mut self) -> Vec<Effect> {
        let Some(task_gid) = self.selected_task_gid().map(str::to_owned) else {
            self.toast = Some(Toast {
                text: "Only tasks can be added to projects".into(),
                is_error: false,
            });
            return Vec::new();
        };
        let exclude = match self.selected_detail() {
            Some(Detail::Loaded(detail)) => detail
                .task
                .memberships
                .iter()
                .map(|m| m.project.gid.clone())
                .collect(),
            _ => Vec::new(),
        };
        let mut picker = Picker::new(task_gid, exclude);
        let seq = picker.set_query(String::new());
        self.picker = Some(picker);
        vec![Effect::ScheduleSearch { seq }]
    }

    /// A key while the picker is open: typing edits the query, control keys
    /// move, choose or close.
    fn picker_key(&mut self, outcome: Outcome) -> Vec<Effect> {
        let Some(picker) = &mut self.picker else {
            return Vec::new();
        };
        let schedule = |picker: &mut Picker, query: String| {
            let seq = picker.set_query(query);
            vec![Effect::ScheduleSearch { seq }]
        };
        match outcome {
            Outcome::Insert(c) if picker.stage == picker::Stage::Project => {
                let query = format!("{}{c}", picker.query);
                schedule(picker, query)
            }
            Outcome::Action(Action { cmd, .. }) => match cmd {
                Cmd::DeleteChar if picker.stage == picker::Stage::Project => {
                    let mut query = picker.query.clone();
                    query.pop();
                    schedule(picker, query)
                }
                Cmd::Next => {
                    picker.move_by(1);
                    Vec::new()
                }
                Cmd::Prev => {
                    picker.move_by(-1);
                    Vec::new()
                }
                Cmd::Cancel => {
                    self.picker = None;
                    Vec::new()
                }
                Cmd::Submit => self.picker_submit(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn picker_submit(&mut self) -> Vec<Effect> {
        let Some(picker) = &mut self.picker else {
            return Vec::new();
        };
        let Some(item) = picker.current().cloned() else {
            return Vec::new();
        };
        match picker.stage.clone() {
            picker::Stage::Project => {
                let project = Named {
                    gid: item.gid.unwrap_or_default(),
                    name: item.name,
                };
                let project_gid = project.gid.clone();
                picker.choose_project(project);
                vec![Effect::LoadSections { project_gid }]
            }
            picker::Stage::Section { project } => {
                let task_gid = picker.task_gid.clone();
                let section = item.gid.map(|gid| Named {
                    gid,
                    name: item.name,
                });
                self.picker = None;
                if let Some(Detail::Loaded(detail)) = self.details.get_mut(&task_gid) {
                    detail.task.memberships.push(Membership {
                        project: project.clone(),
                        section: section.clone(),
                    });
                }
                vec![Effect::AddToProject {
                    task_gid,
                    project,
                    section,
                }]
            }
        }
    }

    /// Opens the editor for a comment on the selected task, with any draft
    /// left by a failed post.
    fn start_comment(&mut self) -> Vec<Effect> {
        let Some(task_gid) = self.selected_task_gid().map(str::to_owned) else {
            self.toast = Some(Toast {
                text: "Comments work on tasks only".into(),
                is_error: false,
            });
            return Vec::new();
        };
        let initial = self.drafts.remove(&task_gid).unwrap_or_default();
        vec![Effect::EditComment { task_gid, initial }]
    }

    /// Shows the comment right away (if the task is on screen) and posts it.
    fn show_and_post_comment(&mut self, task_gid: String, text: String) -> Vec<Effect> {
        self.next_local_id += 1;
        let local_gid = format!("local-{}", self.next_local_id);
        if let Some(Detail::Loaded(detail)) = self.details.get_mut(&task_gid) {
            detail.comments.push(TaskStory {
                gid: local_gid.clone(),
                kind: "comment".into(),
                resource_subtype: Some("comment_added".into()),
                text: Some(text.clone()),
                created_at: format::timestamp(self.now),
                created_by: Some(Named {
                    gid: "me".into(),
                    name: "You".into(),
                }),
            });
        }
        self.posting.insert(local_gid.clone(), text.clone());
        vec![Effect::PostComment {
            task_gid,
            local_gid,
            text,
        }]
    }

    /// Opens the selected item: a loaded task by its permalink, otherwise by
    /// Asana's URL pattern for its type. The notifications API returns no
    /// links, so types without a known pattern can't be opened.
    fn open_in_browser(&mut self) -> Vec<Effect> {
        if let Some(Detail::Loaded(detail)) = self.selected_detail() {
            return vec![Effect::OpenUrl {
                url: detail.task.permalink_url.clone(),
            }];
        }
        let Some(object) = self.selected().and_then(|n| n.work_objects.first()) else {
            return Vec::new();
        };
        let url = match object.resource_type.as_str() {
            "task" => format!("https://app.asana.com/0/0/{}", object.gid),
            "project" => format!("https://app.asana.com/0/{}", object.gid),
            other => {
                self.toast = Some(Toast {
                    text: format!("asn can't open {other}s yet; open it from the web Inbox"),
                    is_error: false,
                });
                return Vec::new();
            }
        };
        vec![Effect::OpenUrl { url }]
    }

    /// After a refresh: keep showing the selected task while reloading it,
    /// and forget the other cached tasks so they're fresh when revisited.
    fn reload_shown_detail(&mut self) -> Vec<Effect> {
        let shown = self.selected_task_gid().map(str::to_owned);
        self.details.retain(|gid, _| Some(gid) == shown.as_ref());
        match (shown, self.selected_detail()) {
            (Some(gid), Some(Detail::Loaded(_) | Detail::Failed(_))) => {
                vec![Effect::LoadDetail { gid }]
            }
            _ => Vec::new(),
        }
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
        for gid in &gids {
            self.pending_marks.insert(gid.clone(), read);
        }
        self.rebuild_visible();
        // Rows marked read disappear in unread-only mode; land on what follows.
        self.move_to(*range.start());
        let mut effects = vec![Effect::MarkRead { gids, read }];
        effects.extend(self.load_more_if_needed());
        effects
    }

    /// Loads that were built before a mark reached Asana still carry the old
    /// state; re-apply marks that are still in flight.
    fn apply_pending_marks(&mut self) {
        for n in &mut self.notifications {
            if let Some(&read) = self.pending_marks.get(&n.gid) {
                n.is_read = read;
            }
        }
    }

    fn set_read(&mut self, gids: &[String], read: bool) {
        for n in &mut self.notifications {
            if gids.contains(&n.gid) {
                n.is_read = read;
            }
        }
    }

    fn refresh(&mut self) -> Vec<Effect> {
        // Never mix a fresh first page with an older page still loading.
        if self.loading || self.loading_more {
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
        let query = self.search.as_ref().map(|s| s.query.to_lowercase());
        self.visible = (0..self.notifications.len())
            .filter(|&i| !unread_only || !self.notifications[i].is_read)
            .filter(|&i| {
                query
                    .as_deref()
                    .is_none_or(|q| matches_search(&self.notifications[i], q))
            })
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

/// Whether `n` matches the lowercase `query`: its actor, action or item name.
fn matches_search(n: &Notification, query: &str) -> bool {
    let actor = n.created_by.as_ref().map_or("Asana", |a| a.name.as_str());
    let object = n.work_objects.first().map_or("", |o| o.name.as_str());
    [actor, &format::describe(&n.story_resource_subtype), object]
        .iter()
        .any(|field| field.to_lowercase().contains(query))
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
#[path = "app_tests.rs"]
mod tests;
