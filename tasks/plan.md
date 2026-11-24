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
- [x] Human review: go on with names (6b) — decided by the user, 2026-11-18. Finding so far: project lookup by name was missing (fixed in T31); the day of use continues alongside 6b

#### 6b: Names where GIDs are taken (F19)
- [x] T26: Name resolution in `core`, and users by name in `asn task assign`
- [x] T27: Sections by name in `asn project move`
- [x] T28: Projects by name and section names in `project new`/`add`
- [x] T29: The skill uses names (v2)

#### Checkpoint S2: acceptance
- [x] The manual acceptance script (T29) passes, including the prompt-injection task (the user, 2026-11-19)
- [x] Success criteria 1–6 of SPEC-skill.md hold (6: fmt, clippy and tests are clean locally; CI runs on push)

#### 6c: Ship
- [x] T30: Docs (F21)

### Phase 7: Swimlanes and single-select fields (F24–F27, see below; before v0.3.0)

#### 7a: Data and CLI first (ADR-001)
- [x] T32: Spec F24–F27 in SPEC-board.md
- [x] T33: Single-select fields on the board (project field settings, values on cards)
- [x] T34: `asn board <gid> --lanes <field>` (pure lane grouping)
- [x] T35: `asn task field` and `asn project fields` (set a single-select field by name)

#### Checkpoint L1: data is right
- [x] `asn board --lanes Initiative` matches the web board grouped by Initiative on 1–2 real projects
- [x] `asn task field <task> Initiative <option>` shows up in the web app; clearing it too
- [x] Human review before the TUI (the user, 2026-11-19)

#### 7b: TUI
- [x] T36: Help overlay makes room for new keys (room for three; Phase 7 needs two)
- [x] T37: Pure lane state in `tui/board.rs` (grid cursor, paging, cursor by GID across layouts)
- [x] T38: `v` toggles columns ↔ lanes and draws the grid
- [x] T39: `F` sets a single-select field (field, then option), and the card follows

#### Checkpoint L2: glance and edit work
- [x] The user dogfoods lanes at 80, 120 and 200 columns, with `v` and `F`
- [x] Human review

#### 7c: Ship
- [x] T40: Skill, README, help, screenshot and spec updates
- [x] T41: Release v0.3.0 (ask first)

### Phase 8: Fresh boards (F28–F30, see below)

#### 8a: Measure first (F28)
- [x] T42: Spec F28–F30 and this plan
- [x] T43: API latency probe (`examples/latency.rs`)
- [x] T44: Rex end-to-end harness (`scripts/tui-latency.sh`)

#### Checkpoint F1: baseline and decision
- [x] Both tools ran on the throwaway board against v0.3.0 and HEAD; the baseline is in the spec's Findings from Use
- [x] P1, P2, P3 and P5 are settled. Asana's reads are fresh (P1, P2 failed), events take about 4 s (P3 failed), and P5 is moot
- [x] Decision applied: the bug F28 found (a false "changed on the web" after own writes) becomes F29a and T45. T46 and T47 are dropped. Events were reconsidered and are still not used
- [ ] Human review before building (the user asked for every task in one go; reported with the results)

#### 8b: Own writes (F29a)
- [x] T45: A description save is checked against the description only (F29a)
- ~~T46, T47~~: dropped at Checkpoint F1 (F29's pending changes; Asana's reads are fresh)

#### Checkpoint F2: own writes show and stay
- [x] Second harness run: each write on screen within `confirm-ms` plus one reload, and nothing flicks back (the description included)
- [ ] Human review

#### 8c: Refresh while you're looking (F30)
- [x] T48: The `refresh` setting drives the poll (default 5 s)
- [x] T49: Back off while the terminal isn't in front
- [x] T49a: A load older than an own write isn't shown (found at Checkpoint F3)

#### Checkpoint F3: near-live
- [x] Third run of both tools; a change made with the CLI in another pane shows within the interval plus Asana's lag (0.05–1.2 s); P4 partly (Rex, by sequence; tmux not checked)
- [ ] Human review

#### 8d: Ship
- [x] T50: README, ADR-011 and spec updates
- [ ] T51: Release v0.4.0 (ask first)

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

## Phase 7: Swimlanes and single-select fields

Requested 2026-11-19, before releasing v0.3.0. Decisions from the user: a **grid** (one horizontal lane per option, each with the section columns), lanes by **any single-select field** on the board (`v` cycles columns → each field, Initiative first → columns), and editing **single-select fields only** for now. Initiative is a single-select field in the user's workspace.

