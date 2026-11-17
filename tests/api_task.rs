//! `core::task_detail` against a fake Asana serving the task fixtures.

use asn::api::Client;
use asn::api::attachments::ATTACHMENT_FIELDS;
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

/// Serves the task, and (unless a test mounts its own) no attachments.
async fn serve_task(server: &MockServer, body: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/tasks/{TASK}")))
        .and(query_param("opt_fields", TASK_FIELDS))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/attachments"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "data": [], "next_page": null })),
        )
        .with_priority(10)
        .mount(server)
        .await;
}

async fn serve_attachments(server: &MockServer, offset: Option<&str>, body: Value) {
    let mock = Mock::given(method("GET"))
        .and(path("/attachments"))
        .and(query_param("parent", TASK))
        .and(query_param("opt_fields", ATTACHMENT_FIELDS));
    let mock = match offset {
        Some(offset) => mock.and(query_param("offset", offset)),
        None => mock.and(query_param_is_missing("offset")),
    };
    mock.respond_with(ResponseTemplate::new(200).set_body_json(body))
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

#[tokio::test]
async fn comment_posts_the_text_and_returns_the_new_story() {
    let server = MockServer::start().await;
    let stories = fixture("stories_thread.json");
    let created = stories["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["type"] == "comment")
        .unwrap()
        .clone();
    Mock::given(method("POST"))
        .and(path(format!("/tasks/{TASK}/stories")))
        .and(query_param("opt_fields", STORY_FIELDS))
        .and(wiremock::matchers::body_json(
            json!({ "data": { "text": "Ship it\n" } }),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "data": created })))
        .mount(&server)
        .await;

    let story = core::comment(&client(&server), TASK, "Ship it\n")
        .await
        .unwrap();

    assert!(story.is_comment());
    assert_eq!(story.gid, created["gid"]);
}

// --- Description edits (T11) ----------------------------------------------

#[tokio::test]
async fn set_notes_writes_the_text_when_the_task_is_unchanged() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/tasks/{TASK}")))
        .and(query_param("opt_fields", "modified_at"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "data": { "gid": TASK, "modified_at": "2026-11-16T10:00:00.000Z" } }),
        ))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("/tasks/{TASK}")))
        .and(wiremock::matchers::body_json(
            json!({ "data": { "notes": "New text" } }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .expect(1)
        .mount(&server)
        .await;

    core::set_notes(
        &client(&server),
        TASK,
        "New text",
        Some("2026-11-16T10:00:00.000Z"),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn set_notes_refuses_when_the_task_changed_meanwhile() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/tasks/{TASK}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "data": { "gid": TASK, "modified_at": "2026-11-16T11:30:00.000Z" } }),
        ))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": {} })))
        .expect(0)
        .mount(&server)
        .await;

    let err = core::set_notes(
        &client(&server),
        TASK,
        "New text",
        Some("2026-11-16T10:00:00.000Z"),
    )
    .await
    .unwrap_err();

    assert!(matches!(err, core::SaveNotesError::Conflict), "{err:?}");
}

#[test]
fn rich_text_is_detected_from_html_notes() {
    let plain = fixture("task_plain.json")["data"]["html_notes"]
        .as_str()
        .unwrap()
        .to_owned();
    let rich = fixture("task_rich.json")["data"]["html_notes"]
        .as_str()
        .unwrap()
        .to_owned();

    assert!(!core::has_formatting(&plain));
    assert!(core::has_formatting(&rich));
    assert!(core::has_formatting(
        "<body>see <a href=\"https://x\">this</a></body>"
    ));
    assert!(core::has_formatting("<body><ul><li>one</li></ul></body>"));
    assert!(core::has_formatting("<body><strong>bold</strong></body>"));
    assert!(core::has_formatting(
        "<body>hi <a data-asana-gid=\"1\" data-asana-type=\"user\">@Ada</a></body>"
    ));
    assert!(!core::has_formatting(
        "<body>Line one\nLine two &amp; more</body>"
    ));
    assert!(!core::has_formatting(""));
}

