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
fn task_assign_takes_a_name_as_well_as_a_gid() {
    let output = asn(&["task", "assign", "--help"], &[]);
    let help = String::from_utf8_lossy(&output.stdout);

    assert!(help.contains("USER_GID|NAME|me|none"), "{help}");

    // A name with a space is one argument, and parses up to the token check.
    let output = asn(
        &["task", "assign", "1200000000000031", "Ada Lovelace"],
        &[("ASANA_WORKSPACE", "1234567890123456")],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
}

#[test]
fn project_move_takes_a_section_name_as_well_as_a_gid() {
    let output = asn(&["project", "move", "--help"], &[]);
    let help = String::from_utf8_lossy(&output.stdout);

    assert!(help.contains("SECTION_GID|NAME"), "{help}");

    let output = asn(
        &["project", "move", "1200000000000031", "In Review"],
        &[("ASANA_WORKSPACE", "1234567890123456")],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
}

#[test]
fn project_new_and_add_take_project_and_section_names() {
    for sub in ["new", "add"] {
        let output = asn(&["project", sub, "--help"], &[]);
        let help = String::from_utf8_lossy(&output.stdout);

        assert!(help.contains("PROJECT_GID|NAME"), "{sub}: {help}");
        assert!(help.contains("SECTION_GID|NAME"), "{sub}: {help}");
    }

    let env = [("ASANA_WORKSPACE", "1234567890123456")];
    let new = asn(
        &[
            "project",
            "new",
            "Web Platform",
            "--section",
            "Backlog",
            "Write",
            "it",
        ],
        &env,
    );
    assert_eq!(new.status.code(), Some(2), "{}", stderr(&new));
    let add = asn(
        &[
            "project",
            "add",
            "1200000000000031",
            "Web Platform",
            "--section",
            "Backlog",
        ],
        &env,
    );
    assert_eq!(add.status.code(), Some(2), "{}", stderr(&add));
}

#[test]
fn board_lanes_takes_a_field() {
    let output = asn(&["board", "--help"], &[]);
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("--lanes <FIELD>"), "{help}");

    let output = asn(
        &[
            "board",
            "1200000000000042",
            "--lanes",
            "Initiative",
            "--json",
        ],
        &[("ASANA_WORKSPACE", "1234567890123456")],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
}

#[test]
fn task_field_and_project_fields_are_commands() {
    let task = String::from_utf8_lossy(&asn(&["task", "--help"], &[]).stdout).into_owned();
    assert!(task.contains("field"), "{task}");
    let project = String::from_utf8_lossy(&asn(&["project", "--help"], &[]).stdout).into_owned();
    assert!(project.contains("fields"), "{project}");

    let env = [("ASANA_WORKSPACE", "1234567890123456")];
    for args in [
        &[
            "task",
            "field",
            "1200000000000031",
            "Initiative",
            "Checkout",
        ][..],
        &["task", "field", "1200000000000031", "Initiative", "none"],
        &["project", "fields", "Web Platform"],
    ] {
        let output = asn(args, &env);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn asn_projects_takes_words_to_search_for() {
    let output = asn(
        &["projects", "web", "platform"],
        &[("ASANA_WORKSPACE", "1234567890123456")],
    );
    // Parsed: it gets as far as needing a token (exit 2).
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));

    let output = asn(&["projects"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("QUERY"), "{}", stderr(&output));
}

#[test]
fn task_brief_and_json_cannot_be_combined() {
    let output = asn(&["task", "1200000000000031", "--brief", "--json"], &[]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("cannot be used with"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn asn_task_with_a_gid_still_shows_a_task() {
    // Parsed as before: it gets as far as needing a token (exit 2), not a
    // usage error (exit 1).
    for args in [
        &["task", "1200000000000031"][..],
        &["task", "1200000000000031", "--json"],
        &["task", "1200000000000031", "--brief"],
    ] {
        let output = asn(args, &[("ASANA_WORKSPACE", "1234567890123456")]);

        assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    }
}

/// A config directory with `config.toml` holding `text`, for `XDG_CONFIG_HOME`.
fn config_home(name: &str, text: &str) -> String {
    let home = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(home.join("asn")).unwrap();
    std::fs::write(home.join("asn/config.toml"), text).unwrap();
    home.display().to_string()
}

#[test]
fn the_config_file_supplies_the_workspace() {
    let home = config_home("cfg-workspace", "workspace = \"1200000000000001\"\n");

    // The workspace is found, so the next problem is the missing token.
    let output = asn(&["inbox"], &[("XDG_CONFIG_HOME", &home)]);

    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("No valid Asana token"));
}

#[test]
fn a_token_in_the_config_file_exits_1_without_using_it() {
    let home = config_home("cfg-token", "token = \"secret-pat\"\n");

    let output = asn(&["inbox"], &[("XDG_CONFIG_HOME", &home)]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("\"token\" isn't allowed"));
    assert!(!stderr(&output).contains("secret-pat"));
}

#[test]
fn asn_init_needs_a_terminal() {
    let output = asn(&["init"], &[("HOME", env!("CARGO_TARGET_TMPDIR"))]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("asn init needs a terminal"));
}

#[test]
fn a_subcommand_without_a_config_file_never_prompts() {
    let home = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cfg-none");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();

    let output = asn(&["inbox"], &[("HOME", home.to_str().unwrap())]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("run asn init"));
    assert!(output.stdout.is_empty(), "it prompted");
    assert!(!home.join(".config").exists(), "it wrote something");
}