### Overview

On a board, `v` switches to swimlanes: one lane per option of a single-select field (in the field's option order), plus a `(no <Field>)` lane, each lane showing the section columns with its cards. `v` again goes to the next single-select field, and back to columns after the last. `F` on a task sets one of its single-select fields from a picker (field, then option or none); on a board the card then moves to its new lane. Both are commands first: `asn board <gid> --lanes <field>` and `asn task field <task> <field> <option|none>`, plus `asn project fields <project>` to list fields and options.

### Architecture Decisions

- **Field definitions come from the project, values from the tasks.** `GET /projects/{gid}/custom_field_settings` (one more request per board load, loaded in the same `try_join!`) gives the board's single-select fields and their options in order, including options no card uses yet. Board tasks add `custom_fields.gid,custom_fields.enum_value.gid` to their `opt_fields`, so the payload grows by a GID per field. Other field types are ignored for lanes.
- **Lanes are pure grouping in `core`, like columns.** `core::lanes(&Board, field_gid) -> Vec<Lane { option: Option<Named>, columns: Vec<Column> }>`: one lane per option that has cards (in option order), then `(no <Field>)` when non-empty; every lane has all the board's columns, so columns line up across lanes. The CLI and the TUI both use it.
- **The lane view is a layout of `BoardView`, not a new screen.** `tui/board.rs` gets `Layout { Columns, Lanes { field } }` and a grid cursor (lane, column, row). The selected task stays keyed by GID, so toggling layouts, reloading and editing a field keep the cursor on the same card. `app.rs` only dispatches `v`/`F` and runs the effects (ADR-002).
- **Editing goes through the task, so it works anywhere.** `F` uses the selected-task-GID seam (board card, its detail, an Inbox task). The task's own `custom_fields` (with `resource_subtype`, `enum_value` and `enum_options` added to `TASK_FIELDS`) list its single-select fields and options, so no project context is needed. Writing is `PUT /tasks/{gid}` `{custom_fields: {<field gid>: <option gid> | null}}`.
- **Names via F19.** `asn task field` resolves the field among the task's single-select fields and the option among that field's enabled options with `resolve::pick`; ambiguity writes nothing and lists candidates, as for sections.
- **Keys:** `v` (view; the v0.2 spec already reserved it for an alternative layout) and `F` (field) are free on the board and in the Inbox. The help overlay is full (it fails its 100×24 test with one more line), so its layout changes first (T36, per CLAUDE.md).
- **No new theme roles.** Lane headers use `title` (like column headers) and the lane separator uses `border`.

### Dependency Graph

```
api: custom_field_settings + enum values on board tasks ......... T33
  ├── core::lanes + `asn board --lanes` ......................... T34
  │     └── tui/board.rs grid state (pure) ...................... T37
  │           └── `v` toggle + grid render ...................... T38
  └── task single-select fields + PUT + `asn task field`,
      `asn project fields` ...................................... T35
            └── `F` picker, reload, card follows ................ T39 (after T38)
help overlay layout ................................................ T36 → T38, T39
                                                    T40 docs/skill → T41 release
```

T34 and T35 are independent after T33, and T36 can be done any time before T38. T37–T39 touch `board.rs`/`app.rs`/`ui.rs` and run in order.

### Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| The help overlay is full | High (the help test fails on the first new key) | T36 put `j / k` and `n / N` on one line each: room for three more entries; `v` and `F` used two, so one is left. A further key needs another layout change (e.g. shorter descriptions or a scrolling overlay). |
| A tall grid in a short terminal: many lanes × 2-line cards | Med | Lanes scroll vertically as one list and keep the selected card visible; empty lanes are hidden; lane height is its tallest column. Checked at Checkpoint L2. |
| `app.rs` (≈2500 lines) grows further | Med | Grid navigation and cursor logic live in `tui/board.rs`; `app.rs` only adds the `v`/`F` dispatch and two effects. |
| Fields on tasks that aren't the project's fields (added from other projects) | Low | Lanes use only the project's single-select fields; `F` uses the task's own fields, so it can still edit those. |
| Two fields or options with the same name | Low | `pick` makes it ambiguous: the CLI lists candidates, the TUI picker shows both. |
| `asn board --json` tasks now include `custom_fields` | Low (additive) | Agreed by the user; Asana's `data` objects stay unchanged otherwise; called out in the README. |
| Real GIDs or names in fixtures | Med (public repo) | New fixtures use `12000000000000xx` and invented field/option names ("Initiative", "Checkout"). |

### Open Questions (Phase 7)

- ~~`asn board --json` gains each task's `custom_fields`~~: agreed by the user (2026-11-19), additive.
- ~~Remember the layout per board for the session~~: yes, like the cursor (the user, 2026-11-19).

## Phase 8: Fresh boards

Requested 2026-11-24. A task created or changed in the TUI can take minutes to show on the board and in the detail, even though every write already starts a reload as soon as Asana confirms it. Decisions from the user:
- everything should be near-live
- the refresh interval is a setting, with a 5 s default, and the docs state its request cost against the free plan's limit (150 a minute) and paid plans' (1,500, Enterprise included)
- the Rex harness is committed
- the probe completes its tasks rather than deleting them

Spec: F28–F30 in [`docs/SPEC-board.md`](../docs/SPEC-board.md).

### Overview

Measure first: an API probe and a Rex harness on a throwaway board. They show how long Asana takes to reflect a write in the board's list read, the single-task read and the events feed, and how long the TUI takes to show it. What that shows decides the next step (F28's decision table). The expected path:
- **F29:** the TUI shows its own writes at once and keeps them through stale reloads, until a load confirms them.
- **F30:** while the terminal is in front, the TUI refreshes every `refresh` seconds (5 by default); in the background, every 60 s.

