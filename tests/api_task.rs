//! `core::task_detail` against a fake Asana serving the task fixtures.

use asn::api::Client;
use asn::api::tasks::{STORY_FIELDS, TASK_FIELDS};
use asn::config::Token;
use asn::core;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TASK: &str = "1200000000000031";

fn fixture(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap())
        .unwrap()
}

fn client(server: &MockServer) -> Client {
    Client::new(&Token::new("test-token"), server.uri())
}

async fn serve_task(server: &MockServer, body: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/tasks/{TASK}")))
        .and(query_param("opt_fields", TASK_FIELDS))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

async fn serve_stories(server: &MockServer, offset: Option<&str>, body: Value) {
    let mock = Mock::given(method("GET"))
        .and(path(format!("/tasks/{TASK}/stories")))
        .and(query_param("opt_fields", STORY_FIELDS));
    let mock = match offset {
        Some(offset) => mock.and(query_param("offset", offset)),
        None => mock.and(query_param_is_missing("offset")),
    };
    mock.respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn returns_the_task_with_its_fields() {
    let server = MockServer::start().await;
    serve_task(&server, fixture("task_full.json")).await;
    serve_stories(&server, None, fixture("stories_thread.json")).await;

    let detail = core::task_detail(&client(&server), TASK).await.unwrap();

    let expected = fixture("task_full.json");
    assert_eq!(detail.task.name, expected["data"]["name"]);
    assert_eq!(
        detail.task.due_on.as_deref(),
        expected["data"]["due_on"].as_str()
    );
    assert_eq!(detail.task.memberships.len(), 2);
    assert_eq!(detail.task.custom_fields[0].display_value, None);
}

#[tokio::test]
async fn keeps_only_comments_in_order_and_counts_the_activity() {
    let server = MockServer::start().await;
    serve_task(&server, fixture("task_full.json")).await;
    let stories = fixture("stories_thread.json");
    serve_stories(&server, None, stories.clone()).await;

    let detail = core::task_detail(&client(&server), TASK).await.unwrap();

    let all = stories["data"].as_array().unwrap();
    let comments: Vec<&str> = all
        .iter()
        .filter(|s| s["type"] == "comment")
        .map(|s| s["gid"].as_str().unwrap())
        .collect();
    let got: Vec<&str> = detail.comments.iter().map(|c| c.gid.as_str()).collect();
    assert_eq!(got, comments);
    assert_eq!(detail.comments.len(), 7);
    assert_eq!(detail.activity_count, all.len() - 7);
}

#[tokio::test]
async fn follows_story_pages_to_get_every_comment() {
    let server = MockServer::start().await;
    serve_task(&server, fixture("task_full.json")).await;
    let mut first = fixture("stories_thread.json");
    first["next_page"] = json!({ "offset": "s2", "path": "/x", "uri": "https://x" });
    serve_stories(&server, None, first).await;
    serve_stories(&server, Some("s2"), fixture("stories_thread.json")).await;

    let detail = core::task_detail(&client(&server), TASK).await.unwrap();

    assert_eq!(detail.comments.len(), 14);
}

#[tokio::test]
async fn completed_task_parses() {
    let server = MockServer::start().await;
    serve_task(&server, fixture("task_full_completed.json")).await;
    serve_stories(&server, None, json!({ "data": [], "next_page": null })).await;

    let detail = core::task_detail(&client(&server), TASK).await.unwrap();

    assert!(detail.task.completed);
    assert!(detail.comments.is_empty());
}
