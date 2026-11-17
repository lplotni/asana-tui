# Tasks: asn v0.2 (Board view)

Plan: [`tasks/plan.md`](plan.md). Spec: [`docs/SPEC-board.md`](../docs/SPEC-board.md). Feature IDs (F9–F13) refer to that spec.

**Definition of done (every task):**
- The acceptance criteria are met and verified at runtime.
- New behavior has tests that fail without the change.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` are clean.
- No token, real GID or real project/person name appears in any output, fixture, log or commit.
- `docs/SPEC-board.md` is updated if behavior had to deviate from it.
- One commit per task, directly on `main`.

---

## Phase 1: Data and de-risk

### - [x] T1: `asn board <gid> [--json]`

**Description:** Load a project's open tasks grouped by section, and print them. This adds:
- `api`: a paginated `GET /projects/{gid}/tasks` (`completed_since=now`, `limit=100`, `opt_fields=name,assignee.name,due_on,due_at,memberships.project.gid,memberships.section.gid`), following `next_page` until `null`
- `core::board(client, project_gid) -> Board { sections, tasks }`: sections from the existing `sections()`. Each task is placed by its membership in *this* project. Unknown sections go to a leading "(no section)" group that exists only when non-empty. Empty sections are kept.
- `cli/board.rs`: text and JSON rendering, wired into `main.rs`

**Acceptance criteria:**
- [x] `asn board <gid>` prints each section (`gid  name`) in board order, followed by one indented TSV line per task (`gid  name  assignee  due_on`, with empty fields for missing values), neutralized with `terminal_safe`.
- [x] `asn board <gid> --json` prints `{"sections": [...], "tasks": [...]}` with Asana's `data` objects unchanged. Exit codes follow F7.
- [x] A task in two projects lands only in this project's section. Empty sections appear with no tasks.

**Verification:**
- [x] Tests: `wiremock` test in `tests/api_project.rs` following `next_page` and asserting the query parameters. `core::board` grouping tests (two-project task, unknown section, empty section). CLI rendering tests for text and JSON. Run with `cargo test board`.
- [x] New fixtures (`project_tasks_page*.json`) are anonymized. Grep them for real names and GIDs.
- [x] Manual (Checkpoint A): compare `asn board <gid> --json` with the web board on 2–3 real projects, and time a ~300-task project.

**Dependencies:** None

**Files likely touched:** `src/api/projects.rs`, `src/core/mod.rs`, `src/cli/board.rs` (new), `src/cli/mod.rs`, `src/main.rs`, `tests/api_project.rs`, `tests/fixtures/project_tasks_*.json`

**Estimated scope:** M

---

### - [x] T2: Pure board state in `tui/board.rs`

**Description:** A `Board` state struct built from `core::Board`, with no I/O and no rendering, in the style of `tui/picker.rs`. It handles:
- columns and the cursor (column + row)
- per-column vertical scroll
- `left`/`right`/`up`/`down`/`top`/`bottom`/`half_page` moves, with counts
- the visible column window for a given width (≥ 28 cells per column; columns share the width evenly when all fit, otherwise as many as fit and the selected one stays visible)
- hidden-column counts on each side
- `replace(core::Board)`: keeps the cursor on the same task GID, or clamps to the same position in the same column when the task is gone

**Acceptance criteria:**
- [x] Moving across uneven columns, including empty ones, never panics and keeps the row clamped. `h`/`l` move through empty columns.
- [x] The column window keeps the selected column visible and reports correct `‹ n` / `n ›` counts at 80, 120 and 200 cells. A width narrower than two columns shows one.
- [x] `replace` keeps the cursor by GID, and clamps when the task has gone.

**Verification:**
- [x] Tests: unit tests in `tui/board.rs` for every bullet above. Run with `cargo test tui::board`.

**Dependencies:** The `core::Board` type from T1 (define the type first if both run in parallel)

**Files likely touched:** `src/tui/board.rs` (new), `src/tui/mod.rs`

**Estimated scope:** S

---

### Checkpoint A: data is right
- [x] A1: card order and the open-task set match the web board on 2–3 real projects. If they don't, change `core::board` to `GET /sections/{gid}/tasks` per section before continuing, and update the spec.
- [x] A ~300-task project loads in < 2 s.
- [x] fmt, clippy and tests are clean.

---

## Phase 2: A board you can look at

### - [x] T3: Board screen opened with `asn --project`

**Description:** The board becomes visible. This adds:
- `App` gets `screen: Screen { Inbox, Board }` and the active board, plus `Effect::LoadBoard { project_gid }` / `Msg::BoardLoaded`.
- `asn --project <gid>` (digits only, validated like `ASANA_WORKSPACE`) starts on that board, and the Inbox loads in the background.
- `ui.rs` draws the columns:
  - headers `Section (n)` in the `title` role
  - the focused column has the focused border
  - two-line cards: name with `…`, then initials and the short due date, with overdue styled as in the detail pane plus `!`
  - `no tasks` in empty columns, in `text_muted`
  - `‹ n` / `n ›` edge markers
  - loading and error states
- Card text (initials, short due date, overdue) comes from pure functions in `present/`.
- On the board, `h`/`j`/`k`/`l`/`gg`/`G`/`Ctrl-d`/`Ctrl-u`/counts go to `tui/board.rs`. `e`/`u`/`x`/`V`/`f` do nothing. `q` quits, and `Esc` at the root does nothing. The hint line has a `board` context.

**Acceptance criteria:**
- [x] `cargo run -- --project <gid>` shows the project's board, and all navigation keys work as in F12.
- [x] Initials and due text are correct (one word, many words, unassigned, overdue vs. not, using local dates). Overdue is never shown by color alone.
- [x] Inbox-only keys do nothing on the board. A non-numeric `--project` exits with a message before any request.

**Verification:**
- [x] Tests: `present/` unit tests for card text. `app_tests.rs` covering key dispatch on the board and Inbox-only keys ignored. `TestBackend` snapshots: board that fits, board paged with markers, one-column narrow, empty section, overdue card, loading, error, `NO_COLOR` board. CLI arg validation test.
- [x] Manual: run against a real project at 80, 120 and 200 columns.

**Dependencies:** T1, T2

**Files likely touched:** `src/tui/app.rs`, `src/tui/ui.rs`, `src/tui/keys.rs`, `src/tui/mod.rs`, `src/present/format.rs`, `src/main.rs` (+ snapshots, `app_tests.rs`)

**Estimated scope:** M (at the upper edge: if it grows, split the rendering from the `--project` wiring)

---

### - [x] T4: Polling and `R`

**Description:** The 60 s `Msg::Tick` also reloads the active board when the board screen is visible, and `R` reloads it now. Results go through `Board::replace` (the cursor is kept by GID). A failed reload keeps the last data and shows an error toast. A reload isn't started while one is running. (The session cache of several boards moved to T6, the first task that can open a second board.)

**Acceptance criteria:**
- [x] A Tick on the board screen emits `LoadInbox` and `LoadBoard`. A Tick on the Inbox screen emits only `LoadInbox`.
- [x] After a reload, the cursor stays on the same task, or is clamped if the task has gone. A failed reload keeps the board and shows a toast.
- [x] Two reloads never overlap (moved: the cached-board criterion is now in T6).

**Verification:**
- [x] Tests: `app_tests.rs` for each bullet. Run with `cargo test`.
- [x] Manual: move a task on the web, and it moves in `asn` within 60 s or on `R`.

**Dependencies:** T3

**Files likely touched:** `src/tui/app.rs`, `src/tui/app_tests.rs`, `src/tui/mod.rs`

**Estimated scope:** S

---

### Checkpoint B: glance works
- [x] The user dogfoods `asn --project` on real projects for a few days at 80, 120 and 200 columns. Record the results for A2 (are 28-cell cards enough?) and A3 (are initials unambiguous?) in the spec.
- [x] **Human review before Phase 3.**

---

## Phase 3: Act and navigate

### - [x] T5: `Enter` → full-width detail, and actions on cards

**Description:** On the board, the shown task is the selected card: `selected_task_gid()` / `shown_task_name()` resolve the card, through the same seam that `opened` uses for search. `Enter` focuses the detail full-width, reusing the narrow-terminal detail rendering. `h`/`Esc`/`q` return to the board with its cursor. Details load and cache per GID as in F2, and scrolling keys work. `c`/`Tab c`/`Tab e`/`p`/`m`/`o` work on the card both from the board and from its detail. After a successful `m` or `p`, the board reloads and the cursor follows the task.

**Acceptance criteria:**
- [x] `Enter` shows the card's task full-width, and `h`/`Esc`/`q` return to the same card.
- [x] Comment, description edit, add to project, move and open work on a card. Commenting still marks that task's loaded unread notifications read (F3).
- [x] After `m`, the card shows in its new column with the cursor on it.

**Verification:**
- [x] Tests: `app_tests.rs` (Enter/back keeps the cursor; each action emits the right effect for the card's GID; `MovedToSection` triggers `LoadBoard`, and the cursor follows after reload). A snapshot of the full-width detail from the board.
- [ ] Manual: comment on a task from the board, then move it.

**Dependencies:** T4

**Files likely touched:** `src/tui/app.rs`, `src/tui/app_tests.rs`, `src/tui/ui.rs`, `src/tui/board.rs`

**Estimated scope:** M

---

### - [x] T6: `P` project switcher and Inbox ↔ board peers

**Description:** Boards opened this session stay in memory with their cursors, keyed by project GID (moved here from T4). Reopening one shows it at once and reloads it in the background. `P` (bound in `keys.rs` on both screens) opens a new picker stage that reuses the project typeahead, debounce and fzf keys. With an empty query it lists the projects opened this session, most recent first. `Enter` opens the board (from the cache if present). `Tab i` from the board shows the Inbox with its cursor unchanged. The Inbox keeps polling, and `P Enter` returns to the last board.

**Acceptance criteria:**
- [x] `P`, type, `Enter` opens any project's board. `P` with an empty query lists recents first, and `P Enter` reopens the last board with its cursor.
- [x] `Tab i` and back keeps both screens' cursors and data.
- [x] Reopening a cached board shows its old data and cursor immediately and emits one `LoadBoard`.
- [x] The no-collision key test passes, and `P` appears in the help overlay.

**Verification:**
- [x] Tests: switcher tests in `app_tests.rs` (recents on an empty query, results replacing them as you type, recents again when the query is cleared). `app_tests.rs` for screen switching and keeping cursors. A snapshot of the switcher with recents. The `keys.rs` collision test.
- [ ] Manual: switch between three projects and the Inbox.

**Dependencies:** T5

**Files likely touched:** `src/tui/picker.rs`, `src/tui/app.rs`, `src/tui/keys.rs`, `src/tui/app_tests.rs`, `src/tui/ui.rs`

**Estimated scope:** M

---

### - [x] T7: `b` from an Inbox task

**Description:** `b` on an Inbox task opens its project's board with the cursor on that task. It uses the task's loaded detail memberships (toast "loading…" if they aren't loaded yet, as `m` does). A task in several projects gets the project picker first, reusing the `MoveProject`-style stage. A task with no project, or a non-task item, gets a toast saying why. If the task isn't on the board (e.g. completed), the cursor starts at the first card and a toast says so.

**Acceptance criteria:**
- [x] `b` on a one-project task opens that board with the card selected.
- [x] `b` on a multi-project task opens the picker first, then the chosen board.
- [x] Non-task items, tasks with no project, and tasks not on the board each give a clear toast.

**Verification:**
- [x] Tests: `app_tests.rs` for each case, plus the key collision test.
- [ ] Manual: `b` from a real notification.

**Dependencies:** T6

**Files likely touched:** `src/tui/app.rs`, `src/tui/keys.rs`, `src/tui/picker.rs`, `src/tui/app_tests.rs`

**Estimated scope:** S

---

### - [x] T8: `/`, `n`, `N` jump on the board

**Description:** On the board, `/` takes a query (the same prompt as the Inbox filter). It jumps to the first card whose name matches, in board order (column by column, top to bottom), without filtering anything out. `n`/`N` go to the next and previous match, wrapping. No match shows a toast.

**Acceptance criteria:**
- [x] `/` with a matching query selects the first match (across columns, paging if needed). `n`/`N` cycle and wrap.
- [x] No match leaves the cursor where it was and shows a toast.

**Verification:**
- [x] Tests: `tui/board.rs` match iteration, and `app_tests.rs` for the key flow.

**Dependencies:** T5

**Files likely touched:** `src/tui/board.rs`, `src/tui/app.rs`, `src/tui/app_tests.rs`

**Estimated scope:** S

---

### Checkpoint C: feature complete
- [x] SPEC-board.md success criteria 1–8 are checked by hand on real projects.
- [x] fmt, clippy and tests are clean.
- [x] **Human review.**

---

## Phase 3b: New task (F14, added after Checkpoint C)

### - [x] T11: `asn project new`

**Description:** `core::create_task(client, project_gid, section_gid, name)` creates the task unassigned in the project (`POST /tasks`). With a section, it then adds the task to it (`POST /sections/{gid}/addTask`). It returns the new task, plus the section error if only that step failed. `asn project new <project-gid> [--section <gid>] <name…>` prints the GID.

**Acceptance criteria:**
- [x] `POST /tasks` sends `{name, projects: [project]}` and no assignee. With `--section`, `addTask` follows for the new GID.
- [x] If the section step fails, the GID is still printed, stderr says why, and the exit code is 1. If creating fails, nothing is printed to stdout, and the exit code follows F7.

**Verification:**
- [x] Tests: `wiremock` tests in `tests/api_project.rs` (create only, create + section, section refused). A CLI help test for `project new`.

**Dependencies:** None

**Files likely touched:** `src/api/tasks.rs` or `src/api/projects.rs`, `src/core/mod.rs`, `src/main.rs`, `tests/api_project.rs`, `tests/cli.rs`

**Estimated scope:** S

---

### - [x] T12: `Tab q` on the board

**Description:** `Tab q` on the board (card not open) starts the name prompt for the selected column. `Enter` emits `CreateTask`, and the result reloads the board with the new card selected, queuing a reload if one is running. Failures follow F14: a toast, and the name kept as a draft. The prompt shows in the status bar, the hint line has a "new task" context, and `Tab q` appears in the help overlay and README.

**Acceptance criteria:**
- [x] `Tab q`, type, `Enter` creates the task in the selected column's section, and the new card is then selected.
- [x] `Esc` or an empty name creates nothing. A failed create keeps the name for the next `Tab q`. A section failure says the task was created.
- [x] A reload requested while one is running happens after it, for `Tab q` and also for `m`/`p`.

**Verification:**
- [x] Tests: `app_tests.rs` for each criterion. A status-bar test for the prompt. The key collision test and the help overlay snapshot.
- [ ] Manual: create a task on a real board.

**Dependencies:** T11

**Files likely touched:** `src/tui/app.rs`, `src/tui/keys.rs`, `src/tui/ui.rs`, `src/tui/mod.rs`, `src/tui/app_tests.rs`, `README.md`

**Estimated scope:** M

---

## Phase 4: Ship

### - [x] T9: Help, README and spec updates

**Description:** Make the docs match what shipped:
- README: the "Why asn?" board line, a board key table, `asn board` / `asn --project`, the API table rows, and the version note
- `docs/SPEC-board.md`: assumption checkboxes and findings from use
- `docs/screenshot.svg`: a board screenshot, if the README shows one, with anonymized data

**Acceptance criteria:**
- [x] Every key in F12 is in the README and the help overlay, with matching descriptions.
- [x] Every API call the board makes is in the README's API table.

**Verification:**
- [x] Manual: read the README top to bottom against `asn --help`, `asn board --help` and the `?` overlay.

**Dependencies:** Checkpoint C

**Files likely touched:** `README.md`, `docs/SPEC-board.md`, `docs/screenshot.svg`

**Estimated scope:** S

---

### - [ ] T10: Release v0.2.0

**Description:** Bump the version in `Cargo.toml` / `Cargo.lock` (`chore: release v0.2.0`), tag it, and let the release workflow build the binaries and update Homebrew, as for v0.1.1. **Ask the user before tagging or pushing.**

**Acceptance criteria:**
- [ ] `v0.2.0` is tagged, the release has binaries attached, and `brew upgrade asn` installs it.

**Verification:**
- [ ] Manual: `asn --version` from the Homebrew install prints 0.2.0.

**Dependencies:** T9, user approval

**Files likely touched:** `Cargo.toml`, `Cargo.lock`

**Estimated scope:** XS

---

### Checkpoint: Complete
- [ ] All SPEC-board.md success criteria are met, and v0.2.0 is released.
- [ ] Two weeks of status checks happen in `asn` instead of the web board (the spec's success metric).
