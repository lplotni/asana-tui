//! `core::mark_read` against a fake Asana.

use asn::api::Client;
use asn::config::Token;
use asn::core;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const WORKSPACE: &str = "1234567890123456";

fn client(server: &MockServer) -> Client {
    Client::new(&Token::new("test-token"), server.uri())
}

fn gids(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("13000000000{i:05}")).collect()
}

async fn ok_on(server: &MockServer, endpoint: &str) {
    Mock::given(method("POST"))
        .and(path(format!("/users/me/notifications/{endpoint}")))
        .and(header("Authorization", "Bearer test-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "failed_gids": [] } })),
        )
        .mount(server)
        .await;
}

async fn sent_bodies(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

#[tokio::test]
async fn sends_120_gids_as_batches_of_50_50_and_20() {
    let server = MockServer::start().await;
    ok_on(&server, "markRead").await;
    let input = gids(120);

    let failed = core::mark_read(&client(&server), WORKSPACE, &input, true)
        .await
        .failed;

    assert!(failed.is_empty());
    let bodies = sent_bodies(&server).await;
    let sizes: Vec<usize> = bodies
        .iter()
        .map(|b| b["data"]["notification_gids"].as_array().unwrap().len())
        .collect();
    assert_eq!(sizes, [50, 50, 20]);
    let sent: Vec<String> = bodies
        .iter()
        .flat_map(|b| b["data"]["notification_gids"].as_array().unwrap().clone())
        .map(|g| g.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(sent, input);
    assert!(bodies.iter().all(|b| b["data"]["workspace"] == WORKSPACE));
}

#[tokio::test]
async fn collects_failed_gids_from_every_batch() {
    let server = MockServer::start().await;
    let partial: Value = serde_json::from_str(
        &std::fs::read_to_string("tests/fixtures/mark_read_partial.json").unwrap(),
    )
    .unwrap();
    Mock::given(method("POST"))
        .and(path("/users/me/notifications/markRead"))
        .respond_with(ResponseTemplate::new(200).set_body_json(partial))
        .mount(&server)
        .await;

    let failed = core::mark_read(&client(&server), WORKSPACE, &gids(60), true)
        .await
        .failed;

    assert_eq!(failed, ["1", "1"]);
}

#[tokio::test]
async fn unread_uses_mark_unread() {
    let server = MockServer::start().await;
    ok_on(&server, "markUnread").await;

    let failed = core::mark_read(&client(&server), WORKSPACE, &gids(2), false)
        .await
        .failed;

    assert!(failed.is_empty());
    assert_eq!(sent_bodies(&server).await.len(), 1);
}

#[tokio::test]
async fn no_gids_makes_no_request() {
    let server = MockServer::start().await;

    let failed = core::mark_read(&client(&server), WORKSPACE, &[], true)
        .await
        .failed;

    assert!(failed.is_empty());
    assert!(sent_bodies(&server).await.is_empty());
}

#[tokio::test]
async fn an_unknown_workspace_names_asana_workspace() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/users/me/notifications/markRead"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "errors": [{ "message": "workspace: Not a recognized ID: 1" }]
        })))
        .mount(&server)
        .await;

    let err = core::mark_read(&client(&server), WORKSPACE, &gids(1), true)
        .await
        .error
        .unwrap();

    assert!(err.to_string().contains("ASANA_WORKSPACE"), "{err}");
}

#[tokio::test]
async fn a_failing_batch_only_fails_its_own_gids_and_reports_the_error() {
    let server = MockServer::start().await;
    let input = gids(60);
    // First batch (50) succeeds, the second (10) gets a server error.
    Mock::given(method("POST"))
        .and(path("/users/me/notifications/markRead"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "failed_gids": [] } })),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/users/me/notifications/markRead"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(json!({ "errors": [{ "message": "Server error" }] })),
        )
        .mount(&server)
        .await;

    let outcome = core::mark_read(&client(&server), WORKSPACE, &input, true).await;

    assert_eq!(outcome.failed, input[50..].to_vec());
    assert!(outcome.error.unwrap().to_string().contains("Server error"));
}
