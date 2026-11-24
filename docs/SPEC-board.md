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
| Single-select fields (F24) | `GET /projects/{gid}/custom_field_settings?opt_fields=custom_field.name,custom_field.resource_subtype,custom_field.enum_options.name,custom_field.enum_options.enabled` | Follows `next_page`. Only `enum` fields are kept. |
| Card values (F24) | `custom_fields.gid,custom_fields.enum_value.gid` added to the cards' `opt_fields` | |
| Set a field (F25, F27) | `PUT /tasks/{gid}` `{custom_fields: {<field gid>: <option gid> \| null}}` | The task's own fields and options come from `GET /tasks/{gid}` (`custom_fields.resource_subtype,enum_value.name,enum_options.name,enum_options.enabled`). |

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
| `v` | board | columns ↔ lanes by each single-select field (F26) | asn |
| `F` | Inbox, board | set a single-select field (F27) | asn |

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

F18–F21 (asn in Claude Code) are specified in [SPEC-skill.md](SPEC-skill.md).

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
- **First run of the TUI:** `asn` with no subcommand, no config file, and stdin/stdout on a terminal runs the same interview before the TUI starts, then continues with the new settings. It doesn't when the environment already sets `ASANA_WORKSPACE` and `ASANA_TOKEN` or `ASANA_TOKEN_CMD`: such setups keep working unchanged (ADR-010), and `asn init` still writes a file on request. CLI subcommands never prompt; without a file they behave exactly as before.

### F23. Completion animation

- When `d` completes a task on a board, its card plays a short animation before it leaves its column, instead of disappearing at once (F16):
  1. **Strike (about 240 ms):** a strike-through sweeps across the name from left to right in a band of colours that moves with it (roles `celebrate_1`–`celebrate_4`, one per letter, cycling), and the second line reads `✓ done`, cycling through the same colours.
  2. **Collapse (about 160 ms):** the card shrinks to its name line, then is gone, and the cards below move up.
- Frames come every 40 ms. Other keys keep working during the animation. The cursor stays on the card until it leaves, then behaves as in F16.
- **Undo:** `d` at any point while the animation plays, even after other keys, reopens the task: the animation stops and the card stays as it was. If Asana hasn't confirmed the completion yet, the reopen is sent once it does; another `d` before then keeps the completion and restarts the animation, with no second write.
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

## Feature Spec — Swimlanes and single-select fields (F24–F27)

Added after F23 at the user's request (2026-11-19), before v0.3.0. The user chose a grid of lanes, lanes by any single-select field, and editing single-select fields only.

### F24. Single-select fields on the board

- A board also loads the project's custom field settings (`GET /projects/{gid}/custom_field_settings`, in the same parallel load as the rest). Its **single-select fields** (Asana `resource_subtype` `enum`) are kept, each with its options in Asana's order. Disabled options are kept but never offered for editing. Other field types are ignored by F24–F27.
- Each card also carries its value for those fields (`custom_fields.gid`, `custom_fields.enum_value.gid`).
- **Request budget:** one load is 3 + ⌈tasks / 100⌉ requests (6 for 300 tasks), still polled only while the board is on screen.

### F25. CLI

```
asn board <project-gid> --lanes <field>        # tasks by lane, then section
asn task field <task-gid> <field> <option|none> # set (or clear) a single-select field
asn project fields <project-gid|name>          # a project's single-select fields and their options
```

- `--lanes` takes a field name or GID among the board's single-select fields, matched as in F19 (exact name ignoring case, else the only match; otherwise nothing is printed, exit 1, candidates on stderr). Text output: per lane, a line `option-gid  option-name` (empty gid and `(no <Field>)` for tasks without a value), then that lane's sections and tasks as in F13, each indented one more level. Every lane lists every section, so lanes line up.
- `--json`: each task in `tasks` now includes its `custom_fields` (field and option GIDs; additive, agreed 2026-11-19). With `--lanes`, the object also has `"lanes": [{"option": {gid, name} | null, "sections": [{"section": gid | null, "tasks": [task gids]}]}]`.
- `asn task field` resolves the field among the task's single-select fields and the option among that field's enabled options, both as in F19; `none` clears the field. It prints `<Field>: <Option>` (or `<Field>: none`). An unresolved name, a disabled option, or a field that isn't single-select writes nothing and exits 1.
- `asn project fields` prints each single-select field as `gid  name`, followed by its enabled options as indented `gid  name` lines.
- Fields, options and `--lanes` fields come from lists known up front, so a name that matches nothing lists *all* of them (not none), and a GID must be one of them.
- `asn task --json`: each custom field also includes `resource_subtype`, `enum_value` and `enum_options` (additive; needed to set single-select fields; agreed by the user 2026-11-19).

