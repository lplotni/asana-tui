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

use crate::api::Client;
use crate::config::Config;
use crate::core;
use app::{App, Effect, Msg};
use theme::Theme;

/// Inbox items loaded on start.
const INBOX_LIMIT: usize = 200;

/// Runs the TUI until the user quits. The theme (including the terminal color
/// query) is resolved before the terminal enters raw mode.
pub async fn run(config: Config, client: Client) -> Result<(), Box<dyn std::error::Error>> {
    let theme = Theme::load(|key| std::env::var(key).ok())?;
    let client = Arc::new(client);
    let workspace: Arc<str> = config.workspace.into();
    let (tx, mut rx) = unbounded_channel();

    // ratatui::init enters raw mode and the alternate screen, and installs a
    // panic hook that restores the terminal before the panic is printed.
    let mut terminal = ratatui::init();
    spawn_input_reader(tx.clone());

    let (mut app, effects) = App::new();
    app.update(Msg::Resize {
        list_rows: ui::list_rows(terminal.size()?.into()),
    });
    run_effects(effects, &client, &workspace, &tx);

    let result = loop {
        if let Err(err) = terminal.draw(|frame| ui::render(frame, &app, &theme, now())) {
            break Err(err.into());
        }
        let Some(msg) = rx.recv().await else {
            break Ok(());
        };
        let effects = app.update(msg);
        run_effects(effects, &client, &workspace, &tx);
        if app.should_quit {
            break Ok(());
        }
    };

    ratatui::restore();
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
) {
    for effect in effects {
        let (client, workspace, tx) = (client.clone(), workspace.clone(), tx.clone());
        match effect {
            Effect::LoadInbox => {
                tokio::spawn(async move {
                    let result = core::inbox(&client, &workspace, INBOX_LIMIT, false)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Msg::InboxLoaded(result));
                });
            }
        }
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
