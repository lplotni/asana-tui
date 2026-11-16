# Implementation Plan: asn v0.1 (Inbox MVP)

Source of truth for behavior: [`docs/SPEC.md`](../docs/SPEC.md). Tasks with full acceptance criteria are in [`tasks/todo.md`](todo.md).

## Overview

Build `asn`, a Rust single-binary Asana client. With no arguments it opens a ratatui TUI on the user's Asana Inbox. From there the user can open a task, comment on it, edit its description and add it to projects, using vim keys plus Asana web aliases, with a theme that follows their terminal colors. Every action also exists as a scriptable CLI subcommand with `--json` output.

## Architecture Decisions

- **One crate, a library plus one binary.** `src/lib.rs` exposes `api`, `core`, `cli` and `tui`, and `src/main.rs` is a thin clap entry point. The library split makes integration tests possible without a Cargo workspace. We'll split into multiple crates only when a second consumer needs it.
- **The CLI comes before the TUI for every operation.** Each slice lands first as `core::*` plus an `asn <cmd>`, and then gets a key binding. This keeps principle 2 ("scriptable core") enforced by the order of work, and gives each slice a way to test against real Asana before any UI exists.
- **The TUI state machine is pure.** `tui/app.rs` takes `(App, Action)` and returns `(App, Vec<Effect>)`, with no I/O. The event loop runs effects (API calls, launching the editor) as tokio tasks and feeds results back as actions. Nearly all TUI behavior is unit-testable without a terminal.
- **Fixtures come from real responses.** Task 0 captures anonymized Asana JSON into `tests/fixtures/`. All `wiremock` tests use those fixtures, so parsing is tested against what Asana actually returns, not what we guess it returns.
- **Theme and keymap are data, built before the first screen.** No rendering code exists before `Theme` and the key table, so no hard-coded colors or ad-hoc key handling ever land.
- **Terminal safety from the first TUI commit.** Raw mode and the alternate screen are always restored, including on panic (a panic hook) and when the editor is launched (suspend/resume).

## Dependency Graph

```
T0 API spike + fixtures
 └── T1 scaffold + env config
      ├── T2 api client + `asn inbox` ──── T3 `asn read`
      ├── T4 theme loader ── T5 terminal tints
      └── T6 keymap
                │
   T2,T3,T4,T6 ─┴─ T7 TUI inbox list ── T8 TUI read actions, filter, polling
                                              │
                         T9 task detail (CLI + pane)
                          ├── T10 editor + comment ── T11 edit description
                          └── T12 add-to-project picker
                                              │
                         T13 search + command line   T14 help overlay + narrow layout
                                              │
                                    T15 CI, release, README
```

**Parallelizable after T1:** {T2→T3}, {T4→T5} and {T6} are independent of each other. After T9: {T10→T11} and {T12} are independent. T13 and T14 are independent.

## Task List

### Phase 0: De-risk (no Rust)
- [x] T0: Validate Notifications API access and capture fixtures

### Phase 1: Scriptable core
- [x] T1: Cargo scaffold, env config, error and exit-code contract
- [x] T2: API client and `asn inbox`
- [x] T3: `asn read` (mark read/unread, batched)

### Checkpoint A: core works against real Asana
- [x] `asn inbox --unread --json | jq '.[].work_objects[0].name'` lists real items
- [x] `asn read <gid>` shows as read in the web Inbox, and `--unread` reverts it
- [x] `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` are clean

### Phase 2: TUI foundation
- [x] T4: Theme loader (roles, built-ins, user file, NO_COLOR)
- [x] T5: Terminal color query and tints
- [x] T6: Keymap table and key parser
- [x] T7: TUI Inbox list with navigation
- [x] T8: TUI read actions, visual mode, filter, polling, pagination

### Checkpoint B: daily triage possible
- [ ] Open `asn` and get through a real Inbox to empty using only `j`/`k`/`e`/`V`
- [ ] Looks right in Rex with `terminal`, `dark`, `light` and `NO_COLOR`
- [ ] The terminal is always restored (`q`, Ctrl-C, forced panic)
- [ ] **Human review before Phase 3**

### Phase 3: Act on tasks
- [x] T9: Task detail (`asn task` and the detail pane)
- [x] T10: `$EDITOR` round-trip and add comment
- [x] T11: Edit description with rich-text and concurrency guards
- [x] T12: Add to project (fuzzy project picker, then section picker)

### Checkpoint C: feature complete
- [ ] The full spec flow works: Inbox, open, comment, edit description, add to project, mark read
- [ ] Every TUI action has a matching CLI subcommand that passes its tests
- [ ] **Human review before Phase 4**

### Phase 4: Finish and ship
- [x] T13: `/` search and the `:` command line
- [x] T14: `?` help overlay and the narrow (<100 cols) layout
- [ ] T15: CI, release binaries, README

### Checkpoint D: v0.1 released
- [ ] Every acceptance criterion in the spec is met
- [ ] Tagged `v0.1.0`, binaries attached, install instructions verified on a clean machine
- [ ] Handed to 3 teammates for dogfooding (the spec's success metric)

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| A PAT can't read `/users/me/notifications` on our plan | High: the core feature is dead | T0 checks this before any code. If it fails, stop and rethink. |
| Fixtures don't cover real-world variety (unknown `story_resource_subtype`, non-task `work_objects`, `null` story or actor) | Med: parse errors crash the Inbox | Capture about 200 notifications from several users in T0. Unknown enum values are kept as strings, and there's a test for a fixture with all fields `null`. |
| Editing `notes` strips formatting | Med: data loss in user content | T11 adds the rich-text guard and confirmation, and stops for a decision if T0 shows most descriptions are rich. |
| Terminal left broken after a panic or editor crash | Med: users won't trust the tool | A panic hook and a restore guard from T7. T10 tests suspend/resume, including the editor exiting non-zero. |
| Rex doesn't answer OSC 10/11 | Low: falls back to the palette-only theme | Checked in T0. The fallback is designed to look intentional (T5). |
| Rate limit (~150/min on the free plan) during fast cursor movement | Low | 250 ms dwell debounce and a per-task cache (T9), plus Retry-After handling (T2). |
| The Asana alias keys differ from the current web app | Low | Checked in T0, and all keys live in one table (T6). |

## Open Questions

- **Should a mutation mark the notification read?** (spec Q3): the default is no, and it can be decided while dogfooding.
- ~~Publishing~~ **Decided:** repo `github.com/lplotni/asana-tui`; v0.1 ships as GitHub Releases (macOS arm64/x86_64, Linux) plus a Homebrew formula in the same repo (`brew tap lplotni/asn https://github.com/lplotni/asana-tui && brew install lplotni/asn/asn`). No crates.io.
- ~~Rich descriptions~~ **Decided (T11):** edit plain text; a description with formatting asks for confirmation before it's replaced.
