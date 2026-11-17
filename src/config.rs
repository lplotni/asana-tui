//! Runtime configuration, resolved once at startup: environment variables
//! win over the config file (`~/.config/asn/config.toml`), which wins over
//! the defaults (spec F22, ADR-010). The token itself never comes from the
//! file, only a command that prints it.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

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
    pub search: SearchMode,
    /// A built-in theme name or a theme file path; `None` is the default.
    pub theme: Option<String>,
    /// Whether a completed board card animates before it leaves (F23).
    pub animations: bool,
}

/// Which Asana API finds tasks (`ASN_SEARCH`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// Full-text task search (`tasks/search`); paid plans only.
    #[default]
    Tasks,
    /// Name matches as in the web search box (`typeahead`); every plan.
    Typeahead,
}

/// What the config file may set. Every field is optional; a missing file is
/// the same as an empty one.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub workspace: Option<String>,
    pub token_cmd: Option<String>,
    pub search: Option<String>,
    pub theme: Option<String>,
    pub animations: Option<bool>,
    /// Only here to reject it with a clear message. Its value is never kept.
    #[serde(default)]
    token: Option<serde::de::IgnoredAny>,
}

/// Where a setting came from, as named in error messages.
const FILE_WORKSPACE: &str = "workspace in config.toml";
const FILE_SEARCH: &str = "search in config.toml";
const FILE_TOKEN_CMD: &str = "token_cmd in config.toml";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error(
        "No valid Asana token — set ASANA_TOKEN or ASANA_TOKEN_CMD, or token_cmd in \
         config.toml (asn init sets it up; create a PAT at {PAT_URL})"
    )]
    MissingToken,
    #[error("{setting} failed ({})", exit_description(*.exit))]
    TokenCmdFailed {
        setting: &'static str,
        exit: Option<i32>,
    },
    #[error("{setting} printed no token")]
    TokenCmdEmpty { setting: &'static str },
    #[error("{setting} couldn't be run: {reason}")]
    TokenCmdNotRun {
        setting: &'static str,
        reason: String,
    },
    #[error(
        "ASANA_WORKSPACE is not set and config.toml has no workspace — run asn init, or find \
         your workspace GID with: \
         curl -H \"Authorization: Bearer $ASANA_TOKEN\" https://app.asana.com/api/1.0/workspaces"
    )]
    MissingWorkspace,
    #[error("{setting} must be a numeric GID, got \"{got}\"{}", suggestion(got))]
    InvalidWorkspace { setting: &'static str, got: String },
    #[error("{setting} must be \"tasks\" or \"typeahead\", got \"{got}\"")]
    InvalidSearch { setting: &'static str, got: String },
    #[error("ASN_ANIMATIONS must be \"on\" or \"off\", got \"{got}\"")]
    InvalidAnimations { got: String },
    #[error(
        "{path}: \"token\" isn't allowed — put a command that prints it in token_cmd, \
         or set ASANA_TOKEN"
    )]
    TokenInFile { path: String },
    #[error("{path}{}: {message}", line.map(|l| format!(" line {l}")).unwrap_or_default())]
    File {
        path: String,
        line: Option<usize>,
        message: String,
    },
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
            | Self::TokenCmdEmpty { .. }
            | Self::TokenCmdNotRun { .. } => 2,
            Self::MissingWorkspace
            | Self::InvalidWorkspace { .. }
            | Self::InvalidSearch { .. }
            | Self::InvalidAnimations { .. }
            | Self::TokenInFile { .. }
            | Self::File { .. } => 1,
        }
    }
}

/// The config file's path: `$XDG_CONFIG_HOME/asn/config.toml`, or
/// `$HOME/.config/asn/config.toml`. `None` when neither is set.
pub fn file_path(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |key| env(key).filter(|v: &String| !v.is_empty());
    let base = match set("XDG_CONFIG_HOME").map(PathBuf::from) {
        Some(xdg) if xdg.is_absolute() => xdg,
        _ => PathBuf::from(set("HOME")?).join(".config"),
    };
    Some(base.join("asn").join("config.toml"))
}

