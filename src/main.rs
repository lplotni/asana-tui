use std::process::ExitCode;

use asn::api::{ASANA_API_URL, Client};
use asn::cli::inbox;
use asn::config::{self, Config, ConfigError, FileConfig};
use asn::core;
use asn::setup;
use clap::{Parser, Subcommand};

/// Asana Inbox and project boards in your terminal. Runs the TUI when no
/// subcommand is given.
#[derive(Parser)]
#[command(name = "asn", version, args_conflicts_with_subcommands = true)]
struct Cli {
    /// Open the TUI on this project's board
    #[arg(long, value_name = "PROJECT_GID", value_parser = numeric_gid)]
    project: Option<String>,
    #[command(subcommand)]
    command: Option<Command>,
}

/// A GID given on the command line: digits only, checked before any request.
fn numeric_gid(value: &str) -> Result<String, String> {
    if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
        Ok(value.to_owned())
    } else {
        Err(format!("must be a numeric GID, got \"{value}\""))
    }
}

#[derive(Subcommand)]
enum Command {
    /// Set up ~/.config/asn/config.toml: your token command and workspace
    Init,
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
    /// Show a task with its comments, or complete or assign it
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Task {
        #[arg(value_name = "TASK_GID", required = true)]
        gid: Option<String>,
        /// Print JSON
        #[arg(long)]
        json: bool,
        /// Print only a short summary: where it is, who has it, and counts
        #[arg(long, conflicts_with = "json")]
        brief: bool,
        #[command(subcommand)]
        command: Option<TaskCommand>,
    },
    /// Add a comment to a task (opens $EDITOR without -m or -); @<gid> mentions
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
    /// Replace a task's description (opens $EDITOR without --file or -); @<gid> mentions
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
    /// Find tasks in the workspace (ASN_SEARCH picks the API)
    Search {
        /// Words to search for
        #[arg(required = true, value_name = "QUERY")]
        query: Vec<String>,
        /// Print JSON
        #[arg(long)]
        json: bool,
    },
    /// Find people in the workspace (gid and name), e.g. for @<gid> mentions
    Users {
        /// Part of a name
        #[arg(required = true, value_name = "QUERY")]
        query: Vec<String>,
    },
    /// Find projects in the workspace (gid and name), e.g. for `asn board`
    Projects {
        /// Part of a name
        #[arg(required = true, value_name = "QUERY")]
        query: Vec<String>,
    },
    /// List a project's open tasks by section, in board order
    Board {
        #[arg(value_name = "PROJECT_GID")]
        project: String,
        /// Print JSON
        #[arg(long)]
        json: bool,
    },
    /// Manage a task's projects
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
}

#[derive(Subcommand)]
enum TaskCommand {
    /// Mark a task complete
    Done {
        #[arg(value_name = "TASK_GID")]
        gid: String,
        /// Mark it incomplete again instead
        #[arg(long)]
        undo: bool,
    },
    /// Assign a task; prints whom it's assigned to now
    Assign {
        #[arg(value_name = "TASK_GID")]
        gid: String,
        /// A user gid (see `asn users`), `me`, or `none` to unassign
        #[arg(value_name = "USER_GID|me|none")]
        assignee: String,
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
    /// Move a task to another section of a project it's in
    Move {
        #[arg(value_name = "TASK_GID")]
        task: String,
        #[arg(value_name = "SECTION_GID")]
        section: String,
    },
    /// List a project's sections (gid and name)
    Sections {
        #[arg(value_name = "PROJECT_GID")]
        project: String,
    },
    /// Create an unassigned task in a project and print its gid
    New {
        #[arg(value_name = "PROJECT_GID")]
        project: String,
        /// Put it in this section (at the top)
        #[arg(long, value_name = "SECTION_GID")]
        section: Option<String>,
        /// The task's name
        #[arg(required = true, value_name = "NAME")]
        name: Vec<String>,
    },
}

/// Default for `asn inbox --limit`.
const DEFAULT_INBOX_LIMIT: u32 = 50;

fn is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// The setup interview on this terminal, writing the config file.
async fn init() -> Result<setup::Outcome, ExitCode> {
    if !is_terminal() {
        return Err(fail(&"asn init needs a terminal", 1));
    }
    let env = |key: &str| std::env::var(key).ok();
    let Some(path) = config::file_path(env) else {
        return Err(fail(
            &"Can't tell where the config file goes: set HOME or XDG_CONFIG_HOME",
            1,
        ));
    };
    let mut stdout = std::io::stdout();
    setup::run(
        &path,
        &mut std::io::stdin().lock(),
        &mut stdout,
        env,
        ASANA_API_URL,
    )
    .await
    .map_err(|err| fail(&err, 1))
}

/// The settings from the environment, then the config file (spec F22).
fn load_config() -> Result<Config, ConfigError> {
    let env = |key: &str| std::env::var(key).ok();
    let file = match config::file_path(env) {
        Some(path) => FileConfig::read(&path)?,
        None => FileConfig::default(),
    };
    Config::resolve(env, &file)
}

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

