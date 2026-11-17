//! The TUI state machine: `update(msg)` changes state and returns effects for
//! the event loop to run. No I/O happens here.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui_textarea::{CursorMove, TextArea};

use super::board::BoardView;
use super::keys::{Action, Cmd, Context, KeyParser, Outcome};
use super::picker::{self, Picker};
use crate::present::{detail, format};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::api::notifications::Notification;
use crate::api::search::FoundTask;
use crate::api::tasks::{Membership, Named, TaskStory};
use crate::core::{Board, TaskDetail};

/// Notifications fetched on start. Asana returns ~10 per page, so this is a
/// few requests; scrolling loads more.
pub const INITIAL_LOAD: usize = 30;
/// Notifications re-fetched on refresh (the newest ones); older loaded ones
/// are kept and merged.
pub const REFRESH_WINDOW: usize = 30;
/// Load the next page when the cursor is this close to the end.
const LOAD_MORE_MARGIN: usize = 10;
/// Rows of the task pane given to each image attachment.
pub const IMAGE_ROWS: usize = 12;
/// Decoded images kept in memory; older ones are downloaded again if needed.
pub const MAX_IMAGES: usize = 16;

/// Notifications plus the offset to continue from (`None` at the end).
pub type Loaded = Result<(Vec<Notification>, Option<String>), String>;

