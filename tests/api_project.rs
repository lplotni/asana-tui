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
