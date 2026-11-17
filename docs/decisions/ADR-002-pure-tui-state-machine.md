# ADR-002: The TUI is a pure state machine; effects run in the event loop

## Status
Accepted

## Date
2026-11-16 (recorded 2026-11-17)

## Context
The TUI must never block on the network (Asana can be slow or rate-limited), and most of its behaviour should be testable without a terminal or a server.

## Decision
- `tui/app.rs` holds all state. `App::update(Msg) -> Vec<Effect>` changes state and returns work to do, and performs no I/O.
- `tui/mod.rs` owns the terminal and the event loop. It runs effects (API calls, the external editor, opening a URL) as tokio tasks and feeds their results back as `Msg`s.
- Larger pure pieces get their own module in the style of `tui/picker.rs` and `tui/board.rs`, and `app.rs` only dispatches to them.
- `tui/ui.rs` only draws, from `&App` and `&Theme`.

## Alternatives Considered
- **Calling the API directly from key handlers (async).** Simpler at first, but it couples state to I/O, makes ordering bugs hard to test, and risks blocking the UI.
- **A TUI framework with its own component model.** More structure than needed, and harder to test as plain values.

## Consequences
- Nearly all behaviour is tested as state transitions in `app_tests.rs`, and rendering with `TestBackend` snapshots.
- Races are handled explicitly as state: search sequence numbers (only the latest query's results count), the board's `reloading` / `reload_queued`, and the 250 ms detail dwell.
- Writes are awaited on quit (up to 5 s), so a change made just before `q` isn't lost.
