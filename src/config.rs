//! Runtime configuration, resolved once at startup from the environment.

use std::fmt;
use std::process::{Command, Stdio};

const PAT_URL: &str = "https://app.asana.com/0/my-apps";

/// An Asana Personal Access Token. `Debug` never reveals it.
pub struct Token(String);

impl Token {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

#[derive(Debug)]
pub struct Config {
    pub token: Token,
    pub workspace: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error(
        "No valid Asana token — set ASANA_TOKEN or ASANA_TOKEN_CMD (create a PAT at {PAT_URL})"
    )]
    MissingToken,
    #[error("ASANA_TOKEN_CMD failed ({})", exit_description(*.exit))]
    TokenCmdFailed { exit: Option<i32> },
    #[error("ASANA_TOKEN_CMD printed no token")]
    TokenCmdEmpty,
    #[error("ASANA_TOKEN_CMD couldn't be run: {reason}")]
    TokenCmdNotRun { reason: String },
    #[error(
        "ASANA_WORKSPACE is not set — find your workspace GID with: \
         curl -H \"Authorization: Bearer $ASANA_TOKEN\" https://app.asana.com/api/1.0/workspaces"
    )]
    MissingWorkspace,
    #[error(
        "ASANA_WORKSPACE must be a numeric GID, got \"{got}\"{}",
        suggestion(got)
    )]
    InvalidWorkspace { got: String },
}

fn exit_description(exit: Option<i32>) -> String {
    match exit {
        Some(code) => format!("exit {code}"),
        None => "killed by signal".to_string(),
    }
}

fn suggestion(got: &str) -> String {
    let digits: String = got.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        String::new()
    } else {
        format!(" — did you mean {digits}?")
    }
}

impl ConfigError {
    /// Process exit code: 2 for authentication problems, 1 for everything else.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::MissingToken
            | Self::TokenCmdFailed { .. }
            | Self::TokenCmdEmpty
            | Self::TokenCmdNotRun { .. } => 2,
            Self::MissingWorkspace | Self::InvalidWorkspace { .. } => 1,
        }
    }
}

impl Config {
    /// Resolves the config from `env`. The workspace is validated first, so a
    /// misconfigured workspace never makes the user unlock a password manager.
    pub fn from_env(env: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let workspace = workspace(&env)?;
        let token = token(&env)?;
        Ok(Self { token, workspace })
    }
}

fn non_empty(env: &impl Fn(&str) -> Option<String>, key: &str) -> Option<String> {
    env(key).filter(|v| !v.is_empty())
}

fn workspace(env: &impl Fn(&str) -> Option<String>) -> Result<String, ConfigError> {
    let workspace = non_empty(env, "ASANA_WORKSPACE").ok_or(ConfigError::MissingWorkspace)?;
    if workspace.chars().all(|c| c.is_ascii_digit()) {
        Ok(workspace)
    } else {
        Err(ConfigError::InvalidWorkspace { got: workspace })
    }
}

fn token(env: &impl Fn(&str) -> Option<String>) -> Result<Token, ConfigError> {
    if let Some(token) = non_empty(env, "ASANA_TOKEN") {
        return Ok(Token(token));
    }
    let cmd = non_empty(env, "ASANA_TOKEN_CMD").ok_or(ConfigError::MissingToken)?;
    run_token_cmd(&cmd)
}

