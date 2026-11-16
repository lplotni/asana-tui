//! `GET /tasks/{gid}` and `GET /tasks/{gid}/stories`.

use serde::{Deserialize, Serialize};

use super::{ApiError, Client, Page};

/// Task fields shown in the detail pane.
pub const TASK_FIELDS: &str = "name,completed,completed_at,assignee.name,due_on,due_at,start_on,\
notes,html_notes,permalink_url,modified_at,memberships.project.name,memberships.section.name,\
tags.name,custom_fields.name,custom_fields.display_value,parent.name,num_subtasks";

/// Story fields: enough to show comments and count the rest.
pub const STORY_FIELDS: &str = "type,resource_subtype,text,created_at,created_by.name";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub created_at: String,
    #[serde(default)]
    pub created_by: Option<Named>,
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