impl FileConfig {
    /// Reads the file at `path`. A file that doesn't exist is an empty one.
    pub fn read(path: &Path) -> Result<FileConfig, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => FileConfig::parse(path, &text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(FileConfig::default()),
            Err(e) => Err(ConfigError::File {
                path: path.display().to_string(),
                line: None,
                message: e.to_string(),
            }),
        }
    }

    /// Parses the file's `text`. Errors give the line, never a snippet of
    /// the file, in case a mistyped line holds something secret.
    pub fn parse(path: &Path, text: &str) -> Result<FileConfig, ConfigError> {
        let path = path.display().to_string();
        let file: FileConfig = toml::from_str(text).map_err(|e| ConfigError::File {
            line: e
                .span()
                .map(|span| text[..span.start].matches('\n').count() + 1),
            message: e.message().trim_end().to_owned(),
            path: path.clone(),
        })?;
        if file.token.is_some() {
            return Err(ConfigError::TokenInFile { path });
        }
        Ok(file)
    }
}

impl Config {
    /// Resolves the config from `env`, then `file`, then the defaults. The
    /// token comes last, so a misconfiguration never makes the user unlock a
    /// password manager.
    pub fn resolve(
        env: impl Fn(&str) -> Option<String>,
        file: &FileConfig,
    ) -> Result<Self, ConfigError> {
        let env = |key: &str| env(key).filter(|v| !v.is_empty());
        let workspace = workspace(&env, file)?;
        let search = search(&env, file)?;
        let theme = env("ASN_THEME").or_else(|| file.theme.clone());
        let animations = animations(&env, file)?;
        let token = token(&env, file)?;
        Ok(Self {
            token,
            workspace,
            search,
            theme,
            animations,
        })
    }
}

/// The value of env variable `var`, else the file's, with where it came from.
fn setting(
    env: &impl Fn(&str) -> Option<String>,
    var: &'static str,
    file_value: Option<&String>,
    file_label: &'static str,
) -> Option<(String, &'static str)> {
    env(var)
        .map(|v| (v, var))
        .or_else(|| file_value.map(|v| (v.clone(), file_label)))
}

fn workspace(
    env: &impl Fn(&str) -> Option<String>,
    file: &FileConfig,
) -> Result<String, ConfigError> {
    let (workspace, setting) = setting(
        env,
        "ASANA_WORKSPACE",
        file.workspace.as_ref(),
        FILE_WORKSPACE,
    )
    .ok_or(ConfigError::MissingWorkspace)?;
    if !workspace.is_empty() && workspace.chars().all(|c| c.is_ascii_digit()) {
        Ok(workspace)
    } else {
        Err(ConfigError::InvalidWorkspace {
            setting,
            got: workspace,
        })
    }
}

fn search(
    env: &impl Fn(&str) -> Option<String>,
    file: &FileConfig,
) -> Result<SearchMode, ConfigError> {
    match setting(env, "ASN_SEARCH", file.search.as_ref(), FILE_SEARCH) {
        None => Ok(SearchMode::Tasks),
        Some((value, _)) if value == "tasks" => Ok(SearchMode::Tasks),
        Some((value, _)) if value == "typeahead" => Ok(SearchMode::Typeahead),
        Some((got, setting)) => Err(ConfigError::InvalidSearch { setting, got }),
    }
}

fn animations(
    env: &impl Fn(&str) -> Option<String>,
    file: &FileConfig,
) -> Result<bool, ConfigError> {
    match env("ASN_ANIMATIONS").as_deref() {
        None => Ok(file.animations.unwrap_or(true)),
        Some("on") => Ok(true),
        Some("off") => Ok(false),
        Some(got) => Err(ConfigError::InvalidAnimations {
            got: got.to_owned(),
        }),
    }
}

fn token(env: &impl Fn(&str) -> Option<String>, file: &FileConfig) -> Result<Token, ConfigError> {
    if let Some(token) = env("ASANA_TOKEN") {
        return Ok(Token(token));
    }
    let (cmd, setting) = setting(
        env,
        "ASANA_TOKEN_CMD",
        file.token_cmd.as_ref(),
        FILE_TOKEN_CMD,
    )
    .ok_or(ConfigError::MissingToken)?;
    run_token_cmd(&cmd, setting)
}

