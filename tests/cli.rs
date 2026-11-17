//! Runs the real `asn` binary with a controlled environment.

use std::process::{Command, Output};

fn asn(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_asn"));
    cmd.args(args).env_clear();
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("failed to run asn")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn help_lists_every_subcommand_from_the_spec() {
    let output = asn(&["--help"], &[]);
    let help = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    for sub in [
        "inbox", "read", "task", "comment", "desc", "project", "search", "board", "users",
    ] {
        assert!(help.contains(sub), "missing `{sub}` in help:\n{help}");
    }
}

#[test]
fn missing_token_exits_2_with_guidance() {
    let output = asn(&["inbox"], &[("ASANA_WORKSPACE", "1234567890123456")]);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("set ASANA_TOKEN or ASANA_TOKEN_CMD"));
}

#[test]
fn missing_workspace_exits_1_with_guidance() {
    let output = asn(&["inbox"], &[("ASANA_TOKEN", "secret-pat")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("ASANA_WORKSPACE is not set"));
}

#[test]
fn prefixed_workspace_exits_1_with_suggestion() {
    let output = asn(
        &["inbox"],
        &[
            ("ASANA_TOKEN", "secret-pat"),
            ("ASANA_WORKSPACE", "gid:1234567890123456"),
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("did you mean 1234567890123456?"));
}

#[test]
fn failing_token_cmd_exits_2_without_echoing_its_stdout() {
    let output = asn(
        &["inbox"],
        &[
            ("ASANA_TOKEN_CMD", "echo leaked-token; exit 1"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("ASANA_TOKEN_CMD failed (exit 1)"));
    assert!(!stderr(&output).contains("leaked-token"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("leaked-token"));
}

#[test]
fn usage_errors_exit_1_not_the_auth_code_2() {
    let output = asn(&["read"], &[]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("NOTIFICATION_GID"));
}

#[test]
fn an_unknown_search_backend_exits_1_with_guidance() {
    let output = asn(
        &["search", "checkout"],
        &[
            ("ASANA_TOKEN", "secret-pat"),
            ("ASANA_WORKSPACE", "1234567890123456"),
            ("ASN_SEARCH", "fulltext"),
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("ASN_SEARCH must be"));
}

#[test]
fn project_help_lists_add_move_and_sections() {
    let output = asn(&["project", "--help"], &[]);
    let help = String::from_utf8_lossy(&output.stdout);

    for sub in ["add", "move", "sections", "new"] {
        assert!(help.contains(sub), "missing `{sub}` in help:\n{help}");
    }
}

#[test]
fn a_non_numeric_project_exits_1_before_any_request() {
    let output = asn(
        &["--project", "gid:42"],
        &[
            ("ASANA_TOKEN", "secret-pat"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("must be a numeric GID"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn project_only_opens_the_tui_so_it_cannot_be_combined_with_a_subcommand() {
    let output = asn(&["--project", "42", "inbox"], &[]);

    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn help_mentions_opening_a_project_board() {
    let output = asn(&["--help"], &[]);
    let help = String::from_utf8_lossy(&output.stdout);

    assert!(help.contains("--project"), "{help}");
}

#[test]
fn task_help_lists_assign_and_done() {
    let output = asn(&["task", "--help"], &[]);
    let help = String::from_utf8_lossy(&output.stdout);

    for sub in ["assign", "done"] {
        assert!(help.contains(sub), "missing `{sub}` in help:\n{help}");
    }
}

#[test]
fn asn_task_with_a_gid_still_shows_a_task() {
    // Parsed as before: it gets as far as needing a token (exit 2), not a
    // usage error (exit 1).
    for args in [
        &["task", "1200000000000031"][..],
        &["task", "1200000000000031", "--json"],
    ] {
        let output = asn(args, &[("ASANA_WORKSPACE", "1234567890123456")]);

        assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    }
}
