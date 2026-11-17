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
| Board title | `GET /projects/{gid}?opt_fields=name` | The project's name for the header (`asn --project` only has the GID). Loaded at the same time as the rest. |
| Columns | `GET /projects/{gid}/sections` | Already used by F5 and `m`. Gives column order and empty columns. |
| New task (F14) | `POST /tasks` `{name, projects: [project]}`, then `POST /sections/{section}/addTask` `{task}` | Unassigned. `addTask` puts it at the top of the section. |
| Cards | `GET /projects/{gid}/tasks?completed_since=now&limit=100&opt_fields=name,assignee.name,due_on,due_at,memberships.project.gid,memberships.section.gid` | Follows `next_page` until `null`. `completed_since=now` returns open tasks only. Top-level tasks only (subtasks aren't project members unless added). |
| Project switcher | `GET /workspaces/{gid}/typeahead?resource_type=project` | Already used by F5 (`core::search_projects`). |

A card's column is its membership whose `project.gid` is this project. Other memberships are ignored, because a task can be in several projects. A task whose section isn't in the section list goes into a leading **(no section)** column, which is shown only when it has cards.

**Request budget:** one load is 2 + ⌈tasks / 100⌉ requests, i.e. 5 for 300 tasks. Only the board on screen is polled, every 60 s. Together with the Inbox's 1/min, that stays far below the ~150 req/min free-plan limit.

## Feature Spec — v0.2

### F9. Board screen

- **Layout:** one column per section, in Asana's section order. The header reads `Section name (n)` and uses the `title` role. When every column fits at ≥ 28 cells, the columns share the full width evenly. When they don't, as many 28-cell columns as fit are shown, and the view pages horizontally to keep the selected column visible. The right end of the header line shows how many columns are hidden on each side (`‹ 2  3 ›`), so every column keeps its full width. The status bar shows the card's position in its column (`2/5`), not the Inbox's. A terminal narrower than two columns shows one column at a time.
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
| `/`, `n` / `N` | board | jump to cards whose name matches, across columns, in board order (Enter jumps to the first; `Esc` ends the search). Separate from the Inbox filter, which it never changes | vim |
| `Enter` | board | open the task detail full-width | — |
| `P` | anywhere | project switcher | asn |
| `b` | Inbox | open the selected task's project board | asn |
| `R` | board | refresh the board | asn |
| `Tab q` | board | new task in the selected column (F14) | Asana |
| `d` | Inbox, board | complete / reopen the task (F16); `d` again right after undoes | asn |
| `Tab a` | Inbox, board | assign the task (F17) | Asana |

`V` (visual mode), `e`/`u`/`x` (read state) and `f` (unread filter) are Inbox-only and do nothing on the board. The hint line gets a `board` context. The help overlay and the no-collision test both cover the new bindings, because they are generated from the same table in `keys.rs`.

### F13. CLI

```
asn board <project-gid> [--json]    # open tasks of a project, grouped by section
asn --project <project-gid>         # TUI, opened on that board
asn project new <project-gid> [--section <gid>] <name…>   # F14: create a task, print its gid
```

- Text output: for each section in board order, a line with the section's `gid` and `name`, followed by one indented tab-separated line per task: `gid  name  assignee  due_on`, with an empty field when a value is missing. Plain text, neutralized like the rest of the CLI.
- `--json`: `{"sections": [...], "tasks": [...]}`, where both are the Asana `data` objects unchanged (the F7 contract). Section membership is read from each task's `memberships`.
- Exit codes as in F7.

### F14. New task on the board

Added after Checkpoint C at the user's request. It was in Not Doing before.

- `Tab q` (Asana's quick-add key) on the board, with no card open, starts a one-line prompt in the status bar: `NEW TASK` and `Name in <Section>: …`. Typing edits the name, `Enter` creates the task, and `Esc` cancels. An empty name creates nothing.
- The task is created unassigned in the board's project (`POST /tasks`), then put in the selected column's section (`POST /sections/{gid}/addTask`, which places it at the top). In the "(no section)" column, or on a board with no columns, it isn't put in any section.
- Once Asana confirms, the board reloads and the new card is selected. If a reload is already running, another one follows it, so the new card is never missed.
- **Failures never lose the name or make duplicates:**
  - If creating fails, an error toast is shown, and the next `Tab q` starts with the name typed before.
  - If the task was created but couldn't be put in the section, a toast says so (`Created, but not in <Section>: …`). The task is still on the board, in Asana's default section, and `m` can move it.
- CLI: `asn project new <project-gid> [--section <gid>] <name…>` prints the new task's GID. If the task was created but not put in the section, it still prints the GID, and it warns on stderr and exits 1.

### F15. @-mentions in comments and descriptions

Added after F14 at the user's request. It applies to both screens, because the editor is shared.

- **How Asana sees mentions:** a mention is rich text: `<a data-asana-gid="GID"/>` inside `html_text` (comments) or `html_notes` (descriptions). Asana shows it as a link to the person, task or project, and a mentioned person is notified. Plain `@Name` text is not a mention.
- **In the editor:** typing `@` opens a picker titled "Mention someone" over the editor. Typing searches people, projects and tasks with Asana's typeahead, as three requests at once with the 200 ms debounce. People are listed first, then projects, then tasks, each labelled. `Enter` inserts `@Name` at the cursor and remembers which GID it stands for. `Esc` inserts a plain `@`.
- **Saving:**
  - The text is turned into rich text only if it still contains a remembered `@Name`. Each one becomes the mention tag, and all other text is HTML-escaped and wrapped in `<body>…</body>`.
  - Text without mentions saves exactly as before, as plain `text` or `notes`.
  - A remembered `@Name` that was edited away is plain text.
  - Comment drafts keep their mentions, and `Ctrl-o` (`$EDITOR`) keeps them as long as the `@Name` text is left intact.
- **Descriptions:** the concurrency guard is unchanged. The formatting guard (F4) no longer counts mentions as formatting, because they are now kept: the editor shows them as `@Name`, remembers them, and saves them back as mentions. A description saved with a mention is written as `html_notes`, which replaces formatting just as a plain save does.
- **Reading:** Asana's plain `text`/`notes` show a mention as a URL (`https://app.asana.com/…/profile/<gid>` for a person, or a task/project URL). The task view (both screens and `asn task`) shows its label instead: `@Ada Lovelace` for a person, the name for a task or project.
  - The label comes from the matching mention link in `html_text`/`html_notes`, so no extra requests are made.
  - URLs that aren't mentions, and mentions without a label, stay as they are.
  - `--json` is unchanged.
- **CLI:** in `asn comment` and `asn desc`, `@` followed by a GID of 10+ digits (e.g. `@1200000000000011`) becomes a mention of that user, task or project. `asn users <query…>` prints matching people as `gid  name`, and `asn search` already finds tasks.

### F16. Complete a task (`d`)

Added after F15 at the user's request. It works on both screens, on the selected task or card.

- `d` toggles completion with `PUT /tasks/{gid}` `{completed}`. A task whose details are loaded uses their state. A board card is open by definition.
- **On a board**, a completed card leaves its column at once, because boards show open tasks. The board then reloads. In the Inbox, the task pane shows `✓ Completed`.
- **Undo:** the toast says `Completed "<name>" · d to undo`. Pressing `d` as the next key reopens that task, and on a board its card comes back selected. Any other key ends the undo.
- **Failure:** the card or state comes back and an error toast is shown.
- **CLI:** `asn task done <task-gid> [--undo]`.

### F17. Assign a task (`Tab a`)

Added after F15 at the user's request. `Tab a` is Asana's own shortcut, reserved in v0.1's F6.

- `Tab a` opens a people picker. "Me" and "Unassigned" are listed first, and typing searches people in the workspace (the user typeahead from F15). `Enter` assigns.
- The task is updated with `PUT /tasks/{gid}` `{assignee: <gid> | "me" | null}`. The task pane shows the assignee Asana returns, and a board reloads so the card's initials change.
- **CLI:** `asn task assign <task-gid> <user-gid | me | none>`.
- **CLI layout:** `asn task <gid> [--json]` keeps working unchanged. `assign` and `done` are subcommands next to it, and task GIDs are numeric, so they can't clash.

## Feature Spec — Config file and completion animation (F22–F23)

Added after F21 at the user's request (2026-11-17). F22 supersedes ADR-003 with [ADR-010](decisions/ADR-010-config-file-without-secrets.md).

### F22. Config file and first-run setup

- **Where:** `$XDG_CONFIG_HOME/asn/config.toml`, or `~/.config/asn/config.toml` when `XDG_CONFIG_HOME` is unset or empty. The same path on macOS and Linux.
- **Keys** (all optional in the file; each has an environment variable that wins over it):

  | Key | Env | Values | Default |
  |---|---|---|---|
  | `workspace` | `ASANA_WORKSPACE` | numeric GID, as a string | none (required) |
  | `token_cmd` | `ASANA_TOKEN_CMD` | a command that prints the token | none |
  | `search` | `ASN_SEARCH` | `"tasks"`, `"typeahead"` | `"tasks"` |
  | `theme` | `ASN_THEME` | `"terminal"`, `"dark"`, `"light"`, or a theme file path | `"terminal"` |
  | `animations` | `ASN_ANIMATIONS` (`on`/`off`) | `true`, `false` | `true` |

- **Precedence:** a non-empty environment variable, then the file, then the default. `ASANA_TOKEN` still wins over any `token_cmd`. `NO_COLOR` stays environment-only.
- **No secrets in the file:** a `token` key is an error: `config.toml: "token" isn't allowed — put a command that prints it in token_cmd, or set ASANA_TOKEN`. Unknown keys and wrongly typed values are errors that name the file, the key and the line (exit 1).
- **Errors name both places:** e.g. `ASANA_WORKSPACE is not set and config.toml has no workspace — run asn init, or …`, and a bad value says where it came from (`workspace in config.toml must be a numeric GID, …`). A missing token keeps exit code 2.
- **`asn init`:** an interview on the terminal, then the file is written.
  1. If the file exists: `~/.config/asn/config.toml exists. Replace it? [y/N]`. Anything but `y` leaves it untouched (exit 0).
  2. **Token command:** explains that the token is never stored and shows examples (`op read …`, `pass show …`, and on macOS `security find-generic-password -s asn -w` with the matching `add-generic-password` line). The default offered is `$ASANA_TOKEN_CMD` if set. An empty answer is accepted only when `ASANA_TOKEN` is set (the file then has no `token_cmd`). The command is run once to check it prints something.
  3. **Workspace:** fetched with `GET /workspaces` using that token. One workspace is chosen and shown. Several are listed with numbers and you pick one; the default is `$ASANA_WORKSPACE` if it's among them. A failed request (e.g. 401) shows the error and goes back to step 2.
  4. The file is written with mode 0600 (directories created as needed), with the chosen `workspace` and `token_cmd`, and the other keys present as commented-out defaults so they can be found. Then: `Saved ~/.config/asn/config.toml`.
  - stdin and stdout must be terminals; otherwise `asn init` exits 1 with `asn init needs a terminal`. Ctrl-D or Ctrl-C at any prompt aborts without writing.
- **First run of the TUI:** `asn` with no subcommand, no config file, and stdin/stdout on a terminal runs the same interview before the TUI starts, then continues with the new settings. CLI subcommands never prompt; without a file they behave exactly as before.

### F23. Completion animation

- When `d` completes a task on a board, its card plays a short animation before it leaves its column, instead of disappearing at once (F16):
  1. **Strike (about 240 ms):** a strike-through sweeps across the name from left to right in a band of colours that moves with it (roles `celebrate_1`–`celebrate_4`, one per letter, cycling), and the second line reads `✓ done`, cycling through the same colours.
  2. **Collapse (about 160 ms):** the card shrinks to its name line, then is gone, and the cards below move up.
- Frames come every 40 ms. Other keys keep working during the animation. The cursor stays on the card until it leaves, then behaves as in F16.
- **Undo:** `d` during the animation (the next key, as in F16) reopens the task: the animation stops and the card stays as it was.
- **Board reloads** that arrive during the animation are applied when it ends, so the card isn't cut off mid-way.
- **Failure:** if Asana refuses the completion, the card is restored (as in F16), whether or not the animation has finished.
- **Off switch:** `animations = false` in the config file or `ASN_ANIMATIONS=off` gives F16's instant behaviour.
- The Inbox and search task pane are unchanged: they show `✓ Completed` at once, and nothing disappears there.
- Styles come from theme roles: four new ones, `celebrate_1`–`celebrate_4` (magenta, yellow, green and cyan in `terminal`; the matching gruvbox colours in `dark` and `light`; no colours under `NO_COLOR`), and `text_muted` with a crossed-out modifier during the collapse. Added at the user's request (2026-11-17).

**Tests (F22–F23):**
- `config.rs`: precedence (env over file over default) for every key; `token` rejected; unknown key and wrong type rejected with the key named; `XDG_CONFIG_HOME` and `HOME` path resolution; a missing file is the old env-only behaviour.
- The interview, driven by scripted input and output: single and multiple workspaces, replace declined, empty token command with and without `ASANA_TOKEN`, a failing token command, a 401 from `GET /workspaces` (wiremock), and the written file (contents and mode 0600).
- `tests/cli.rs`: `asn init` without a terminal exits 1; a subcommand never prompts.
- App tests: the animation's state from `d` through the frames to removal; undo during it; a reload during it is held; a failure restores; animations off removes at once.
- Snapshots: a card mid-strike and mid-collapse.

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
  - persist recents or caches to disk (the F22 config file, written only by `asn init`, is the one exception; ADR-010)
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

- [x] **A1: `GET /projects/{gid}/tasks` returns tasks in board order within each section.** Confirmed by the user on real projects (2026-11-17); a large project also loads well under 2 s. Test: compare `asn board --json` with the web board on 2–3 projects before building the TUI. If it fails, load each section with `GET /sections/{gid}/tasks` (one request per section, which still fits the budget).
- [x] **A2: 28-cell cards with name + initials + due are enough to glance at.** Confirmed by the user at Checkpoint B (2026-11-17), cards unchanged. Test: dogfood on the widest and narrowest terminals in use. If not, first try 3-line cards, before considering a stacked layout.
- [x] **A3: Initials are unambiguous enough in a project's assignee set.** No problem found at Checkpoint B. Test: dogfood. If two people share initials, fall back to the first name.
- [ ] **A4: Hiding completed tasks matches how the board is read.** Test: dogfood. The web board collapses them by default.
- [x] **A5: The detail and action code can be keyed by task GID instead of an Inbox item.** Confirmed in planning: `selected_task_gid()` was already that seam (search used it); T5 made it return the selected card. Test: check the `Detail`/`Effect` paths in `tui/app.rs` during planning. If not, separating them is the first task.

- [x] **M1: Asana accepts a mention written as `<a data-asana-gid="GID"/>` in `html_text`/`html_notes`, keeps line breaks (`\n`) inside `<body>`, and notifies a mentioned person.** Test, before T14: `asn comment <task> -m $'Line one @<your-user-gid>\nLine two'` on a test task, then check it in the web app. Confirmed by the user (2026-11-17): the mention links and notifies, the line break is kept, and `&`/`<` show literally.

## Not Doing (and Why)

- **Stacked/list layout or a `v` toggle:** one renderer first. The sections → tasks model leaves room for one later.
- **Custom fields, tags, subtask/comment counts on cards:** assignee and due date are what a glance needs, and narrow cards depend on that.
- **Reordering cards, editing sections:** these are workflow, not status. `m` already covers moving between sections, and `o` covers the rest.
- **Section summaries and "what changed since I last looked":** strong ideas for glancing, but post-MVP. Revisit if scanning 300 cards turns out slow.
- **Persisted recents, pinned projects (`ASN_PROJECTS`), Events API sync, local mirror:** 60 s polling and session recents cover the job, and nothing is written to disk.
- **Showing completed tasks:** they are hidden, to keep columns about current work.
- **Multiple boards side by side, portfolios, My Tasks board:** one project at a time. "My Tasks" as a startup view stays a separate idea.

## Findings from Use

- Checkpoints A–C (2026-11-17): board order, load time, card readability and every success criterion confirmed by the user on real projects.
- The editor and pickers were too small on large terminals; they now grow with the terminal, and the editor soft-wraps long lines (a change outside this spec, made at the user's request during Checkpoint C).

## Open Questions

1. Should `asn --project` also accept a project **name** (resolved through typeahead), or only a GID? GID only is assumed for v0.2.
