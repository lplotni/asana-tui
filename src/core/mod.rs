//! Operations shared by the CLI and the TUI.

use crate::api::attachments::Attachment;
use crate::api::notifications::{MARK_BATCH, Notification};
use crate::api::search::FoundTask;
use crate::api::tasks::{Named, Task, TaskStory};
use crate::api::{ApiError, Client, Page};
use crate::config::SearchMode;

/// Page size requested from Asana. Pages can still come back shorter or empty.
const PAGE_SIZE: u32 = 100;

/// Fetches up to `limit` notifications, newest first, following `next_page`
/// until Asana reports the end. `unread_only` filters on our side because the
/// API has no unread filter.
pub async fn inbox(
    client: &Client,
    workspace: &str,
    limit: usize,
    unread_only: bool,
) -> Result<Vec<Notification>, ApiError> {
    let (mut found, _) = inbox_pages(client, workspace, limit, None, |n| {
        !unread_only || !n.is_read
    })
    .await?;
    found.truncate(limit);
    Ok(found)
}

/// Fetches pages starting at `offset` until at least `target` notifications
/// matching `keep` were collected or the Inbox ends. Returns them with the
/// offset to continue from (`None` at the end). Empty pages are skipped.
pub async fn inbox_pages(
    client: &Client,
    workspace: &str,
    target: usize,
    offset: Option<&str>,
    keep: impl Fn(&Notification) -> bool,
) -> Result<(Vec<Notification>, Option<String>), ApiError> {
    let mut found = Vec::new();
    let mut offset = offset.map(str::to_owned);
    loop {
        let page = client
            .notifications_page(workspace, PAGE_SIZE, offset.as_deref())
            .await
            .map_err(workspace_hint)?;
        found.extend(page.data.into_iter().filter(|n| keep(n)));
        offset = page.next_page.map(|next| next.offset);
        if offset.is_none() || found.len() >= target {
            return Ok((found, offset));
        }
    }
}

/// A task with all its comments (oldest first) and attachments.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TaskDetail {
    pub task: Task,
    pub comments: Vec<TaskStory>,
    /// System stories (activity) not shown as comments.
    pub activity_count: usize,
    pub attachments: Vec<Attachment>,
    /// Why the comments couldn't be loaded. Asana refuses the stories of
    /// some tasks (e.g. with replies the public API can't represent); the
    /// rest of the task is still shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comments_error: Option<String>,
}

/// Fetches the task, every page of its stories and every page of its
/// attachments at the same time.
pub async fn task_detail(client: &Client, gid: &str) -> Result<TaskDetail, ApiError> {
    let (task, stories, attachments) = tokio::join!(
        client.task(gid),
        all_pages(|offset| async move { client.stories_page(gid, offset.as_deref()).await }),
        all_pages(|offset| async move { client.attachments_page(gid, offset.as_deref()).await }),
    );
    let task = task?;
    let (stories, comments_error) = match stories {
        Ok(stories) => (stories, None),
        Err(err) => (Vec::new(), Some(err.to_string())),
    };
    let (comments, activity): (Vec<TaskStory>, Vec<TaskStory>) =
        stories.into_iter().partition(TaskStory::is_comment);
    Ok(TaskDetail {
        task,
        comments,
        activity_count: activity.len(),
        attachments: attachments?,
        comments_error,
    })
}

/// Adds a comment to a task and returns it as Asana stored it.
pub async fn comment(client: &Client, task_gid: &str, text: &str) -> Result<TaskStory, ApiError> {
    client.add_comment(task_gid, text).await
}

/// Projects matching `query` (Asana's typeahead; works on every plan).
pub async fn search_projects(
    client: &Client,
    workspace: &str,
    query: &str,
) -> Result<Vec<Named>, ApiError> {
    client.typeahead_projects(workspace, query).await
}

/// Tasks in the workspace matching `query`, using the `ASN_SEARCH` backend.
/// A blank query finds nothing (task search would return every task).
pub async fn search_tasks(
    client: &Client,
    workspace: &str,
    query: &str,
    mode: SearchMode,
) -> Result<Vec<FoundTask>, ApiError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    match mode {
        SearchMode::Tasks => client
            .search_tasks(workspace, query)
            .await
            .map_err(search_plan_hint),
        SearchMode::Typeahead => client.typeahead_tasks(workspace, query).await,
    }
}

