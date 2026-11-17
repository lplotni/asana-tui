//! `core::search_tasks` against a fake Asana, for both `ASN_SEARCH` backends.

use asn::api::Client;
use asn::api::search::SEARCH_FIELDS;
use asn::config::{SearchMode, Token};
use asn::core;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const WS: &str = "1200000000000001";

fn client(server: &MockServer) -> Client {
    Client::new(&Token::new("test-token"), server.uri())
}

fn found() -> serde_json::Value {
    json!({ "data": [
        { "gid": "11", "name": "Checkout redirect loop", "completed": false },
        { "gid": "12", "name": "Checkout copy", "completed": true }
    ] })
}

#[tokio::test]
async fn task_search_asks_for_the_most_relevant_tasks_matching_the_text() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/workspaces/{WS}/tasks/search")))
        .and(query_param("text", "checkout"))
        .and(query_param("sort_by", "relevance"))
        .and(query_param("limit", "50"))
        .and(query_param("opt_fields", SEARCH_FIELDS))
        .respond_with(ResponseTemplate::new(200).set_body_json(found()))
        .expect(1)
        .mount(&server)
        .await;

    let tasks = core::search_tasks(&client(&server), WS, "checkout", SearchMode::Tasks)
        .await
        .unwrap();

    let names: Vec<&str> = tasks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Checkout redirect loop", "Checkout copy"]);
    assert_eq!(tasks[0].gid, "11");
    assert!(tasks[1].completed);
}

#[tokio::test]
async fn typeahead_asks_for_tasks_by_name() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/workspaces/{WS}/typeahead")))
        .and(query_param("resource_type", "task"))
        .and(query_param("query", "checkout"))
        .and(query_param("count", "50"))
        .and(query_param("opt_fields", SEARCH_FIELDS))
        .respond_with(ResponseTemplate::new(200).set_body_json(found()))
        .expect(1)
        .mount(&server)
        .await;

    let tasks = core::search_tasks(&client(&server), WS, "checkout", SearchMode::Typeahead)
        .await
        .unwrap();

    assert_eq!(tasks.len(), 2);
}

#[tokio::test]
async fn task_search_on_a_plan_without_it_suggests_typeahead() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(402).set_body_json(json!({
            "errors": [{ "message": "Search is only available to premium users." }]
        })))
        .mount(&server)
        .await;

    let err = core::search_tasks(&client(&server), WS, "checkout", SearchMode::Tasks)
        .await
        .unwrap_err();

    let message = err.to_string();
    assert!(message.contains("premium"), "{message}");
    assert!(message.contains("ASN_SEARCH=typeahead"), "{message}");
}

#[tokio::test]
async fn a_blank_query_finds_nothing_without_asking_asana() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(found()))
        .expect(0)
        .mount(&server)
        .await;

    let tasks = core::search_tasks(&client(&server), WS, "   ", SearchMode::Tasks)
        .await
        .unwrap();

    assert!(tasks.is_empty());
}

#[tokio::test]
async fn people_are_found_with_the_user_typeahead() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/workspaces/1234567890123456/typeahead",
        ))
        .and(wiremock::matchers::query_param("resource_type", "user"))
        .and(wiremock::matchers::query_param("query", "ada"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "gid": "1200000000000011", "name": "Ada Lovelace" }]
            })),
        )
        .mount(&server)
        .await;
    let client = asn::api::Client::new(&asn::config::Token::new("t"), server.uri());

    let people = asn::core::search_users(&client, "1234567890123456", "ada")
        .await
        .unwrap();

    assert_eq!(people.len(), 1);
    assert_eq!(people[0].name, "Ada Lovelace");
}

#[tokio::test]
async fn mention_targets_are_people_then_projects_then_tasks() {
    let server = MockServer::start().await;
    for (kind, gid, name) in [
        ("user", "11", "Ada Lovelace"),
        ("project", "42", "Ada's roadmap"),
        ("task", "31", "Thank Ada"),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/workspaces/{WS}/typeahead")))
            .and(query_param("resource_type", kind))
            .and(query_param("query", "ada"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [{ "gid": gid, "name": name }]
            })))
            .mount(&server)
            .await;
    }

    let targets = core::search_mentions(&client(&server), WS, "ada")
        .await
        .unwrap();

    let found: Vec<(&str, &str, &str)> = targets
        .iter()
        .map(|t| (t.kind, t.gid.as_str(), t.name.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            ("person", "11", "Ada Lovelace"),
            ("project", "42", "Ada's roadmap"),
            ("task", "31", "Thank Ada"),
        ]
    );
}
