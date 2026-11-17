# ADR-006: Write in an in-app editor overlay, with `$EDITOR` as an escape hatch

## Status
Accepted. It supersedes the `$EDITOR`-only design in [SPEC.md](../SPEC.md) (Principle 3, F3/F4).

## Date
2026-11-17

## Context
v0.1 wrote comments and descriptions by suspending the TUI and opening `$EDITOR`. That gives full vim, but it was too heavy for short replies, and leaving the screen loses context. Later features (@-mentions) also need to react to what's typed.

## Decision
- `c` and `Tab e` open a centred editor overlay (`ratatui-textarea`) that soft-wraps long lines and grows with the terminal.
- `Ctrl-s` posts or saves, `Esc` cancels (a comment is kept as a draft), and `Ctrl-o` continues the same text in `$EDITOR`.
- The CLI keeps `$EDITOR` when no text is given.

## Alternatives Considered
- **`$EDITOR` only.** Full editing power, but slow for one-liners, and no in-editor features such as the `@` picker.
- **A single-line inline prompt for comments.** Too limited for descriptions.

## Consequences
- In-editor features are possible: `@` opens the mention picker over the editor.
- Editor state has to travel with the text: mentions survive drafts, failed posts and the `Ctrl-o` round trip.
- The overlay's size rules are pure functions (`ui::editor_area` / `ui::picker_area`) with tests.