#[tokio::test]
async fn comment_reactions_are_parsed() {
    let server = MockServer::start().await;
    serve_task(&server, fixture("task_full.json")).await;
    let story = json!({
        "gid": "1", "type": "comment", "text": "hi",
        "created_at": "2026-11-03T09:11:43.087Z", "created_by": null,
        "reaction_summary": [
            { "emoji_base": "👍", "variant": "👍🏽", "count": 2, "reacted": true }
        ]
    });
    serve_stories(&server, None, json!({ "data": [story], "next_page": null })).await;

    let detail = core::task_detail(&client(&server), TASK).await.unwrap();

    let reactions = &detail.comments[0].reaction_summary;
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].variant, "👍🏽");
    assert_eq!(reactions[0].count, 2);
}

#[test]
fn story_fields_request_reactions() {
    assert!(STORY_FIELDS.contains("reaction_summary.variant"));
    assert!(STORY_FIELDS.contains("reaction_summary.count"));
}

// --- Attachments ------------------------------------------------------------

#[tokio::test]
async fn task_detail_includes_the_attachments() {
    let server = MockServer::start().await;
    serve_task(&server, fixture("task_full.json")).await;
    serve_stories(&server, None, json!({ "data": [], "next_page": null })).await;
    serve_attachments(&server, None, fixture("attachments.json")).await;

    let detail = core::task_detail(&client(&server), TASK).await.unwrap();

    let names: Vec<&str> = detail.attachments.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Checkout flow.png", "Spec.pdf", "Roadmap"]);
    assert_eq!(detail.attachments[0].size, Some(123_456));
    assert!(detail.attachments[0].download_url.is_some());
    assert_eq!(detail.attachments[2].resource_subtype, "gdrive");
    assert_eq!(detail.attachments[2].size, None);
}

#[tokio::test]
async fn task_detail_follows_attachment_pages() {
    let server = MockServer::start().await;
    serve_task(&server, fixture("task_full.json")).await;
    serve_stories(&server, None, json!({ "data": [], "next_page": null })).await;
    let mut first = fixture("attachments.json");
    first["next_page"] = json!({ "offset": "a2", "path": "/x", "uri": "https://x" });
    serve_attachments(&server, None, first).await;
    serve_attachments(&server, Some("a2"), fixture("attachments.json")).await;

    let detail = core::task_detail(&client(&server), TASK).await.unwrap();

    assert_eq!(detail.attachments.len(), 6);
}

#[tokio::test]
async fn download_fetches_the_file_without_the_asana_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/assets/shot.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8, 2, 3]))
        .mount(&server)
        .await;

    let bytes = asn::api::attachments::download(&format!("{}/assets/shot.png", server.uri()), 10)
        .await
        .unwrap();

    assert_eq!(bytes, [1, 2, 3]);
    let requests = server.received_requests().await.unwrap();
    assert!(requests[0].headers.get("authorization").is_none());
}

#[tokio::test]
async fn download_refuses_files_over_the_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 2000]))
        .mount(&server)
        .await;

    let result = asn::api::attachments::download(&format!("{}/big.png", server.uri()), 1000).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn download_reports_an_expired_link() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;

    let result = asn::api::attachments::download(&format!("{}/old.png", server.uri()), 1000).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn a_task_whose_stories_asana_refuses_still_loads_without_comments() {
    let server = MockServer::start().await;
    serve_task(&server, fixture("task_full.json")).await;
    Mock::given(method("GET"))
        .and(path(format!("/tasks/{TASK}/stories")))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "errors": [{ "message": "The requested stories has the following unsupported parent in the public API: CommentStory" }]
        })))
        .mount(&server)
        .await;

    let detail = core::task_detail(&client(&server), TASK).await.unwrap();

    assert_eq!(detail.task.name, fixture("task_full.json")["data"]["name"]);
    assert!(detail.comments.is_empty());
    let error = detail.comments_error.unwrap();
    assert!(error.contains("unsupported parent"), "{error}");
}

#[tokio::test]
async fn a_failed_download_never_shows_the_signed_link() {
    let url = "http://127.0.0.1:1/assets/shot.png?X-Amz-Signature=secret";

    let err = asn::api::attachments::download(url, 1000)
        .await
        .unwrap_err();

    assert!(!err.to_string().contains("X-Amz-Signature"), "{err}");
}
