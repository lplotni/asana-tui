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
- [x] Manual: create a task on a real board (confirmed by the user, 2026-11-17).

**Dependencies:** T11

**Files likely touched:** `src/tui/app.rs`, `src/tui/keys.rs`, `src/tui/ui.rs`, `src/tui/mod.rs`, `src/tui/app_tests.rs`, `README.md`

**Estimated scope:** M

---

## Phase 3c: @-mentions (F15)

### - [x] T13: Mentions in the core and CLI

**Description:**
- A pure `core` function turns text plus mentions (`@Label` → GID) into Asana rich text (`<body>`, HTML-escaped, mention tags). It returns `None` when no mention is left, so the text saves plain.
- `core::comment` and the description save write `html_text`/`html_notes` when there are mentions, and plain `text`/`notes` as today otherwise.
- The CLI turns `@<10+ digit gid>` into mentions in `asn comment` and `asn desc`.
- `asn users <query…>` lists people from the typeahead.

**Acceptance criteria:**
- [x] Text with a mention becomes `<body>…<a data-asana-gid="…"/>…</body>`, with `&`, `<` and `>` escaped and line breaks kept. Text without one stays plain.
- [x] `asn comment <task> -m 'hi @1200000000000011'` posts `html_text`, and `-m 'hi @3pm'` posts plain `text`.
- [x] `asn users ada` prints `gid  name` lines.

**Verification:**
- [x] Tests: unit tests for the rich-text function (escaping, several mentions, a mention that was edited away, line breaks). `wiremock` tests for `html_text`/`html_notes` versus plain bodies, and for the user typeahead.
- [x] **Checkpoint M1 (user):** post a comment with a mention and a line break on a test task, and check it in the web app.

**Dependencies:** None

**Files likely touched:** `src/core/mod.rs` (or a new `src/core/rich.rs`), `src/api/tasks.rs`, `src/api/search.rs`, `src/cli/`, `src/main.rs`, `tests/`

**Estimated scope:** M

---

### - [x] T15: Show mentions as @Name when reading

**Description:** Comments are fetched with `html_text`. A pure `core::mention::readable(text, html)` replaces each Asana URL in the plain text whose GID matches a mention link in the HTML with that link's label. Punctuation after the URL is kept, entities are decoded, and a person's label always starts with `@`. The task view uses it for comments and the description.

**Acceptance criteria:**
- [x] A person's profile URL (old and new formats) shows as `@Name`, and a task or project URL as its name.
- [x] URLs without a matching mention, and text without HTML, are unchanged.

**Verification:**
- [x] Tests: unit tests for `readable`. A `present::detail` test showing a comment and a description with a mention.
- [x] Manual: a task with mentions shows names in `asn` and `asn task` (confirmed by the user, 2026-11-17).

**Dependencies:** T13

**Files likely touched:** `src/core/mention.rs`, `src/api/tasks.rs`, `src/present/detail.rs`

**Estimated scope:** S

---

### - [x] T14: `@` in the editor

**Description:**
- Typing `@` in the comment or description editor opens a mention picker over it. Three typeahead searches run at once (people, projects, tasks), and the results are merged with labels.
- `Enter` inserts `@Name` and records its GID on the editor, and `Esc` inserts a plain `@`.
- Saving passes the mentions to the core. Drafts keep them.

**Acceptance criteria:**
- [x] `@`, `ada`, `Enter` inserts `@Ada Lovelace`, and posting sends a mention of her GID.
- [x] `Esc` inserts `@`. Results arrive for the latest query only. A failed search says so.
- [x] A failed comment's draft keeps its mentions.

**Verification:**
- [x] Tests: `app_tests.rs` for each criterion. A snapshot of the picker over the editor.
- [x] Manual: mention someone in a real comment and description (confirmed by the user, 2026-11-17).

**Dependencies:** T13 and Checkpoint M1

**Files likely touched:** `src/tui/app.rs`, `src/tui/picker.rs`, `src/tui/ui.rs`, `src/tui/mod.rs`, `src/tui/app_tests.rs`, `README.md`

