use std::process::ExitCode;

use asn::api::{ASANA_API_URL, Client};
use asn::cli::inbox;
use asn::config::Config;
use asn::core;
use clap::{Parser, Subcommand};

/// Asana Inbox in your terminal. Runs the TUI when no subcommand is given.
#[derive(Parser)]
#[command(name = "asn", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List Inbox notifications
    Inbox {
        /// Only unread notifications
        #[arg(long)]
        unread: bool,
        /// Maximum number of notifications
        #[arg(long, value_name = "N")]
        limit: Option<u32>,
        /// Print Asana's JSON objects
        #[arg(long)]
        json: bool,
    },
    /// Mark notifications read (or unread)
    Read {
        #[arg(required = true, value_name = "NOTIFICATION_GID")]
        gids: Vec<String>,
        /// Mark unread instead
        #[arg(long)]
        unread: bool,
    },
    /// Show a task with its comments
    Task {
        #[arg(value_name = "TASK_GID")]
        gid: String,
        /// Print JSON
        #[arg(long)]
        json: bool,
    },
    /// Add a comment to a task (opens $EDITOR without -m or -)
    Comment {
        #[arg(value_name = "TASK_GID")]
        gid: String,
        /// Comment text
        #[arg(short, long, value_name = "TEXT", conflicts_with = "stdin")]
        message: Option<String>,
        /// `-` reads the comment from stdin
        #[arg(value_name = "-", value_parser = ["-"])]
        stdin: Option<String>,
    },
    /// Replace a task's description (opens $EDITOR without --file or -)
    Desc {
        #[arg(value_name = "TASK_GID")]
        gid: String,
        /// Read the description from a file
        #[arg(long, value_name = "PATH", conflicts_with = "stdin")]
        file: Option<std::path::PathBuf>,
        /// `-` reads the description from stdin
        #[arg(value_name = "-", value_parser = ["-"])]
        stdin: Option<String>,
        /// Overwrite rich-text descriptions without asking
        #[arg(long)]
        force: bool,
    },
    /// Manage a task's projects
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
}

#[derive(Subcommand)]
enum ProjectCommand {
    /// Add a task to a project (keeps its existing projects)
    Add {
        #[arg(value_name = "TASK_GID")]
        task: String,
        #[arg(value_name = "PROJECT_GID")]
        project: String,
        /// Section within the project
        #[arg(long, value_name = "SECTION_GID")]
        section: Option<String>,
    },
}

/// Default for `asn inbox --limit`.
const DEFAULT_INBOX_LIMIT: u32 = 50;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        // Exit 2 is reserved for auth problems, so usage errors exit 1.
        Err(err) => {
            let _ = err.print();
            return if err.use_stderr() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            };
        }
    };

    let config = match Config::from_env(|key| std::env::var(key).ok()) {
        Ok(config) => config,
        Err(err) => return fail(&err, err.exit_code()),
    };
    let client = Client::new(&config.token, ASANA_API_URL);

    match cli.command {
        Some(Command::Inbox {
            unread,
            limit,
            json,
        }) => {
            let limit = limit.unwrap_or(DEFAULT_INBOX_LIMIT) as usize;
            match core::inbox(&client, &config.workspace, limit, unread).await {
                Ok(notifications) if json => println!("{}", inbox::render_json(&notifications)),
                Ok(notifications) => print!("{}", inbox::render_text(&notifications)),
                Err(err) => return fail(&err, err.exit_code()),
            }
            ExitCode::SUCCESS
        }
        Some(Command::Read { gids, unread }) => {
            let outcome = core::mark_read(&client, &config.workspace, &gids, !unread).await;
            for gid in &outcome.failed {
                eprintln!("Asana could not update notification {gid}");
            }
            match (outcome.error, outcome.failed.is_empty()) {
                (Some(err), _) => fail(&err, err.exit_code()),
                (None, true) => ExitCode::SUCCESS,
                (None, false) => ExitCode::FAILURE,
            }
        }
        None => match asn::tui::run(config, client).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => fail(&err, 1),
        },
        Some(Command::Task { gid, json }) => match core::task_detail(&client, &gid).await {
            Ok(detail) if json => {
                println!("{}", asn::cli::task::render_json(&detail));
                ExitCode::SUCCESS
            }
            Ok(detail) => {
                print!("{}", asn::cli::task::render_text(&detail, unix_now()));
                ExitCode::SUCCESS
            }
            Err(err) => fail(&err, err.exit_code()),
        },
        Some(Command::Comment {
            gid,
            message,
            stdin,
        }) => {
            let text = match (message, stdin) {
                (Some(text), _) => Some(text),
                (None, Some(_)) => {
                    let mut text = String::new();
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
                        .ok()
                        .map(|_| text)
                }
                (None, None) => {
                    let editor = asn::editor::editor_command(|key| std::env::var(key).ok());
                    asn::editor::edit(&editor, "").ok().flatten()
                }
            };
            let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
                eprintln!("Empty comment; nothing posted");
                return ExitCode::FAILURE;
            };
            match core::comment(&client, &gid, &text).await {
                Ok(story) => {
                    println!("{}", story.gid);
                    ExitCode::SUCCESS
                }
                Err(err) => fail(&err, err.exit_code()),
            }
        }
        Some(Command::Project {
            command:
                ProjectCommand::Add {
                    task,
                    project,
                    section,
                },
        }) => match core::add_to_project(&client, &task, &project, section.as_deref()).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => fail(&err, err.exit_code()),
        },
        Some(Command::Desc {
            gid,
            file,
            stdin,
            force,
        }) => edit_description(&client, &gid, file, stdin.is_some(), force).await,
    }
}

fn fail(err: &dyn std::fmt::Display, code: i32) -> ExitCode {
    eprintln!("{err}");
    ExitCode::from(code as u8)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// `asn desc`: replace a task's description with plain text from a file,
/// stdin or `$EDITOR`. Refuses formatted descriptions without `--force`, and
/// never overwrites a task that changed meanwhile.
async fn edit_description(
    client: &Client,
    gid: &str,
    file: Option<std::path::PathBuf>,
    from_stdin: bool,
    force: bool,
) -> ExitCode {
    let task = match client.task(gid).await {
        Ok(task) => task,
        Err(err) => return fail(&err, err.exit_code()),
    };
    if !force && task.html_notes.as_deref().is_some_and(core::has_formatting) {
        eprintln!(
            "This description has formatting (links, lists, …) that a plain-text save removes; use --force to replace it anyway"
        );
        return ExitCode::FAILURE;
    }
    let text = if let Some(path) = file {
        std::fs::read_to_string(&path).ok()
    } else if from_stdin {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
            .ok()
            .map(|_| text)
    } else {
        let editor = asn::editor::editor_command(|key| std::env::var(key).ok());
        asn::editor::edit(&editor, &task.notes).ok().flatten()
    };
    let Some(text) = text else {
        eprintln!("No description given; nothing saved");
        return ExitCode::FAILURE;
    };
    if text.trim_end() == task.notes.trim_end() {
        eprintln!("No changes");
        return ExitCode::SUCCESS;
    }
    match core::set_notes(client, gid, &text, task.modified_at.as_deref()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            match asn::editor::save_recovery(&format!("description-{gid}"), &text) {
                Ok(path) => eprintln!("{err}; your text is in {}", path.display()),
                Err(_) => eprintln!("{err}"),
            }
            match err {
                core::SaveNotesError::Api(api) => ExitCode::from(api.exit_code() as u8),
                core::SaveNotesError::Conflict => ExitCode::FAILURE,
            }
        }
    }
}
