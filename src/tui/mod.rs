//! The terminal UI. [`run`] owns the terminal and the event loop; state lives
//! in [`app`], drawing in [`ui`].

pub mod app;
pub mod format;
pub mod keys;
pub mod theme;
pub mod tint;
pub mod ui;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::crossterm::event::{self, Event};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::task::JoinSet;

use crate::api::Client;
use crate::config::Config;
use crate::core;
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
    spawn_input_reader(tx.clone());
    spawn_refresh_timer(tx.clone());

    let (mut app, effects) = App::new();
    app.update(Msg::Resize {
        list_rows: ui::list_rows(terminal.size()?.into()),
    });
    run_effects(effects, &client, &workspace, &tx, &mut writes);

    let result = loop {
        if let Err(err) = terminal.draw(|frame| ui::render(frame, &app, &theme, now())) {
            break Err(err.into());
        }
        let Some(msg) = rx.recv().await else {
            break Ok(());
        };
        let effects = app.update(msg);
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

/// Reads terminal events on a dedicated thread so the UI never waits on them.
fn spawn_input_reader(tx: UnboundedSender<Msg>) {
    std::thread::spawn(move || {
        loop {
            let msg = match event::read() {
                Ok(Event::Key(key)) => Msg::Key(key),
                Ok(Event::Resize(width, height)) => Msg::Resize {
                    list_rows: ui::list_rows(ratatui::layout::Rect::new(0, 0, width, height)),
                },
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
        let is_write = matches!(effect, Effect::MarkRead { .. });
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