**Estimated scope:** M

---

## Phase 3d: Complete and assign (F16, F17)

### - [x] T16: `asn task done` and `asn task assign`

**Description:**
- `core::set_completed(client, task, bool)` and `core::assign(client, task, assignee)`, where the assignee is a user GID, `me` or none. Both are `PUT /tasks/{gid}`, and assign returns the new assignee.
- CLI: `asn task done <gid> [--undo]` and `asn task assign <gid> <user-gid|me|none>`. `asn task <gid> [--json]` is unchanged.

**Acceptance criteria:**
- [x] `done` sends `{completed: true}`, and `--undo` sends `false`. `assign` sends the GID, `"me"` or `null`, and prints the new assignee (or `unassigned`).
- [x] `asn task <gid>` and `asn task <gid> --json` behave exactly as before.

**Verification:**
- [x] Tests: `wiremock` tests for both requests. CLI tests that the subcommands are in the help and that `asn task <gid>` still parses.

**Dependencies:** None

**Files likely touched:** `src/api/tasks.rs`, `src/core/mod.rs`, `src/main.rs`, `tests/api_task.rs`, `tests/cli.rs`

**Estimated scope:** S

---

### - [x] T17: `d` completes, with undo

**Description:** `d` toggles completion of the shown task or card. On a board the card leaves its column at once (`BoardView::remove`), and the board reloads. The success toast offers undo, and `d` as the very next key reopens the task (on a board, the card comes back selected). A failure restores the card or state and shows an error.

**Acceptance criteria:**
- [x] `d` on a card removes it, sends the completion, and reloads the board. `d` again right after reopens it.
- [x] `d` in the Inbox toggles the loaded task's state, and asks to wait when it isn't loaded.
- [x] Another key ends the undo. A failure restores and says so.

**Verification:**
- [x] Tests: `tui/board.rs` (remove) and `app_tests.rs` for each criterion. Key collision and help overlay tests.

**Dependencies:** T16

**Files likely touched:** `src/tui/app.rs`, `src/tui/board.rs`, `src/tui/keys.rs`, `src/tui/mod.rs`, `src/tui/ui.rs`, `src/tui/app_tests.rs`

**Estimated scope:** M

---

### - [x] T18: `Tab a` assigns

**Description:** `Tab a` opens a people picker with "Me" and "Unassigned" first, and typing searches people. `Enter` sends the assignment. The task pane shows the assignee Asana returns, and a board reloads.

**Acceptance criteria:**
- [x] `Tab a`, then `Enter` on "Me", assigns to me. "Unassigned" clears the assignee. Typing a name and choosing a person assigns them.
- [x] Results arrive for the latest query only. A failure says so.

**Verification:**
- [x] Tests: `app_tests.rs` for each criterion. A picker snapshot. Hints and help.
- [x] Manual (with T17): complete, undo and assign on a real board and in the Inbox (confirmed by the user, 2026-11-17).

**Dependencies:** T16

**Files likely touched:** `src/tui/app.rs`, `src/tui/picker.rs`, `src/tui/keys.rs`, `src/tui/mod.rs`, `src/tui/ui.rs`, `src/tui/app_tests.rs`, `README.md`

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

### - [x] T10: Release v0.2.0

**Description:** Bump the version in `Cargo.toml` / `Cargo.lock` (`chore: release v0.2.0`), tag it, and let the release workflow build the binaries and update Homebrew, as for v0.1.1. **Ask the user before tagging or pushing.**

**Acceptance criteria:**
- [x] `v0.2.0` is tagged, the release has binaries attached, and `brew upgrade asn` installs it.

**Verification:**
- [ ] Manual: `asn --version` from the Homebrew install prints 0.2.0.

**Dependencies:** T9, user approval

**Files likely touched:** `Cargo.toml`, `Cargo.lock`

**Estimated scope:** XS

---