    // `asn init`, or the TUI's first run without a config file (spec F22).
    // CLI subcommands never prompt.
    let env = |key: &str| std::env::var(key).ok();
    let first_run = cli.command.is_none()
        && is_terminal()
        && config::file_path(env).is_some_and(|path| !path.exists());
    if matches!(cli.command, Some(Command::Init)) || first_run {
        match init().await {
            Ok(_) if !first_run => return ExitCode::SUCCESS,
            Ok(_) => {}
            Err(code) => return code,
        }
    }

    let config = match load_config() {
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
        Some(Command::Init) => unreachable!("handled before the config is loaded"),
        None => match asn::tui::run(config, client, cli.project).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => fail(&err, 1),
        },
        Some(Command::Task {
            command: Some(TaskCommand::Done { gid, undo }),
            ..
        }) => match core::set_completed(&client, &gid, !undo).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => fail(&err, err.exit_code()),
        },
        Some(Command::Task {
            command: Some(TaskCommand::Assign { gid, assignee }),
            ..
        }) => {
            let assignee = (assignee != "none").then_some(assignee.as_str());
            match core::assign(&client, &gid, assignee).await {
                Ok(now) => {
                    let name = now.map_or("unassigned".to_owned(), |a| a.name);
                    println!("{}", asn::cli::terminal_safe(&name));
                    ExitCode::SUCCESS
                }
                Err(err) => fail(&err, err.exit_code()),
            }
        }
        Some(Command::Task {
            gid: Some(gid),
            json,
            brief,
            command: None,
        }) => match core::task_detail(&client, &gid).await {
            Ok(detail) if json => {
                println!("{}", asn::cli::task::render_json(&detail));
                ExitCode::SUCCESS
            }
            Ok(detail) if brief => {
                print!("{}", asn::cli::task::render_brief(&detail, unix_now()));
                ExitCode::SUCCESS
            }
            Ok(detail) => {
                print!("{}", asn::cli::task::render_text(&detail, unix_now()));
                ExitCode::SUCCESS
            }
            Err(err) => fail(&err, err.exit_code()),
        },
        // Clap requires the gid when there's no subcommand.
        Some(Command::Task { .. }) => fail(&"a task gid is required", 1),
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
            let mentions = core::mention::gid_mentions(&text);
            match core::comment(&client, &gid, &text, &mentions).await {
                Ok(story) => {
                    println!("{}", story.gid);
                    ExitCode::SUCCESS
                }
                Err(err) => fail(&err, err.exit_code()),
            }
        }
        Some(Command::Search { query, json }) => {
            let query = query.join(" ");
            match core::search_tasks(&client, &config.workspace, &query, config.search).await {
                Ok(tasks) if json => println!("{}", asn::cli::search::render_json(&tasks)),
                Ok(tasks) => print!("{}", asn::cli::search::render_text(&tasks)),
                Err(err) => return fail(&err, err.exit_code()),
            }
            ExitCode::SUCCESS
        }
        Some(Command::Users { query }) => {
            match core::search_users(&client, &config.workspace, &query.join(" ")).await {
                Ok(people) => {
                    print!("{}", asn::cli::render_named(&people));
                    ExitCode::SUCCESS
                }
                Err(err) => fail(&err, err.exit_code()),
            }
        }
        Some(Command::Projects { query }) => {
            match core::search_projects(&client, &config.workspace, &query.join(" ")).await {
                Ok(projects) => {
                    print!("{}", asn::cli::render_named(&projects));
                    ExitCode::SUCCESS
                }
                Err(err) => fail(&err, err.exit_code()),
            }
        }
        Some(Command::Board { project, json }) => match core::board(&client, &project).await {
            Ok(board) if json => {
                println!("{}", asn::cli::board::render_json(&board));
                ExitCode::SUCCESS
            }
            Ok(board) => {
                print!("{}", asn::cli::board::render_text(&board));
                ExitCode::SUCCESS
            }
            Err(err) => fail(&err, err.exit_code()),
        },
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
        Some(Command::Project {
            command: ProjectCommand::Move { task, section },
        }) => match core::move_to_section(&client, &task, &section).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => fail(&err, err.exit_code()),
        },
        Some(Command::Project {
            command: ProjectCommand::Sections { project },
        }) => match core::sections(&client, &project).await {
            Ok(sections) => {
                print!("{}", asn::cli::render_named(&sections));
                ExitCode::SUCCESS
            }
            Err(err) => fail(&err, err.exit_code()),
        },
        Some(Command::Project {
            command:
                ProjectCommand::New {
                    project,
                    section,
                    name,
                },
        }) => {
            let name = name.join(" ");
            match core::create_task(&client, &project, section.as_deref(), &name).await {
                Ok(created) => {
                    println!("{}", created.task.gid);
                    match created.section_error {
                        None => ExitCode::SUCCESS,
                        Some(err) => fail(
                            &format!("Created, but not put in the section: {err}"),
                            err.exit_code(),
                        ),
                    }
                }
                Err(err) => fail(&err, err.exit_code()),
            }
        }
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
    let mentions = core::mention::gid_mentions(&text);
    match core::set_notes(client, gid, &text, &mentions, task.modified_at.as_deref()).await {
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
