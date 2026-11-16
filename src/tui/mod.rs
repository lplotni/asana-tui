//! The terminal UI. [`run`] owns the terminal and the event loop; state lives
//! in [`app`], drawing in [`ui`].

pub mod app;
pub mod detail;
pub mod format;
pub mod keys;
pub mod picker;
pub mod theme;
pub mod tint;
pub mod ui;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::crossterm::event::{self, Event};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::task::JoinSet;

use crate::api::Client;
use crate::config::Config;
use crate::core;
use crate::editor;
use app::{App, Effect, Msg};
use theme::Theme;

/// How long quitting waits for unfinished writes.
const WRITE_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// How often the Inbox is polled (the API has no push).
const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Runs the TUI until the user quits. The theme (including the terminal color
/// query) is resolved before the terminal enters raw mode.
pub async fn run(config: Config, client: Client) -> Result<(), Box<dyn std::error::Error>> {
    let theme = Theme::load(|key| std::env::var(key).ok())?;
    let client = Arc::new(client);
    let workspace: Arc<str> = config.workspace.into();
    let (tx, mut rx) = unbounded_channel();
    // Writes are awaited on quit so a change made just before `q` isn't lost.
    let mut writes = JoinSet::new();

    // ratatui::init enters raw mode and the alternate screen, and installs a
    // panic hook that restores the terminal before the panic is printed.
    let mut terminal = ratatui::init();
    let input = Arc::new(InputGate::default());
    spawn_input_reader(tx.clone(), input.clone());
    let editor = editor::editor_command(|key| std::env::var(key).ok());
    spawn_refresh_timer(tx.clone());

    let (mut app, effects) = App::new();
    app.update(Msg::Resize(ui::sizes(terminal.size()?.into())));
    run_effects(effects, &client, &workspace, &tx, &mut writes);

    let result = loop {
        if let Err(err) = terminal.draw(|frame| ui::render(frame, &app, &theme, now())) {
            break Err(err.into());
        }
        let Some(msg) = rx.recv().await else {
            break Ok(());
        };
        app.now = now();
        let effects = app.update(msg);
        let (edits, effects): (Vec<_>, Vec<_>) = effects.into_iter().partition(|e| {
            matches!(
                e,
                Effect::EditComment { .. } | Effect::EditDescription { .. }
            )
        });
        for edit in edits {
            match edit {
                Effect::EditComment { task_gid, initial } => {
                    let text = edit_in_terminal(&mut terminal, &input, &editor, initial).await;
                    let _ = tx.send(Msg::CommentWritten { task_gid, text });
                }
                Effect::EditDescription { task_gid, initial } => {
                    let text = edit_in_terminal(&mut terminal, &input, &editor, initial).await;
                    let _ = tx.send(Msg::DescriptionWritten { task_gid, text });
                }
                _ => {}
            }
        }
        run_effects(effects, &client, &workspace, &tx, &mut writes);
        while writes.try_join_next().is_some() {}
        if app.should_quit {
            break Ok(());
        }
    };

    ratatui::restore();
    let unfinished = finish_writes(&mut writes, WRITE_GRACE).await;
    if unfinished > 0 {
        eprintln!("{unfinished} change(s) may not have reached Asana; check with `asn inbox`");
    }
    result
}

/// Lets the event loop stop the input reader while another program (the
/// editor) owns the terminal, so none of its keystrokes are stolen.
#[derive(Default)]
struct InputGate {
    paused: AtomicBool,
    /// Set by the reader once it has seen `paused` and stopped reading.
    idle: AtomicBool,
}

impl InputGate {
    /// Stops the reader and waits until it confirms (at most 1 s).
    fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while !self.idle.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn resume(&self) {
        self.idle.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
    }
}

/// Suspends the TUI, runs the editor in the foreground with `initial`, and
/// restores the TUI. Returns the text, or `None` if cancelled or it failed.
async fn edit_in_terminal(
    terminal: &mut ratatui::DefaultTerminal,
    input: &InputGate,
    editor: &str,
    initial: String,
) -> Option<String> {
    input.pause();
    ratatui::restore();
    let editor = editor.to_owned();
    let text = tokio::task::spawn_blocking(move || editor::edit(&editor, &initial))
        .await
        .ok()
        .and_then(Result::ok)
        .flatten();
    let _ = ratatui::crossterm::terminal::enable_raw_mode();
    let _ = ratatui::crossterm::execute!(
        std::io::stdout(),
        ratatui::crossterm::terminal::EnterAlternateScreen
    );
    let _ = terminal.clear();
    input.resume();
    text
}

/// Reads terminal events on a dedicated thread so the UI never waits on them.
/// While the gate is paused it doesn't read at all.
fn spawn_input_reader(tx: UnboundedSender<Msg>, gate: Arc<InputGate>) {
    std::thread::spawn(move || {
        loop {
            if gate.paused.load(Ordering::SeqCst) {
                gate.idle.store(true, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            match event::poll(std::time::Duration::from_millis(50)) {
                Ok(true) if !gate.paused.load(Ordering::SeqCst) => {}
                Ok(_) => continue,
                Err(_) => break,
            }
            let msg = match event::read() {
                Ok(Event::Key(key)) => Msg::Key(key),
                Ok(Event::Resize(width, height)) => {
                    Msg::Resize(ui::sizes(ratatui::layout::Rect::new(0, 0, width, height)))
                }
                Ok(_) => continue,
                Err(_) => break,
            };
            if tx.send(msg).is_err() {
                break;
            }
        }
    });
}

fn run_effects(
    effects: Vec<Effect>,
    client: &Arc<Client>,
    workspace: &Arc<str>,
    tx: &UnboundedSender<Msg>,
    writes: &mut JoinSet<()>,
) {
    for effect in effects {
        let (client, workspace, tx) = (client.clone(), workspace.clone(), tx.clone());
        let is_write = matches!(
            effect,
            Effect::MarkRead { .. }
                | Effect::PostComment { .. }
                | Effect::AddToProject { .. }
                | Effect::SaveNotes { .. }
        );
        let task = async move {
            let msg = match effect {
                Effect::LoadInbox { target } => Msg::InboxLoaded(
                    core::inbox_pages(&client, &workspace, target, None, |_| true)
                        .await
                        .map_err(|e| e.to_string()),
                ),
                Effect::LoadMore { offset } => Msg::MoreLoaded(
                    core::inbox_pages(&client, &workspace, 1, Some(&offset), |_| true)
                        .await
                        .map_err(|e| e.to_string()),
                ),
                Effect::ScheduleDetail { gid } => {
                    tokio::time::sleep(app::DETAIL_DWELL).await;
                    Msg::DetailDue { gid }
                }
                Effect::LoadDetail { gid } => {
                    let result = core::task_detail(&client, &gid)
                        .await
                        .map(Box::new)
                        .map_err(|e| e.to_string());
                    Msg::DetailLoaded { gid, result }
                }
                Effect::OpenUrl { url } => {
                    open_in_browser(&url);
                    return;
                }
                Effect::PostComment {
                    task_gid,
                    local_gid,
                    text,
                } => {
                    let result = core::comment(&client, &task_gid, &text)
                        .await
                        .map(Box::new)
                        .map_err(|e| e.to_string());
                    Msg::CommentPosted {
                        task_gid,
                        local_gid,
                        result,
                    }
                }
                // Needs the terminal; handled by the event loop.
                Effect::EditComment { .. } | Effect::EditDescription { .. } => return,
                Effect::SaveNotes {
                    task_gid,
                    text,
                    expected_modified_at,
                } => {
                    let result =
                        core::set_notes(&client, &task_gid, &text, expected_modified_at.as_deref())
                            .await
                            .map_err(|err| app::SaveFailure {
                                message: err.to_string(),
                                // Never lose what the user wrote.
                                kept_at: editor::save_recovery(
                                    &format!("description-{task_gid}"),
                                    &text,
                                )
                                .ok()
                                .map(|path| path.display().to_string()),
                            });
                    Msg::NotesSaved { task_gid, result }
                }
                Effect::ScheduleSearch { seq } => {
                    tokio::time::sleep(picker::SEARCH_DEBOUNCE).await;
                    Msg::SearchDue { seq }
                }
                Effect::SearchProjects { query, seq } => Msg::ProjectsFound {
                    seq,
                    result: core::search_projects(&client, &workspace, &query)
                        .await
                        .map_err(|e| e.to_string()),
                },
                Effect::LoadSections { project_gid } => {
                    let result = core::sections(&client, &project_gid)
                        .await
                        .map_err(|e| e.to_string());
                    Msg::SectionsLoaded {
                        project_gid,
                        result,
                    }
                }
                Effect::AddToProject {
                    task_gid,
                    project,
                    section,
                } => {
                    let section_gid = section.as_ref().map(|s| s.gid.as_str());
                    let result =
                        core::add_to_project(&client, &task_gid, &project.gid, section_gid)
                            .await
                            .map_err(|e| e.to_string());
                    Msg::AddedToProject {
                        task_gid,
                        project_gid: project.gid,
                        result,
                    }
                }
                Effect::MarkRead { gids, read } => {
                    let result = core::mark_read(&client, &workspace, &gids, read)
                        .await
                        .map_err(|e| e.to_string());
                    Msg::Marked { gids, read, result }
                }
            };
            let _ = tx.send(msg);
        };
        if is_write {
            writes.spawn(task);
        } else {
            tokio::spawn(task);
        }
    }
}

/// Sends [`Msg::Tick`] every [`REFRESH_INTERVAL`] to poll the Inbox.
fn spawn_refresh_timer(tx: UnboundedSender<Msg>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(REFRESH_INTERVAL);
        interval.tick().await; // The first tick fires immediately.
        loop {
            interval.tick().await;
            if tx.send(Msg::Tick).is_err() {
                break;
            }
        }
    });
}

/// Opens `url` with the system's default handler, without touching the TUI.
fn open_in_browser(url: &str) {
    if let Some(mut command) = browser_command(url) {
        let _ = command.spawn();
    }
}

/// The command that opens `url`: only `https://` links, and without the
/// Asana token in the child's environment.
fn browser_command(url: &str) -> Option<std::process::Command> {
    if !url.starts_with("https://") {
        return None;
    }
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut command = std::process::Command::new(opener);
    command
        .arg(url)
        .env_remove("ASANA_TOKEN")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    Some(command)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Waits up to `timeout` for unfinished writes. Returns how many didn't finish.
async fn finish_writes(writes: &mut JoinSet<()>, timeout: std::time::Duration) -> usize {
    let all_done = async { while writes.join_next().await.is_some() {} };
    let _ = tokio::time::timeout(timeout, all_done).await;
    writes.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[test]
    fn the_browser_never_receives_the_asana_token() {
        let command = browser_command("https://app.asana.com/0/0/1").unwrap();

        let removed: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_owned())
            .collect();
        assert!(removed.iter().any(|k| k == "ASANA_TOKEN"), "{removed:?}");
    }

    #[test]
    fn only_https_links_are_opened() {
        assert!(browser_command("file:///etc/passwd").is_none());
        assert!(browser_command("javascript:alert(1)").is_none());
        assert!(browser_command("https://app.asana.com/0/0/1").is_some());
    }

    #[tokio::test]
    async fn quitting_waits_for_a_write_that_is_still_in_flight() {
        let landed = Arc::new(AtomicBool::new(false));
        let mut writes = JoinSet::new();
        let flag = landed.clone();
        writes.spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            flag.store(true, Ordering::SeqCst);
        });

        let unfinished = finish_writes(&mut writes, Duration::from_secs(5)).await;

        assert_eq!(unfinished, 0);
        assert!(landed.load(Ordering::SeqCst), "the write was dropped");
    }

    #[tokio::test]
    async fn quitting_gives_up_after_the_timeout_and_reports_what_is_left() {
        let mut writes = JoinSet::new();
        writes.spawn(async { tokio::time::sleep(Duration::from_secs(60)).await });

        let unfinished = finish_writes(&mut writes, Duration::from_millis(50)).await;

        assert_eq!(unfinished, 1);
    }
}
