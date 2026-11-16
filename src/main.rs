use std::process::ExitCode;

use asn::config::Config;
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

fn main() -> ExitCode {
    let cli = Cli::parse();

    let _config = match Config::from_env(|key| std::env::var(key).ok()) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(err.exit_code() as u8);
        }
    };

    let what = match cli.command {
        None => "the TUI",
        Some(Command::Inbox { .. }) => "`asn inbox`",
        Some(Command::Read { .. }) => "`asn read`",
        Some(Command::Task { .. }) => "`asn task`",
        Some(Command::Comment { .. }) => "`asn comment`",
        Some(Command::Desc { .. }) => "`asn desc`",
        Some(Command::Project { .. }) => "`asn project`",
    };
    eprintln!("{what} is not implemented yet");
    ExitCode::FAILURE
}
