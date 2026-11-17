# asana-tui — Spec v0.2 (Board view)

Builds on [the v0.1 spec](SPEC.md). Its principles, configuration, architecture, theming rules (F8) and keymap conventions (F6) all still apply. This document covers only what v0.2 adds.

## Problem Statement

How might we let you see where any Asana project stands from the terminal, as a real board, well enough that you stop opening the Asana web board?

## Target User & Success

- **Primary user:** the same terminal-native engineers as v0.1. Here they are **glancing at status** across **many projects, opened ad hoc**, typically **50–300 open tasks** each.
- **Success:** for two weeks of normal use, the author checks project status in `asn` and doesn't open the web board for it.
- **The job is reading, not moving.** The board shows where things stand. The existing task actions (comment, description, add to project, move to section) work from it, and nothing beyond them is added.

## Recommended Direction

A **columns board**: each section of a project is a column, each open task a card with its name, assignee initials and due date. The board is a second top-level screen next to the Inbox, not an overlay on it. Both keep their data and cursor while you switch between them.

You reach a board three ways: the project switcher (`P`), `b` on an Inbox task, or `asn --project <gid>` at startup. `asn board <gid> [--json]` prints the same data for scripts. Per principle 2, it is built before the TUI screen, on the same core function.

## Asana API Surface (v0.2 additions)

