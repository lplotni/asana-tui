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

// --- Sections by name in `asn project move` (T27) ---

const WEB: &str = "1200000000000043";
const OPS: &str = "1200000000000045";
const WEB_REVIEW: &str = "1200000000000044";
const WEB_DONE: &str = "1200000000000046";
const OPS_REVIEW: &str = "1200000000000047";

/// The task, in the given (project gid, project name, section gid, section
/// name) memberships. `times` limits how often this answer is given, so a
/// later mount can answer the read after the move.
async fn task_in(server: &MockServer, memberships: &[(&str, &str, &str, &str)], times: u64) {
    let memberships: Vec<_> = memberships
        .iter()
        .map(|(pg, pn, sg, sn)| {
            json!({ "project": { "gid": pg, "name": pn }, "section": { "gid": sg, "name": sn } })
        })
        .collect();
    Mock::given(method("GET"))
        .and(path(format!("/tasks/{TASK}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {
            "gid": TASK,
            "name": "Fix checkout redirect loop",
            "completed": false,
            "permalink_url": "https://app.asana.com/0/0/1200000000000031",
            "memberships": memberships,
        } })))
        .up_to_n_times(times)
        .mount(server)
        .await;
}

async fn sections(server: &MockServer, project: &str, found: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(format!("/projects/{project}/sections")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": found })))
        .mount(server)
        .await;
}

async fn add_to_section(server: &MockServer, section: &str, times: u64) {
    Mock::given(method("POST"))
        .and(path(format!("/sections/{section}/addTask")))
        .and(body_json(json!({ "data": { "task": TASK } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .expect(times)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_task_moves_to_a_section_named_in_any_case_and_reports_where() {
    let server = MockServer::start().await;
    task_in(&server, &[(WEB, "Web", WEB_DONE, "Done")], 1).await;
    task_in(&server, &[(WEB, "Web", WEB_REVIEW, "In Review")], 1).await;
    sections(
        &server,
        WEB,
        json!([
            { "gid": WEB_REVIEW, "name": "In Review" },
            { "gid": WEB_DONE, "name": "Done" }
        ]),
    )
    .await;
    add_to_section(&server, WEB_REVIEW, 1).await;

    let moved = core::move_to(&client(&server), TASK, "in review")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(moved.as_deref(), Some("Web › In Review"));
}

#[tokio::test]
async fn part_of_a_section_name_is_enough_when_only_one_section_has_it() {
    let server = MockServer::start().await;
    task_in(&server, &[(WEB, "Web", WEB_DONE, "Done")], 2).await;
    sections(
        &server,
        WEB,
        json!([
            { "gid": WEB_REVIEW, "name": "In Review" },
            { "gid": WEB_DONE, "name": "Done" }
        ]),
    )
    .await;
    add_to_section(&server, WEB_REVIEW, 1).await;

    let moved = core::move_to(&client(&server), TASK, "review")
        .await
        .unwrap();

    assert!(moved.is_ok(), "{moved:?}");
}

#[tokio::test]
async fn the_same_section_name_in_two_projects_moves_nothing_and_lists_both() {
    let server = MockServer::start().await;
    task_in(
        &server,
        &[
            (WEB, "Web", WEB_DONE, "Done"),
            (OPS, "Ops", OPS_REVIEW, "In Review"),
        ],
        1,
    )
    .await;
    sections(
        &server,
        WEB,
        json!([
            { "gid": WEB_REVIEW, "name": "In Review" },
            { "gid": WEB_DONE, "name": "Done" }
        ]),
    )
    .await;
    sections(
        &server,
        OPS,
        json!([{ "gid": OPS_REVIEW, "name": "In Review" }]),
    )
    .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let unresolved = core::move_to(&client(&server), TASK, "In Review")
        .await
        .unwrap()
        .unwrap_err();

    assert_eq!(unresolved.kind, "section");
    let labels: Vec<_> = unresolved
        .candidates
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(labels, ["Web › In Review", "Ops › In Review"]);
}

#[tokio::test]
async fn an_unknown_section_moves_nothing() {
    let server = MockServer::start().await;
    task_in(&server, &[(WEB, "Web", WEB_DONE, "Done")], 1).await;
    sections(&server, WEB, json!([{ "gid": WEB_DONE, "name": "Done" }])).await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let unresolved = core::move_to(&client(&server), TASK, "Backlog")
        .await
        .unwrap()
        .unwrap_err();

    assert!(unresolved.candidates.is_empty());
}

#[tokio::test]
async fn a_section_gid_moves_without_a_lookup_and_still_reports_where() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/projects/{WEB}/sections")))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    add_to_section(&server, WEB_REVIEW, 1).await;
    task_in(&server, &[(WEB, "Web", WEB_REVIEW, "In Review")], 1).await;

    let moved = core::move_to(&client(&server), TASK, WEB_REVIEW)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(moved.as_deref(), Some("Web › In Review"));
}

#[tokio::test]
async fn a_failed_read_after_the_move_still_counts_as_moved() {
    let server = MockServer::start().await;
    add_to_section(&server, WEB_REVIEW, 1).await;
    Mock::given(method("GET"))
        .and(path(format!("/tasks/{TASK}")))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let moved = core::move_to(&client(&server), TASK, WEB_REVIEW).await;

    assert!(matches!(moved, Ok(Ok(None))), "{moved:?}");
}

// --- Projects and sections by name in `project new` / `project add` (T28) ---

const BACKLOG: &str = "1200000000000048";

async fn projects(server: &MockServer, query: &str, found: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(format!("/workspaces/{WS}/typeahead")))
        .and(query_param("resource_type", "project"))
        .and(query_param("query", query))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": found })))
        .mount(server)
        .await;
}

async fn no_writes(server: &MockServer) {
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_task_is_created_in_a_project_and_section_given_by_name() {
    let server = MockServer::start().await;
    projects(
        &server,
        "Web",
        json!([
            { "gid": WEB, "name": "Web" },
            { "gid": OPS, "name": "Web Ops" }
        ]),
    )
    .await;
    sections(
        &server,
        WEB,
        json!([
            { "gid": BACKLOG, "name": "Backlog" },
            { "gid": WEB_DONE, "name": "Done" }
        ]),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/tasks"))
        .and(body_json(
            json!({ "data": { "name": "Write the launch post", "projects": [WEB] } }),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(
            json!({ "data": { "gid": "1200000000000099", "name": "Write the launch post" } }),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/sections/{BACKLOG}/addTask")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .expect(1)
        .mount(&server)
        .await;

    let created = core::create_in(
        &client(&server),
        WS,
        "Web",
        Some("backlog"),
        "Write the launch post",
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(created.task.gid, "1200000000000099");
    assert!(created.section_error.is_none());
}

#[tokio::test]
async fn an_ambiguous_project_creates_nothing() {
    let server = MockServer::start().await;
    projects(
        &server,
        "Web",
        json!([
            { "gid": WEB, "name": "Web Platform" },
            { "gid": OPS, "name": "Web Ops" }
        ]),
    )
    .await;
    no_writes(&server).await;

    let unresolved = core::create_in(&client(&server), WS, "Web", None, "Write the launch post")
        .await
        .unwrap()
        .unwrap_err();

    assert_eq!(unresolved.kind, "project");
    assert_eq!(unresolved.candidates.len(), 2);
}

#[tokio::test]
async fn an_unknown_section_in_the_project_creates_nothing() {
    let server = MockServer::start().await;
    projects(&server, "Web", json!([{ "gid": WEB, "name": "Web" }])).await;
    sections(&server, WEB, json!([{ "gid": WEB_DONE, "name": "Done" }])).await;
    no_writes(&server).await;

    let unresolved = core::create_in(&client(&server), WS, "Web", Some("Backlog"), "Write it")
        .await
        .unwrap()
        .unwrap_err();

    assert_eq!(unresolved.kind, "section");
}

#[tokio::test]
async fn project_and_section_gids_create_without_a_lookup() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/tasks"))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(
                json!({ "data": { "gid": "1200000000000099", "name": "Write it" } }),
            ),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/sections/{BACKLOG}/addTask")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .expect(1)
        .mount(&server)
        .await;

    let created = core::create_in(&client(&server), WS, WEB, Some(BACKLOG), "Write it").await;

    assert!(matches!(created, Ok(Ok(_))), "{created:?}");
}

#[tokio::test]
async fn a_task_is_added_to_a_project_and_section_by_name_and_says_where() {
    let server = MockServer::start().await;
    projects(&server, "ops", json!([{ "gid": OPS, "name": "Ops" }])).await;
    sections(
        &server,
        OPS,
        json!([
            { "gid": OPS_REVIEW, "name": "In Review" },
            { "gid": BACKLOG, "name": "Backlog" }
        ]),
    )
    .await;
    Mock::given(method("POST"))
        .and(path(format!("/tasks/{TASK}/addProject")))
        .and(body_json(
            json!({ "data": { "project": OPS, "section": OPS_REVIEW } }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .expect(1)
        .mount(&server)
        .await;
    task_in(
        &server,
        &[
            (WEB, "Web", WEB_DONE, "Done"),
            (OPS, "Ops", OPS_REVIEW, "In Review"),
        ],
        1,
    )
    .await;

    let added = core::add_to(&client(&server), WS, TASK, "ops", Some("review"))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(added.as_deref(), Some("Ops › In Review"));
}

#[tokio::test]
async fn an_unknown_project_adds_nothing() {
    let server = MockServer::start().await;
    projects(&server, "Mobile", json!([])).await;
    no_writes(&server).await;

    let unresolved = core::add_to(&client(&server), WS, TASK, "Mobile", None)
        .await
        .unwrap()
        .unwrap_err();

    assert_eq!(unresolved.kind, "project");
    assert!(unresolved.candidates.is_empty());
}
