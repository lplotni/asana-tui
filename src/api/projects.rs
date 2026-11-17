//! Project search, sections, a project's open tasks, adding a task to a
//! project and moving it between sections.

use serde::{Deserialize, Serialize};

use super::tasks::{Membership, Named};
use super::{ApiError, Client, Page};

/// How many projects the typeahead returns.
const TYPEAHEAD_COUNT: u32 = 20;

/// Fields of a board card: what it shows, and its memberships to find its
/// column.
const BOARD_TASK_FIELDS: &str =
    "name,assignee.name,due_on,due_at,memberships.project.name,memberships.section.name";

/// An open task of a project, as a card on its board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardTask {
    pub gid: String,
    pub name: String,
    #[serde(default)]
    pub assignee: Option<Named>,
    #[serde(default)]
    pub due_on: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub memberships: Vec<Membership>,
}

impl Client {
    pub async fn typeahead_projects(
        &self,
        workspace: &str,
        query: &str,
    ) -> Result<Vec<Named>, ApiError> {
        self.typeahead(workspace, "project", query, TYPEAHEAD_COUNT, "name")
            .await
    }

    pub async fn sections(&self, project_gid: &str) -> Result<Vec<Named>, ApiError> {
        let query = [
            ("opt_fields", "name".to_owned()),
            ("limit", "100".to_owned()),
        ];
        let page: Page<Named> = self
            .get(&format!("/projects/{project_gid}/sections"), &query)
            .await?;
        Ok(page.data)
    }

    /// One page of the project's incomplete tasks, in the project's order.
    pub async fn project_tasks_page(
        &self,
        project_gid: &str,
        offset: Option<&str>,
    ) -> Result<Page<BoardTask>, ApiError> {
        let mut query = vec![
            ("completed_since", "now".to_owned()),
            ("limit", "100".to_owned()),
            ("opt_fields", BOARD_TASK_FIELDS.to_owned()),
        ];
        if let Some(offset) = offset {
            query.push(("offset", offset.to_owned()));
        }
        self.get(&format!("/projects/{project_gid}/tasks"), &query)
            .await
    }

    pub async fn add_project(
        &self,
        task_gid: &str,
        project_gid: &str,
        section_gid: Option<&str>,
    ) -> Result<(), ApiError> {
        let mut data = serde_json::json!({ "project": project_gid });
        if let Some(section) = section_gid {
            data["section"] = section.into();
        }
        let _: serde_json::Value = self
            .post(
                &format!("/tasks/{task_gid}/addProject"),
                &[],
                &serde_json::json!({ "data": data }),
            )
            .await?;
        Ok(())
    }

    /// Moves the task into `section_gid` (out of its current section of that
    /// project). The task must already be in the section's project.
    pub async fn add_task_to_section(
        &self,
        section_gid: &str,
        task_gid: &str,
    ) -> Result<(), ApiError> {
        let _: serde_json::Value = self
            .post(
                &format!("/sections/{section_gid}/addTask"),
                &[],
                &serde_json::json!({ "data": { "task": task_gid } }),
            )
            .await?;
        Ok(())
    }
}
