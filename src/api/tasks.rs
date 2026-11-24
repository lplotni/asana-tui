//! `GET /tasks/{gid}` and `GET /tasks/{gid}/stories`.

use serde::{Deserialize, Serialize};

use super::projects::{SelectField, SelectOption};
use super::{ApiError, Client, Page};

/// Task fields shown in the detail pane.
pub const TASK_FIELDS: &str = "name,completed,completed_at,assignee.name,due_on,due_at,start_on,\
notes,html_notes,permalink_url,modified_at,memberships.project.name,memberships.section.name,\
tags.name,custom_fields.name,custom_fields.display_value,custom_fields.resource_subtype,\
custom_fields.enum_value.name,custom_fields.enum_options.name,custom_fields.enum_options.enabled,\
parent.name,num_subtasks";

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

impl Membership {
    /// `Project › Section`, or the project alone when it has no section.
    pub fn place(&self) -> String {
        match &self.section {
            Some(section) => format!("{} › {}", self.project.name, section.name),
            None => self.project.name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomField {
    pub gid: String,
    pub name: String,
    pub display_value: Option<String>,
    /// [`SINGLE_SELECT`] for a single-select field (F27); other types
    /// aren't edited.
    #[serde(default)]
    pub resource_subtype: String,
    /// The chosen option of a single-select field.
    #[serde(default)]
    pub enum_value: Option<Named>,
    /// A single-select field's options, in Asana's order.
    #[serde(default)]
    pub enum_options: Vec<SelectOption>,
}

/// Asana's `resource_subtype` of a single-select field.
const SINGLE_SELECT: &str = "enum";

impl CustomField {
    /// Single-select `field` as a task has it, set to the option whose GID
    /// is `value`.
    pub fn single_select(field: &SelectField, value: Option<&str>) -> CustomField {
        CustomField {
            gid: field.gid.clone(),
            name: field.name.clone(),
            display_value: None,
            resource_subtype: SINGLE_SELECT.to_owned(),
            enum_value: field
                .options
                .iter()
                .find(|o| value == Some(o.gid.as_str()))
                .map(SelectOption::named),
            enum_options: field.options.clone(),
        }
    }

    /// Whether this is a single-select field, the only kind asn sets.
    pub fn is_single_select(&self) -> bool {
        self.resource_subtype == SINGLE_SELECT
    }

    /// This field as a [`SelectField`], if it is a single-select field.
    pub fn into_select(self) -> Option<SelectField> {
        self.is_single_select().then_some(SelectField {
            gid: self.gid,
            name: self.name,
            options: self.enum_options,
        })
    }

    pub fn named(&self) -> Named {
        Named {
            gid: self.gid.clone(),
            name: self.name.clone(),
        }
    }
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

    /// The task's description as plain text (for conflict checks).
    pub async fn notes(&self, gid: &str) -> Result<String, ApiError> {
        #[derive(Deserialize)]
        struct Response {
            data: Notes,
        }
        #[derive(Deserialize)]
        struct Notes {
            #[serde(default)]
            notes: String,
        }
        let query = [("opt_fields", "notes".to_owned())];
        let response: Response = self.get(&format!("/tasks/{gid}"), &query).await?;
        Ok(response.data.notes)
    }

    /// Replaces the description with plain text (Asana derives `html_notes`).
    /// Replaces the description: `notes`, or `html_notes` for rich text.
    pub async fn set_notes(&self, gid: &str, notes: Text<'_>) -> Result<(), ApiError> {
        let body = serde_json::json!({ "data": notes.field("notes") });
        let _: serde_json::Value = self.put(&format!("/tasks/{gid}"), &[], &body).await?;
        Ok(())
    }

    /// Marks the task complete, or incomplete again.
    pub async fn set_completed(&self, gid: &str, completed: bool) -> Result<(), ApiError> {
        let body = serde_json::json!({ "data": { "completed": completed } });
        let _: serde_json::Value = self.put(&format!("/tasks/{gid}"), &[], &body).await?;
        Ok(())
    }

    /// Assigns the task to `assignee` (a user GID or `"me"`), or to nobody;
    /// returns whom it's assigned to now.
    pub async fn set_assignee(
        &self,
        gid: &str,
        assignee: Option<&str>,
    ) -> Result<Option<Named>, ApiError> {
        #[derive(Deserialize)]
        struct Assigned {
            #[serde(default)]
            assignee: Option<Named>,
        }
        #[derive(Deserialize)]
        struct Data {
            data: Assigned,
        }
        let query = [("opt_fields", "assignee.name".to_owned())];
        let body = serde_json::json!({ "data": { "assignee": assignee } });
        let updated: Data = self.put(&format!("/tasks/{gid}"), &query, &body).await?;
        Ok(updated.data.assignee)
    }

    /// Sets the single-select field `field_gid` to `option_gid`, or clears it.
    pub async fn set_select_field(
        &self,
        gid: &str,
        field_gid: &str,
        option_gid: Option<&str>,
    ) -> Result<(), ApiError> {
        let mut fields = serde_json::Map::new();
        fields.insert(field_gid.to_owned(), option_gid.into());
        let body = serde_json::json!({ "data": { "custom_fields": fields } });
        let query = [("opt_fields", "gid".to_owned())];
        let _: serde_json::Value = self.put(&format!("/tasks/{gid}"), &query, &body).await?;
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
