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
            match core::mark_read(&client, &config.workspace, &gids, !unread).await {
                Ok(failed) if failed.is_empty() => ExitCode::SUCCESS,
                Ok(failed) => {
                    for gid in failed {
                        eprintln!("Asana could not update notification {gid}");
                    }
                    ExitCode::FAILURE
                }
                Err(err) => fail(&err, err.exit_code()),
            }
        }
        None => match asn::tui::run(config, client).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => fail(&err, 1),
        },
        other => {
            let what = match other {
                Some(Command::Task { .. }) => "`asn task`",
                Some(Command::Comment { .. }) => "`asn comment`",
                Some(Command::Desc { .. }) => "`asn desc`",
                Some(Command::Project { .. }) => "`asn project`",
                None | Some(Command::Inbox { .. } | Command::Read { .. }) => unreachable!(),
            };
            eprintln!("{what} is not implemented yet");
            ExitCode::FAILURE
        }
    }
}

fn fail(err: &dyn std::fmt::Display, code: i32) -> ExitCode {
    eprintln!("{err}");
    ExitCode::from(code as u8)
}
