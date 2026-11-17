//! Project search, sections, adding a task to a project and moving it
//! between sections.

use super::tasks::Named;
use super::{ApiError, Client, Page};

/// How many projects the typeahead returns.
const TYPEAHEAD_COUNT: u32 = 20;

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