| Need | Endpoint | Notes |
|---|---|---|
| Columns | `GET /projects/{gid}/sections` | Already used by F5 and `m`. Gives column order and empty columns. |
| Cards | `GET /projects/{gid}/tasks?completed_since=now&limit=100&opt_fields=name,assignee.name,due_on,due_at,memberships.project.gid,memberships.section.gid` | Follows `next_page` until `null`. `completed_since=now` returns open tasks only. Top-level tasks only (subtasks aren't project members unless added). |
| Project switcher | `GET /workspaces/{gid}/typeahead?resource_type=project` | Already used by F5 (`core::search_projects`). |

A card's column is its membership whose `project.gid` is this project. Other memberships are ignored, because a task can be in several projects. A task whose section isn't in the section list goes into a leading **(no section)** column, which is shown only when it has cards.

**Request budget:** one load is 1 + ⌈tasks / 100⌉ requests, i.e. 4 for 300 tasks. Only the board on screen is polled, every 60 s. Together with the Inbox's 1/min, that stays far below the ~150 req/min free-plan limit.

## Feature Spec — v0.2

### F9. Board screen

- **Layout:** one column per section, in Asana's section order. The header reads `Section name (n)` and uses the `title` role. When every column fits at ≥ 28 cells, the columns share the full width evenly. When they don't, as many 28-cell columns as fit are shown, and the view pages horizontally to keep the selected column visible. The edges show how many columns are hidden on each side (`‹ 2`, `3 ›`). A terminal narrower than two columns shows one column at a time.
- **Cards:** two lines each.
  - Line 1: the task name, truncated with `…`.
  - Line 2: assignee initials (first letter of the first and last word of `assignee.name`, `—` when unassigned) and the due date in the detail pane's short format.
  - An overdue due date gets the same style the detail pane uses, plus a `!` suffix, so the meaning is never carried by color alone (F8).

  Card text gets the same control-character and bidi neutralizing as everything else from Asana.
- **Order:** cards appear in the order the API returns them, which is assumed to be board order (see assumption A1).
- **Focus:** the selected column has the focused border (`┏━`), and the selected card uses the `selection` role. Columns scroll vertically on their own and keep the selected card visible.
- **States:**
  - Loading: `Loading <project name>…`, shown while the first load runs.
  - Empty section: an empty column with `no tasks` in `text_muted`.
  - Failed first load: an error screen naming the project and the reason.
  - Failed poll: the board keeps its last data, and an error toast is shown.
- **Refresh:** the visible board polls every 60 s, and `R` refreshes it now. After a refresh the cursor stays on the same task GID. If that task is gone (completed, moved out of the project), the cursor goes to the same position in the same column, clamped.
- **Session cache:** every board opened this session stays in memory with its cursor. Reopening one shows it at once and refreshes it in the background. Nothing is written to disk.

### F10. Task detail from the board

- `Enter` opens the selected task's detail (F2 content, fetched and cached the same way) **full-width**, replacing the board. `h`/`Esc`/`q` return to the board, with the cursor where it was.
- These actions work on the selected card from the board itself and from its detail view, exactly as in the Inbox:
  - `c`/`Tab c` (comment)
  - `Tab e` (edit description)
  - `p`/`Tab p` (add to project)
  - `m` (move to section)
  - `o` (open in browser)

  Inbox-only rules don't apply here. Commenting from the board still marks that task's loaded unread notifications read, per F3.
- After a successful `m` or `p`, the board reloads. The cursor follows the task into its new column if it is still in this project.

### F11. Getting to a board

- **`P`, the project switcher:** the F5 picker (typeahead, 200 ms debounce, fzf keys). With an empty query, it lists the projects opened this session, most recent first, so `P Enter` returns to the last board. Choosing a project opens its board. `P` works on both screens.
- **`b` in the Inbox:** with a task selected, `b` opens the board of its project with the cursor on that task. A task in several projects gets the project picker first (as `m` does). A task with no project, or a non-task item, gets a toast saying why.
- **`Tab i`** (the existing Asana alias) goes from the board to the Inbox. Both screens keep their cursor and data, and the Inbox keeps polling while hidden.
- **Startup:** `asn --project <project-gid>` opens the TUI on that board, and the Inbox loads in the background. The GID is validated as digits, like `ASANA_WORKSPACE`. A 403/404 shows the F9 error screen, not an exit.
- `q` on the board root quits, as on the Inbox list. `Esc` on the board root does nothing (root).

### F12. Board keymap (additions to F6)

| Keys | Context | Action | Origin |
|---|---|---|---|
| `h` / `l` | board | previous / next column (pages when needed) | vim |
| `j` / `k`, `{count}j` | board | down / up within the column | vim and Asana |
| `gg` / `G`, `Ctrl-d` / `Ctrl-u` | board | top / bottom / half page within the column | vim |
| `/`, `n` / `N` | board | jump to cards whose name matches, across columns, in board order | vim |
| `Enter` | board | open the task detail full-width | — |
| `P` | anywhere | project switcher | asn |
| `b` | Inbox | open the selected task's project board | asn |
| `R` | board | refresh the board | asn |

`V` (visual mode), `e`/`u`/`x` (read state) and `f` (unread filter) are Inbox-only and do nothing on the board. The hint line gets a `board` context. The help overlay and the no-collision test both cover the new bindings, because they are generated from the same table in `keys.rs`.

### F13. CLI

```
asn board <project-gid> [--json]    # open tasks of a project, grouped by section
asn --project <project-gid>         # TUI, opened on that board
```

- Text output: for each section in board order, a line with the section's `gid` and `name`, followed by one indented tab-separated line per task: `gid  name  assignee  due_on`, with an empty field when a value is missing. Plain text, neutralized like the rest of the CLI.
- `--json`: `{"sections": [...], "tasks": [...]}`, where both are the Asana `data` objects unchanged (the F7 contract). Section membership is read from each task's `memberships`.
- Exit codes as in F7.

## Tech Stack, Commands, Structure

- **Stack:** unchanged (`ratatui`, `crossterm`, `tokio`, `reqwest`, `serde`, `clap`). **No new dependencies.**
- **Commands** (as in CI):
  ```
  cargo fmt --check
  cargo clippy --all-targets -- -D warnings
  cargo test
  cargo run -- board <project-gid> --json
  cargo run -- --project <project-gid>
  ```
- **Where things go:**
  - `api/projects.rs`: the paginated project-tasks request and its response type
  - `core/`: `board(client, project_gid) -> Board { sections, tasks }`, used by both the CLI and the TUI
  - `cli/board.rs`: text/JSON rendering
  - `present/`: card text (initials, short due, overdue)
  - `tui/app.rs`: a `Screen` (Inbox | Board) and board state as pure transitions
  - `tui/keys.rs`: bindings
  - `tui/ui.rs`: column rendering

## Code Style

Follow the existing modules: thin `core` functions over `api`, doc comments that say what something is for, and `Result<_, ApiError>`:

```rust
/// The sections of a project, in board order.
pub async fn sections(client: &Client, project_gid: &str) -> Result<Vec<Named>, ApiError> {
    client.sections(project_gid).await
}
```

Rendering asks for theme roles, never colors (F8). New theme roles are added only if an existing one can't express the meaning.

## Testing Strategy

- **`api/`:** `wiremock` tests. Task pagination follows `next_page` across 3 pages, and `completed_since`/`opt_fields` are sent.
- **`core::board`:**
  - tasks are grouped by this project's membership only (fixture: a task in two projects)
  - an unknown section goes to (no section)
  - empty sections are kept
- **`present/`:** initials (one word, many words, unassigned) and overdue vs. not, using the local date as the existing tests do.
- **`tui/app.rs`, as pure transitions:**
  - `h`/`l`/`j`/`k` across uneven columns
  - horizontal paging keeps the selection visible
  - the cursor is kept by GID across refresh, with clamping when the task is gone
  - Inbox ↔ board switching keeps both cursors
  - `b` lands on the task, and gets the picker when the task is in several projects
  - `P` with an empty query lists recents first
  - the board reloads after `m`
  - only the visible board polls
- **`keys.rs`:** the new bindings parse, and the existing no-collision test passes.
- **`TestBackend` snapshots:**
  - board that fits
  - board paged with edge markers
  - one-column narrow terminal
  - empty section
  - overdue card
  - loading
  - error
  - full-width detail from the board
  - `NO_COLOR` board
- **CLI:** text and JSON rendering for a fixture board.
- All fixtures use anonymized GIDs and names.

## Boundaries

- **Always:**
  - run `cargo fmt --check`, `clippy -D warnings` and `cargo test` before each commit
  - build the core function and CLI before the key binding
  - use theme roles only
  - keep the TUI event loop non-blocking
- **Ask first:**
  - adding a dependency
  - adding theme roles
  - rebinding an existing v0.1 key
  - anything that writes to disk
  - changing the `--json` contract of an existing command
- **Never:**
  - persist recents or caches to disk
  - put a real workspace/project GID or name in tests, fixtures, docs or commits
  - log or print the token

## Success Criteria

1. `asn board <gid> --json` on 2–3 real projects has the same sections, card order and open-task set as the web board.
2. A 300-task project's board appears in under 2 s on a normal connection (timed with `asn board`).
3. `P` → type → `Enter` opens any project, and `P Enter` returns to the last one.
4. `b` on an Inbox task opens its board with that card selected. `Tab i` returns to the Inbox with the cursor unchanged.
5. `Enter`, `c`, `Tab e`, `p`, `m` and `o` work on a card, and `m` shows the card in its new column.
6. At 80, 120 and 200 columns the board is readable without horizontal page scrolling of the terminal, and the hidden-column markers are correct.
7. All tests listed above pass, and CI (fmt, clippy, test) is green.
8. The board keeps working through a failed poll (toast, stale data) and shows a clear error for a project you can't access.

## Key Assumptions to Validate

- [ ] **A1: `GET /projects/{gid}/tasks` returns tasks in board order within each section.** Test: compare `asn board --json` with the web board on 2–3 projects before building the TUI. If it fails, load each section with `GET /sections/{gid}/tasks` (one request per section, which still fits the budget).
- [ ] **A2: 28-cell cards with name + initials + due are enough to glance at.** Test: dogfood on the widest and narrowest terminals in use. If not, first try 3-line cards, before considering a stacked layout.
- [ ] **A3: Initials are unambiguous enough in a project's assignee set.** Test: dogfood. If two people share initials, fall back to the first name.
- [ ] **A4: Hiding completed tasks matches how the board is read.** Test: dogfood. The web board collapses them by default.
- [ ] **A5: The detail and action code can be keyed by task GID instead of an Inbox item.** Test: check the `Detail`/`Effect` paths in `tui/app.rs` during planning. If not, separating them is the first task.

## Not Doing (and Why)

- **Stacked/list layout or a `v` toggle:** one renderer first. The sections → tasks model leaves room for one later.
- **Custom fields, tags, subtask/comment counts on cards:** assignee and due date are what a glance needs, and narrow cards depend on that.
- **Reordering cards, editing sections, creating tasks, completing tasks:** these are workflow, not status. `m` already covers moving between sections, and `o` covers the rest.
- **Section summaries and "what changed since I last looked":** strong ideas for glancing, but post-MVP. Revisit if scanning 300 cards turns out slow.
- **Persisted recents, pinned projects (`ASN_PROJECTS`), Events API sync, local mirror:** 60 s polling and session recents cover the job, and nothing is written to disk.
- **Showing completed tasks:** they are hidden, to keep columns about current work.
- **Multiple boards side by side, portfolios, My Tasks board:** one project at a time. "My Tasks" as a startup view stays a separate idea.

## Open Questions

1. Should `asn --project` also accept a project **name** (resolved through typeahead), or only a GID? GID only is assumed for v0.2.