/// Task search needs a paid plan; Asana answers 402 otherwise.
fn search_plan_hint(err: ApiError) -> ApiError {
    match err {
        ApiError::Status {
            status: 402,
            message,
        } => ApiError::Status {
            status: 402,
            message: format!("{message} — set ASN_SEARCH=typeahead to search task names instead"),
        },
        other => other,
    }
}

/// The sections of a project, in board order.
pub async fn sections(client: &Client, project_gid: &str) -> Result<Vec<Named>, ApiError> {
    client.sections(project_gid).await
}

/// Adds the task to a project (and optionally a section), keeping the
/// projects it's already in.
pub async fn add_to_project(
    client: &Client,
    task_gid: &str,
    project_gid: &str,
    section_gid: Option<&str>,
) -> Result<(), ApiError> {
    client.add_project(task_gid, project_gid, section_gid).await
}

/// Moves the task to another section of a project it's already in.
pub async fn move_to_section(
    client: &Client,
    task_gid: &str,
    section_gid: &str,
) -> Result<(), ApiError> {
    client.add_task_to_section(section_gid, task_gid).await
}

/// Why a description couldn't be saved.
#[derive(Debug, thiserror::Error)]
pub enum SaveNotesError {
    #[error("the task was changed on the web while you were editing")]
    Conflict,
    #[error(transparent)]
    Api(#[from] ApiError),
}

/// Whether Asana's `html_notes` has anything beyond plain paragraphs (links,
/// lists, bold, mentions, …) that saving plain `notes` would remove.
pub fn has_formatting(html_notes: &str) -> bool {
    html_notes.split('<').skip(1).any(|tag| {
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        !name.is_empty() && name != "body"
    })
}

/// Replaces the task's description with plain `text`, unless the task was
/// modified after `expected_modified_at` (then nothing is written).
pub async fn set_notes(
    client: &Client,
    task_gid: &str,
    text: &str,
    expected_modified_at: Option<&str>,
) -> Result<(), SaveNotesError> {
    if let Some(expected) = expected_modified_at {
        let current = client.modified_at(task_gid).await?;
        if current.as_deref() != Some(expected) {
            return Err(SaveNotesError::Conflict);
        }
    }
    client.set_notes(task_gid, text).await?;
    Ok(())
}

/// Every item of a paginated list: calls `fetch` with each page's offset
/// (`None` first) until Asana reports the end.
async fn all_pages<T, F, Fut>(mut fetch: F) -> Result<Vec<T>, ApiError>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<Page<T>, ApiError>>,
{
    let mut items = Vec::new();
    let mut offset: Option<String> = None;
    loop {
        let page = fetch(offset.take()).await?;
        items.extend(page.data);
        match page.next_page {
            Some(next) => offset = Some(next.offset),
            None => return Ok(items),
        }
    }
}

/// The Inbox call is workspace-scoped, so 403/404 almost always mean a wrong
/// `ASANA_WORKSPACE`.
fn workspace_hint(err: ApiError) -> ApiError {
    match err {
        ApiError::Status {
            status: status @ (403 | 404),
            message,
        } => ApiError::Status {
            status,
            message: format!("{message} — check ASANA_WORKSPACE"),
        },
        other => other,
    }
}

/// What happened when marking notifications: the GIDs that weren't updated,
/// and the first error if a whole batch failed.
#[derive(Debug, Default)]
pub struct MarkOutcome {
    pub failed: Vec<String>,
    pub error: Option<ApiError>,
}

/// Marks notifications read (or unread) in batches of [`MARK_BATCH`]. Every
/// batch is tried: a failing batch puts its own GIDs into `failed` without
/// affecting the others. No GIDs means no requests.
pub async fn mark_read(
    client: &Client,
    workspace: &str,
    gids: &[String],
    read: bool,
) -> MarkOutcome {
    let mut outcome = MarkOutcome::default();
    for batch in gids.chunks(MARK_BATCH) {
        match client.mark_notifications(workspace, batch, read).await {
            Ok(failed) => outcome.failed.extend(failed),
            Err(err) => {
                outcome.failed.extend_from_slice(batch);
                outcome.error.get_or_insert(workspace_hint(err));
            }
        }
    }
    outcome
}
