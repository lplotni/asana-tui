//! Project search, sections and adding a task to a project.

use asn::api::Client;
use asn::config::Token;
use asn::core;
use serde_json::{Value, json};
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const WORKSPACE: &str = "1234567890123456";

fn fixture(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap())
        .unwrap()
}

fn client(server: &MockServer) -> Client {
    Client::new(&Token::new("test-token"), server.uri())
}

#[tokio::test]
async fn search_projects_uses_the_typeahead_for_projects() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/workspaces/{WORKSPACE}/typeahead")))
        .and(query_param("resource_type", "project"))
        .and(query_param("query", "web"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("typeahead_projects.json")))
        .mount(&server)
        .await;

    let projects = core::search_projects(&client(&server), WORKSPACE, "web")
        .await
        .unwrap();

    assert_eq!(projects.len(), 5);
    assert_eq!(
        projects[0].name,
        fixture("typeahead_projects.json")["data"][0]["name"]
    );
}

#[tokio::test]
async fn sections_of_a_project() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/projects/42/sections"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("sections.json")))
        .mount(&server)
        .await;

    let sections = core::sections(&client(&server), "42").await.unwrap();

    assert_eq!(sections.len(), 8);
}

#[tokio::test]
async fn sections_follow_every_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/projects/42/sections"))
        .and(query_param("offset", "page2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [
            { "gid": "1200000000000082", "name": "Done" }
        ], "next_page": null })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/projects/42/sections"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [
            { "gid": "1200000000000081", "name": "Done soon" }
        ], "next_page": { "offset": "page2" } })))
        .mount(&server)
        .await;

    let sections = core::sections(&client(&server), "42").await.unwrap();

    let names: Vec<&str> = sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Done soon", "Done"]);
}

#[tokio::test]
async fn adding_to_a_project_uses_add_project_and_never_replaces_projects() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/7/addProject"))
        .and(body_json(
            json!({ "data": { "project": "42", "section": "43" } }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .expect(1)
        .mount(&server)
        .await;

    core::add_to_project(&client(&server), "7", "42", Some("43"))
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|r| r.method.as_str() == "POST" && r.url.path() == "/tasks/7/addProject")
    );
}

#[tokio::test]
async fn adding_without_a_section_sends_only_the_project() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/7/addProject"))
        .and(body_json(json!({ "data": { "project": "42" } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .expect(1)
        .mount(&server)
        .await;

    core::add_to_project(&client(&server), "7", "42", None)
        .await
        .unwrap();
}

#[tokio::test]
async fn moving_to_a_section_adds_the_task_to_that_section() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sections/77/addTask"))
        .and(body_json(json!({ "data": { "task": "31" } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .expect(1)
        .mount(&server)
        .await;

    core::move_to_section(&client(&server), "31", "77")
        .await
        .unwrap();
}

#[tokio::test]
async fn moving_to_a_section_reports_asanas_refusal() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "errors": [{ "message": "task: Not in the section's project" }]
        })))
        .mount(&server)
        .await;

    let err = core::move_to_section(&client(&server), "31", "77")
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("Not in the section's project"),
        "{err}"
    );
}

