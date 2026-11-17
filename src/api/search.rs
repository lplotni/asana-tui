//! Finding tasks in a workspace: full-text task search or typeahead.

use serde::{Deserialize, Serialize};

use super::{ApiError, Client};

/// Fields of each task found.
pub const SEARCH_FIELDS: &str = "name,completed";

/// How many tasks one search returns (the API allows up to 100).
const SEARCH_LIMIT: u32 = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundTask {
    pub gid: String,
    pub name: String,
    #[serde(default)]
    pub completed: bool,
}

#[derive(Deserialize)]
struct List {
    data: Vec<FoundTask>,
}

impl Client {
    /// `GET /workspaces/{ws}/tasks/search`: name and description, most
    /// relevant first. Paid plans only (402 otherwise).
    pub async fn search_tasks(
        &self,
        workspace: &str,
        text: &str,
    ) -> Result<Vec<FoundTask>, ApiError> {
        let query = [
            ("text", text.to_owned()),
            ("sort_by", "relevance".to_owned()),
            ("limit", SEARCH_LIMIT.to_string()),
            ("opt_fields", SEARCH_FIELDS.to_owned()),
        ];
        let list: List = self
            .get(&format!("/workspaces/{workspace}/tasks/search"), &query)
            .await?;
        Ok(list.data)
    }

    /// `GET /workspaces/{ws}/typeahead` for tasks: names, ranked like the web
    /// search box. Works on every plan.
    pub async fn typeahead_tasks(
        &self,
        workspace: &str,
        text: &str,
    ) -> Result<Vec<FoundTask>, ApiError> {
        let query = [
            ("resource_type", "task".to_owned()),
            ("query", text.to_owned()),
            ("count", SEARCH_LIMIT.to_string()),
            ("opt_fields", SEARCH_FIELDS.to_owned()),
        ];
        let list: List = self
            .get(&format!("/workspaces/{workspace}/typeahead"), &query)
            .await?;
        Ok(list.data)
    }
}