### Architecture Decisions

- **Checkpoint F1 is a real gate.** If the list read turns out fresh (P1 fails), the delay is a bug in `asn`. Then T45–T47 are replaced by a task that reproduces it in an app test and fixes it. If the single-task read is fresh (P2 fails), T47 is dropped. No F29 code is written before the numbers are in.
- **The probe is an example, not a subcommand.** It's a developer tool, not a user action, so ADR-001's CLI-first rule doesn't apply. It makes its writes and reads through the same `core`/`api` functions as the TUI, so it measures the requests the TUI actually sends.
  - Events are read with a plain `reqwest` request inside the example, so no `api` surface is added for a feature that isn't adopted.
  - `load_config` moves from `main.rs` into `config::load`, so the example loads settings exactly as `asn` does, without a copy.
  - Its pure checks (does this read show the write) get unit tests, and `[[example]] test = true` makes `cargo test` run them in CI.
- **The Rex harness is a shell script next to `scripts/formula.sh`.** It drives `target/release/asn`, and prints that binary's version and build time first, because a stale binary is a known trap (CLAUDE.md). It reads the screen with `rex capture`.
  - Probe names are short and unique (`p<n>-<hhmmss>`), so a 28-cell card doesn't truncate them.
  - Column checks use each column header's horizontal range on the captured screen.
- **Pending changes are a pure module, `tui/pending.rs`** (ADR-002), the way `board.rs` and `picker.rs` are. It generalises what `apply_pending_marks` already does for the Inbox.
  - Entries are keyed by task GID and the thing written. Each keeps the value before and after the write, which is how a load that shows a third value counts as "superseded".
  - An entry is recorded on the **confirming** `Msg`, not when the write is sent. So a failed write leaves nothing, and today's optimistic edits and restores stay as they are.
- **The board goes through `BoardView::replace`.** On a confirmed create or move, and on every `BoardLoaded`, the app applies the pending changes to the board, then hands it to `replace`. Cursor-by-GID, lanes and the held reload during the completion animation (F23) keep working with no new `BoardView` API. A new card is inserted at the top of its section, which is where Asana's `addTask` puts it.
- **No clock in `App`.** Expiry and the background back-off count ticks. The timer ticks every `refresh` seconds, and the app decides whether a tick polls: in the background, every ⌈60 / refresh⌉-th tick. Expiry is ⌈600 / refresh⌉ ticks, so 10 minutes at any interval.
- **Focus reports use crossterm's `EnableFocusChange`.** They're switched off wherever raw mode is restored, including the panic path, so a crash doesn't leave the shell echoing `^[[I`/`^[[O`.
- **No new dependencies, theme roles or keys.** The help overlay is unchanged.

### Dependency Graph

