//! Project search, sections, a project's open tasks, adding a task to a
//! project and moving it between sections.

use serde::{Deserialize, Serialize};

use super::tasks::{CustomField, Membership, Named};
use super::{ApiError, Client, Page};

/// How many projects the typeahead returns.
const TYPEAHEAD_COUNT: u32 = 20;

/// Fields of a board card: what it shows, and its memberships to find its
/// column.
const BOARD_TASK_FIELDS: &str = "name,assignee.name,due_on,due_at,memberships.project.name,\
memberships.section.name,custom_fields.gid,custom_fields.enum_value.gid";

/// Fields of a project's custom field settings: enough to keep the
/// single-select fields and their options (F24).
const FIELD_SETTING_FIELDS: &str = "custom_field.name,custom_field.resource_subtype,\
custom_field.enum_options.name,custom_field.enum_options.enabled";

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
    /// The card's custom field values (field GID and, for a single-select
    /// field, the chosen option's GID).
    #[serde(default)]
    pub custom_fields: Vec<FieldValue>,
}

/// One custom field value of a card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldValue {
    pub gid: String,
    #[serde(default)]
    pub enum_value: Option<OptionRef>,
}

/// A single-select option, by GID only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionRef {
    pub gid: String,
}

impl BoardTask {
    /// The GID of the option chosen for the single-select field `field_gid`.
    pub fn option_of(&self, field_gid: &str) -> Option<&str> {
        self.custom_fields
            .iter()
            .find(|f| f.gid == field_gid)
            .and_then(|f| f.enum_value.as_ref())
            .map(|o| o.gid.as_str())
    }
}

/// A project's single-select custom field, with its options in Asana's order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectField {
    pub gid: String,
    pub name: String,
    pub options: Vec<SelectOption>,
}

impl SelectField {
    pub fn named(&self) -> Named {
        Named {
            gid: self.gid.clone(),
            name: self.name.clone(),
        }
    }
}

/// An option of a single-select field. A disabled option can still be set on
/// old tasks, but is never offered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectOption {
    pub gid: String,
    pub name: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

impl SelectOption {
    pub fn named(&self) -> Named {
        Named {
            gid: self.gid.clone(),
            name: self.name.clone(),
        }
    }
}

fn enabled() -> bool {
    true
}

/// One custom field setting of a project, as Asana returns it: the field's
/// definition, without a value. Only single-select fields are used.
#[derive(Debug, Clone, Deserialize)]
pub struct FieldSetting {
    pub custom_field: CustomField,
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

    /// The project's GID and name.
    pub async fn project(&self, project_gid: &str) -> Result<Named, ApiError> {
        #[derive(Deserialize)]
        struct Data {
            data: Named,
        }
        let query = [("opt_fields", "name".to_owned())];
        let body: Data = self
            .get(&format!("/projects/{project_gid}"), &query)
            .await?;
        Ok(body.data)
    }

    /// One page of the project's sections, in board order.
    pub async fn sections_page(
        &self,
        project_gid: &str,
        offset: Option<&str>,
    ) -> Result<Page<Named>, ApiError> {
        let mut query = vec![
            ("opt_fields", "name".to_owned()),
            ("limit", "100".to_owned()),
        ];
        if let Some(offset) = offset {
            query.push(("offset", offset.to_owned()));
        }
        self.get(&format!("/projects/{project_gid}/sections"), &query)
            .await
    }

    /// One page of the project's custom field settings.
    pub async fn field_settings_page(
        &self,
        project_gid: &str,
        offset: Option<&str>,
    ) -> Result<Page<FieldSetting>, ApiError> {
        let mut query = vec![
            ("limit", "100".to_owned()),
            ("opt_fields", FIELD_SETTING_FIELDS.to_owned()),
        ];
        if let Some(offset) = offset {
            query.push(("offset", offset.to_owned()));
        }
        self.get(
            &format!("/projects/{project_gid}/custom_field_settings"),
            &query,
        )
        .await
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

    /// Creates an unassigned task named `name` in the project.
    pub async fn create_task(&self, project_gid: &str, name: &str) -> Result<Named, ApiError> {
        #[derive(Deserialize)]
        struct Data {
            data: Named,
        }
        let body = serde_json::json!({ "data": { "name": name, "projects": [project_gid] } });
        let created: Data = self.post("/tasks", &[], &body).await?;
        Ok(created.data)
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