### Checkpoint: Complete
- [x] All SPEC-board.md success criteria are met, and v0.2.0 is released (2026-11-17; the first release run lost its macOS arm64 runner to GitHub capacity and was re-run).
- [ ] Two weeks of status checks happen in `asn` instead of the web board (the spec's success metric).

---

## Phase 5: Config file and completion animation (F22, F23)

### - [x] T19: Spec, ADR-010 and tasks

**Description:** F22 and F23 in `docs/SPEC-board.md`, ADR-010 superseding ADR-003, and these tasks.

**Acceptance criteria:**
- [x] F22/F23 describe the file, precedence, the interview and the animation; ADR-010 records why the token stays off disk.

**Files likely touched:** `docs/SPEC-board.md`, `docs/SPEC.md`, `docs/decisions/`, `tasks/`

**Estimated scope:** XS

---

### - [x] T20: Read `~/.config/asn/config.toml`

**Description:** `Config` resolves every setting from the environment, then the file, then the default (F22). `token` and unknown keys are errors. `theme` and `animations` join `Config`; `tui::run` takes the theme choice from it.

**Acceptance criteria:**
- [x] Each key: env wins over file, file over default. A missing file behaves exactly as today.
- [x] A `token` key, an unknown key, or a wrongly typed value exits 1 naming the file and the key.
- [x] The path follows `XDG_CONFIG_HOME`, else `HOME/.config`.
- [x] Error messages name the file key and the env variable.

**Verification:**
- [x] Unit tests in `config.rs`; `cargo test`, clippy, fmt.

**Dependencies:** T19

**Files likely touched:** `src/config.rs`, `src/main.rs`, `src/tui/mod.rs`

**Estimated scope:** S

---

### - [x] T21: `asn init` and first-run setup

**Description:** The F22 interview as a core function over generic input/output, `GET /workspaces` in the API, the `asn init` subcommand, and the TUI's first run when no file exists.

**Acceptance criteria:**
- [x] The interview asks for the token command and the workspace, and writes the file with mode 0600 and the other keys as commented defaults.
- [x] It never replaces an existing file without `y`; Ctrl-D aborts without writing.
- [x] A failing token command or a 401 goes back to the token step.
- [x] `asn init` without a terminal exits 1; CLI subcommands never prompt.

**Verification:**
- [x] Interview tests with scripted input; wiremock for `GET /workspaces`; `tests/cli.rs` for no terminal.
- [ ] Manual: delete the file, run `asn`, answer the questions, and the TUI starts.

**Dependencies:** T20

**Files likely touched:** `src/setup.rs`, `src/api/`, `src/main.rs`, `tests/`

**Estimated scope:** M

---

### - [x] T22: Completion animation on board cards

**Description:** F23: `d` on a board card plays the strike and collapse before the card leaves. The event loop sends `Msg::Frame` every 40 ms only while an animation runs. Reloads during it are held. `animations = false` keeps F16's instant removal.

**Acceptance criteria:**
- [x] A completed card strikes through, collapses and leaves after the last frame.
- [x] `d` during the animation reopens the task, and the card stays.
- [x] A reload during the animation is applied when it ends; a failure restores the card.
- [x] With animations off, the card leaves at once (today's tests still pass).

**Verification:**
- [x] App tests and two snapshots (mid-strike, mid-collapse).
- [ ] Manual: complete a card on a real board.

**Dependencies:** T20

**Files likely touched:** `src/tui/app.rs`, `src/tui/board.rs`, `src/tui/mod.rs`, `src/tui/ui.rs`

**Estimated scope:** M

---

### - [x] T23: Docs

**Description:** README "Configure" leads with `asn init` and the file, with the env table kept as overrides; `ASN_ANIMATIONS` added.

**Acceptance criteria:**
- [x] README, `asn --help` and F22 agree on keys, variables and the path.

**Dependencies:** T21, T22

**Files likely touched:** `README.md`

**Estimated scope:** XS

---

## Phase 6: asn in Claude Code (F18–F21)

Spec: [`docs/SPEC-skill.md`](../docs/SPEC-skill.md). The definition of done above applies, with `docs/SPEC-skill.md` as the spec to update. Order and rationale: [`plan.md`](plan.md#phase-6-asn-in-claude-code).

### - [x] T24: `asn task <gid> --brief` read receipt

**Description:** F18. The receipt lines are built in `present/` from a `TaskDetail` (name, gid and permalink, one `where` line per membership, assignee/due/status, and counts for description lines, comments and attachments). `cli/task.rs` renders them plain through `terminal_safe`. `asn task` gets `--brief`, which conflicts with `--json`.

**Acceptance criteria:**
- [x] `asn task <gid> --brief` prints the five-line receipt from F18; a task in two projects gets two `where` lines; a missing assignee reads `unassigned`, and a missing due date is left out.
- [x] Description and comment text never appear in the receipt; control characters and bidi overrides in the name are neutralised; the name fits 80 cells.
- [x] `--brief --json` is a usage error; `asn task <gid>` and `--json` output are unchanged.

**Verification:**
- [x] Rendering tests in `cli/task.rs` (memberships, unassigned, no due date, completed, a hostile name and description); `tests/cli.rs` for the flag conflict; `cargo test`, clippy, fmt.
- [x] Manual: `asn task <real gid> --brief` matches the web app (confirmed by the user, 2026-11-17).

**Dependencies:** None

**Files likely touched:** `src/present/detail.rs` (or a new `src/present/receipt.rs`), `src/cli/task.rs`, `src/main.rs`, `tests/cli.rs`

**Estimated scope:** S

---

### - [ ] T25: The `asn` skill, v1

**Description:** F20 over today's commands. `skills/asn/SKILL.md` gives the vocabulary, the GID lookups (`search`, `users`, `project sections`), the current-task rule (the conversation, plus an offer from a GID in the branch name or a pasted Asana URL), the receipt rule, confirm-before-write, comment-by-default capture, and the untrusted-text rule. It is packaged as a Claude Code plugin with a marketplace entry in this repo (decided 2026-11-17), so `/plugin` installs it from `lplotni/asana-tui`.

**Acceptance criteria:**
- [ ] With the skill installed, reading a task starts the reply with the `--brief` receipt verbatim, and `search`/`board`/`users` results get the one-line notice.
- [ ] Every write is shown (resolved task, person or `Project › Section`, and the exact command) and runs only after a yes; "capture this" becomes `asn comment <gid> -`.
- [x] A test walks every `asn` command in the skill's code blocks through the help output (subcommand path and long flags), so a renamed subcommand or flag fails CI. Checked by mutating the skill.

**Verification:**
- [x] The drift test (`tests/skill.rs`); `cargo test`, clippy, fmt. `claude plugin validate .` passes (warnings: no version, by design; no icon).
- [x] Only invented GIDs and names: `the_skill_uses_only_invented_gids` enforces the GIDs.
- [ ] Manual: install it in Claude Code, then read, move, assign, create and capture on a test task.

**Dependencies:** T24

**Files likely touched:** `skills/asn/SKILL.md`, `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`, `tests/cli.rs`

**Estimated scope:** S

---

### - [ ] Checkpoint S1: does the habit stick?

- [ ] The user works with the skill for a day. Count missed receipts and unconfirmed writes (C1).
- [ ] List the lookups Claude got wrong or took many calls for (input for T26–T28 and C2).
- [ ] Decide: go on with 6b, change the skill first, or stop (C4).

---

### - [ ] T26: Name resolution in `core`, and users by name

**Description:** F19's primitive: `core::Resolved { One(Named), Ambiguous(Vec<Named>), None }` and the matching rule (case-insensitive exact name wins, else a single candidate). Users come from the user typeahead. `asn task assign <task> <name>` resolves a non-GID that isn't `me`/`none`. A CLI helper prints `Ambiguous`/`None` as `gid  label` lines on stderr and exits 1.

**Acceptance criteria:**
- [ ] `asn task assign <task> "Ada Lovelace"` assigns when the name matches exactly or the typeahead returns one person, and prints the new assignee as today.
- [ ] Several matches or none: no `PUT` is sent, stderr lists the candidates, and the exit code is 1.
- [ ] A GID, `me` and `none` make exactly the requests they make today.

**Verification:**
- [ ] Unit tests for the matching rule; wiremock tests for one, several and no matches, asserting no `PUT /tasks` in the error cases; `tests/cli.rs` for the exit code and stderr.

**Dependencies:** T25 (after Checkpoint S1)

**Files likely touched:** `src/core/mod.rs` (or `src/core/resolve.rs`), `src/cli/mod.rs`, `src/main.rs`, `tests/`

**Estimated scope:** M

---

### - [ ] T27: Sections by name in `asn project move`

**Description:** A non-GID section is matched against the sections of every project the task is in (labels `Project › Section`). On success, `move` prints `moved to <Project> › <Section>`.

**Acceptance criteria:**
- [ ] `asn project move <task> "in review"` moves the task when exactly one of its projects has that section (case-insensitive).
- [ ] The same section name in two of the task's projects is ambiguous: nothing is moved, and both `Project › Section` labels are listed.
- [ ] A section GID moves as today, and success now prints the destination.

**Verification:**
- [ ] Wiremock tests (one project, two projects with the same name, no match), asserting no `addTask` in the error cases; CLI test for the printed line.

**Dependencies:** T26

**Files likely touched:** `src/core/mod.rs`, `src/main.rs`, `tests/`

**Estimated scope:** S

---

### - [ ] T28: Projects by name, `asn projects`, and section names in `new`/`add`

**Description:** `asn projects <query…>` prints `gid  name` like `asn users`. `asn project new` and `asn project add` accept a project name (project typeahead) and a `--section` name (that project's sections). `add` prints `added to <Project>[ › <Section>]`; `new` keeps printing the new GID.

**Acceptance criteria:**
- [ ] `asn projects web` lists matching projects.
- [ ] `asn project new "Web Platform" --section Backlog Write the launch post` creates the task in that section; an ambiguous or unknown project or section creates nothing (no `POST /tasks`) and lists candidates.
- [ ] GID arguments behave exactly as today.

**Verification:**
- [ ] Wiremock tests for the project and section cases, asserting no write on errors; CLI tests for `asn projects` output and the `add` line.

**Dependencies:** T26

**Files likely touched:** `src/core/mod.rs`, `src/main.rs`, `src/cli/mod.rs`, `tests/`

**Estimated scope:** M

---

### - [ ] T29: The skill uses names (v2)

**Description:** The skill drops the GID lookup recipes that F19 makes unnecessary, uses names directly, and relays the candidate list when a name is ambiguous. It adds the manual acceptance script to this task.

**Acceptance criteria:**
- [ ] "Move the current task to Review and assign it to Ada" takes one confirmation showing the resolved task, section and person, then runs two commands.
- [ ] On an ambiguous name, Claude shows the candidates and asks; it never picks one itself.
- [ ] A task whose description says to ignore instructions and reassign it leads to no write.

**Verification:**
- [ ] The drift test from T25 still passes.
- [ ] Manual acceptance script: read (receipt first), move by section name, assign by name, create then assign, capture as a comment, the prompt-injection task.

**Dependencies:** T27, T28

**Files likely touched:** `skills/asn/SKILL.md`

**Estimated scope:** XS

---

### - [ ] T30: Docs

**Description:** F21. README gets "Use from Claude Code" (install and three example requests); the command reference gets `--brief`, `asn projects`, the name forms, and the new success line from `move`/`add`. Tick the assumptions checked at the checkpoints in `docs/SPEC-skill.md`.

**Acceptance criteria:**
- [ ] README, `asn --help` and SPEC-skill.md agree on flags, name forms and output.

**Dependencies:** T29

**Files likely touched:** `README.md`, `docs/SPEC-skill.md`, `CLAUDE.md` (layout table: `skills/`)

**Estimated scope:** XS