/// Something that happened: a key press, a timer, or the result of an effect.
#[derive(Debug)]
pub enum Msg {
    Key(KeyEvent),
    /// How much fits on screen (sent on start and resize).
    Resize(Sizes),
    /// A project's board, or why it couldn't be loaded.
    BoardLoaded {
        project_gid: String,
        result: Result<Board, String>,
    },
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
    /// An image attachment was downloaded and decoded (or wasn't).
    ImageLoaded {
        attachment_gid: String,
        result: Result<Arc<image::DynamicImage>, String>,
    },
    /// `$EDITOR` closed: the text, or `None` if cancelled.
    Written {
        kind: EditorKind,
        task_gid: String,
        text: Option<String>,
    },
    CommentPosted {
        task_gid: String,
        local_gid: String,
        result: Result<Box<TaskStory>, String>,
    },
    NotesSaved {
        task_gid: String,
        result: Result<(), SaveFailure>,
    },
    TasksFound {
        seq: u64,
        result: Result<Vec<FoundTask>, String>,
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
    MovedToSection {
        task_gid: String,
        project_gid: String,
        section: Named,
        previous: Option<Named>,
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
    /// Load a project's board, then send [`Msg::BoardLoaded`].
    LoadBoard {
        project_gid: String,
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
    /// Download and decode an image attachment, then send [`Msg::ImageLoaded`].
    LoadImage {
        attachment_gid: String,
        url: String,
    },
    /// Hand the terminal to `$EDITOR`, starting with `initial`, then send
    /// [`Msg::Written`].
    EditExternal {
        kind: EditorKind,
        task_gid: String,
        initial: String,
    },
    PostComment {
        task_gid: String,
        local_gid: String,
        text: String,
    },
    /// Save the description unless the task changed since `expected_modified_at`.
    SaveNotes {
        task_gid: String,
        text: String,
        expected_modified_at: Option<String>,
    },
    /// Send [`Msg::SearchDue`] for `seq` after [`picker::SEARCH_DEBOUNCE`].
    ScheduleSearch {
        seq: u64,
    },
    SearchProjects {
        query: String,
        seq: u64,
    },
    /// Find tasks in the workspace, then send [`Msg::TasksFound`].
    SearchTasks {
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
    /// Move the task to `section` of `project_gid` (it was in `previous`).
    MoveToSection {
        task_gid: String,
        project_gid: String,
        section: Named,
        previous: Option<Named>,
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

/// What the editor overlay writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorKind {
    Comment,
    Description,
}

impl EditorKind {
    /// What `Ctrl-s` does: "post" or "save".
    pub fn verb(self) -> &'static str {
        match self {
            EditorKind::Comment => "post",
            EditorKind::Description => "save",
        }
    }

    /// The editor's title for the task named `task`.
    pub fn title(self, task: &str) -> String {
        match self {
            EditorKind::Comment => format!(" Comment on {task} "),
            EditorKind::Description => format!(" Description of {task} "),
        }
    }
}

/// How a key closes the editor.
enum EditorExit {
    Save,
    External,
    Cancel,
}

/// The text editor overlay for a comment or a task description.
#[derive(Debug, Clone)]
pub struct Editor {
    pub kind: EditorKind,
    pub task_gid: String,
    pub textarea: TextArea<'static>,
}

impl Editor {
    /// Starts with `text`: a comment draft continues at its end, a
    /// description is edited from the top.
    fn new(kind: EditorKind, task_gid: String, text: &str) -> Editor {
        let mut textarea = TextArea::new(text.split('\n').map(str::to_owned).collect());
        textarea.set_cursor_line_style(ratatui::style::Style::default());
        if kind == EditorKind::Comment {
            textarea.move_cursor(CursorMove::Bottom);
            textarea.move_cursor(CursorMove::End);
        }
        Editor {
            kind,
            task_gid,
            textarea,
        }
    }

    pub fn text(&self) -> String {
        self.textarea.lines().join("\n")
    }
}

/// An image attachment, by attachment GID.
#[derive(Debug, Clone)]
pub enum ImageState {
    Loading,
    Loaded(Arc<image::DynamicImage>),
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
    /// Cells across the board's columns.
    pub board_width: usize,
    /// Cards that fit in a board column.
    pub board_card_rows: usize,
}

/// Which top-level screen is shown.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Inbox,
    Board,
}

/// The board of one project, as far as it's loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardSlot {
    pub project_gid: String,
    pub state: BoardState,
    /// A load of this board is running; another one isn't started.
    reloading: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoardState {
    Loading,
    Failed(String),
    Loaded(BoardView),
}

/// Why a description wasn't saved, and where the user's text was kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFailure {
    pub message: String,
    pub kept_at: Option<String>,
}

/// A yes/no question in the status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub question: String,
    task_gid: String,
    initial: String,
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
    pub screen: Screen,
    /// The project board opened this session, if any.
    pub board: Option<BoardSlot>,
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
    /// The running inbox load is a refresh, to be merged with what's loaded.
    merging_refresh: bool,
    /// A failed "load more" already triggered a reload from the first page.
    recovering: bool,
    /// Why the Inbox couldn't be loaded (shown when there is nothing to show).
    pub error: Option<String>,
    pub toast: Option<Toast>,
    /// A pending yes/no question (e.g. before removing formatting).
    pub confirm: Option<Confirm>,
    /// Descriptions being saved: the text to restore if saving fails.
    previous_notes: HashMap<String, String>,
    /// The `?` help overlay is open.
    pub help: bool,
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
    /// The comment or description being written, when the editor is open.
    pub editor: Option<Editor>,
    /// A task opened from workspace search: the task pane shows it instead
    /// of the selected notification's task until focus returns to the list.
    pub opened: Option<Named>,
    /// Image attachments of the tasks looked at, at most [`MAX_IMAGES`].
    pub images: HashMap<String, ImageState>,
    /// Keys of `images`, oldest first (the next to be forgotten).
    image_order: VecDeque<String>,
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

    /// A new app that shows the board of `project_gid` and loads the Inbox
    /// behind it.
    pub fn new_on_board(project_gid: String) -> (App, Vec<Effect>) {
        let (mut app, mut effects) = App::new();
        app.screen = Screen::Board;
        app.board = Some(BoardSlot {
            project_gid: project_gid.clone(),
            state: BoardState::Loading,
            reloading: true,
        });
        effects.push(Effect::LoadBoard { project_gid });
        (app, effects)
    }

    pub fn update(&mut self, msg: Msg) -> Vec<Effect> {
        let before = self.selected_task_gid().map(str::to_owned);
        let mut effects = self.handle(msg);
        let after = self.selected_task_gid().map(str::to_owned);
        if after != before {
            self.detail_scroll = 0;
            if let Some(gid) = after {
                if self.details.contains_key(&gid) {
                    effects.extend(self.load_images(&gid));
                } else {
                    effects.push(Effect::ScheduleDetail { gid });
                }
            }
        }
        effects
    }

    fn handle(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Key(_) if self.help => {
                // Any key closes the help and does nothing else.
                self.help = false;
                Vec::new()
            }
            Msg::Key(event) if self.editor.is_some() => self.editor_key(event),
            Msg::Key(event) if self.confirm.is_some() => {
                let confirm = self.confirm.take().expect("checked above");
                if matches!(event.code, KeyCode::Char('y' | 'Y')) {
                    self.editor = Some(Editor::new(
                        EditorKind::Description,
                        confirm.task_gid,
                        &confirm.initial,
                    ));
                    Vec::new()
                } else {
                    self.toast = Some(Toast::info("Description not edited"));
                    Vec::new()
                }
            }
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
                    Outcome::Action(action) if self.screen == Screen::Board => {
                        self.run_board(action)
                    }
                    Outcome::Action(action) => self.run(action),
                    Outcome::Insert(_) | Outcome::Pending | Outcome::Ignored => Vec::new(),
                }
            }
            Msg::Resize(sizes) => {
                if let Some(view) = self.board_view_mut() {
                    view.resize(sizes.board_width, sizes.board_card_rows);
                }
                self.list_rows = sizes.list_rows.max(1);
                self.sizes = sizes;
                self.move_to(self.cursor);
                self.load_more_if_needed()
            }
            Msg::Tick => {
                let mut effects = self.refresh();
                effects.extend(self.reload_board());
                effects
            }
            Msg::BoardLoaded {
                project_gid,
                result,
            } => {
                self.board_loaded(&project_gid, result);
                Vec::new()
            }
            Msg::InboxLoaded(Ok((fresh, next))) => {
                self.loading = false;
                self.recovering = false;
                self.error = None;
                self.visual_anchor = None;
                let merge = std::mem::take(&mut self.merging_refresh) && next.is_some();
                self.replace_keeping_selection(|app| {
                    if merge {
                        app.merge_refresh(fresh, next);
                    } else {
                        app.notifications = fresh;
                        app.next_offset = next;
                    }
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
                self.details.insert(gid.clone(), detail);
                self.detail_scroll = self.detail_scroll.min(self.detail_max_scroll());
                if self.selected_task_gid() == Some(gid.as_str()) {
                    self.load_images(&gid)
                } else {
                    Vec::new()
                }
            }
            Msg::ImageLoaded {
                attachment_gid,
                result,
            } => {
                let state = match result {
                    Ok(image) => ImageState::Loaded(image),
                    Err(message) => ImageState::Failed(message),
                };
                self.remember_image(attachment_gid, state);
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
                self.merging_refresh = false;
                vec![Effect::LoadInbox {
                    target: self.notifications.len().max(INITIAL_LOAD),
                }]
            }
            Msg::Written {
                kind,
                task_gid,
                text: Some(text),
            } => self.finish_editing(kind, task_gid, text),
            Msg::Written { text: None, .. } => Vec::new(),
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
                        self.toast = Some(Toast::info("Comment posted"));
                        // Commenting means the task was dealt with.
                        return self.mark_task_read(&task_gid);
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
            Msg::NotesSaved {
                task_gid,
                result: Ok(()),
            } => {
                self.previous_notes.remove(&task_gid);
                self.toast = Some(Toast::info("Description saved"));
                // Reload for Asana's html_notes and modified_at.
                vec![Effect::LoadDetail { gid: task_gid }]
            }
            Msg::NotesSaved {
                task_gid,
                result: Err(failure),
            } => {
                if let Some(old) = self.previous_notes.remove(&task_gid)
                    && let Some(Detail::Loaded(detail)) = self.details.get_mut(&task_gid)
                {
                    detail.task.notes = old;
                }
                let kept = failure
                    .kept_at
                    .map_or_else(String::new, |path| format!(". Your text is in {path}"));
                self.toast = Some(Toast::error(format!(
                    "Description not saved: {}{kept}",
                    failure.message
                )));
                Vec::new()
            }
            Msg::SearchDue { seq } => {
                let Some(p) = self.picker.as_mut().filter(|p| p.search_seq == seq) else {
                    return Vec::new();
                };
                let query = p.query.clone();
                let effect = match p.stage {
                    picker::Stage::Project => Effect::SearchProjects { query, seq },
                    // A blank query finds nothing; don't ask Asana.
                    picker::Stage::Task if query.trim().is_empty() => {
                        p.accept_tasks(seq, Vec::new());
                        return Vec::new();
                    }
                    picker::Stage::Task => Effect::SearchTasks { query, seq },
                    _ => return Vec::new(),
                };
                p.loading = true;
                vec![effect]
            }
            Msg::TasksFound { seq, result } => {
                match result {
                    Ok(tasks) => {
                        if let Some(p) = &mut self.picker {
                            p.accept_tasks(seq, tasks);
                        }
                    }
                    Err(message) => self.search_failed(seq, "Task", &message),
                }
                Vec::new()
            }
            Msg::ProjectsFound { seq, result } => {
                match result {
                    Ok(projects) => {
                        if let Some(p) = &mut self.picker {
                            p.accept_projects(seq, projects);
                        }
                    }
                    Err(message) => self.search_failed(seq, "Project", &message),
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
                        self.toast = Some(Toast::info("Added to project"));
                        return self.reload_board();
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
            Msg::MovedToSection {
                task_gid,
                project_gid,
                section,
                previous,
                result,
            } => {
                match result {
                    Ok(()) => {
                        self.toast = Some(Toast::info(format!("Moved to {}", section.name)));
                        return self.reload_board();
                    }
                    Err(message) => {
                        self.set_section(&task_gid, &project_gid, previous);
                        self.toast = Some(Toast::error(format!("Not moved: {message}")));
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

    /// The task in the task pane: one opened from search, else the task the
    /// selected notification is about (if it is about a task). On the board,
    /// the selected card.
    pub fn selected_task_gid(&self) -> Option<&str> {
        if self.screen == Screen::Board {
            return Some(self.selected_card()?.gid.as_str());
        }
        if let Some(opened) = &self.opened {
            return Some(&opened.gid);
        }
        let object = self.selected()?.work_objects.first()?;
        (object.resource_type == "task").then_some(object.gid.as_str())
    }

    /// Moves focus; leaving the task pane closes a task opened from search,
    /// so the pane shows the selected notification's task again.
    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        if focus == Focus::List {
            self.opened = None;
        }
    }

    /// Whether an overlay (help, picker, editor) covers the screen.
    pub fn overlay_open(&self) -> bool {
        self.help || self.picker.is_some() || self.editor.is_some()
    }

    /// The name of what the task pane shows, known before its details load.
    pub fn shown_task_name(&self) -> Option<&str> {
        if self.screen == Screen::Board {
            return Some(self.selected_card()?.name.as_str());
        }
        if let Some(opened) = &self.opened {
            return Some(&opened.name);
        }
        Some(self.selected()?.work_objects.first()?.name.as_str())
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
                self.set_focus(Focus::List);
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
            Cmd::SearchTasks => self.picker = Some(Picker::tasks()),
            Cmd::Help => self.help = true,
            Cmd::GoToInbox => {
                self.set_focus(Focus::List);
                self.move_to(0);
            }
            Cmd::Back | Cmd::FocusLeft => self.set_focus(Focus::List),
            Cmd::FocusRight | Cmd::Open if self.selected().is_some() => {
                self.set_focus(Focus::Detail)
            }
            Cmd::OpenInBrowser => return self.open_in_browser(),
            Cmd::Comment => return self.start_comment(),
            Cmd::EditDescription => return self.start_description(),
            Cmd::AddToProject => return self.open_picker(),
            Cmd::MoveToSection => return self.open_move(),
            Cmd::MarkRead => return self.mark(Some(true)),
            Cmd::MarkUnread => return self.mark(Some(false)),
            Cmd::ToggleRead => return self.mark(None),
            Cmd::ToggleFilter => {
                self.visual_anchor = None;
                self.replace_keeping_selection(|app| app.unread_only = !app.unread_only);
            }
            Cmd::Refresh => return self.refresh(),
            Cmd::Quit if self.visual_anchor.is_some() => self.visual_anchor = None,
            Cmd::Quit if self.focus == Focus::Detail => self.set_focus(Focus::List),
            Cmd::Quit | Cmd::ForceQuit => self.should_quit = true,
            _ => {}
        }
        self.load_more_if_needed()
    }

    /// A key command on the board screen: motions move on the board, or
    /// scroll the open card; task actions work on the selected card; Inbox
    /// commands do nothing here.
    fn run_board(&mut self, Action { cmd, count }: Action) -> Vec<Effect> {
        let n = count.unwrap_or(1);
        let half_page = (self.sizes.board_card_rows / 2).max(1);
        if self.focus == Focus::Detail {
            if self.scroll_detail(cmd, count) {
                return Vec::new();
            }
            if matches!(cmd, Cmd::Back | Cmd::FocusLeft | Cmd::Quit) {
                self.set_focus(Focus::List);
                return Vec::new();
            }
        } else if let Some(view) = self.board_view_mut() {
            match cmd {
                Cmd::MoveDown => view.down(n),
                Cmd::MoveUp => view.up(n),
                Cmd::Top => view.top(),
                Cmd::Bottom => match count {
                    Some(line) => {
                        view.top();
                        view.down(line.saturating_sub(1));
                    }
                    None => view.bottom(),
                },
                Cmd::HalfPageDown => view.down(half_page),
                Cmd::HalfPageUp => view.up(half_page),
                Cmd::FocusRight => view.right(n),
                Cmd::FocusLeft => view.left(n),
                _ => {}
            }
        }
        match cmd {
            Cmd::Open if self.selected_card().is_some() => self.set_focus(Focus::Detail),
            Cmd::Comment => return self.start_comment(),
            Cmd::EditDescription => return self.start_description(),
            Cmd::AddToProject => return self.open_picker(),
            Cmd::MoveToSection => return self.open_move(),
            Cmd::OpenInBrowser => return self.open_in_browser(),
            Cmd::Help => self.help = true,
            Cmd::Refresh => return self.reload_board(),
            Cmd::Quit | Cmd::ForceQuit => self.should_quit = true,
            _ => {}
        }
        Vec::new()
    }

    /// Reloads the board on screen, unless a load of it is running.
    fn reload_board(&mut self) -> Vec<Effect> {
        if self.screen != Screen::Board {
            return Vec::new();
        }
        let Some(slot) = self.board.as_mut().filter(|slot| !slot.reloading) else {
            return Vec::new();
        };
        slot.reloading = true;
        vec![Effect::LoadBoard {
            project_gid: slot.project_gid.clone(),
        }]
    }

    /// The selected card of the loaded board, if any.
    fn selected_card(&self) -> Option<&crate::api::projects::BoardTask> {
        self.board_view()?.selected_task()
    }

    /// The loaded board on screen, if any.
    pub fn board_view(&self) -> Option<&BoardView> {
        match &self.board.as_ref()?.state {
            BoardState::Loaded(view) => Some(view),
            _ => None,
        }
    }

    fn board_view_mut(&mut self) -> Option<&mut BoardView> {
        match &mut self.board.as_mut()?.state {
            BoardState::Loaded(view) => Some(view),
            _ => None,
        }
    }

    /// Shows a loaded board, keeping the selection of the one shown before.
    /// A failure replaces a board only if none is shown yet.
    fn board_loaded(&mut self, project_gid: &str, result: Result<Board, String>) {
        let sizes = self.sizes;
        let Some(slot) = self
            .board
            .as_mut()
            .filter(|slot| slot.project_gid == project_gid)
        else {
            return;
        };
        slot.reloading = false;
        match (&mut slot.state, result) {
            (BoardState::Loaded(view), Ok(board)) => view.replace(board),
            (_, Ok(board)) => {
                let mut view = BoardView::new(board);
                view.resize(sizes.board_width, sizes.board_card_rows);
                slot.state = BoardState::Loaded(view);
            }
            (BoardState::Loaded(_), Err(message)) => {
                self.toast = Some(Toast::error(format!("Refresh failed: {message}")));
            }
            (_, Err(message)) => slot.state = BoardState::Failed(message),
        }
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

    /// Starts downloading the images of task `gid` that aren't loaded or
    /// loading. Only `https://` links are followed.
    fn load_images(&mut self, gid: &str) -> Vec<Effect> {
        let Some(Detail::Loaded(detail)) = self.details.get(gid) else {
            return Vec::new();
        };
        let wanted: Vec<(String, String)> = detail
            .attachments
            .iter()
            .filter(|a| a.is_image())
            .filter(|a| matches!(self.images.get(&a.gid), None | Some(ImageState::Failed(_))))
            .filter_map(|a| {
                let url = a.download_url.as_ref()?;
                url.starts_with("https://")
                    .then(|| (a.gid.clone(), url.clone()))
            })
            .collect();
        wanted
            .into_iter()
            .map(|(attachment_gid, url)| {
                self.remember_image(attachment_gid.clone(), ImageState::Loading);
                Effect::LoadImage {
                    attachment_gid,
                    url,
                }
            })
            .collect()
    }

    /// Stores an image's state, forgetting the oldest beyond [`MAX_IMAGES`].
    fn remember_image(&mut self, attachment_gid: String, state: ImageState) {
        self.image_order.retain(|gid| *gid != attachment_gid);
        self.image_order.push_back(attachment_gid.clone());
        self.images.insert(attachment_gid, state);
        while self.images.len() > MAX_IMAGES {
            let Some(oldest) = self.image_order.pop_front() else {
                break;
            };
            self.images.remove(&oldest);
        }
    }

    /// The last scroll position that still fills the detail pane.
    /// Text columns of the task view: the detail pane next to the Inbox, or
    /// the whole width on the board (inside borders and padding).
    pub fn detail_width(&self) -> usize {
        match self.screen {
            Screen::Board => self.sizes.board_width.saturating_sub(4),
            Screen::Inbox => self.sizes.detail_width,
        }
    }

    fn detail_max_scroll(&self) -> usize {
        match self.selected_detail() {
            Some(Detail::Loaded(detail)) => {
                detail::lines_with_images(detail, self.detail_width(), self.now, IMAGE_ROWS)
                    .0
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
                self.toast = Some(Toast::info(format!(
                    "Marking {} notification(s) read",
                    gids.len()
                )));
                vec![Effect::MarkRead { gids, read: true }]
            }
            other => {
                self.toast = Some(Toast::error(format!("Unknown command: {other}")));
                Vec::new()
            }
        }
    }

    /// Search `seq` of the picker failed: stop its spinner and say why,
    /// unless a newer search replaced it.
    fn search_failed(&mut self, seq: u64, what: &str, message: &str) {
        if let Some(p) = self.picker.as_mut().filter(|p| p.search_seq == seq) {
            p.loading = false;
            self.toast = Some(Toast::error(format!("{what} search failed: {message}")));
        }
    }

    /// Starts moving the shown task to another section: straight to the
    /// sections when it's in one project, else asks which project first.
    fn open_move(&mut self) -> Vec<Effect> {
        let Some((task_gid, detail)) = self.loaded_shown_task("Only tasks can be moved") else {
            return Vec::new();
        };
        let mut projects: Vec<Named> = detail
            .task
            .memberships
            .iter()
            .map(|m| m.project.clone())
            .collect();
        match projects.len() {
            0 => {
                self.toast = Some(Toast::info(
                    "The task isn't in a project yet; p adds it to one",
                ));
                Vec::new()
            }
            1 => self.open_sections(task_gid, projects.remove(0)),
            _ => {
                self.picker = Some(Picker::move_within(task_gid, projects));
                Vec::new()
            }
        }
    }

    /// Lists the sections of `project` to move task `task_gid` to.
    fn open_sections(&mut self, task_gid: String, project: Named) -> Vec<Effect> {
        let current = self.membership_section(&task_gid, &project.gid);
        let project_gid = project.gid.clone();
        self.picker = Some(Picker::move_to_section(
            task_gid,
            project,
            current.map(|s| s.gid),
        ));
        vec![Effect::LoadSections { project_gid }]
    }

    /// The shown task's GID and details, once loaded. Otherwise explains why
    /// not (`not_a_task` when the selection isn't a task) and returns `None`.
    fn loaded_shown_task(&mut self, not_a_task: &str) -> Option<(String, &TaskDetail)> {
        let Some(task_gid) = self.selected_task_gid().map(str::to_owned) else {
            self.toast = Some(Toast::info(not_a_task));
            return None;
        };
        if !matches!(self.details.get(&task_gid), Some(Detail::Loaded(_))) {
            self.toast = Some(Toast::info("Wait for the task to load"));
            return None;
        }
        match self.details.get(&task_gid) {
            Some(Detail::Loaded(detail)) => Some((task_gid, detail)),
            _ => None,
        }
    }

    /// The section task `task_gid` is in within `project_gid`, if loaded.
    fn membership_section(&self, task_gid: &str, project_gid: &str) -> Option<Named> {
        let Some(Detail::Loaded(detail)) = self.details.get(task_gid) else {
            return None;
        };
        detail
            .task
            .memberships
            .iter()
            .find(|m| m.project.gid == project_gid)
            .and_then(|m| m.section.clone())
    }

    /// Shows task `task_gid` in `section` of `project_gid` (if loaded).
    fn set_section(&mut self, task_gid: &str, project_gid: &str, section: Option<Named>) {
        if let Some(Detail::Loaded(detail)) = self.details.get_mut(task_gid)
            && let Some(membership) = detail
                .task
                .memberships
                .iter_mut()
                .find(|m| m.project.gid == project_gid)
        {
            membership.section = section;
        }
    }

    /// Opens the project picker for the selected task, hiding the projects
    /// it's already in.
    fn open_picker(&mut self) -> Vec<Effect> {
        let Some(task_gid) = self.selected_task_gid().map(str::to_owned) else {
            self.toast = Some(Toast::info("Only tasks can be added to projects"));
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
            Outcome::Insert(c) if picker.has_query() => {
                let query = format!("{}{c}", picker.query);
                schedule(picker, query)
            }
            // Lists without a query move with j/k too.
            Outcome::Insert(c @ ('j' | 'k')) => {
                picker.move_by(if c == 'j' { 1 } else { -1 });
                Vec::new()
            }
            Outcome::Action(Action { cmd, .. }) => match cmd {
                Cmd::DeleteChar if picker.has_query() => {
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
            picker::Stage::MoveProject => {
                let task_gid = picker.task_gid.clone();
                self.open_sections(task_gid, item.into_named())
            }
            picker::Stage::MoveSection { project, .. } => {
                let task_gid = picker.task_gid.clone();
                self.picker = None;
                if item.checked {
                    self.toast = Some(Toast::info(format!("Already in {}", item.name)));
                    return Vec::new();
                }
                let section = item.into_named();
                let previous = self.membership_section(&task_gid, &project.gid);
                self.set_section(&task_gid, &project.gid, Some(section.clone()));
                vec![Effect::MoveToSection {
                    task_gid,
                    project_gid: project.gid,
                    section,
                    previous,
                }]
            }
            picker::Stage::Task => {
                self.picker = None;
                let task = item.into_named();
                let gid = task.gid.clone();
                self.opened = Some(task);
                self.set_focus(Focus::Detail);
                if self.details.contains_key(&gid) {
                    return Vec::new();
                }
                self.details.insert(gid.clone(), Detail::Loading);
                vec![Effect::LoadDetail { gid }]
            }
            picker::Stage::Project => {
                let project = item.into_named();
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

    /// Opens the editor with the selected task's description. A description
    /// with formatting asks first, since saving plain text removes it.
    fn start_description(&mut self) -> Vec<Effect> {
        let Some((task_gid, detail)) = self.loaded_shown_task("Only tasks have descriptions")
        else {
            return Vec::new();
        };
        let initial = detail.task.notes.clone();
        if detail
            .task
            .html_notes
            .as_deref()
            .is_some_and(crate::core::has_formatting)
        {
            self.confirm = Some(Confirm {
                question: "This will remove formatting. Continue? (y/N)".into(),
                task_gid,
                initial,
            });
            return Vec::new();
        }
        self.editor = Some(Editor::new(EditorKind::Description, task_gid, &initial));
        Vec::new()
    }

    /// Shows the new description right away and saves it, remembering the old
    /// one in case saving fails. Unchanged text isn't saved.
    fn show_and_save_description(&mut self, task_gid: String, text: String) -> Vec<Effect> {
        let Some(Detail::Loaded(detail)) = self.details.get_mut(&task_gid) else {
            return Vec::new();
        };
        if text.trim_end() == detail.task.notes.trim_end() {
            self.toast = Some(Toast::info("No changes"));
            return Vec::new();
        }
        let old = std::mem::replace(&mut detail.task.notes, text.clone());
        let expected_modified_at = detail.task.modified_at.clone();
        self.previous_notes.insert(task_gid.clone(), old);
        vec![Effect::SaveNotes {
            task_gid,
            text,
            expected_modified_at,
        }]
    }

    /// Marks every loaded unread notification about `task_gid` read.
    fn mark_task_read(&mut self, task_gid: &str) -> Vec<Effect> {
        let gids: Vec<String> = self
            .notifications
            .iter()
            .filter(|n| !n.is_read && n.work_objects.first().is_some_and(|o| o.gid == task_gid))
            .map(|n| n.gid.clone())
            .collect();
        if gids.is_empty() {
            return Vec::new();
        }
        self.replace_keeping_selection(|app| app.set_read(&gids, true));
        for gid in &gids {
            self.pending_marks.insert(gid.clone(), true);
        }
        vec![Effect::MarkRead { gids, read: true }]
    }

    /// Opens the editor for a comment on the selected task, with any draft
    /// left by a failed post.
    fn start_comment(&mut self) -> Vec<Effect> {
        let Some(task_gid) = self.selected_task_gid().map(str::to_owned) else {
            self.toast = Some(Toast::info("Comments work on tasks only"));
            return Vec::new();
        };
        let initial = self.drafts.remove(&task_gid).unwrap_or_default();
        self.editor = Some(Editor::new(EditorKind::Comment, task_gid, &initial));
        Vec::new()
    }

    /// A key in the editor overlay: `Ctrl-s` posts or saves, `Esc`/`Ctrl-c`
    /// cancel (a comment is kept as a draft), `Ctrl-o` continues in
    /// `$EDITOR`; everything else edits the text.
    fn editor_key(&mut self, event: KeyEvent) -> Vec<Effect> {
        if event.kind == KeyEventKind::Release {
            return Vec::new();
        }
        self.toast = None;
        let ctrl = |c: char| {
            event.modifiers.contains(KeyModifiers::CONTROL) && event.code == KeyCode::Char(c)
        };
        let exit = if ctrl('s') {
            EditorExit::Save
        } else if ctrl('o') {
            EditorExit::External
        } else if ctrl('c') || event.code == KeyCode::Esc {
            EditorExit::Cancel
        } else {
            if let Some(editor) = &mut self.editor {
                editor.textarea.input(event);
            }
            return Vec::new();
        };
        let Some(editor) = self.editor.take() else {
            return Vec::new();
        };
        let (kind, task_gid, text) = (editor.kind, editor.task_gid.clone(), editor.text());
        match exit {
            EditorExit::Save => self.finish_editing(kind, task_gid, text),
            EditorExit::External => vec![Effect::EditExternal {
                kind,
                task_gid,
                initial: text,
            }],
            EditorExit::Cancel => {
                if kind == EditorKind::Comment && !text.trim().is_empty() {
                    self.drafts.insert(task_gid, text);
                }
                Vec::new()
            }
        }
    }

    /// Posts the comment or saves the description written in either editor.
    fn finish_editing(&mut self, kind: EditorKind, task_gid: String, text: String) -> Vec<Effect> {
        match kind {
            EditorKind::Comment if text.trim().is_empty() => {
                self.toast = Some(Toast::info("Empty comment; nothing posted"));
                Vec::new()
            }
            EditorKind::Comment => self.show_and_post_comment(task_gid, text),
            EditorKind::Description => self.show_and_save_description(task_gid, text),
        }
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
                reaction_summary: Vec::new(),
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
        if let Some(gid) = self.selected_task_gid() {
            return vec![Effect::OpenUrl {
                url: format!("https://app.asana.com/0/0/{gid}"),
            }];
        }
        let Some(object) = self.selected().and_then(|n| n.work_objects.first()) else {
            return Vec::new();
        };
        let url = match object.resource_type.as_str() {
            "project" => format!("https://app.asana.com/0/{}", object.gid),
            other => {
                self.toast = Some(Toast::info(format!(
                    "asn can't open {other}s yet; open it from the web Inbox"
                )));
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
        self.merging_refresh = true;
        vec![Effect::LoadInbox {
            target: REFRESH_WINDOW,
        }]
    }

    /// Puts the newest notifications from a refresh in front of the older
    /// ones already loaded. Loaded items as new as the refreshed ones are
    /// replaced (so archived ones disappear); older ones are kept, and so is
    /// the offset that continues after them.
    fn merge_refresh(&mut self, fresh: Vec<Notification>, fresh_next: Option<String>) {
        let Some(oldest) = fresh.last().map(|n| n.created_at.clone()) else {
            self.notifications = fresh;
            self.next_offset = fresh_next;
            return;
        };
        let tail: Vec<Notification> = std::mem::take(&mut self.notifications)
            .into_iter()
            .filter(|n| n.created_at < oldest && !fresh.iter().any(|f| f.gid == n.gid))
            .collect();
        if tail.is_empty() {
            self.next_offset = fresh_next;
        }
        self.notifications = fresh;
        self.notifications.extend(tail);
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
    fn info(text: impl Into<String>) -> Toast {
        Toast {
            text: text.into(),
            is_error: false,
        }
    }

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
