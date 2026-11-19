//! First-run setup (spec F22): asks for the token command and the workspace,
//! then writes the config file. The token itself is never stored (ADR-010).
//! Input and output are passed in, so the interview runs in tests.

use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::api::Client;
use crate::api::tasks::Named;
use crate::cli::terminal_safe;
use crate::config::{self, Token};

const PAT_URL: &str = "https://app.asana.com/0/my-apps";

/// What the interview found out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answers {
    /// `None` when the token comes from `ASANA_TOKEN` instead.
    pub token_cmd: Option<String>,
    pub workspace: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The file was written.
    Saved,
    /// The file existed and the user kept it.
    Kept,
}

#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("Setup cancelled; nothing was written")]
    Cancelled,
    #[error("{0}")]
    Io(#[from] io::Error),
}

/// Asks before replacing an existing file, runs the interview, and writes
/// the file at `path`.
pub async fn run(
    path: &Path,
    input: &mut impl BufRead,
    out: &mut impl Write,
    env: impl Fn(&str) -> Option<String>,
    api_url: &str,
) -> Result<Outcome, SetupError> {
    if path.exists() {
        let answer = ask(
            input,
            out,
            &format!("{} exists. Replace it? [y/N] ", path.display()),
        )?;
        if !answer.eq_ignore_ascii_case("y") {
            return Ok(Outcome::Kept);
        }
    }
    writeln!(out, "Setting up asn. Settings go to {}.\n", path.display())?;
    let answers = interview(input, out, env, api_url).await?;
    write_file(path, &file_text(&answers))?;
    writeln!(out, "Saved {}", path.display())?;
    Ok(Outcome::Saved)
}

/// Asks for the token command (checked by running it and listing the
/// workspaces it can see), then for the workspace.
pub async fn interview(
    input: &mut impl BufRead,
    out: &mut impl Write,
    env: impl Fn(&str) -> Option<String>,
    api_url: &str,
) -> Result<Answers, SetupError> {
    let env = |key: &str| env(key).filter(|v| !v.is_empty());
    writeln!(
        out,
        "Your Asana token is never stored. asn runs a command that prints it, e.g.\n  \
         op read op://Private/Asana/token\n  \
         pass show asana/pat\n  \
         security find-generic-password -s asn -w\n    \
         (macOS; store the token first with: security add-generic-password -s asn -a asana -w)\n\
         Create a token at {PAT_URL}\n"
    )?;
    let default_cmd = env("ASANA_TOKEN_CMD");
    let (token_cmd, workspaces) = loop {
        let prompt = match &default_cmd {
            Some(cmd) => format!("Token command [{cmd}]: "),
            None => "Token command: ".to_owned(),
        };
        let answer = ask(input, out, &prompt)?;
        let cmd = if answer.is_empty() {
            default_cmd.clone()
        } else {
            Some(answer)
        };
        let token = match &cmd {
            Some(cmd) => match config::run_token_cmd(cmd, "The token command") {
                Ok(token) => token,
                Err(err) => {
                    writeln!(out, "{err}. Try again.")?;
                    continue;
                }
            },
            None => match env("ASANA_TOKEN") {
                Some(token) => {
                    writeln!(out, "Using ASANA_TOKEN from the environment.")?;
                    Token::new(token)
                }
                None => {
                    writeln!(out, "A token command is needed.")?;
                    continue;
                }
            },
        };
        match Client::new(&token, api_url).workspaces().await {
            Ok(workspaces) if workspaces.is_empty() => {
                writeln!(out, "That token sees no workspaces. Try another.")?;
            }
            Ok(workspaces) => break (cmd, workspaces),
            Err(err) => writeln!(out, "Asana didn't accept that token: {err}. Try again.")?,
        }
    };
    let workspace = choose_workspace(input, out, &workspaces, env("ASANA_WORKSPACE"))?;
    Ok(Answers {
        token_cmd,
        workspace,
    })
}

/// The one workspace, or the one picked by number from the list.
fn choose_workspace(
    input: &mut impl BufRead,
    out: &mut impl Write,
    workspaces: &[Named],
    current: Option<String>,
) -> Result<String, SetupError> {
    let label = |w: &Named| format!("{} ({})", terminal_safe(&w.name), w.gid);
    if let [only] = workspaces {
        writeln!(out, "Workspace: {}", label(only))?;
        return Ok(only.gid.clone());
    }
    writeln!(out, "Workspaces:")?;
    for (number, workspace) in workspaces.iter().enumerate() {
        writeln!(out, "  {}) {}", number + 1, label(workspace))?;
    }
    let default = current.and_then(|gid| workspaces.iter().position(|w| w.gid == gid));
    let prompt = match default {
        Some(index) => format!("Workspace [{}]: ", index + 1),
        None => format!("Workspace (1–{}): ", workspaces.len()),
    };
    loop {
        let answer = ask(input, out, &prompt)?;
        let index = match answer.parse::<usize>() {
            _ if answer.is_empty() => default,
            Ok(n) if (1..=workspaces.len()).contains(&n) => Some(n - 1),
            _ => None,
        };
        match index {
            Some(index) => return Ok(workspaces[index].gid.clone()),
            None => writeln!(out, "Enter a number from 1 to {}.", workspaces.len())?,
        }
    }
}

