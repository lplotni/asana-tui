//! Operations shared by the CLI and the TUI.

use crate::api::notifications::{MARK_BATCH, Notification};
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
    let mut found = Vec::new();
    let mut offset: Option<String> = None;

    while found.len() < limit {
        let page = client
            .notifications_page(workspace, PAGE_SIZE, offset.as_deref())
            .await
            .map_err(workspace_hint)?;
        found.extend(page.data.into_iter().filter(|n| !unread_only || !n.is_read));
        match page.next_page {
            Some(next) => offset = Some(next.offset),
            None => break,
        }
    }

    found.truncate(limit);
    Ok(found)
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
