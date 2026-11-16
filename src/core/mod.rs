//! Operations shared by the CLI and the TUI.

use crate::api::notifications::{MARK_BATCH, Notification};
use crate::api::tasks::{Task, TaskStory};
use crate::api::{ApiError, Client};

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

/// A task with all its comments, oldest first.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TaskDetail {
    pub task: Task,
    pub comments: Vec<TaskStory>,
    /// System stories (activity) not shown as comments.
    pub activity_count: usize,
}

/// Fetches the task and every page of its stories at the same time.
pub async fn task_detail(client: &Client, gid: &str) -> Result<TaskDetail, ApiError> {
    let (task, stories) = tokio::join!(client.task(gid), all_stories(client, gid));
    let (comments, activity): (Vec<TaskStory>, Vec<TaskStory>) =
        stories?.into_iter().partition(TaskStory::is_comment);
    Ok(TaskDetail {
        task: task?,
        comments,
        activity_count: activity.len(),
    })
}

/// Adds a comment to a task and returns it as Asana stored it.
pub async fn comment(client: &Client, task_gid: &str, text: &str) -> Result<TaskStory, ApiError> {
    client.add_comment(task_gid, text).await
}

async fn all_stories(client: &Client, gid: &str) -> Result<Vec<TaskStory>, ApiError> {
    let mut stories = Vec::new();
    let mut offset: Option<String> = None;
    loop {
        let page = client.stories_page(gid, offset.as_deref()).await?;
        stories.extend(page.data);
        match page.next_page {
            Some(next) => offset = Some(next.offset),
            None => return Ok(stories),
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

/// Marks notifications read (or unread) in batches of [`MARK_BATCH`].
/// Returns every GID Asana could not update. No GIDs means no requests.
pub async fn mark_read(
    client: &Client,
    workspace: &str,
    gids: &[String],
    read: bool,
) -> Result<Vec<String>, ApiError> {
    let mut failed = Vec::new();
    for batch in gids.chunks(MARK_BATCH) {
        let batch_failed = client
            .mark_notifications(workspace, batch, read)
            .await
            .map_err(workspace_hint)?;
        failed.extend(batch_failed);
    }
    Ok(failed)
}
