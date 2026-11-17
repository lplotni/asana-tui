//! The Claude Code skill (F20) and its plugin manifests. The skill is prose,
//! but every `asn` command it shows must exist, so a renamed subcommand or
//! flag fails here instead of silently breaking the skill.

use std::process::Command;

const SKILL: &str = "skills/asn/SKILL.md";

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn help(path: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_asn"))
        .args(path)
        .arg("--help")
        .env_clear()
        .output()
        .expect("failed to run asn");
    assert!(output.status.success(), "asn {path:?} --help failed");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The subcommand names listed under `Commands:` in a help text.
fn subcommands(help: &str) -> Vec<String> {
    help.lines()
        .skip_while(|l| l.trim() != "Commands:")
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next().map(str::to_owned))
        .collect()
}

/// Every `asn …` command inside the skill's code blocks, as words, without
/// a trailing `# comment`. Commands after a `|` count too.
fn skill_commands() -> Vec<Vec<String>> {
    let mut in_code = false;
    let mut out = Vec::new();
    for line in read(SKILL).lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if !in_code {
            continue;
        }
        let line = line.split(" # ").next().unwrap_or_default();
        for part in line.split('|') {
            let words: Vec<String> = part.split_whitespace().map(str::to_owned).collect();
            if words.first().is_some_and(|w| w == "asn") {
                out.push(words[1..].to_vec());
            }
        }
    }
    out
}

#[test]
fn every_asn_command_in_the_skill_exists() {
    let commands = skill_commands();
    assert!(commands.len() >= 10, "found only {commands:?}");

    for words in commands {
        // Walk the subcommand path while the parent has subcommands.
        let mut path: Vec<&str> = Vec::new();
        for word in &words {
            let is_name =
                word.chars().all(|c| c.is_ascii_lowercase() || c == '-') && !word.starts_with('-');
            if !is_name {
                break;
            }
            let names = subcommands(&help(&path));
            if names.is_empty() {
                break;
            }
            // A parent with subcommands that also takes arguments (`asn task
            // <gid>`) is followed by a GID or placeholder, never a bare word.
            assert!(
                names.iter().any(|n| n == word),
                "`asn {}`: no subcommand `{word}`",
                words.join(" ")
            );
            path.push(word);
        }
        let text = help(&path);
        for flag in words.iter().filter(|w| w.starts_with("--")) {
            let flag = flag.split('=').next().unwrap_or_default();
            let listed = text
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .any(|w| w == flag);
            assert!(
                listed,
                "`asn {}`: `asn {}` has no {flag}",
                words.join(" "),
                path.join(" ")
            );
        }
    }
}

#[test]
fn the_skill_uses_only_invented_gids() {
    let text = read(SKILL);
    let mut digits = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if digits.len() >= 10 {
            assert!(
                digits.starts_with("12000000000000"),
                "{digits} doesn't look invented (use 12000000000000xx)"
            );
        }
        digits.clear();
    }
}

#[test]
fn the_plugin_ships_the_asn_skill() {
    let plugin: serde_json::Value =
        serde_json::from_str(&read(".claude-plugin/plugin.json")).unwrap();
    let market: serde_json::Value =
        serde_json::from_str(&read(".claude-plugin/marketplace.json")).unwrap();
    let skill = read(SKILL);

    assert_eq!(plugin["name"], "asn");
    assert_eq!(market["plugins"][0]["name"], "asn");
    assert_eq!(market["plugins"][0]["source"], "./");
    assert!(
        skill.starts_with("---\nname: asn\ndescription: "),
        "frontmatter"
    );
}