#[tokio::test]
async fn a_board_loads_open_tasks_from_every_page_grouped_by_section() {
    let server = MockServer::start().await;
    let project = "1200000000000042";
    Mock::given(method("GET"))
        .and(path(format!("/projects/{project}/sections")))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("board_sections.json")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/projects/{project}/tasks")))
        .and(query_param("completed_since", "now"))
        .and(query_param("limit", "100"))
        .and(query_param(
            "offset",
            "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.page2",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("project_tasks_page2.json")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/projects/{project}/tasks")))
        .and(query_param("completed_since", "now"))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("project_tasks_page1.json")))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/projects/{project}")))
        .and(query_param("opt_fields", "name"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "data": { "gid": project, "name": "Maple willow mike" } })),
        )
        .mount(&server)
        .await;

    no_fields(&server, project).await;

    let board = core::board(&client(&server), project).await.unwrap();

    assert_eq!(board.project.name, "Maple willow mike");
    let columns: Vec<(&str, Vec<&str>)> = board
        .columns
        .iter()
        .map(|c| {
            (
                c.section.as_ref().map_or("", |s| s.name.as_str()),
                c.tasks.iter().map(|t| t.gid.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        columns,
        [
            ("Zulu golf river", vec!["1200000000000202"]),
            (
                "Foxtrot victor",
                vec!["1200000000000201", "1200000000000203"]
            ),
            ("Cedar bravo", vec![]),
        ]
    );
    let requests = server.received_requests().await.unwrap();
    let opt_fields = requests
        .iter()
        .find(|r| r.url.path().ends_with("/tasks"))
        .and_then(|r| r.url.query_pairs().find(|(k, _)| k == "opt_fields"))
        .map(|(_, v)| v.into_owned())
        .unwrap();
    for field in [
        "name",
        "assignee.name",
        "due_on",
        "due_at",
        "memberships.section.name",
    ] {
        assert!(opt_fields.split(',').any(|f| f == field), "{opt_fields}");
    }
}

#[tokio::test]
async fn a_new_task_is_created_unassigned_in_the_project() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks"))
        .and(body_json(
            json!({ "data": { "name": "Write the launch post", "projects": ["42"] } }),
        ))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(
                json!({ "data": { "gid": "900", "name": "Write the launch post" } }),
            ),
        )
        .expect(1)
        .mount(&server)
        .await;

    let created = core::create_task(&client(&server), "42", None, "Write the launch post")
        .await
        .unwrap();

    assert_eq!(created.task.gid, "900");
    assert!(created.section_error.is_none());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_new_task_is_then_put_in_the_section() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({ "data": { "gid": "900", "name": "Post" } })),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/sections/77/addTask"))
        .and(body_json(json!({ "data": { "task": "900" } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .expect(1)
        .mount(&server)
        .await;

    let created = core::create_task(&client(&server), "42", Some("77"), "Post")
        .await
        .unwrap();

    assert_eq!(created.task.gid, "900");
    assert!(created.section_error.is_none());
}

#[tokio::test]
async fn a_task_created_but_refused_by_the_section_is_still_returned() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({ "data": { "gid": "900", "name": "Post" } })),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/sections/77/addTask"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "errors": [{ "message": "Forbidden" }]
        })))
        .mount(&server)
        .await;

    let created = core::create_task(&client(&server), "42", Some("77"), "Post")
        .await
        .unwrap();

    assert_eq!(created.task.gid, "900");
    assert!(
        created
            .section_error
            .is_some_and(|e| e.to_string().contains("Forbidden"))
    );
}

// --- Single-select fields on the board (F24, T33) ---

const BOARD: &str = "1200000000000042";
const INITIATIVE: &str = "1200000000000061";
const CHECKOUT: &str = "1200000000000062";
const ONBOARDING: &str = "1200000000000063";
const LEGACY: &str = "1200000000000064";
const PRIORITY: &str = "1200000000000065";
const HIGH: &str = "1200000000000066";

/// A project with no custom fields.
async fn no_fields(server: &MockServer, project: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/projects/{project}/custom_field_settings")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(server)
        .await;
}

