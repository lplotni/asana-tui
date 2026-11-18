# Implementation Plan: asn v0.2 (Board view)

Source of truth for behavior: [`docs/SPEC-board.md`](../docs/SPEC-board.md) (F9–F17, F22–F23) and, for Phase 6, [`docs/SPEC-skill.md`](../docs/SPEC-skill.md) (F18–F21). Tasks with full acceptance criteria are in [`tasks/todo.md`](todo.md). The v0.1 plan is archived in [`tasks/v0.1/`](v0.1/plan.md).

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

### Phase 3b: New task (F14, added after Checkpoint C)
- [x] T11: `asn project new` (create in project, then section)
- [x] T12: `Tab q` on the board (name prompt, reload, select the new card)

### Phase 3c: @-mentions (F15)
- [x] T13: Mentions in the core and CLI (`@<gid>`, `asn users`, rich-text writes)
- [x] Checkpoint M1: the user verifies a mention and a line break on real Asana
- [x] T15: Show mentions as @Name when reading (comments and descriptions)
- [x] T14: `@` in the editor (mention picker, recorded mentions, drafts)

### Phase 3d: Complete and assign (F16, F17)
- [x] T16: `asn task done` and `asn task assign` (core and CLI)
- [x] T17: `d` completes, with undo (board cards leave at once)
- [x] T18: `Tab a` assigns ("Me" and "Unassigned" first, then people search)

### Phase 4: Ship
- [x] T9: Help, README and spec updates
- [x] T10: Release v0.2.0 (ask first)

### Phase 5: Config file and completion animation (F22, F23; ADR-010)
- [x] T19: Spec, ADR-010 and tasks
- [x] T20: Read `~/.config/asn/config.toml` (env over file over default)
- [x] T21: `asn init` interview, and first-run setup in the TUI
- [x] T22: Completion animation on board cards (frame timer only while animating)
- [x] T23: Docs

### Phase 6: asn in Claude Code (F18–F21, see below)

#### 6a: Receipt and a first skill (de-risk the model, C1/C4)
- [x] T24: `asn task <gid> --brief` read receipt (F18)
- [x] T25: The `asn` skill, v1 over GIDs and the existing lookup commands (F20)
- [x] T31: `asn projects <query>`, pulled forward from T28 (Checkpoint S1 finding: the skill could not find a project by name)

#### Checkpoint S1: does the habit stick?
- [ ] The user works with the skill for a day: count receipt and confirmation misses (C1), and note which lookups Claude gets wrong (input for C2)
- [ ] Human review: go on with names (6b), change the skill, or stop (C4)

#### 6b: Names where GIDs are taken (F19)
- [ ] T26: Name resolution in `core`, and users by name in `asn task assign`
- [ ] T27: Sections by name in `asn project move`
- [ ] T28: Projects by name and section names in `project new`/`add`
- [ ] T29: The skill uses names (v2)

#### Checkpoint S2: acceptance
- [ ] The manual acceptance script (T29) passes, including the prompt-injection task
- [ ] Success criteria 1–6 of SPEC-skill.md hold

#### 6c: Ship
- [ ] T30: Docs (F21)

## Phase 6: asn in Claude Code

### Architecture Decisions

- **The skill comes before the names, not after.** The riskiest assumption is the model (does Claude keep to receipt-first and confirm-before-write, C1, and is the skill used at all, C4), not the Rust. So a v1 skill ships right after the receipt, over GIDs and today's lookup commands (`search`, `users`, `project sections`). Checkpoint S1 then shows which lookups actually go wrong, and that drives F19. If the skill isn't used, 6b is cut and T24 still stands on its own for scripts.
- **One resolution primitive.** `core::Resolved { One(Named), Ambiguous(Vec<Named>), None }` and one `resolve(candidates, name)` rule (case-insensitive exact name wins, else a single candidate), used for users, sections and projects. Each kind only supplies its candidates: user typeahead, the sections of the task's or project's projects, project typeahead. The CLI turns `Ambiguous`/`None` into exit 1 with `gid  label` lines on stderr, in one helper.
- **A GID stays a GID.** An argument of 10+ digits (and `me`/`none` for users) skips resolution, so every existing call makes the same requests as before. Today a non-GID argument is sent to Asana and fails, so no working call changes.
- **The receipt is `present/` text.** Like the detail pane, the receipt lines are built in `present/` and rendered plain by `cli/task.rs`, so they go through `terminal_safe` like the rest of `asn task`. It reuses `core::task_detail` (same requests as `asn task`; spec open question 3 keeps the counts unless the user decides otherwise).
- **The skill is content, guarded by one test.** `skills/asn/SKILL.md` can't be unit-tested, but a `tests/cli.rs` test runs `asn <subcommand path> --help` for every `asn …` command in its code blocks, so a renamed subcommand breaks CI rather than the skill.

### Dependency Graph

```
present receipt + `asn task --brief` ......................... T24
  └── skill v1 (GIDs, lookups, receipt + confirm rules) ...... T25
        └── Checkpoint S1 (dogfood a day)
              └── core::Resolved + users by name ............. T26
                    ├── sections by name in `move` ........... T27
                    └── projects by name, `asn projects`,
                        `--section` names in new/add ......... T28
                          └── skill v2 + acceptance script ... T29
                                └── docs ..................... T30
```

T27 and T28 both only need T26 and touch different commands, so they can run in either order. Everything else is sequential.

### Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Claude drifts from receipt-first or confirm-before-write in long sessions (C1) | High (the trust promise breaks) | Found at Checkpoint S1, before 6b. Next step if it drifts: a `PostToolUse` hook on `asn task` that repeats the rule (ask first: it's config). |
| A task description tries to steer Claude (prompt injection) | High | The receipt carries no description text, and every write needs a confirmation. A hostile fixture description is in the T29 acceptance script. |
| The user typeahead doesn't return the exact name in its results | Med | Exact-or-single, never a guess: the worst case is an ambiguity error with candidates. T26 asks for enough results (`count`) to contain an exact match. |
| The plugin manifest format changes | Low | The skill file doesn't depend on it; only `.claude-plugin/` would change. T25 checks the layout against Claude Code's plugin docs. |
| `project move`/`add` now print a line, and a script relied on empty output | Low | Called out in the README (T30). Exit codes don't change. |
| Real names or GIDs end up in the skill's examples (public repo) | Med | Examples use `12000000000000xx` and "Ada Lovelace", like the fixtures. Grep is a T25/T29 verification step. |

### Open Questions (Phase 6)

- ~~Distribution (spec open question 1)~~: decided, a plugin from this repo (2026-11-17).
- ~~Spec open question 3~~: decided, `--brief` keeps the counts (2026-11-17).

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
