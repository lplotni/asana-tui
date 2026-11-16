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
            .map(|i| {
                let mut n = Notification {
                    gid: i.to_string(),
                    is_read: read(i),
                    ..template.clone()
                };
                n.work_objects[0].gid = format!("t{i}");
                n
            })
            .collect()
    }

    fn loaded_with(notifications: Vec<Notification>, next: Option<&str>, list_rows: usize) -> App {
        let (mut app, _) = App::new();
        app.update(Msg::Resize(Sizes {
            list_rows,
            detail_width: 60,
            detail_rows: 20,
        }));
        app.update(Msg::InboxLoaded(Ok((
            notifications,
            next.map(str::to_owned),
        ))));
        app
    }

    fn loaded(n: usize, list_rows: usize) -> App {
        loaded_with(inbox(n, |_| false), None, list_rows)
    }

    /// Every effect of typing `typed`.
    fn keys_all(app: &mut App, typed: &str) -> Vec<Effect> {
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

    /// The effects of typing `typed`, minus detail scheduling (tested apart).
    fn keys(app: &mut App, typed: &str) -> Vec<Effect> {
        ignoring_detail(keys_all(app, typed))
    }

    fn ignoring_detail(effects: Vec<Effect>) -> Vec<Effect> {
        effects
            .into_iter()
            .filter(|e| !matches!(e, Effect::ScheduleDetail { .. }))
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
        app.update(Msg::Resize(Sizes {
            list_rows: 10,
            detail_width: 60,
            detail_rows: 20,
        }));

        let effects = ignoring_detail(app.update(Msg::InboxLoaded(Ok((
            inbox(3, |_| false),
            Some("p2".into()),
        )))));

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

    // --- Task detail ---------------------------------------------------------

    fn task_detail() -> TaskDetail {
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
        TaskDetail {
            task,
            comments,
            activity_count: activity.len(),
        }
    }

    fn schedules(effects: &[Effect]) -> Vec<&str> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::ScheduleDetail { gid } => Some(gid.as_str()),
                _ => None,
            })
            .collect()
    }

    fn loaded_detail(app: &mut App, gid: &str) {
        app.update(Msg::DetailDue { gid: gid.into() });
        app.update(Msg::DetailLoaded {
            gid: gid.into(),
            result: Ok(Box::new(task_detail())),
        });
    }

    #[test]
    fn loading_the_inbox_schedules_the_first_tasks_details() {
        let (mut app, _) = App::new();

        let effects = app.update(Msg::InboxLoaded(Ok((inbox(3, |_| false), None))));

        assert_eq!(schedules(&effects), ["t0"]);
        assert_eq!(app.selected_task_gid(), Some("t0"));
    }

    #[test]
    fn moving_fast_over_ten_rows_loads_only_the_task_it_stops_on() {
        let mut app = loaded(20, 10);
        let mut scheduled: Vec<String> = Vec::new();
        for _ in 0..10 {
            scheduled.extend(
                schedules(&keys_all(&mut app, "j"))
                    .into_iter()
                    .map(str::to_owned),
            );
        }

        let loads: Vec<Effect> = scheduled
            .into_iter()
            .flat_map(|gid| app.update(Msg::DetailDue { gid }))
            .collect();

        assert_eq!(loads, [Effect::LoadDetail { gid: "t10".into() }]);
        assert_eq!(app.selected_detail(), Some(&Detail::Loading));
    }

    #[test]
    fn a_cached_task_is_not_loaded_again() {
        let mut app = loaded(3, 10);
        loaded_detail(&mut app, "t0");
        keys(&mut app, "j");

        let effects = keys_all(&mut app, "k");

        assert!(schedules(&effects).is_empty());
        assert!(matches!(app.selected_detail(), Some(Detail::Loaded(_))));
    }

    #[test]
    fn a_response_for_another_task_does_not_replace_the_selected_one() {
        let mut app = loaded(3, 10);
        app.update(Msg::DetailDue { gid: "t0".into() });

        app.update(Msg::DetailLoaded {
            gid: "t2".into(),
            result: Ok(Box::new(task_detail())),
        });

        assert_eq!(app.selected_detail(), Some(&Detail::Loading));
    }

    #[test]
    fn a_failed_load_is_shown_for_that_task() {
        let mut app = loaded(3, 10);
        app.update(Msg::DetailDue { gid: "t0".into() });

        app.update(Msg::DetailLoaded {
            gid: "t0".into(),
            result: Err("timed out".into()),
        });

        assert_eq!(
            app.selected_detail(),
            Some(&Detail::Failed("timed out".into()))
        );
    }

    #[test]
    fn notifications_about_non_tasks_load_nothing() {
        let (mut app, _) = App::new();
        let mut notifications = inbox(1, |_| false);
        notifications[0].work_objects[0].resource_type = "project".into();

        let effects = app.update(Msg::InboxLoaded(Ok((notifications, None))));

        assert!(schedules(&effects).is_empty());
        assert_eq!(app.selected_task_gid(), None);
    }

    #[test]
    fn l_enter_h_and_esc_move_focus_between_panes() {
        let mut app = loaded(3, 10);

        keys(&mut app, "l");
        assert_eq!(app.focus, Focus::Detail);
        keys(&mut app, "h");
        assert_eq!(app.focus, Focus::List);
        key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Detail);
        key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::List);
    }

    #[test]
    fn with_the_detail_focused_motions_scroll_it_instead_of_the_list() {
        let mut app = loaded(3, 10);
        loaded_detail(&mut app, "t0");
        keys(&mut app, "l");

        keys(&mut app, "3j");
        assert_eq!((app.cursor, app.detail_scroll), (0, 3));
        keys(&mut app, "k");
        assert_eq!(app.detail_scroll, 2);
        keys(&mut app, "gg");
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn capital_g_scrolls_the_detail_to_its_last_page() {
        let mut app = loaded(3, 10);
        loaded_detail(&mut app, "t0");
        keys(&mut app, "l");
        let total = crate::tui::detail::lines(
            &task_detail(),
            &crate::tui::theme::Theme::no_color(),
            60,
            app.now,
        )
        .len();

        keys(&mut app, "G");

        assert_eq!(app.detail_scroll, total - 20);
        keys(&mut app, "j");
        assert_eq!(app.detail_scroll, total - 20, "can't scroll past the end");
    }

    #[test]
    fn selecting_another_notification_resets_the_detail_scroll() {
        let mut app = loaded(3, 10);
        loaded_detail(&mut app, "t0");
        keys(&mut app, "l3jh");

        keys(&mut app, "j");

        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn o_opens_the_loaded_task_in_the_browser() {
        let mut app = loaded(3, 10);
        loaded_detail(&mut app, "t0");

        let effects = keys(&mut app, "o");

        assert_eq!(
            effects,
            [Effect::OpenUrl {
                url: task_detail().task.permalink_url
            }]
        );
    }

    #[test]
    fn refresh_reloads_the_shown_task_without_hiding_it_and_forgets_the_rest() {
        let mut app = loaded(3, 10);
        loaded_detail(&mut app, "t1");
        loaded_detail(&mut app, "t0");
        keys(&mut app, "R");

        let effects = app.update(Msg::InboxLoaded(Ok((inbox(3, |_| false), None))));

        assert_eq!(effects, [Effect::LoadDetail { gid: "t0".into() }]);
        assert!(matches!(app.selected_detail(), Some(Detail::Loaded(_))));
        assert!(!app.details.contains_key("t1"));
    }

    // --- Review fixes --------------------------------------------------------

    #[test]
    fn q_in_the_detail_pane_goes_back_instead_of_quitting() {
        let mut app = loaded(3, 10);
        keys(&mut app, "l");

        keys(&mut app, "q");

        assert!(!app.should_quit);
        assert_eq!(app.focus, Focus::List);
    }

    #[test]
    fn q_in_visual_mode_leaves_visual_mode() {
        let mut app = loaded(3, 10);
        keys(&mut app, "Vj");

        keys(&mut app, "q");

        assert!(!app.should_quit);
        assert_eq!(app.visual_anchor, None);
    }

    #[test]
    fn ctrl_c_quits_from_anywhere() {
        let mut app = loaded(3, 10);
        keys(&mut app, "lV");

        key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);

        assert!(app.should_quit);
    }

    #[test]
    fn o_opens_a_project_notification_by_its_url() {
        let (mut app, _) = App::new();
        let mut notifications = inbox(1, |_| false);
        notifications[0].work_objects[0].resource_type = "project".into();
        notifications[0].work_objects[0].gid = "555".into();
        app.update(Msg::InboxLoaded(Ok((notifications, None))));

        assert_eq!(
            keys(&mut app, "o"),
            [Effect::OpenUrl {
                url: "https://app.asana.com/0/555".into()
            }]
        );
    }

    #[test]
    fn o_opens_a_task_even_before_its_details_load() {
        let mut app = loaded(3, 10);

        assert_eq!(
            keys(&mut app, "o"),
            [Effect::OpenUrl {
                url: "https://app.asana.com/0/0/t0".into()
            }]
        );
    }

    #[test]
    fn o_on_an_unknown_kind_of_item_explains_it_cannot_open_it() {
        let (mut app, _) = App::new();
        let mut notifications = inbox(1, |_| false);
        notifications[0].work_objects[0].resource_type = "goal".into();
        app.update(Msg::InboxLoaded(Ok((notifications, None))));

        assert!(keys(&mut app, "o").is_empty());
        assert!(app.toast.as_ref().unwrap().text.contains("goal"));
    }

    #[test]
    fn a_refresh_that_started_before_a_mark_does_not_undo_it() {
        let mut app = loaded(4, 10);
        keys(&mut app, "f");
        keys(&mut app, "R");
        keys(&mut app, "je");
        assert_eq!(visible_gids(&app), ["0", "2", "3"]);

        // The refresh was built before markRead reached Asana: "1" is unread.
        app.update(Msg::InboxLoaded(Ok((inbox(4, |_| false), None))));

        assert_eq!(visible_gids(&app), ["0", "2", "3"]);
        app.update(Msg::Marked {
            gids: vec!["1".into()],
            read: true,
            result: Ok(vec![]),
        });
        app.update(Msg::InboxLoaded(Ok((inbox(4, |i| i == 1), None))));
        assert_eq!(visible_gids(&app), ["0", "2", "3"]);
    }

    #[test]
    fn no_refresh_starts_while_more_items_are_loading() {
        let mut app = loaded_with(inbox(30, |_| false), Some("p2"), 10);
        keys(&mut app, "25j");
        assert!(app.loading_more);

        assert!(keys(&mut app, "R").is_empty());
        assert!(app.update(Msg::Tick).is_empty());
    }

    #[test]
    fn marking_a_visual_range_lands_on_the_item_after_it() {
        let mut app = loaded(6, 10);
        keys(&mut app, "fjVj");

        keys(&mut app, "e");

        assert_eq!(visible_gids(&app), ["0", "3", "4", "5"]);
        assert_eq!(selected_gid(&app), "3");
    }

    #[test]
    fn a_shorter_reload_of_the_shown_task_keeps_the_scroll_in_range() {
        let mut app = loaded(3, 10);
        loaded_detail(&mut app, "t0");
        keys(&mut app, "lG");
        assert!(app.detail_scroll > 0);
        let mut short = task_detail();
        short.comments.clear();
        short.task.notes = String::new();

        app.update(Msg::DetailLoaded {
            gid: "t0".into(),
            result: Ok(Box::new(short)),
        });

        assert_eq!(app.detail_scroll, 0);
    }
    // --- Comments ------------------------------------------------------------

    fn comments_of(app: &App, gid: &str) -> Vec<(String, String)> {
        match app.details.get(gid) {
            Some(Detail::Loaded(d)) => d
                .comments
                .iter()
                .map(|c| (c.gid.clone(), c.text.clone().unwrap_or_default()))
                .collect(),
            _ => panic!("detail for {gid} not loaded"),
        }
    }

    #[test]
    fn c_and_tab_c_open_the_editor_for_the_selected_task() {
        let mut app = loaded(3, 10);

        assert_eq!(
            keys(&mut app, "c"),
            [Effect::EditComment {
                task_gid: "t0".into(),
                initial: String::new()
            }]
        );
        key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(
            keys(&mut app, "c"),
            [Effect::EditComment {
                task_gid: "t0".into(),
                initial: String::new()
            }]
        );
    }

    #[test]
    fn c_on_a_non_task_explains_why_nothing_happens() {
        let (mut app, _) = App::new();
        let mut notifications = inbox(1, |_| false);
        notifications[0].work_objects[0].resource_type = "project".into();
        app.update(Msg::InboxLoaded(Ok((notifications, None))));

        assert!(keys(&mut app, "c").is_empty());
        assert!(app.toast.is_some());
    }

    #[test]
    fn a_cancelled_editor_posts_nothing() {
        let mut app = loaded(3, 10);

        let effects = app.update(Msg::CommentWritten {
            task_gid: "t0".into(),
            text: None,
        });

        assert!(effects.is_empty());
    }

    #[test]
    fn a_written_comment_appears_at_once_and_is_posted() {
        let mut app = loaded(3, 10);
        loaded_detail(&mut app, "t0");
        let before = comments_of(&app, "t0").len();

        let effects = app.update(Msg::CommentWritten {
            task_gid: "t0".into(),
            text: Some("Ship it\n".into()),
        });

        assert_eq!(
            effects,
            [Effect::PostComment {
                task_gid: "t0".into(),
                local_gid: "local-1".into(),
                text: "Ship it\n".into()
            }]
        );
        let comments = comments_of(&app, "t0");
        assert_eq!(comments.len(), before + 1);
        assert_eq!(
            comments.last().unwrap(),
            &("local-1".to_string(), "Ship it\n".to_string())
        );
    }

    #[test]
    fn the_posted_comment_replaces_the_local_one() {
        let mut app = loaded(3, 10);
        loaded_detail(&mut app, "t0");
        app.update(Msg::CommentWritten {
            task_gid: "t0".into(),
            text: Some("Ship it\n".into()),
        });
        let mut stored = task_detail().comments[0].clone();
        stored.gid = "999".into();
        stored.text = Some("Ship it\n".into());

        app.update(Msg::CommentPosted {
            task_gid: "t0".into(),
            local_gid: "local-1".into(),
            result: Ok(Box::new(stored)),
        });

        let comments = comments_of(&app, "t0");
        assert_eq!(comments.last().unwrap().0, "999");
        assert!(!comments.iter().any(|(gid, _)| gid == "local-1"));
        assert!(!app.toast.as_ref().unwrap().is_error);
    }

    #[test]
    fn a_failed_post_removes_the_comment_and_keeps_the_text_as_a_draft() {
        let mut app = loaded(3, 10);
        loaded_detail(&mut app, "t0");
        let before = comments_of(&app, "t0").len();
        app.update(Msg::CommentWritten {
            task_gid: "t0".into(),
            text: Some("Ship it\n".into()),
        });

        app.update(Msg::CommentPosted {
            task_gid: "t0".into(),
            local_gid: "local-1".into(),
            result: Err("timed out".into()),
        });

        assert_eq!(comments_of(&app, "t0").len(), before);
        assert!(app.toast.as_ref().unwrap().is_error);
        assert_eq!(
            keys(&mut app, "c"),
            [Effect::EditComment {
                task_gid: "t0".into(),
                initial: "Ship it\n".into()
            }]
        );
    }

    #[test]
    fn commenting_works_before_the_details_have_loaded() {
        let mut app = loaded(3, 10);

        let effects = app.update(Msg::CommentWritten {
            task_gid: "t0".into(),
            text: Some("Hi".into()),
        });

        assert_eq!(
            effects,
            [Effect::PostComment {
                task_gid: "t0".into(),
                local_gid: "local-1".into(),
                text: "Hi".into()
            }]
        );
    }
}
