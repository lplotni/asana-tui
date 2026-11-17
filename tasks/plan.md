# Implementation Plan: asn v0.2 (Board view)

Source of truth for behavior: [`docs/SPEC-board.md`](../docs/SPEC-board.md) (F9–F13). Tasks with full acceptance criteria are in [`tasks/todo.md`](todo.md). The v0.1 plan is archived in [`tasks/v0.1/`](v0.1/plan.md).

## Overview

Add a columns board for any Asana project as a second top-level screen next to the Inbox. Sections are columns, and open tasks are cards (name, assignee initials, due date). You reach a board with `P` (project switcher), with `b` from an Inbox task, or with `asn --project <gid>`. `asn board <gid> [--json]` prints the same data. The board polls every 60 s, keeps its cursor across refreshes, and supports the existing task actions plus a full-width detail view.

## Architecture Decisions

- **The CLI comes first, as in v0.1.** `api` → `core::board` → `asn board` lands before any TUI code (principle 2). It is also how assumption A1 (board order) gets checked against real projects before columns are drawn.
- **Board state is its own pure module, `tui/board.rs`,** in the style of `tui/picker.rs`. It owns columns, the cursor (column + row), per-column scroll, horizontal paging and cursor-by-GID on reload. It has no I/O and no rendering. `app.rs` (already about 1600 lines) only holds `screen`, the boards opened this session, `recent_projects`, and the dispatch into it. That keeps new logic out of the Inbox code and testable on its own.
- **`App` gets a `Screen { Inbox, Board }`.** The Inbox fields stay where they are. Moving them into a sub-struct would be a large refactor with no behavior gained; the screen enum is enough for two peers.
- **Detail and actions reuse the task-GID seam.** `selected_task_gid()` and `shown_task_name()` (`tui/app.rs:748`) already resolve either the selected notification or `opened: Option<Named>` (a task opened from search). On the board they resolve the selected card. Comment, description, `p`, `m` and `o` then work with no changes, and so do the per-GID detail cache and image loading. Assumption A5 is confirmed by reading the code, so no refactor task comes first.
- **The full-width detail view reuses the narrow-terminal rendering path,** which already draws the task alone when it has focus (snapshot `narrow_terminals_show_only_the_task_when_it_has_focus`).
- **The project switcher is a new picker stage,** reusing the typeahead search, debounce and fzf keys of `Stage::Project`. With an empty query it lists session recents instead of results.
- **Polling:** the existing 60 s `Msg::Tick` loads the Inbox as now, and also the active board when the board screen is visible. No second timer.
- **Card text belongs in `present/`.** Initials and the short due date (with overdue) are pure functions there, next to the detail pane's date formatting, so the CLI and TUI can never diverge.

## Dependency Graph

```
api::project_tasks (paginated)
  └── core::board (group by this project's membership, keep empty sections)
        ├── cli board + `asn board` ............................ T1
        └── tui/board.rs pure state (nav, paging, cursor by GID) T2
              └── Screen::Board + render + `--project` ......... T3
                    ├── polling, R, session cache .............. T4
                    ├── Enter detail + actions on cards ........ T5
                    ├── P switcher + Tab i peers ............... T6
                    │     └── b from Inbox ..................... T7
                    └── / n N jump ............................. T8
                          └── docs, help, README ............... T9 → release T10
```

T1 and T2 are independent (T2 only needs the `core::Board` type, which is defined at the start of T1). Everything after T3 touches `app.rs` and runs sequentially.

## Task List

### Phase 1: Data and de-risk
- [x] T1: `asn board <gid> [--json]` (F13, API surface)
- [x] T2: Pure board state in `tui/board.rs` (F9 navigation, paging, cursor)

### Checkpoint A: data is right
- [x] A1 checked: `asn board --json` matches the web board on 2–3 real projects (or switch to per-section loading)
- [x] A 300-task project loads in < 2 s
- [x] fmt, clippy and tests are clean

### Phase 2: A board you can look at
- [x] T3: Board screen opened with `asn --project` (F9 render, F11 startup)
- [x] T4: Polling and `R` (F9 refresh; the session cache moved to T6)

### Checkpoint B: glance works
- [x] The user dogfoods `asn --project` at 80, 120 and 200 columns (success criterion 6, A2, A3)
- [x] Human review before Phase 3

### Phase 3: Act and navigate
- [x] T5: `Enter` → full-width detail, and actions on cards (F10)
- [x] T6: `P` project switcher, session cache of boards, and Inbox ↔ board peers (F11)
- [x] T7: `b` from an Inbox task (F11)
- [x] T8: `/`, `n`, `N` jump on the board (F12)

### Checkpoint C: feature complete
- [x] Success criteria 1–8 of SPEC-board.md hold
- [x] Human review

### Phase 4: Ship
- [x] T9: Help, README and spec updates
- [ ] T10: Release v0.2.0 (ask first)

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| A1: `/projects/{gid}/tasks` isn't in board order | High (the board looks wrong) | Checked at Checkpoint A, before any TUI work. The fallback (`GET /sections/{gid}/tasks` per section) only changes `core::board`. |
| `app.rs` grows into a second god-object | Med | All board logic lives in `tui/board.rs`. `app.rs` only switches screens and dispatches. |
| Inbox-only keys and focus rules (`e`/`u`/`x`/`V`/`f`, `Esc`, `q`) misbehave on the board | Med | `run()` checks the screen first. Each T3/T5/T6 task has app tests for those keys on the board. |
| Columns too narrow at 80 cols | Med | The one-column fallback is in T3. Checkpoint B is a human dogfood gate before more is built on it. |
| Rate limit with Inbox + board polling | Low | Only the visible board polls, and 1 + ⌈n/100⌉ requests is about 4/min for 300 tasks. The existing 429 `Retry-After` handling applies. |
| Real project names or GIDs leak into fixtures | Med (public repo) | New fixtures are anonymized like the v0.1 ones, and fixture grep is a T1 verification step. |

## Parallelization

T1 and T2 can run in parallel. T3 onward is sequential: each task edits `app.rs`, `keys.rs` and `ui.rs`.

## Open Questions

- `asn --project` takes a GID only in v0.2 (spec open question 1). Name lookup can follow.
