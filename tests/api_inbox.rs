//! `core::inbox` against a fake Asana serving the T0 fixtures.

use std::time::{Duration, Instant};

use asn::api::notifications::OPT_FIELDS;
use asn::api::{ApiError, Client};
use asn::cli::inbox::render_json;
use asn::config::Token;
use asn::core;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const WORKSPACE: &str = "1234567890123456";
const PATH: &str = "/users/me/notifications";

fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn client(server: &MockServer) -> Client {
    Client::new(&Token::new("test-token"), server.uri())
}

/// Serves `body` for the request carrying `offset` (or no offset when `None`).
async fn serve_page(server: &MockServer, offset: Option<&str>, body: Value) {
    let mock = Mock::given(method("GET")).and(path(PATH));
    let mock = match offset {
        Some(offset) => mock.and(query_param("offset", offset)),
        None => mock.and(query_param_is_missing("offset")),
    };
    mock.respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

fn page_with_next(mut page: Value, next_offset: &str) -> Value {
    page["next_page"] =
        json!({ "offset": next_offset, "path": "/ignored", "uri": "https://ignored" });
    page
}

#[tokio::test]
async fn json_output_equals_asanas_data_exactly() {
    let server = MockServer::start().await;
    let page = fixture("notifications_page.json");
    let next = page["next_page"]["offset"].as_str().unwrap().to_owned();
    Mock::given(method("GET"))
        .and(path(PATH))
        .and(header("Authorization", "Bearer test-token"))
        .and(query_param("workspace", WORKSPACE))
        .and(query_param("opt_fields", OPT_FIELDS))
        .and(query_param("limit", "100"))
        .and(query_param_is_missing("offset"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&page))
        .mount(&server)
        .await;
    serve_page(
        &server,
        Some(&next),
        fixture("notifications_last_page_empty.json"),
    )
    .await;

    let notifications = core::inbox(&client(&server), WORKSPACE, 50, false)
        .await
        .unwrap();
    let output: Value = serde_json::from_str(&render_json(&notifications)).unwrap();

    assert_eq!(output, page["data"]);
}

#[tokio::test]
async fn follows_next_page_past_empty_pages_until_it_is_null() {
    let server = MockServer::start().await;
    serve_page(
        &server,
        None,
        page_with_next(fixture("notifications_page.json"), "p2"),
    )
    .await;
    serve_page(
        &server,
        Some("p2"),
        page_with_next(json!({ "data": [] }), "p3"),
    )
    .await;
    serve_page(
        &server,
        Some("p3"),
        fixture("notifications_edge_cases.json"),
    )
    .await;

    let notifications = core::inbox(&client(&server), WORKSPACE, 50, false)
        .await
        .unwrap();

    assert_eq!(notifications.len(), 10 + 4);
}

#[tokio::test]
async fn parses_null_actor_null_story_unknown_values_and_non_task_objects() {
    let server = MockServer::start().await;
    serve_page(&server, None, fixture("notifications_edge_cases.json")).await;

    let n = core::inbox(&client(&server), WORKSPACE, 50, false)
        .await
        .unwrap();

    assert_eq!(n[0].created_by, None);
    assert_eq!(n[0].story, None);
    assert_eq!(n[1].story_resource_subtype, "some_future_subtype");
    assert_eq!(n[1].priority_classification, "some_future_priority");
    assert_eq!(n[1].work_objects[0].resource_type, "project");
    assert!(n[3].work_objects.is_empty());
}

#[tokio::test]
async fn unread_only_returns_only_unread_up_to_limit_without_fetching_more_pages() {
    let server = MockServer::start().await;
    serve_page(
        &server,
        None,
        page_with_next(fixture("notifications_page.json"), "p2"),
    )
    .await;
    Mock::given(method("GET"))
        .and(path(PATH))
        .and(query_param("offset", "p2"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("notifications_edge_cases.json")),
        )
        .expect(0)
        .mount(&server)
        .await;

    let n = core::inbox(&client(&server), WORKSPACE, 3, true)
        .await
        .unwrap();

    assert_eq!(n.len(), 3);
    assert!(n.iter().all(|n| !n.is_read));
}

#[tokio::test]
async fn rate_limit_waits_for_retry_after_then_retries_once() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    serve_page(&server, None, fixture("notifications_edge_cases.json")).await;

    let started = Instant::now();
    let n = core::inbox(&client(&server), WORKSPACE, 50, false)
        .await
        .unwrap();

    assert_eq!(n.len(), 4);
    assert!(started.elapsed() >= Duration::from_secs(1));
}

#[tokio::test]
async fn second_rate_limit_in_a_row_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PATH))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
        .mount(&server)
        .await;

    let err = core::inbox(&client(&server), WORKSPACE, 50, false)
        .await
        .unwrap_err();

    assert!(matches!(err, ApiError::RateLimited), "{err:?}");
}

#[tokio::test]
async fn unauthorized_is_an_auth_error_with_exit_code_2() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PATH))
        .respond_with(ResponseTemplate::new(401).set_body_json(fixture("error_401.json")))
        .mount(&server)
        .await;

    let err = core::inbox(&client(&server), WORKSPACE, 50, false)
        .await
        .unwrap_err();

    assert!(matches!(err, ApiError::Unauthorized), "{err:?}");
    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains("ASANA_TOKEN"));
}

#[tokio::test]
async fn unknown_workspace_names_asana_workspace_and_asanas_message() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PATH))
        .respond_with(ResponseTemplate::new(404).set_body_json(fixture("error_404_workspace.json")))
        .mount(&server)
        .await;

    let err = core::inbox(&client(&server), WORKSPACE, 50, false)
        .await
        .unwrap_err();

    assert_eq!(err.exit_code(), 1);
    let message = err.to_string();
    assert!(message.contains("ASANA_WORKSPACE"), "{message}");
    assert!(message.contains("Not a recognized ID"), "{message}");
}
