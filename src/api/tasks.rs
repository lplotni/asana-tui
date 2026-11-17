//! `GET /tasks/{gid}` and `GET /tasks/{gid}/stories`.

use serde::{Deserialize, Serialize};

use super::{ApiError, Client, Page};

/// Task fields shown in the detail pane.
pub const TASK_FIELDS: &str = "name,completed,completed_at,assignee.name,due_on,due_at,start_on,\
notes,html_notes,permalink_url,modified_at,memberships.project.name,memberships.section.name,\
tags.name,custom_fields.name,custom_fields.display_value,parent.name,num_subtasks";

/// Story fields: enough to show comments and count the rest.
pub const STORY_FIELDS: &str = "type,resource_subtype,text,html_text,created_at,created_by.name,\
reaction_summary.variant,reaction_summary.count";

/// Text to write: plain, or Asana rich text (`<body>…</body>`), which goes
/// to the field's `html_` twin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Text<'a> {
    Plain(&'a str),
    Rich(&'a str),
}

impl Text<'_> {
    /// `{field: text}` or `{html_field: html}`.
    fn field(self, field: &str) -> serde_json::Value {
        match self {
            Text::Plain(text) => serde_json::json!({ field: text }),
            Text::Rich(html) => serde_json::json!({ format!("html_{field}"): html }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Named {
    pub gid: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Membership {
    pub project: Named,
    pub section: Option<Named>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomField {
    pub gid: String,
    pub name: String,
    pub display_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub gid: String,
    pub name: String,
    pub completed: bool,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub assignee: Option<Named>,
    #[serde(default)]
    pub due_on: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub start_on: Option<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub html_notes: Option<String>,
    pub permalink_url: String,
    #[serde(default)]
    pub modified_at: Option<String>,
    #[serde(default)]
    pub memberships: Vec<Membership>,
    #[serde(default)]
    pub tags: Vec<Named>,
    #[serde(default)]
    pub custom_fields: Vec<CustomField>,
    #[serde(default)]
    pub parent: Option<Named>,
    #[serde(default)]
    pub num_subtasks: u32,
}

/// A story: a comment (`type == "comment"`) or a system event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStory {
    pub gid: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub resource_subtype: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    /// The comment as rich text: names its @-mentions, which `text` shows
    /// as URLs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html_text: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub created_by: Option<Named>,
    /// Emoji reactions, one entry per emoji (skin tone included).
    #[serde(default)]
    pub reaction_summary: Vec<Reaction>,
}

/// One emoji and how many people reacted with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    pub variant: String,
    pub count: u32,
}

impl TaskStory {
    pub fn is_comment(&self) -> bool {
        self.kind == "comment"
    }
}

impl Client {
    pub async fn task(&self, gid: &str) -> Result<Task, ApiError> {
        #[derive(Deserialize)]
        struct Response {
            data: Task,
        }
        let query = [("opt_fields", TASK_FIELDS.to_owned())];
        let response: Response = self.get(&format!("/tasks/{gid}"), &query).await?;
        Ok(response.data)
    }

    /// When the task was last modified (for conflict checks).
    pub async fn modified_at(&self, gid: &str) -> Result<Option<String>, ApiError> {
        #[derive(Deserialize)]
        struct Response {
            data: Modified,
        }
        #[derive(Deserialize)]
        struct Modified {
            #[serde(default)]
            modified_at: Option<String>,
        }
        let query = [("opt_fields", "modified_at".to_owned())];
        let response: Response = self.get(&format!("/tasks/{gid}"), &query).await?;
        Ok(response.data.modified_at)
    }

    /// Replaces the description with plain text (Asana derives `html_notes`).
    /// Replaces the description: `notes`, or `html_notes` for rich text.
    pub async fn set_notes(&self, gid: &str, notes: Text<'_>) -> Result<(), ApiError> {
        let body = serde_json::json!({ "data": notes.field("notes") });
        let _: serde_json::Value = self.put(&format!("/tasks/{gid}"), &body).await?;
        Ok(())
    }

    /// Adds a comment (`text`, or `html_text` for rich text); returns the
    /// created story.
    pub async fn add_comment(&self, task_gid: &str, text: Text<'_>) -> Result<TaskStory, ApiError> {
        #[derive(Deserialize)]
        struct Response {
            data: TaskStory,
        }
        let query = [("opt_fields", STORY_FIELDS.to_owned())];
        let body = serde_json::json!({ "data": text.field("text") });
        let response: Response = self
            .post(&format!("/tasks/{task_gid}/stories"), &query, &body)
            .await?;
        Ok(response.data)
    }

    pub async fn stories_page(
        &self,
        task_gid: &str,
        offset: Option<&str>,
    ) -> Result<Page<TaskStory>, ApiError> {
        let mut query = vec![
            ("limit", "100".to_owned()),
            ("opt_fields", STORY_FIELDS.to_owned()),
        ];
        if let Some(offset) = offset {
            query.push(("offset", offset.to_owned()));
        }
        self.get(&format!("/tasks/{task_gid}/stories"), &query)
            .await
    }
}
