//! The first-run interview (spec F22) against a fake Asana, with scripted
//! answers.

use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use asn::setup::{self, Answers, Outcome, SetupError};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const WS1: &str = "1200000000000001";
const WS2: &str = "1200000000000002";

async fn asana(workspaces: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/workspaces"))
        .and(header("authorization", "Bearer good-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": workspaces })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/workspaces"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({ "errors": [] })))
        .mount(&server)
        .await;
    server
}

fn one() -> serde_json::Value {
    json!([{ "gid": WS1, "name": "Acme" }])
}

fn two() -> serde_json::Value {
    json!([{ "gid": WS1, "name": "Acme" }, { "gid": WS2, "name": "Side \u{202e}project" }])
}

fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |key| {
        pairs
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_string())
    }
}

/// Runs the interview with `input`; returns its result and everything shown.
async fn interview(
    server: &MockServer,
    input: &str,
    env: impl Fn(&str) -> Option<String>,
) -> (Result<Answers, SetupError>, String) {
    let mut out = Vec::new();
    let result = setup::interview(&mut Cursor::new(input), &mut out, env, &server.uri()).await;
    (result, String::from_utf8(out).unwrap())
}

#[tokio::test]
async fn one_workspace_is_chosen_for_you() {
    let server = asana(one()).await;

    let (answers, shown) = interview(&server, "printf good-token\n", env(&[])).await;

    assert_eq!(
        answers.unwrap(),
        Answers {
            token_cmd: Some("printf good-token".into()),
            workspace: WS1.into(),
        }
    );
    assert!(
        shown.contains("Workspace: Acme (1200000000000001)"),
        "{shown}"
    );
    assert!(
        !shown.contains("good-token\n"),
        "the token was shown: {shown}"
    );
}

#[tokio::test]
async fn several_workspaces_are_picked_by_number() {
    let server = asana(two()).await;

    let (answers, shown) = interview(&server, "printf good-token\n9\n2\n", env(&[])).await;

    assert_eq!(answers.unwrap().workspace, WS2);
    assert!(
        shown.contains("  2) Side project (1200000000000002)"),
        "{shown}"
    );
    assert!(shown.contains("Enter a number from 1 to 2."), "{shown}");
}

#[tokio::test]
async fn the_current_environment_is_offered_as_the_defaults() {
    let server = asana(two()).await;

    let (answers, shown) = interview(
        &server,
        "\n\n",
        env(&[
            ("ASANA_TOKEN_CMD", "printf good-token"),
            ("ASANA_WORKSPACE", WS2),
        ]),
    )
    .await;

    assert_eq!(
        answers.unwrap(),
        Answers {
            token_cmd: Some("printf good-token".into()),
            workspace: WS2.into(),
        }
    );
    assert!(
        shown.contains("Token command [printf good-token]: "),
        "{shown}"
    );
    assert!(shown.contains("Workspace [2]: "), "{shown}");
}

#[tokio::test]
async fn a_failing_command_or_a_refused_token_asks_again() {
    let server = asana(one()).await;

    let (answers, shown) = interview(
        &server,
        "exit 3\nprintf bad-token\nprintf good-token\n",
        env(&[]),
    )
    .await;

    assert_eq!(
        answers.unwrap().token_cmd.as_deref(),
        Some("printf good-token")
    );
    assert!(
        shown.contains("The token command failed (exit 3). Try again."),
        "{shown}"
    );
    assert!(shown.contains("Asana didn't accept that token"), "{shown}");
}

#[tokio::test]
async fn an_empty_answer_uses_asana_token_only_when_it_is_set() {
    let server = asana(one()).await;

    let (answers, shown) = interview(&server, "\nprintf good-token\n", env(&[])).await;
    assert!(shown.contains("A token command is needed."), "{shown}");
    assert!(answers.unwrap().token_cmd.is_some());

    let (answers, shown) = interview(&server, "\n", env(&[("ASANA_TOKEN", "good-token")])).await;
    assert!(
        shown.contains("Using ASANA_TOKEN from the environment."),
        "{shown}"
    );
    assert_eq!(answers.unwrap().token_cmd, None);
}

#[tokio::test]
async fn end_of_input_cancels() {
    let server = asana(two()).await;

    let (answers, _) = interview(&server, "printf good-token\n", env(&[])).await;

    assert!(matches!(answers, Err(SetupError::Cancelled)));
}

fn config_path(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("asn/config.toml")
}

#[tokio::test]
async fn the_file_is_written_only_readable_by_you() {
    let server = asana(one()).await;
    let path = config_path("setup-writes");
    let mut out = Vec::new();

    let outcome = setup::run(
        &path,
        &mut Cursor::new("printf good-token\n"),
        &mut out,
        env(&[]),
        &server.uri(),
    )
    .await
    .unwrap();

    assert_eq!(outcome, Outcome::Saved);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("workspace = \"1200000000000001\""), "{text}");
    assert!(text.contains("token_cmd = \"printf good-token\""), "{text}");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[tokio::test]
async fn an_existing_file_is_kept_unless_you_say_y() {
    let server = asana(one()).await;
    let path = config_path("setup-keeps");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "workspace = \"1200000000000009\"\n").unwrap();

    let outcome = setup::run(
        &path,
        &mut Cursor::new("\n"),
        &mut Vec::new(),
        env(&[]),
        &server.uri(),
    )
    .await
    .unwrap();

    assert_eq!(outcome, Outcome::Kept);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "workspace = \"1200000000000009\"\n"
    );

    let outcome = setup::run(
        &path,
        &mut Cursor::new("y\nprintf good-token\n"),
        &mut Vec::new(),
        env(&[]),
        &server.uri(),
    )
    .await
    .unwrap();

    assert_eq!(outcome, Outcome::Saved);
    assert!(std::fs::read_to_string(&path).unwrap().contains(WS1));
}

#[tokio::test]
async fn cancelling_writes_nothing() {
    let server = asana(one()).await;
    let path = config_path("setup-cancel");

    let result = setup::run(
        &path,
        &mut Cursor::new(""),
        &mut Vec::new(),
        env(&[]),
        &server.uri(),
    )
    .await;

    assert!(matches!(result, Err(SetupError::Cancelled)));
    assert!(!path.exists());
}
