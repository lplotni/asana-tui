//! `GET /users/me/notifications` and the notification types.

use serde::{Deserialize, Serialize};

use super::{ApiError, Client, Page};

/// Fields requested from Asana. `Notification` models exactly these, so
/// serializing it reproduces Asana's JSON.
pub const OPT_FIELDS: &str = "story_resource_subtype,is_read,created_at,priority_classification,\
work_objects.name,work_objects.resource_type,story.text,story.html_text,\
created_by.name,created_by.resource_type";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub gid: String,
    pub created_at: String,
    pub created_by: Option<Actor>,
    pub is_read: bool,
    pub story: Option<Story>,
    /// Kept as a string: Asana adds new subtypes over time.
    pub story_resource_subtype: String,
    /// Kept as a string: only `unclassified` exists today.
    pub priority_classification: String,
    pub work_objects: Vec<WorkObject>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Actor {
    pub gid: String,
    pub name: String,
    pub resource_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Story {
    pub gid: String,
    /// Absent (not `null`) for some stories, e.g. reactions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkObject {
    pub gid: String,
    pub name: String,
    pub resource_type: String,
}

impl Client {
    pub async fn notifications_page(
        &self,
        workspace: &str,
        limit: u32,
        offset: Option<&str>,
    ) -> Result<Page<Notification>, ApiError> {
        let mut query = vec![
            ("workspace", workspace.to_owned()),
            ("limit", limit.to_string()),
            ("opt_fields", OPT_FIELDS.to_owned()),
        ];
        if let Some(offset) = offset {
            query.push(("offset", offset.to_owned()));
        }
        self.get("/users/me/notifications", &query).await
    }
}

/// At most this many GIDs per `markRead`/`markUnread` call (API limit).
pub const MARK_BATCH: usize = 50;

impl Client {
    /// Marks up to [`MARK_BATCH`] notifications read (or unread). Returns the
    /// GIDs Asana could not update.
    pub async fn mark_notifications(
        &self,
        workspace: &str,
        gids: &[String],
        read: bool,
    ) -> Result<Vec<String>, ApiError> {
        #[derive(Deserialize)]
        struct Response {
            data: MarkResult,
        }
        #[derive(Deserialize)]
        struct MarkResult {
            failed_gids: Vec<String>,
        }

        let endpoint = if read { "markRead" } else { "markUnread" };
        let body = serde_json::json!({
            "data": { "workspace": workspace, "notification_gids": gids }
        });
        let response: Response = self
            .post(&format!("/users/me/notifications/{endpoint}"), &[], &body)
            .await?;
        Ok(response.data.failed_gids)
    }
}