/// Runs `cmd` through `sh -c`. Stdin and stderr stay attached to the terminal so
/// password prompts work. Stdout is captured and never echoed.
fn run_token_cmd(cmd: &str) -> Result<Token, ConfigError> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| ConfigError::TokenCmdNotRun {
            reason: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(ConfigError::TokenCmdFailed {
            exit: output.status.code(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let token = stdout.lines().next().unwrap_or("").trim();
    if token.is_empty() {
        return Err(ConfigError::TokenCmdEmpty);
    }
    Ok(Token(token.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn uses_asana_token_and_workspace() {
        let config = Config::from_env(env(&[
            ("ASANA_TOKEN", "secret-pat"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap();

        assert_eq!(config.token.expose(), "secret-pat");
        assert_eq!(config.workspace, "1234567890123456");
    }

    #[test]
    fn debug_output_never_contains_the_token() {
        let config = Config::from_env(env(&[
            ("ASANA_TOKEN", "secret-pat"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap();

        let debug = format!("{config:?}");

        assert!(!debug.contains("secret-pat"), "token leaked: {debug}");
        assert!(debug.contains("1234567890123456"));
    }

    #[test]
    fn asana_token_wins_over_token_cmd() {
        let config = Config::from_env(env(&[
            ("ASANA_TOKEN", "from-env"),
            ("ASANA_TOKEN_CMD", "printf from-cmd"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap();

        assert_eq!(config.token.expose(), "from-env");
    }

    #[test]
    fn empty_asana_token_falls_back_to_token_cmd() {
        let config = Config::from_env(env(&[
            ("ASANA_TOKEN", ""),
            ("ASANA_TOKEN_CMD", "printf from-cmd"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap();

        assert_eq!(config.token.expose(), "from-cmd");
    }

    #[test]
    fn token_cmd_uses_first_line_trimmed() {
        let config = Config::from_env(env(&[
            ("ASANA_TOKEN_CMD", "printf '  tok  \\nsecond line\\n'"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap();

        assert_eq!(config.token.expose(), "tok");
    }

    #[test]
    fn token_cmd_nonzero_exit_is_an_error_with_the_exit_code() {
        let err = Config::from_env(env(&[
            ("ASANA_TOKEN_CMD", "echo leaked-token; exit 3"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap_err();

        assert_eq!(err, ConfigError::TokenCmdFailed { exit: Some(3) });
        assert!(!err.to_string().contains("leaked-token"));
    }

    #[test]
    fn token_cmd_with_empty_output_is_an_error() {
        let err = Config::from_env(env(&[
            ("ASANA_TOKEN_CMD", "printf '\\n'"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap_err();

        assert_eq!(err, ConfigError::TokenCmdEmpty);
    }

    #[test]
    fn neither_token_variable_is_missing_token() {
        let err = Config::from_env(env(&[("ASANA_WORKSPACE", "1234567890123456")])).unwrap_err();

        assert_eq!(err, ConfigError::MissingToken);
        assert!(err.to_string().contains("ASANA_TOKEN_CMD"));
    }

    #[test]
    fn missing_workspace_is_reported_before_running_token_cmd() {
        // A token command that would fail proves it never ran.
        let err = Config::from_env(env(&[("ASANA_TOKEN_CMD", "exit 1")])).unwrap_err();

        assert_eq!(err, ConfigError::MissingWorkspace);
    }

    #[test]
    fn prefixed_workspace_is_rejected_with_a_suggestion() {
        let err = Config::from_env(env(&[
            ("ASANA_TOKEN", "secret-pat"),
            ("ASANA_WORKSPACE", "gid:1234567890123456"),
        ]))
        .unwrap_err();

        assert_eq!(
            err.to_string(),
            "ASANA_WORKSPACE must be a numeric GID, got \"gid:1234567890123456\" — did you mean 1234567890123456?"
        );
    }

    #[test]
    fn workspace_without_digits_is_rejected_without_a_suggestion() {
        let err = Config::from_env(env(&[
            ("ASANA_TOKEN", "secret-pat"),
            ("ASANA_WORKSPACE", "my-workspace"),
        ]))
        .unwrap_err();

        assert_eq!(
            err.to_string(),
            "ASANA_WORKSPACE must be a numeric GID, got \"my-workspace\""
        );
    }

    #[test]
    fn exit_codes_are_2_for_token_problems_and_1_for_workspace_problems() {
        assert_eq!(ConfigError::MissingToken.exit_code(), 2);
        assert_eq!(ConfigError::TokenCmdFailed { exit: Some(1) }.exit_code(), 2);
        assert_eq!(ConfigError::TokenCmdEmpty.exit_code(), 2);
        assert_eq!(ConfigError::MissingWorkspace.exit_code(), 1);
        assert_eq!(
            ConfigError::InvalidWorkspace { got: "x".into() }.exit_code(),
            1
        );
    }

    #[test]
    fn a_token_command_that_cannot_start_says_so() {
        let err = ConfigError::TokenCmdNotRun {
            reason: "No such file or directory".into(),
        };

        assert_eq!(
            err.to_string(),
            "ASANA_TOKEN_CMD couldn't be run: No such file or directory"
        );
        assert_eq!(err.exit_code(), 2);
    }
}