/// Runs `cmd` through `sh -c`. Stdin and stderr stay attached to the terminal so
/// password prompts work. Stdout is captured and never echoed.
pub fn run_token_cmd(cmd: &str, setting: &'static str) -> Result<Token, ConfigError> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| ConfigError::TokenCmdNotRun {
            setting,
            reason: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(ConfigError::TokenCmdFailed {
            setting,
            exit: output.status.code(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let token = stdout.lines().next().unwrap_or("").trim();
    if token.is_empty() {
        return Err(ConfigError::TokenCmdEmpty { setting });
    }
    Ok(Token(token.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// The config from the environment alone (no config file).
    #[allow(clippy::wrong_self_convention)]
    fn from_env(env: impl Fn(&str) -> Option<String>) -> Result<Config, ConfigError> {
        Config::resolve(env, &FileConfig::default())
    }

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn uses_asana_token_and_workspace() {
        let config = from_env(env(&[
            ("ASANA_TOKEN", "secret-pat"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap();

        assert_eq!(config.token.expose(), "secret-pat");
        assert_eq!(config.workspace, "1234567890123456");
    }

    #[test]
    fn debug_output_never_contains_the_token() {
        let config = from_env(env(&[
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
        let config = from_env(env(&[
            ("ASANA_TOKEN", "from-env"),
            ("ASANA_TOKEN_CMD", "printf from-cmd"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap();

        assert_eq!(config.token.expose(), "from-env");
    }

    #[test]
    fn empty_asana_token_falls_back_to_token_cmd() {
        let config = from_env(env(&[
            ("ASANA_TOKEN", ""),
            ("ASANA_TOKEN_CMD", "printf from-cmd"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap();

        assert_eq!(config.token.expose(), "from-cmd");
    }

    #[test]
    fn token_cmd_uses_first_line_trimmed() {
        let config = from_env(env(&[
            ("ASANA_TOKEN_CMD", "printf '  tok  \\nsecond line\\n'"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap();

        assert_eq!(config.token.expose(), "tok");
    }

    #[test]
    fn token_cmd_nonzero_exit_is_an_error_with_the_exit_code() {
        let err = from_env(env(&[
            ("ASANA_TOKEN_CMD", "echo leaked-token; exit 3"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap_err();

        assert_eq!(
            err,
            ConfigError::TokenCmdFailed {
                setting: "ASANA_TOKEN_CMD",
                exit: Some(3)
            }
        );
        assert!(!err.to_string().contains("leaked-token"));
    }

    #[test]
    fn token_cmd_with_empty_output_is_an_error() {
        let err = from_env(env(&[
            ("ASANA_TOKEN_CMD", "printf '\\n'"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ]))
        .unwrap_err();

        assert_eq!(
            err,
            ConfigError::TokenCmdEmpty {
                setting: "ASANA_TOKEN_CMD"
            }
        );
    }

    #[test]
    fn neither_token_variable_is_missing_token() {
        let err = from_env(env(&[("ASANA_WORKSPACE", "1234567890123456")])).unwrap_err();

        assert_eq!(err, ConfigError::MissingToken);
        assert!(err.to_string().contains("ASANA_TOKEN_CMD"));
    }

    #[test]
    fn missing_workspace_is_reported_before_running_token_cmd() {
        // A token command that would fail proves it never ran.
        let err = from_env(env(&[("ASANA_TOKEN_CMD", "exit 1")])).unwrap_err();

        assert_eq!(err, ConfigError::MissingWorkspace);
    }

    #[test]
    fn prefixed_workspace_is_rejected_with_a_suggestion() {
        let err = from_env(env(&[
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
        let err = from_env(env(&[
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
        assert_eq!(
            ConfigError::TokenCmdFailed {
                setting: "ASANA_TOKEN_CMD",
                exit: Some(1)
            }
            .exit_code(),
            2
        );
        assert_eq!(
            ConfigError::TokenCmdEmpty {
                setting: "ASANA_TOKEN_CMD"
            }
            .exit_code(),
            2
        );
        assert_eq!(ConfigError::MissingWorkspace.exit_code(), 1);
        assert_eq!(
            ConfigError::InvalidWorkspace {
                setting: "ASANA_WORKSPACE",
                got: "x".into()
            }
            .exit_code(),
            1
        );
    }

    #[test]
    fn a_token_command_that_cannot_start_says_so() {
        let err = ConfigError::TokenCmdNotRun {
            setting: "ASANA_TOKEN_CMD",
            reason: "No such file or directory".into(),
        };

        assert_eq!(
            err.to_string(),
            "ASANA_TOKEN_CMD couldn't be run: No such file or directory"
        );
        assert_eq!(err.exit_code(), 2);
    }

    // --- ASN_SEARCH ----------------------------------------------------------

    fn with_search(value: Option<&str>) -> Result<Config, ConfigError> {
        let mut pairs = vec![
            ("ASANA_TOKEN", "secret-pat"),
            ("ASANA_WORKSPACE", "1234567890123456"),
        ];
        if let Some(value) = value {
            pairs.push(("ASN_SEARCH", value));
        }
        from_env(env(&pairs))
    }

    #[test]
    fn search_defaults_to_task_search() {
        assert_eq!(with_search(None).unwrap().search, SearchMode::Tasks);
        assert_eq!(with_search(Some("")).unwrap().search, SearchMode::Tasks);
    }

    #[test]
    fn search_can_be_set_to_either_backend() {
        assert_eq!(
            with_search(Some("tasks")).unwrap().search,
            SearchMode::Tasks
        );
        assert_eq!(
            with_search(Some("typeahead")).unwrap().search,
            SearchMode::Typeahead
        );
    }

    #[test]
    fn an_unknown_search_backend_is_rejected() {
        let err = with_search(Some("fulltext")).unwrap_err();

        assert_eq!(
            err.to_string(),
            "ASN_SEARCH must be \"tasks\" or \"typeahead\", got \"fulltext\""
        );
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn an_unknown_search_backend_is_reported_before_running_token_cmd() {
        let err = from_env(env(&[
            ("ASANA_TOKEN_CMD", "exit 1"),
            ("ASANA_WORKSPACE", "1234567890123456"),
            ("ASN_SEARCH", "nope"),
        ]))
        .unwrap_err();

        assert_eq!(
            err,
            ConfigError::InvalidSearch {
                setting: "ASN_SEARCH",
                got: "nope".into()
            }
        );
    }

    // --- Config file (F22) ---------------------------------------------------

    const WS: &str = "1200000000000001";

    fn file(text: &str) -> FileConfig {
        FileConfig::parse(Path::new("/home/ada/.config/asn/config.toml"), text).unwrap()
    }

    fn file_err(text: &str) -> ConfigError {
        FileConfig::parse(Path::new("/home/ada/.config/asn/config.toml"), text).unwrap_err()
    }

    #[test]
    fn the_file_supplies_every_setting() {
        let file = file(&format!(
            "workspace = \"{WS}\"\ntoken_cmd = \"printf from-file\"\nsearch = \"typeahead\"\n\
             theme = \"dark\"\nanimations = false\n"
        ));

        let config = Config::resolve(env(&[]), &file).unwrap();

        assert_eq!(config.workspace, WS);
        assert_eq!(config.token.expose(), "from-file");
        assert_eq!(config.search, SearchMode::Typeahead);
        assert_eq!(config.theme.as_deref(), Some("dark"));
        assert!(!config.animations);
    }

    #[test]
    fn environment_variables_win_over_the_file() {
        let file = file(&format!(
            "workspace = \"{WS}\"\ntoken_cmd = \"printf from-file\"\nsearch = \"typeahead\"\n\
             theme = \"dark\"\nanimations = false\n"
        ));

        let config = Config::resolve(
            env(&[
                ("ASANA_WORKSPACE", "1200000000000002"),
                ("ASANA_TOKEN_CMD", "printf from-env-cmd"),
                ("ASN_SEARCH", "tasks"),
                ("ASN_THEME", "light"),
                ("ASN_ANIMATIONS", "on"),
            ]),
            &file,
        )
        .unwrap();

        assert_eq!(config.workspace, "1200000000000002");
        assert_eq!(config.token.expose(), "from-env-cmd");
        assert_eq!(config.search, SearchMode::Tasks);
        assert_eq!(config.theme.as_deref(), Some("light"));
        assert!(config.animations);
    }

    #[test]
    fn asana_token_wins_over_the_files_token_cmd() {
        let file = file(&format!("workspace = \"{WS}\"\ntoken_cmd = \"exit 1\"\n"));

        let config = Config::resolve(env(&[("ASANA_TOKEN", "from-env")]), &file).unwrap();

        assert_eq!(config.token.expose(), "from-env");
    }

    #[test]
    fn empty_environment_variables_fall_back_to_the_file() {
        let file = file(&format!("workspace = \"{WS}\"\nsearch = \"typeahead\"\n"));

        let config = Config::resolve(
            env(&[
                ("ASANA_TOKEN", "secret-pat"),
                ("ASANA_WORKSPACE", ""),
                ("ASN_SEARCH", ""),
            ]),
            &file,
        )
        .unwrap();

        assert_eq!(config.workspace, WS);
        assert_eq!(config.search, SearchMode::Typeahead);
    }

    #[test]
    fn defaults_apply_when_neither_sets_a_value() {
        let config = Config::resolve(
            env(&[("ASANA_TOKEN", "secret-pat")]),
            &file(&format!("workspace = \"{WS}\"\n")),
        )
        .unwrap();

        assert_eq!(config.search, SearchMode::Tasks);
        assert_eq!(config.theme, None);
        assert!(config.animations);
    }

    #[test]
    fn asn_animations_turns_them_off_or_is_rejected() {
        let with = |value| {
            from_env(env(&[
                ("ASANA_TOKEN", "secret-pat"),
                ("ASANA_WORKSPACE", WS),
                ("ASN_ANIMATIONS", value),
            ]))
        };

        assert!(!with("off").unwrap().animations);
        assert!(with("on").unwrap().animations);
        let err = with("maybe").unwrap_err();
        assert_eq!(
            err.to_string(),
            "ASN_ANIMATIONS must be \"on\" or \"off\", got \"maybe\""
        );
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn a_token_in_the_file_is_rejected_without_repeating_it() {
        let err = file_err("token = \"secret-pat\"\n");

        assert_eq!(
            err.to_string(),
            "/home/ada/.config/asn/config.toml: \"token\" isn't allowed — put a command that \
             prints it in token_cmd, or set ASANA_TOKEN"
        );
        assert!(!format!("{err:?}").contains("secret-pat"));
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn an_unknown_key_is_rejected_with_its_line() {
        let err = file_err(&format!("workspace = \"{WS}\"\nanimation = false\n"));

        let message = err.to_string();
        assert!(
            message.starts_with("/home/ada/.config/asn/config.toml line 2: "),
            "{message}"
        );
        assert!(message.contains("animation"), "{message}");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn a_wrongly_typed_value_is_rejected_without_a_snippet_of_the_file() {
        let err = file_err("token_cmd = \"op read x\"\nanimations = \"no\"\n");

        let message = err.to_string();
        assert!(message.contains("line 2"), "{message}");
        assert!(!message.contains("op read"), "{message}");
    }

    #[test]
    fn a_bad_value_from_the_file_names_the_file_key() {
        let file = file("workspace = \"gid:1200000000000001\"\nsearch = \"fulltext\"\n");

        let err = Config::resolve(env(&[("ASANA_TOKEN", "x")]), &file).unwrap_err();
        assert_eq!(
            err.to_string(),
            "workspace in config.toml must be a numeric GID, got \"gid:1200000000000001\" \
             — did you mean 1200000000000001?"
        );

        let file = self::file(&format!("workspace = \"{WS}\"\nsearch = \"fulltext\"\n"));
        let err = Config::resolve(env(&[("ASANA_TOKEN", "x")]), &file).unwrap_err();
        assert_eq!(
            err.to_string(),
            "search in config.toml must be \"tasks\" or \"typeahead\", got \"fulltext\""
        );
    }

    #[test]
    fn a_failing_token_cmd_from_the_file_names_the_file_key() {
        let file = file(&format!("workspace = \"{WS}\"\ntoken_cmd = \"exit 4\"\n"));

        let err = Config::resolve(env(&[]), &file).unwrap_err();

        assert_eq!(err.to_string(), "token_cmd in config.toml failed (exit 4)");
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn missing_settings_point_to_the_file_and_asn_init() {
        let err = from_env(env(&[("ASANA_TOKEN", "x")])).unwrap_err();
        assert!(
            err.to_string()
                .contains("config.toml has no workspace — run asn init")
        );

        let err = from_env(env(&[("ASANA_WORKSPACE", WS)])).unwrap_err();
        assert!(err.to_string().contains("token_cmd in config.toml"));
    }

    #[test]
    fn a_missing_file_is_an_empty_one() {
        let file = FileConfig::read(Path::new("/nonexistent/asn/config.toml")).unwrap();

        assert!(file.workspace.is_none() && file.token_cmd.is_none());
    }

    #[test]
    fn the_file_lives_under_xdg_config_home_or_home() {
        assert_eq!(
            file_path(env(&[("XDG_CONFIG_HOME", "/xdg"), ("HOME", "/home/ada")])),
            Some(PathBuf::from("/xdg/asn/config.toml"))
        );
        assert_eq!(
            file_path(env(&[("XDG_CONFIG_HOME", ""), ("HOME", "/home/ada")])),
            Some(PathBuf::from("/home/ada/.config/asn/config.toml"))
        );
        // XDG paths must be absolute; a relative one is ignored.
        assert_eq!(
            file_path(env(&[("XDG_CONFIG_HOME", "rel"), ("HOME", "/home/ada")])),
            Some(PathBuf::from("/home/ada/.config/asn/config.toml"))
        );
        assert_eq!(file_path(env(&[])), None);
    }
}