/// Prints `prompt` and reads one trimmed line. End of input cancels.
fn ask(input: &mut impl BufRead, out: &mut impl Write, prompt: &str) -> Result<String, SetupError> {
    write!(out, "{prompt}")?;
    out.flush()?;
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        writeln!(out)?;
        return Err(SetupError::Cancelled);
    }
    Ok(line.trim().to_owned())
}

/// The config file for `answers`, with the other settings as commented-out
/// defaults so they can be found.
pub fn file_text(answers: &Answers) -> String {
    let quoted = |s: &str| toml::Value::String(s.to_owned()).to_string();
    let token_cmd = match &answers.token_cmd {
        Some(cmd) => format!("token_cmd = {}\n", quoted(cmd)),
        None => {
            "# token_cmd = \"…\"     # unset: ASANA_TOKEN from the environment is used\n".to_owned()
        }
    };
    format!(
        "# asn settings. Environment variables (ASANA_WORKSPACE, ASANA_TOKEN_CMD, ASN_SEARCH,\n\
         # ASN_THEME, ASN_ANIMATIONS) win over this file. The token is never stored here:\n\
         # token_cmd is a command that prints it.\n\
         \n\
         workspace = {}\n\
         {token_cmd}\
         \n\
         # search = \"tasks\"        # \"typeahead\" on Asana plans without full-text search\n\
         # theme = \"terminal\"      # \"dark\", \"light\", or a theme file path\n\
         # animations = true       # false: completed cards leave the board at once\n",
        quoted(&answers.workspace),
    )
}

/// Writes `text` to `path` with mode 0600, creating its directory. The text
/// goes to a file beside it first, which then replaces it, so a failed write
/// never leaves a replaced file half written. A symlink (a dotfile manager's)
/// is written through, not replaced.
pub fn write_file(path: &Path, text: &str) -> io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let path = match std::fs::canonicalize(path) {
        Ok(target) => target,
        Err(e) if e.kind() == io::ErrorKind::NotFound => path.to_owned(),
        Err(e) => return Err(e),
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut temp = path.clone().into_os_string();
    temp.push(".tmp");
    let temp = std::path::PathBuf::from(temp);
    let written = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp)?;
        // `mode` only applies to a new file; a leftover one gets it here.
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, &path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, FileConfig};

    #[test]
    fn the_written_file_reads_back_as_the_answers() {
        let answers = Answers {
            token_cmd: Some("op read \"op://Private/Asana/token\"".to_owned()),
            workspace: "1200000000000001".to_owned(),
        };

        let file = FileConfig::parse(Path::new("config.toml"), &file_text(&answers)).unwrap();

        assert_eq!(file.workspace.as_deref(), Some("1200000000000001"));
        assert_eq!(file.token_cmd, answers.token_cmd);
        assert_eq!(file.search, None);
        assert_eq!(file.theme, None);
        assert_eq!(file.animations, None);
    }

    #[test]
    fn without_a_token_command_the_file_has_none_and_asana_token_is_used() {
        let answers = Answers {
            token_cmd: None,
            workspace: "1200000000000001".to_owned(),
        };
        let file = FileConfig::parse(Path::new("config.toml"), &file_text(&answers)).unwrap();

        let config = Config::resolve(
            |k| (k == "ASANA_TOKEN").then(|| "from-env".to_owned()),
            &file,
        )
        .unwrap();

        assert_eq!(file.token_cmd, None);
        assert_eq!(config.token.expose(), "from-env");
    }

    #[test]
    fn every_commented_default_is_a_valid_setting() {
        let answers = Answers {
            token_cmd: Some("printf x".to_owned()),
            workspace: "1200000000000001".to_owned(),
        };
        let uncommented: String = file_text(&answers)
            .lines()
            .map(|line| match line.strip_prefix("# ") {
                Some(rest) if rest.contains(" = ") && !rest.starts_with("token_cmd") => rest,
                _ if line.starts_with('#') => "",
                _ => line,
            })
            .map(|line| format!("{line}\n"))
            .collect();

        let file = FileConfig::parse(Path::new("config.toml"), &uncommented).unwrap();

        assert_eq!(file.search.as_deref(), Some("tasks"));
        assert_eq!(file.theme.as_deref(), Some("terminal"));
        assert_eq!(file.animations, Some(true));
    }
}