### F26. Lanes view (`v`)

- **Toggle:** `v` on a board cycles Columns → Lanes by each single-select field (a field named "Initiative" first, ignoring case, then the project's order) → Columns. A board without single-select fields shows the toast `No single-select fields on this board`. The layout is remembered per board for the session, like its cursor.
- **Layout:** the header reads `<Project> · <n> open · lanes: <Field>`. Each lane starts with one line across the board's section columns: `<Option> (n)` in the `title` role in the first visible column, a `border` line in the others. That line also separates the lane from the one above. Lanes follow the field's option order, then `(no <Field>)`; lanes without cards are hidden. Columns have the same widths and horizontal paging as F9, shared by all lanes, so they line up. A lane is as tall as its tallest cell; the whole grid scrolls vertically to keep the selected card visible.
- **Cursor:** `h`/`l` move between columns within the lane (an empty cell can be selected, as an empty column can in F9). `j`/`k` move within a cell and continue into the same column of the next or previous lane that has a card there. `gg`/`G` go to the first or last card of the column across lanes. `/`, `n`, `N` search in grid order. After a reload, a layout switch or a field change, the cursor stays on the same task GID.
- **Actions** work as in columns view: `Enter`, `c`, `Tab e`, `p`, `m`, `o`, `d` (the F23 animation plays in the cell), `Tab a`, `F`. `Tab q` creates the task in the selected cell's section **and sets the lane's option**, as a further request after F14's two. If that fails, a toast says `Created, but not in <Option>: …`, and the task is in `(no <Field>)`.
- The status bar's position counts the selected cell's cards (none on an empty cell). The hint line offers `v lanes` after the task actions, so narrow terminals drop it first.
- Card text, `NO_COLOR` and the bidi pass are as in F9.

### F27. Set a single-select field (`F`)

- `F` on a task (an Inbox task, a board card, or its detail) opens a picker titled `Set field` with the task's single-select fields as `<Field>: <current option or —>`. In the lanes view it starts with the lane field; with a single field it starts there too. Choosing a field lists its enabled options (the current one marked) and `(none)`. The picker works like the `m` picker (F10): `j`/`k` or `Ctrl-n`/`Ctrl-p` move, `Enter` chooses, `Esc` closes; there is nothing to type, since the lists are short. The help and the hint line list `F` (`F field`, after `assign`).
- `Enter` on an option writes it (`PUT /tasks/{gid}` `{custom_fields: {<field>: <option> | null}}`). On success a toast says `<Field>: <Option>`, the detail shows the new value, and a board reloads with the cursor following the card into its new lane. On failure an error toast is shown and nothing changes.
- A task without single-select fields gets the toast `No single-select fields on this task`.

**Tests (F24–F27):**
- Wiremock: field settings (single-select kept; multi-select and text dropped; pagination), card values, `set_field` by names and GIDs, clearing, and no `PUT` for an ambiguous or disabled option.
- `core::lanes`: option order, `(no <Field>)`, hidden empty lanes, every lane has every section, a task in two projects.
- `tui/board.rs`: grid navigation across uneven lanes and empty cells, paging, cursor by GID across layouts and reloads.
- App tests: the `v` cycle and its toast, the `F` picker stages, the write effect, the reload, the cursor following the card, `Tab q` in a lane.
- Snapshots: lanes fitting, lanes paged, a tall grid scrolled down, `NO_COLOR`, the option picker.
- CLI: `--lanes` text and JSON, `asn task field`, `asn project fields`, and exit 1 with candidates.

## Feature Spec — Fresh boards (F28–F30)

Added after F27 at the user's request (2026-11-24). A task created, moved or edited in the TUI can take minutes to show on the board and in the detail. Yet every such write already starts a board reload as soon as Asana confirms it (F14, F10, F17, F27; `reload_board` in `tui/app.rs`). So the 60 s poll alone can't explain minutes. The likely cause is that Asana's reads lag behind its writes. Asana's docs don't mention read-after-write consistency, so F28 measures it before anything changes.

**Problem:** How might we make every change show on the board and in the detail fast enough that you never wonder whether it worked?

**Target:** your own writes show at once and never flick back. Changes from elsewhere (the `asn` CLI, the Claude skill, the web app, teammates) show within the refresh interval (5 s by default), plus whatever lag F28 measures on Asana's side.

**Order:** F28 first, as the baseline, then F29, then F30. F28's results settle the points marked *(after F28)*. F28 runs again after F29 and after F30, and all three tables go into Findings from Use. No new dependencies: focus reports come from `crossterm`.

### F28. Latency probe

Two tools. Both write to a throwaway board, such as a personal to-do project. Its GID is given on the command line and is never committed.

- **API probe:** `cargo run --release --example latency -- <project-gid> [--runs 3] [--every 2]`.
  - Each run makes these writes on a new task named `asn latency probe <time> #<n>`, one at a time:
    1. create it, in the first section
    2. move it to the second section
    3. assign it to the token's own user
    4. set the board's first single-select field (skipped when there is none)
    5. complete it
  - It times how long Asana takes to confirm each write (`confirm-ms`). Then it reads the following every `--every` seconds, staggered, until each read shows the write or 5 minutes pass:
    1. `GET /projects/{gid}/tasks` with the board's exact query: what a board reload sees
    2. `GET /tasks/{gid}`: what a detail reload sees
    3. `GET /projects/{gid}/events` with a sync token: what Events would see (Not Doing, P3)
  - It prints one row per write: `write  confirm-ms  list-s  task-s  events-s`, as the median and the maximum over the runs, with `>300` for a timeout.
  - At the end it completes every task it created, even after a failure, rather than deleting them (agreed by the user, 2026-11-24). An interrupted run's open tasks are completed when the next run starts. It never assigns anyone but the token's own user.
  - At the default 2 s it sends about 90 requests a minute, under the free plan's 150.
  - Events are read with a plain request inside the example. No `api` method is added for them.
- **End to end (Rex):** `scripts/tui-latency.sh <project-gid>`. It is committed even though it needs Rex (agreed by the user, 2026-11-24).
  - It starts `target/release/asn --project <gid>` in a new Rex pane at 120×40 and waits for the board.
  - For each write below, it sends the keys and captures the pane every 200 ms until the change is on screen. It then keeps capturing for 2 minutes, to catch the change flicking back.
  - It prints `write  visible-ms  reverted` for each run.

  | Write | Keys | On screen when |
  |---|---|---|
  | create | `Tab q`, name, `Enter` | the card is in its column |
  | move | `m`, section, `Enter` | the card is in the other column |
  | assign | `Tab a`, own name, `Enter` | the card shows the user's initials |
  | field | `F`, option, `Enter` | the detail shows the option |
  | description | `Tab e`, text, `Ctrl-s` | the detail shows the text |
  | comment | `c`, text, `Ctrl-s` | the detail shows the comment |
  | complete | `d` | the card is gone, and stays gone |

- **Decision after F28** (assumptions P1–P3):

  | F28 shows | Then |
  |---|---|
  | The list read shows writes within about 2 s | The delay comes from `asn`, not Asana. Reproduce it in a failing app test and fix that bug first. Keep only the parts of F29 the fix leaves stale. |
  | The list read lags, the task read doesn't | F29, without its detail half. |
  | Both lag | F29 as written. |
  | Events arrive in under 5 s (median) | Reconsider Events as a cheaper way for F30 to notice changes. |

### F29. Own writes show at once

**Superseded at Checkpoint F1 (2026-11-24).** F28 found Asana's reads fresh: the board's list showed every write on its first read, 0.4–0.7 s after Asana confirmed it. So no reload ever undoes a confirmed write, and the pending changes below would guard against something that doesn't happen. They are in Not Doing. What F28 did find, a bug in `asn`, is fixed by F29a. The rest of this section is kept as it was written.


- When Asana confirms a write made in the TUI, the board and the detail show its result right away. They use what the app already has (the request and Asana's response), not a reload:

  | Write | Board | Detail |
  |---|---|---|
  | create (`Tab q`, F14, F26) | the card at the top of its section, in its lane, selected | — |
  | move (`m`) | the card at the top of its new column | the new section |
  | assign (`Tab a`) | the new initials | the new assignee (as today) |
  | field (`F`) | the card in its new lane | the new option |
  | complete or reopen (`d`) | the card gone or back (F16, F23) | `✓ Completed` or not |
  | description (`Tab e`) | — | the new text (already shown while saving) |
  | comment (`c`) | — | the comment (already shown while posting) |

- **A stale read can't undo them.** Each confirmed write is kept as a pending change to that task. Every board load and detail load passes through the pending changes before it's shown. This is how the Inbox already keeps read marks that Asana hasn't confirmed (`apply_pending_marks`). A pending change ends when, as applicable:
  - **confirmed:** a load shows the written value, the new card, the missing completed card, or the posted comment.
  - **superseded:** a load shows a value that is neither the written one nor the one before it, because someone changed it since.
  - **expired:** 10 minutes have passed (*after F28*: check this against the worst lag measured).

  Once a change ends, loads are shown as they come. A later write to the same field replaces the pending change. A failed write never creates one, and today's failure handling (restore and toast) stays as it is.
- The reload after each write stays. It's how a pending change gets confirmed, and it brings in other changes too.
- Nothing on screen marks a change as pending.
- **Where:** a pure module, `tui/pending.rs`, as `board.rs` is. `app.rs` applies it in `board_loaded` and on `DetailLoaded`. F29 sends no new requests.

### F29a. Own writes refresh the task's detail

Found by F28 (2026-11-24). A task's detail is cached as soon as its card or notification is selected, with the task's `modified_at`. Saving a description compares that `modified_at` with Asana's, to avoid overwriting someone else's edit. But a move, an assignment, a completion, a comment (all of which change `modified_at`, checked on Asana) or adding the task to a project didn't refresh the cached detail. So a description saved after any of them, on the same task, was refused with `Description not saved: the task was changed on the web while you were editing`, and the detail showed the old text again.

- After Asana confirms one of these writes, the task's detail reloads if it is cached: move (`m`), assign (`Tab a`), complete or reopen (`d`), add to a project (`p`), comment (`c`). Setting a field (`F`) and saving a description already did. The reload brings Asana's `modified_at`, fields and comments. It is the existing detail load (3 requests), not a new one.
- A failed write doesn't reload anything, and its existing restore and toast are unchanged.


- **Interval:** while the terminal has focus, the Inbox, the board on screen and the shown task's detail reload every `refresh` seconds (default 5, chosen by the user, 2026-11-24). This replaces the fixed 60 s of F9 and the Inbox (ADR-005).
- **Config:** one new key, as in F22:

  | Key | Env | Values | Default |
  |---|---|---|---|
  | `refresh` | `ASN_REFRESH` | whole seconds, 5–60 | `5` |

  A value out of range, or not a number, is an error naming the key and where it came from, like the other keys. `asn init` writes it as a commented-out default.
- **Cost, stated plainly in the README, in `asn init`'s comment for the key, and here:**
  - One refresh is 1 (Inbox) + 3 + ⌈tasks / 100⌉ (board) + 3 (detail: task, stories, attachments) requests.
  - Asana limits each token per minute: **150 requests on the free plan, 1,500 on paid plans, Enterprise included.** The `asn` CLI and the Claude skill share the limit when they use the same token.

  | Board | Requests per refresh | Per minute at 5 s (default) | at 10 s | at 30 s |
  |---|---|---|---|---|
  | 100 tasks | 8 | 96 | 48 | 16 |
  | 300 tasks | 10 | 120 | 60 | 20 |
  | 500 tasks | 12 | 144 | 72 | 24 |
  | 1,000 tasks | 17 | 204 | 102 | 34 |

  - **Free plan:** the 5 s default fits boards up to about 300 tasks, with a little room left for the CLI. Above about 400 tasks, or with heavy CLI use alongside, set `refresh = 10` or more.
  - **Paid plans (Enterprise included):** 5 s fits any board, with plenty of room left.
  - **Over the limit:** Asana answers 429. `asn` waits for `Retry-After` and tries once more, as today. A second 429 fails that refresh: the board keeps its data and shows `Refresh failed: …` (F9), and the next refresh tries again. A CLI command run at the same moment can fail the same way.
- **Focus:** the TUI turns on the terminal's focus reports, and turns them off on exit, as it does raw mode.
  - In the background, it refreshes every 60 s, as today.
  - Back in front, it refreshes at once if the last refresh is older than the interval, then keeps to the interval.
  - A terminal that never reports focus counts as in front.
- `R` still refreshes at once. A reload asked for during a load is still queued (ADR-005), so faster ticks never run two loads of one board at the same time.
- **ADR-011** records the change to ADR-005: a configurable, focus-aware interval. It also says why Events still aren't used.

**Tests (F28–F30):**
- F28: CI builds the example (`clippy --all-targets`). Its "does this read show the write" checks are plain functions, unit-tested on fixture JSON.
- F29, `tui/pending.rs`: each write's effect on a board and on a detail; a change being confirmed, superseded and expired; a later write replacing an earlier one; a task in two projects.
- F29, app tests:
  - for each write, the card or detail changes on the confirming `Msg`, before any reload
  - a stale `BoardLoaded` or `DetailLoaded` after that doesn't undo the change
  - a confirming load ends the pending change
  - a failed write leaves nothing pending
  - a reload held during the completion animation (F23) still works
- F30, `config.rs`: `refresh` from the environment, the file and the default; range and type errors.
- F30, app tests:
  - while focused, ticks at the interval load the board
  - after focus is lost, the board loads once per 60 s
  - regaining focus loads at once, but only when the last refresh is older than the interval
  - `R` and the queued reload work as before
- No new snapshots: nothing is drawn differently.

**Done when:**
1. F28's three tables (before, after F29, after F30) are in Findings from Use.
2. In the Rex run, every write is on screen within the API probe's `confirm-ms` for that write plus one reload, 1 s at most (median over 3 runs). (Was `confirm-ms` + 200 ms, which needed F29; changed at Checkpoint F1.)
3. In the same runs, no change flicks back during its 2-minute watch.
4. A change made with `asn` in another pane shows in the focused TUI within the interval plus the list lag F28 measured.
5. In the background, the board loads at most once a minute (app test).
6. CI is green.

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

- [x] **L1: The project's custom field settings list the single-select fields its cards use (Initiative).** Confirmed by the user at Checkpoint L1 (2026-11-19). Test: Checkpoint L1, `asn project fields` and `asn board --lanes Initiative` against the web board. If cards use fields that aren't the project's, lanes can't show them; `F` still edits them through the task.
- [x] **L2: The grid stays readable at 80 columns with several lanes.** Confirmed by the user at Checkpoint L2 (2026-11-19). Test: Checkpoint L2. If not, first try hiding the lane separator line, before collapsing lanes.
- [ ] **L3: Option order is how the team reads lanes.** Test: dogfood. If not, consider sorting by card count.

- [x] **P1: Asana's list read (`GET /projects/{gid}/tasks`) lags behind writes by more than a few seconds.** Test: F28. If it doesn't, the delay is a bug in `asn`. Reproduce it in an app test and fix it first (F28's decision table). **Didn't hold** (F28, 2026-11-24): the list showed every write on its first read, 0.4–0.7 s after confirmation. The bug F28 found is F29a.
- [x] **P2: The single-task read (`GET /tasks/{gid}`) lags too.** Test: F28. If it doesn't, F29 drops its detail half. **Didn't hold:** the task read showed every write on its first poll.
- [x] **P3: Events arrive about a minute late.** Asana's docs say they are "delivered within a minute (on average)" and "at most once". Test: F28's `events-s` column. If they arrive in under 5 s, reconsider Events for F30. **Didn't hold:** events arrived in about 4 s (7.2 s at most). Reconsidered at Checkpoint F1 and still not used (Not Doing).
- [ ] **P4: The terminals in use report focus.** That includes Rex, and tmux only with `focus-events on`. Test: run F30 in each. If one doesn't, the TUI refreshes at the interval all the time, which costs requests but nothing else.
- [x] **P5: 10 minutes is longer than Asana's worst lag.** Test: F28's maximums. If not, raise F29's expiry. **Moot:** F29's pending changes aren't built (P1).

- [x] **M1: Asana accepts a mention written as `<a data-asana-gid="GID"/>` in `html_text`/`html_notes`, keeps line breaks (`\n`) inside `<body>`, and notifies a mentioned person.** Test, before T14: `asn comment <task> -m $'Line one @<your-user-gid>\nLine two'` on a test task, then check it in the web app. Confirmed by the user (2026-11-17): the mention links and notifies, the line break is kept, and `&`/`<` show literally.

## Not Doing (and Why)

- **Stacked/list layout or a `v` toggle:** one renderer first. The sections → tasks model leaves room for one later.
- **Custom fields, tags, subtask/comment counts on cards:** assignee and due date are what a glance needs, and narrow cards depend on that.
- **Reordering cards, editing sections:** these are workflow, not status. `m` already covers moving between sections, and `o` covers the rest.
- **Section summaries and "what changed since I last looked":** strong ideas for glancing, but post-MVP. Revisit if scanning 300 cards turns out slow.
- **Persisted recents, pinned projects (`ASN_PROJECTS`), Events API sync, local mirror:** 60 s polling and session recents cover the job, and nothing is written to disk.
- **Showing completed tasks:** they are hidden, to keep columns about current work.
- **Multiple boards side by side, portfolios, My Tasks board:** one project at a time. "My Tasks" as a startup view stays a separate idea.

- **(F24–F27) Editing other field types** (multi-select, text, number, date, people): single-select covers Initiative; each other type needs its own editor.
- **(F24–F27) Lanes by multi-select fields:** a card would need to appear in several lanes.
- **(F24–F27) Lanes by assignee, section or due date:** only single-select fields for now.
- **(F24–F27) Editing field definitions or options, reordering or collapsing lanes:** done in Asana; the board shows the result.

- **(F28–F30) Events API:** Asana's docs promise events "within a minute (on average)" and "at most once". F28 measured about 4 s (7.2 s at most), so P3 failed and Events were reconsidered at Checkpoint F1. They would cut F30's requests to one per refresh plus a reload when something changed. But they need sync-token handling (a 412 means a full reload) and still a fallback poll, and they don't cover the Inbox. A 5 s poll already meets the target within the free plan for boards up to about 300 tasks. Revisit if F30's request cost bites.
- **(F28–F30) F29's pending changes:** F28 showed Asana's reads are fresh (P1, P2), so a reload never undoes a confirmed write. Applying writes locally would save the one reload after each write (0.3–0.8 s measured) at the cost of an overlay on every load. Revisit if a lag shows up.
- **(F28–F30) Fetching only the changed task after a write:** with F29, the reload after a write only confirms, and one load path is simpler than two. Revisit if F30's request cost bites.
- **(F28–F30) Retrying the reload after a write until it shows the change:** F29 makes a stale reload harmless, and F30 brings the next one within seconds.
- **(F28–F30) Webhooks:** they need a public HTTPS endpoint, which a terminal app doesn't have.
- **(F28–F30) The CLI telling a running TUI to reload:** it needs a local socket or file, for a case F30 already covers.
- **(F28–F30) A "synced n s ago" line, or a mark on pending cards:** the point is that you don't need to know.
- **(F28–F30) A request budget instead of an interval:** people think in seconds, and the cost per refresh is documented instead.
- **(F28–F30) Refreshing boards that aren't on screen:** as today, only the board on screen polls.
- **(F28–F30) `p` onto the board on screen, in F29:** Asana picks the section, so the card waits for the reload, as today.

## Findings from Use

- Checkpoints A–C (2026-11-17): board order, load time, card readability and every success criterion confirmed by the user on real projects.
- The editor and pickers were too small on large terminals; they now grow with the terminal, and the editor soft-wraps long lines (a change outside this spec, made at the user's request during Checkpoint C).
- Checkpoints L1 and L2 (2026-11-19): `asn board --lanes`, `asn task field` and `asn project fields` matched the web app on real projects; the lanes view, `v` and `F` worked at 80, 120 and 200 columns.
- Building F26–F27: the help overlay was full, so `j / k` and `n / N` now share a line each (one spare row is left); the board's status position counts the selected cell; the grid never scrolls past its end; the `F` picker works like `m` (no typed filter).

- **F28 baseline (2026-11-24), on a throwaway board of about 20 open tasks with five sections and no single-select field.**
  - API probe, 3 runs (median/max). Confirmation: create 4237/4865 ms (two requests), move 1817/2001 ms, assign 2088/2600 ms, complete 2390/2404 ms. Then the list read showed each write in 0.4–0.7 s (its first read), the task read on its first poll, and events in about 4 s (7.2 s at most).
  - Rex harness, on both the Homebrew v0.3.0 binary and HEAD: create on screen 4.7 s after `Enter`, move 2.5–2.8 s, assign 2.5–3.1 s, comment and completion at once. Nothing flicked back, except the description: shown at once, but then refused as changed on the web and shown as before (F29a).
  - So the TUI shows its own writes Asana's confirmation time plus one reload after the key, and Asana's confirmation is most of it. A delay of minutes didn't reproduce in a fresh session.

## Open Questions

1. Should `asn --project` also accept a project **name** (resolved through typeahead), or only a GID? GID only is assumed for v0.2.
2. (F14) Create in one request? `POST /tasks` with `memberships: [{project, section}]` creates the task in its section in one request: 2.0 s instead of 4.2 s, measured on 2026-11-24. But Asana puts the task at the **bottom** of the section, where today's `addTask` puts it at the top (F14). Should `Tab q` and `asn project new` trade the top position for about 2 s?
