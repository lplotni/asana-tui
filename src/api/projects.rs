//! Project search, sections, and adding a task to a project.

use serde::Deserialize;

use super::tasks::Named;
use super::{ApiError, Client};

/// How many projects the typeahead returns.
const TYPEAHEAD_COUNT: u32 = 20;

#[derive(Deserialize)]
struct List {
    data: Vec<Named>,
}

impl Client {
    pub async fn typeahead_projects(
        &self,
        workspace: &str,
        query: &str,
    ) -> Result<Vec<Named>, ApiError> {
        let query = [
            ("resource_type", "project".to_owned()),
            ("query", query.to_owned()),
            ("count", TYPEAHEAD_COUNT.to_string()),
            ("opt_fields", "name".to_owned()),
        ];
        let list: List = self
            .get(&format!("/workspaces/{workspace}/typeahead"), &query)
            .await?;
        Ok(list.data)
    }

    pub async fn sections(&self, project_gid: &str) -> Result<Vec<Named>, ApiError> {
        let query = [
            ("opt_fields", "name".to_owned()),
            ("limit", "100".to_owned()),
        ];
        let list: List = self
            .get(&format!("/projects/{project_gid}/sections"), &query)
            .await?;
        Ok(list.data)
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
}