/// The board's project, sections and one page of tasks.
async fn board_basics(server: &MockServer, tasks: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/projects/{BOARD}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "data": { "gid": BOARD, "name": "Web Platform" } })),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/projects/{BOARD}/sections")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [
            { "gid": "1200000000000051", "name": "Backlog" }
        ] })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/projects/{BOARD}/tasks")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": tasks })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_board_keeps_the_projects_single_select_fields_with_options_in_order() {
    let server = MockServer::start().await;
    board_basics(&server, json!([])).await;
    Mock::given(method("GET"))
        .and(path(format!("/projects/{BOARD}/custom_field_settings")))
        .and(query_param("offset", "page2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [
            { "gid": "1200000000000071", "custom_field": {
                "gid": PRIORITY, "name": "Priority", "resource_subtype": "enum",
                "enum_options": [{ "gid": HIGH, "name": "High", "enabled": true }]
            } }
        ], "next_page": null })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/projects/{BOARD}/custom_field_settings")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [
            { "gid": "1200000000000072", "custom_field": {
                "gid": INITIATIVE, "name": "Initiative", "resource_subtype": "enum",
                "enum_options": [
                    { "gid": CHECKOUT, "name": "Checkout", "enabled": true },
                    { "gid": ONBOARDING, "name": "Onboarding", "enabled": true },
                    { "gid": LEGACY, "name": "Legacy", "enabled": false }
                ]
            } },
            { "gid": "1200000000000073", "custom_field": {
                "gid": "1200000000000067", "name": "Teams", "resource_subtype": "multi_enum",
                "enum_options": [{ "gid": "1200000000000068", "name": "Web", "enabled": true }]
            } },
            { "gid": "1200000000000074", "custom_field": {
                "gid": "1200000000000069", "name": "Notes", "resource_subtype": "text"
            } }
        ], "next_page": { "offset": "page2" } })))
        .mount(&server)
        .await;

    let board = core::board(&client(&server), BOARD).await.unwrap();

    let fields: Vec<(&str, Vec<(&str, bool)>)> = board
        .fields
        .iter()
        .map(|f| {
            (
                f.name.as_str(),
                f.options
                    .iter()
                    .map(|o| (o.name.as_str(), o.enabled))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        fields,
        [
            (
                "Initiative",
                vec![("Checkout", true), ("Onboarding", true), ("Legacy", false)]
            ),
            ("Priority", vec![("High", true)]),
        ]
    );
}

#[tokio::test]
async fn board_cards_carry_their_single_select_values() {
    let server = MockServer::start().await;
    board_basics(
        &server,
        json!([
            { "gid": "1200000000000201", "name": "Fix redirect", "custom_fields": [
                { "gid": INITIATIVE, "enum_value": { "gid": CHECKOUT } },
                { "gid": PRIORITY, "enum_value": null }
            ] },
            { "gid": "1200000000000202", "name": "Welcome mail" }
        ]),
    )
    .await;
    no_fields(&server, BOARD).await;

    let board = core::board(&client(&server), BOARD).await.unwrap();

    let tasks = &board
        .columns
        .iter()
        .flat_map(|c| &c.tasks)
        .collect::<Vec<_>>();
    assert_eq!(tasks[0].option_of(INITIATIVE), Some(CHECKOUT));
    assert_eq!(tasks[0].option_of(PRIORITY), None);
    assert_eq!(tasks[1].option_of(INITIATIVE), None);
}

#[tokio::test]
async fn a_board_asks_for_field_settings_and_card_values() {
    let server = MockServer::start().await;
    board_basics(&server, json!([])).await;
    no_fields(&server, BOARD).await;

    core::board(&client(&server), BOARD).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let opt_fields = |suffix: &str| -> String {
        requests
            .iter()
            .find(|r| r.url.path().ends_with(suffix))
            .and_then(|r| r.url.query_pairs().find(|(k, _)| k == "opt_fields"))
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    let tasks = opt_fields("/tasks");
    for field in ["custom_fields.gid", "custom_fields.enum_value.gid"] {
        assert!(tasks.split(',').any(|f| f == field), "{tasks}");
    }
    let settings = opt_fields("/custom_field_settings");
    for field in [
        "custom_field.name",
        "custom_field.resource_subtype",
        "custom_field.enum_options.name",
        "custom_field.enum_options.enabled",
    ] {
        assert!(settings.split(',').any(|f| f == field), "{settings}");
    }
}