```
T42 spec + plan
  ├── T43 API probe (examples/latency.rs, config::load) ──┐
  └── T44 Rex harness (scripts/tui-latency.sh) ───────────┴── Checkpoint F1 (baseline, decision)
                                                                 └── T45 pending.rs + create, move
                                                                       └── T46 assign, field, complete
                                                                             └── T47 detail side (if P2 lags)
                                                                                   └── Checkpoint F2
                                                                                         └── T48 `refresh` drives the poll
                                                                                               └── T49 focus back-off
                                                                                                     └── Checkpoint F3
                                                                                                           └── T50 docs → T51 release
```

T43 and T44 are independent and can run in parallel. From T45 on, every task edits `app.rs` and `app_tests.rs`, so they run in order.

### Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| The delay is our bug, not Asana's (P1 fails) | High: F29 would treat a symptom | Checkpoint F1 decides before any F29 code; the replacement task reproduces the bug in an app test first. |
| Asana lags longer than 10 minutes, so a pending change expires and the card flicks back | Med | P5: the expiry is checked against F28's maximum, and raised if needed. |
| A pending change hides a real change made by someone else | Med | "Superseded" ends it as soon as a load shows a third value. Create and complete have no third value, so the expiry bounds them. |
| A 5 s refresh hits the free plan's 150 a minute on a large board, or with heavy CLI use alongside | Med | F30's cost table is in the README and the config comment. A 429 is retried once, then that refresh fails with a toast and the next one tries again. `refresh = 10` or more is the documented fix. |
| Focus reporting is left on after a crash | Med | It's turned off in the same cleanup and panic paths as raw mode, and checked by hand at Checkpoint F3. |
| An image that downloads but won't decode is fetched again on every refresh of its shown task: every 5 s instead of 60 s | Low | Oversized files are refused before download (`Content-Length`). If it matters, retry failed images only when a task is newly selected. |
| A 5 s refresh makes a load overlap a write often: a result older than the write undoes what the screen shows (seen: a completed card came back) | High | T49a: loads that may predate an own write aren't shown, and a fresh one follows (`tui/writes.rs`). Writes without an answer are given up after two minutes. |
| A terminal doesn't report focus (P4) | Low | It's treated as in front: it polls at the interval, which costs requests but nothing else. |
| Screen-scraping in the Rex harness is brittle | Med | Short unique names, header-range column checks, a 5-minute timeout per check (`>300`, never a hang), and the API probe as a cross-check. |
| `app.rs` (about 2,750 lines) grows | Med | The logic lives in `pending.rs`; `app.rs` only records entries and applies them at two points. |
| The probe leaves tasks behind, or a real GID lands in the repo | Low | It completes its tasks even after a failure (agreed). GIDs only ever come from the command line, and a grep of the diff is part of each task's verification. |

### Checkpoint F1 outcome (2026-11-24)

F28 overturned the plan's main assumption. Asana's board list showed every write on its first read, 0.4–0.7 s after Asana confirmed it, and the single-task read on its first poll. So the TUI shows its own writes Asana's confirmation time (1.8–4.9 s) plus one reload after the key, on both v0.3.0 and HEAD. A delay of minutes didn't reproduce in a fresh session. What did show up is a bug: a description saved after a move or an assignment on the same task was refused as "changed on the web", because those writes, like comments and completions, change `modified_at` and the cached detail kept the old one. Per F28's decision table:
- **8b becomes T45 (F29a).** At first the plan was for own writes to reload the task's cached detail. That didn't fix it: Asana changes `modified_at` again about 0.65 s after a write. So the description guard now compares the description itself.
- **F29's pending changes (`tui/pending.rs`, T46, T47) are dropped.** The architecture decisions above about them no longer apply. Neither does the tick-counted expiry; the tick-counted back-off (T49) stays.
- **Events:** reconsidered, because they arrive in about 4 s. Still not used: see the spec's Not Doing.
- **New open question:** creating a task in one request saves about 2 s, but puts it at the bottom of its section. That's for the user to decide (spec Open Question 2).

### Open Questions (Phase 8)

- ~~The default interval~~: 5 s, with the cost spelled out (the user, 2026-11-24).
- ~~Commit the Rex harness~~: yes (the user, 2026-11-24).
- ~~Delete or complete the probe tasks~~: complete (the user, 2026-11-24).
- ~~T47 depends on P2~~: dropped at Checkpoint F1 (P2 failed).
- Create in one request, at the bottom of the section instead of the top? Asked with the results (spec Open Question 2).
- The release number: v0.4.0 is assumed (a new setting and new behaviour). Asked at T51.

