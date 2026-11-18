//! Names where GIDs are taken (F19) against a fake Asana: a name that
//! doesn't resolve to exactly one thing never leads to a write.

use asn::api::Client;
use asn::api::tasks::Named;
use asn::config::Token;
use asn::core;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const WS: &str = "1200000000000001";
const TASK: &str = "1200000000000031";

fn client(server: &MockServer) -> Client {
    Client::new(&Token::new("test-token"), server.uri())
}

async fn people(server: &MockServer, query: &str, found: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(format!("/workspaces/{WS}/typeahead")))
        .and(query_param("resource_type", "user"))
        .and(query_param("query", query))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": found })))
        .mount(server)
        .await;
}

/// The assignment `PUT`, expected `times` times.
async fn assignment(server: &MockServer, assignee: serde_json::Value, times: u64) {
    Mock::given(method("PUT"))
        .and(path(format!("/tasks/{TASK}")))
        .and(body_json(json!({ "data": { "assignee": assignee } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {
            "gid": TASK,
            "assignee": { "gid": "1200000000000011", "name": "Ada Lovelace" }
        } })))
        .expect(times)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_person_is_assigned_by_their_exact_name() {
    let server = MockServer::start().await;
    people(
        &server,
        "Ada Lovelace",
        json!([
            { "gid": "1200000000000011", "name": "Ada Lovelace" },
            { "gid": "1200000000000012", "name": "Ada Lovelace-King" }
        ]),
    )
    .await;
    assignment(&server, json!("1200000000000011"), 1).await;

    let now = core::assign_to(&client(&server), WS, TASK, "Ada Lovelace")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(now.unwrap().name, "Ada Lovelace");
}

#[tokio::test]
async fn an_ambiguous_name_assigns_nobody_and_lists_the_candidates() {
    let server = MockServer::start().await;
    people(
        &server,
        "ada",
        json!([
            { "gid": "1200000000000011", "name": "Ada Lovelace" },
            { "gid": "1200000000000012", "name": "Ada Byron" }
        ]),
    )
    .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let unresolved = core::assign_to(&client(&server), WS, TASK, "ada")
        .await
        .unwrap()
        .unwrap_err();

    assert_eq!(unresolved.kind, "person");
    assert_eq!(
        unresolved.candidates,
        vec![
            Named {
                gid: "1200000000000011".into(),
                name: "Ada Lovelace".into()
            },
            Named {
                gid: "1200000000000012".into(),
                name: "Ada Byron".into()
            },
        ]
    );
}

#[tokio::test]
async fn an_unknown_name_assigns_nobody() {
    let server = MockServer::start().await;
    people(&server, "Grace", json!([])).await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let unresolved = core::assign_to(&client(&server), WS, TASK, "Grace")
        .await
        .unwrap()
        .unwrap_err();

    assert!(unresolved.candidates.is_empty());
}

#[tokio::test]
async fn a_gid_me_and_none_are_used_as_given_without_a_lookup() {
    for (who, sent) in [
        ("1200000000000011", json!("1200000000000011")),
        ("me", json!("me")),
        ("none", serde_json::Value::Null),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        assignment(&server, sent, 1).await;

        let result = core::assign_to(&client(&server), WS, TASK, who).await;

        assert!(matches!(result, Ok(Ok(_))), "{who}: {result:?}");
    }
}
