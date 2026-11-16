# Tasks: asn v0.1

Plan: [`tasks/plan.md`](plan.md). Spec: [`docs/SPEC.md`](../docs/SPEC.md). Feature IDs (F1–F8) refer to the spec.

**Definition of done (every task):**
- The acceptance criteria are met and verified at runtime.
- New behavior has tests that fail without the change.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` are clean.
- No token appears in any output, log or error.
- `docs/SPEC.md` is updated if behavior had to deviate from it.

---

## Phase 0: De-risk

### - [ ] T0: Validate Notifications API access and capture fixtures

**Description:** Before writing any Rust, prove that the core assumption holds and collect real data to build against. Run `curl` with a team member's PAT against the Notifications, task and stories endpoints. Then anonymize the responses into fixtures. The same session covers the three cheap spec checks: the terminal color query in Rex, the Asana web keys, and how often descriptions contain rich text.

**Acceptance criteria:**
- [ ] `GET /users/me/notifications?workspace=$ASANA_WORKSPACE` returns 200 with a PAT. If not, stop and escalate.
- [ ] `tests/fixtures/` contains anonymized JSON for:
  - a notifications page with `next_page` and one without
  - a `markRead` response, including one with `failed_gids`
  - a task with plain `html_notes` and a task with rich `html_notes`
  - a stories list
  - a typeahead result
  - sections
- [ ] `tests/fixtures/README.md` records:
  - `work_objects` resource-type counts and rich-description counts from about 200 notifications
  - the Rex OSC 11 reply
  - whether `U` (move to Inbox) works in the web Inbox

**Verification:**
- [ ] Manual: grep the fixtures for real names, emails, GIDs and workspace names. None remain.
- [ ] Manual: update `docs/SPEC.md` assumptions with what was found (checkboxes ticked or the spec revised).

**Dependencies:** None (needs the user's token, e.g. `export ASANA_TOKEN="$(pass show asana/pat)"`, and `ASANA_WORKSPACE`)

**Files likely touched:** `tests/fixtures/*.json`, `tests/fixtures/README.md`, `docs/SPEC.md`

**Estimated scope:** S

---

## Phase 1: Scriptable core

### - [ ] T1: Cargo scaffold, env config, error and exit-code contract

**Description:** Create the crate (`asn` binary, library in `src/lib.rs`) with a clap skeleton. Load the token (from `ASANA_TOKEN`, or by running `ASANA_TOKEN_CMD` as described in the spec) and `ASANA_WORKSPACE` into a `Config` whose `Debug` output redacts the token. Define the error type and how errors map to exit codes (0 ok, 1 API/usage, 2 auth). Running `asn` with no subcommand prints "TUI not implemented yet" for now.

**Acceptance criteria:**
- [ ] If either var is missing, `asn inbox` exits with code 2 (token) or 1 (workspace) and the exact messages from the spec, before making any network call.
- [ ] `format!("{:?}", config)` never contains the token value (unit test).
- [ ] `ASANA_WORKSPACE=gid:1234567890123456` exits with code 1 and suggests `1234567890123456`, without making a request.
- [ ] Token precedence: `ASANA_TOKEN` beats `ASANA_TOKEN_CMD`. `ASANA_TOKEN_CMD="printf 'tok\nextra'"` yields `tok`. A non-zero exit or empty output exits with code 2 and doesn't echo the command's stdout.
- [ ] `asn --help` lists the subcommands from F7 (stubs are fine).

**Verification:**
- [ ] `cargo test` (config tests use an injected env map, not the process env)
- [ ] Manual: `env -u ASANA_TOKEN cargo run -- inbox; echo $?` prints the message and `2`
- [ ] Manual: `ASANA_TOKEN_CMD="pass show asana/pat" cargo run -- inbox` triggers the gpg prompt if needed and then works

**Dependencies:** None (can start in parallel with T0)

**Files likely touched:** `Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/config.rs`, `src/error.rs`

**Estimated scope:** S

### - [ ] T2: API client and `asn inbox`

**Description:** Build `api::Client`, a reqwest client with bearer auth, base URL injection (for wiremock), and typed errors:
- 401 → auth error
- 403/404 → a message naming `ASANA_WORKSPACE`
- 429 → wait for `Retry-After`, then retry once

Add the `Notification` types: unknown enum values stay as strings, and nullable `story` and `created_by` are handled. Add `core::inbox(limit, offset, unread_only)` and the `asn inbox [--unread] [--limit N] [--json]` command. Text output is one line per notification. `--json` prints the `data` array exactly as Asana returned it.

**Acceptance criteria:**
- [ ] `asn inbox --json` output equals the fixture's `data` (round-trip test).
- [ ] An unknown `story_resource_subtype`, a `null` story or actor, and non-task `work_objects` all parse without error.
- [ ] 429 with `Retry-After: 1` is retried once and then succeeds. A second 429 surfaces as an error (wiremock tests).

**Verification:**
- [ ] `cargo test` (wiremock integration tests in `tests/api_inbox.rs`)
- [ ] Manual: `asn inbox --unread` against real Asana lists the same items as the web Inbox

**Dependencies:** T0 (fixtures), T1

**Files likely touched:** `src/api/mod.rs`, `src/api/notifications.rs`, `src/core/mod.rs`, `src/cli/inbox.rs`, `tests/api_inbox.rs`

**Estimated scope:** M

### - [ ] T3: `asn read` (mark read/unread, batched)

**Description:** Add `core::mark_read(gids, read: bool)`, which splits the GIDs into batches of up to 50 and calls `markRead` or `markUnread`, collecting `failed_gids`. Add the `asn read <gid>... [--unread]` command. It exits 1 and lists the failed GIDs on stderr if any fail.

**Acceptance criteria:**
- [ ] 120 GIDs result in 3 requests of 50, 50 and 20 (wiremock asserts the request bodies).
- [ ] `failed_gids` from any batch are reported, and the exit code is 1.
- [ ] Passing zero GIDs is a usage error, and no request is made.

**Verification:**
- [ ] `cargo test`
- [ ] Manual: `asn read <gid>` makes the item show as read in the web Inbox, and `--unread` reverts it

**Dependencies:** T2

**Files likely touched:** `src/api/notifications.rs`, `src/core/mod.rs`, `src/cli/read.rs`, `tests/api_read.rs`

**Estimated scope:** S

## Checkpoint A: core works against real Asana
- [ ] `asn inbox --unread --json | jq` works on real data. `asn read` round-trips with the web Inbox.
- [ ] fmt, clippy and tests are clean.

---

## Phase 2: TUI foundation

### - [ ] T4: Theme loader (roles, built-ins, user file, NO_COLOR)

**Description:** Implement `Theme` as a map from the fixed `Role` enum (the F8 list) to a ratatui `Style`, with color parsing done by ratatui's `Color::from_str`. Add three built-in TOMLs embedded with `include_str!`. Resolution order:
1. `NO_COLOR` wins.
2. `ASN_THEME` set to a built-in name.
3. `ASN_THEME` set to a file path, merged over its `base`.
4. Otherwise `terminal`.

The built-in `terminal` theme uses only ANSI colors and `reset` (tints come in T5).

**Acceptance criteria:**
- [ ] Each built-in parses and defines every `Role` (the test iterates all variants).
- [ ] A partial user file falls back to its `base`, and an unknown role or color errors with `file:line`.
- [ ] Under `NO_COLOR`, no style has a fg or bg color, `selection` is `reversed`, and unread is `bold`.

**Verification:**
- [ ] `cargo test` (unit tests in `theme.rs`, user files written to a temp dir)

**Dependencies:** T1

**Files likely touched:** `src/tui/theme.rs`, `themes/terminal.toml`, `themes/dark.toml`, `themes/light.toml`, `Cargo.toml`

**Estimated scope:** M

### - [ ] T5: Terminal color query and tints

**Description:** At startup, before entering the alternate screen, query the terminal's fg and bg with `terminal-colorsaurus` (100 ms timeout). Use the pure function `tints(fg, bg)` to blend the background toward the foreground:
- `selection` 12%
- `visual_selection` 20%
- `statusbar` 6%
- `border` 35%

Apply the tints to the `terminal` theme, and to user themes based on it, only for roles the user hasn't set. If there's no reply, keep the palette-only theme. The query is skipped under `NO_COLOR` and with any non-`terminal` base.

**Acceptance criteria:**
- [ ] `tints()` gives the expected RGB for one dark pair and one light pair (unit test).
- [ ] A user theme that sets `selection` keeps its own value. Unset tint roles get the derived values.
- [ ] With no reply (stdin not a TTY in tests, or a timeout), startup continues with the palette-only theme and adds no visible delay beyond the 100 ms budget.

**Verification:**
- [ ] `cargo test`
- [ ] Manual in Rex: selection and status bar shading match the terminal theme. After switching the Rex theme between light and dark and restarting `asn`, it still looks right.

**Dependencies:** T4

**Files likely touched:** `src/tui/theme.rs`, `src/tui/tint.rs`, `Cargo.toml`

**Estimated scope:** S

### - [ ] T6: Keymap table and key parser

**Description:** Implement `keys.rs` with a single static table of `(sequence, Action, Origin, help text)` covering all of F6. Add a parser state machine that turns crossterm `KeyEvent`s into `Action`s. It handles count prefixes (`5j`, `12G`), multi-key sequences (`gg`, `Ctrl-w h`, and `Tab c/p/i`), and a pending state (shown as `Tab-` or `5` in the status bar). `Esc` cancels a pending sequence, and an unbound key in a pending state cancels without acting. The parser is context-aware: the same key can mean different things in the list, in the picker, and in a confirm prompt.

**Acceptance criteria:**
- [ ] `5j`, `gg`, `12G`, `Ctrl-w l`, `Tab c`, `Tab` then `Esc`, and `Tab` then `z` all produce the right Action, or none.
- [ ] A test asserts that no two table entries share a sequence in the same context.
- [ ] Each F6 row is in the table with its origin (`vim`, `asana`, `asn`), so the help overlay can be generated from it later.

**Verification:**
- [ ] `cargo test` (pure unit tests, no terminal)

**Dependencies:** T1

**Files likely touched:** `src/tui/keys.rs`, `src/tui/mod.rs`

**Estimated scope:** M

### - [ ] T7: TUI Inbox list with navigation

**Description:** Running `asn` with no subcommand starts the TUI. Pieces:
- **Terminal guard:** raw mode, the alternate screen and a panic hook that always restore the terminal.
- **Tokio event loop:** merges key events, a tick, and effect results from a channel.
- **`App` state machine:** `(App, Action) → (App, Effects)`.
- **Rendering:** the two-pane layout from the mockup. The right pane is a placeholder for now. Includes the header and the status bar (mode, position, key hints) and uses `Theme` roles only.
- **Navigation:** loads the first page via `core::inbox`. `j`/`k`, counts, `gg`/`G`, `Ctrl-d`/`Ctrl-u` and `q` work. Loading and error states are visible, and the UI never blocks on the network.

**Acceptance criteria:**
- [ ] Navigation keys move the cursor correctly at list edges and with counts (state-machine unit tests).
- [ ] Quitting with `q`, with Ctrl-C, or through a forced panic leaves the terminal in its original mode (manual test plus a guard unit test where possible).
- [ ] `TestBackend` snapshot of the Inbox screen with fixture data and the `terminal` theme, plus one with `NO_COLOR`.

**Verification:**
- [ ] `cargo test`
- [ ] Manual: `cargo run` in Rex shows the real Inbox, and keys feel instant while loading

**Dependencies:** T2, T4, T6 (T5 optional: tints plug in without code changes)

**Files likely touched:** `src/tui/mod.rs`, `src/tui/app.rs`, `src/tui/ui.rs`, `src/tui/terminal.rs`, `src/main.rs`

**Estimated scope:** M

### - [ ] T8: TUI read actions, visual mode, filter, polling, pagination

**Description:** Add the F1 behavior on top of T7:
- `e`/`u`/`x` mark read, unread and toggle, optimistically, with rollback and an error toast on failure.
- `V` visual mode applies `e`/`u`/`x` to a range (through `core::mark_read` batching).
- `f` toggles between unread only (the default) and all.
- `R` and a 60 s timer refresh, keeping the cursor on the same notification GID.
- The next page loads when the cursor is within 10 rows of the end.
- Toasts appear in the status bar.

**Acceptance criteria:**
- [ ] Marking read in unread-only mode removes the row, and the cursor lands on the next item. A failed request restores it and shows a toast (state-machine tests with fake effect results).
- [ ] Refresh keeps the cursor on the same GID, or the nearest neighbor if that item is gone.
- [ ] Reaching the last 10 rows triggers exactly one next-page load until it resolves.

**Verification:**
- [ ] `cargo test`
- [ ] Manual: a real Inbox can be processed to empty with `j`/`k`/`e`/`V`, and the web Inbox reflects the read state

**Dependencies:** T3, T7

**Files likely touched:** `src/tui/app.rs`, `src/tui/ui.rs`, `src/tui/mod.rs`

**Estimated scope:** M

## Checkpoint B: daily triage possible
- [ ] Real Inbox processed to empty using only the keyboard. The terminal is always restored.
- [ ] Visual check in Rex: `terminal` (with tints), `dark`, `light`, `NO_COLOR`.
- [ ] **Human review before Phase 3.**

---

## Phase 3: Act on tasks

### - [ ] T9: Task detail (`asn task` and the detail pane)

**Description:** Add `core::task_detail(gid)`, which fetches the task (with spec `opt_fields`) and its stories in parallel and keeps only comments. Add `asn task <gid> [--json]`.

In the TUI:
- The detail pane loads after a 250 ms cursor dwell. Results are cached per task GID, and stale responses (the cursor moved on) are dropped.
- `h`/`l`, `Ctrl-w h/l`, `Enter` and `Esc` move focus. `j`/`k` scroll the focused pane.
- `o` opens `permalink_url` with the OS opener.
- Non-task work objects show compact info only.

**Acceptance criteria:**
- [ ] Moving quickly across 10 rows triggers only one detail fetch (debounce test with a fake clock).
- [ ] A response for a task that is no longer selected doesn't overwrite the current pane.
- [ ] `asn task <gid> --json` returns `{task, comments}`. Text output matches the pane's fields.

**Verification:**
- [ ] `cargo test` (wiremock for core, state-machine tests for debounce and focus, snapshot of the pane)
- [ ] Manual: browsing the real Inbox, the details are correct and `o` opens the right task

**Dependencies:** T8

**Files likely touched:** `src/api/tasks.rs`, `src/core/mod.rs`, `src/cli/task.rs`, `src/tui/app.rs`, `src/tui/ui.rs`

**Estimated scope:** M

### - [ ] T10: `$EDITOR` round-trip and add comment

**Description:** Add `editor.rs`, which:
1. resolves `$VISUAL`, then `$EDITOR`, then `vi`
2. writes a temp file
3. suspends the TUI (leaves the alternate screen and raw mode)
4. runs the editor and waits
5. restores the TUI
6. returns `Some(text)`, or `None` on an empty file or a non-zero exit (`:cq`)

Add `core::comment(task, text)` and `asn comment <gid> [-m TEXT | -]`, which takes text from `-m`, from stdin with `-`, or from the editor if neither is given. In the TUI, `c` and `Tab c` open the editor and post the comment optimistically into the pane, rolling back with a toast on failure.

**Acceptance criteria:**
- [ ] An empty buffer or a non-zero editor exit posts nothing (tested with `EDITOR` set to a stub script).
- [ ] After the editor exits, whether successfully or not, the TUI redraws fully and keys work.
- [ ] An optimistic comment is replaced by the server's version on success and removed on failure.

**Verification:**
- [ ] `cargo test`
- [ ] Manual: in Rex, a comment written in vim appears in the web app. `:cq` cancels.

**Dependencies:** T9

**Files likely touched:** `src/editor.rs`, `src/core/mod.rs`, `src/cli/comment.rs`, `src/tui/app.rs`, `src/tui/mod.rs`

**Estimated scope:** M

### - [ ] T11: Edit description with rich-text and concurrency guards

**Description:** Add `core::set_notes(task, text, expected_modified_at)`. It re-fetches `modified_at`. If that changed, it returns a `Conflict` error and keeps the user's text. Otherwise it sends `PUT notes`. Add a function that decides whether `html_notes` contains more than plain paragraphs.

In the TUI, `Tab e` opens the editor with the current `notes` and does nothing if the text is unchanged. If the task has rich text, a `y/N` confirm prompt appears first. On a conflict, a toast shows the path of the temp file that holds the user's text.

Add `asn desc <gid> [--file PATH | -] [--force]`. Without `--force`, it refuses rich-text tasks.

**Acceptance criteria:**
- [ ] The rich-text detector is true for links, lists, bold and mentions, and false for plain paragraphs (table test using T0 fixtures).
- [ ] On a conflict, no `PUT` is sent and the edited text survives on disk (wiremock).
- [ ] Unchanged text sends no request.

**Verification:**
- [ ] `cargo test`
- [ ] Manual: editing a plain task works. A formatted task asks for confirmation. Editing the task in the web app while vim is open triggers the conflict path.

**Dependencies:** T10

**Files likely touched:** `src/api/tasks.rs`, `src/core/mod.rs`, `src/cli/desc.rs`, `src/tui/app.rs`, `src/tui/ui.rs`

**Estimated scope:** M

### - [ ] T12: Add to project (fuzzy project picker, then section picker)

**Description:** Add `core::search_projects(query)` (typeahead), `core::sections(project)` and `core::add_to_project(task, project, section)`.

Add a reusable picker overlay:
- a query input with a 200 ms debounce
- `j`/`k` and `Ctrl-n`/`Ctrl-p` to move, `Enter` to select, `Esc` to cancel
- results that arrive out of order are ignored

`p` and `Tab p` open the project picker, which hides projects the task is already in. A section picker follows, with "no section" as the default. On success, the detail pane shows the new project.

Add `asn project add <task> <project> [--section GID]`.

**Acceptance criteria:**
- [ ] A task with no projects gets its first project. A task with projects keeps them and gains one (wiremock asserts `addProject` was used, never a replace).
- [ ] Typing `web` quickly sends one typeahead request. A late response for `we` doesn't replace the results for `web`.
- [ ] Projects the task is already in never appear in the picker.

**Verification:**
- [ ] `cargo test`
- [ ] Manual: adding a real task to a second project and section shows correctly in the web app

**Dependencies:** T9 (can run in parallel with T10/T11)

**Files likely touched:** `src/api/projects.rs`, `src/core/mod.rs`, `src/cli/project.rs`, `src/tui/picker.rs`, `src/tui/app.rs`

**Estimated scope:** M

## Checkpoint C: feature complete
- [ ] The full flow works against real Asana: Inbox, open, comment, edit description, add to project, mark read.
- [ ] Every TUI action has a tested CLI counterpart.
- [ ] **Human review before Phase 4.**

---

## Phase 4: Finish and ship

### - [ ] T13: `/` search and the `:` command line

**Description:** `/` opens a query in the status bar that filters the focused list as you type (case-insensitive substring on actor, action and task name). `Enter` keeps the filter, `n`/`N` jump between matches, and `Esc` clears it.

`:` opens a command line with `:q`, `:refresh`, `:read-all` (marks all loaded unread items read, through batching) and `:unread`. An unknown command shows an error toast.

**Acceptance criteria:**
- [ ] Filter, `n`/`N` wraparound and `Esc` restore work as described (state-machine tests).
- [ ] `:read-all` uses batched `markRead` and shows how many items succeeded and failed.
- [ ] `:foo` shows `Unknown command: foo` and changes nothing.

**Verification:**
- [ ] `cargo test`

**Dependencies:** T8

**Files likely touched:** `src/tui/app.rs`, `src/tui/ui.rs`, `src/tui/keys.rs`, `src/tui/command.rs`

**Estimated scope:** M

### - [ ] T14: `?` help overlay and the narrow (<100 cols) layout

**Description:** `?` shows an overlay generated from the `keys.rs` table, grouped by context, with an Asana column marking the web aliases and the note that `e` doesn't archive in the web app. Below 100 columns, the panes stack: only one is visible, `Enter` and `Esc` switch between them, and resizing switches layouts live.

**Acceptance criteria:**
- [ ] Every keymap entry appears in the overlay, generated from the table rather than listed by hand (test: count of entries equals table length).
- [ ] Snapshots at 80 and 120 columns. Resizing across 100 columns keeps the selection and focus.

**Verification:**
- [ ] `cargo test`
- [ ] Manual: resize a Rex pane live

**Dependencies:** T9

**Files likely touched:** `src/tui/ui.rs`, `src/tui/app.rs`, `src/tui/help.rs`

**Estimated scope:** S

### - [ ] T15: CI, release binaries, README

**Description:**
- GitHub Actions CI runs fmt, clippy and test on macOS and Linux.
- A tag-triggered release workflow builds `asn` for macOS (arm64 and x86_64) and Linux (x86_64 and arm64) and attaches the binaries to a GitHub Release.
- The README covers install, the required env vars (with the `curl` command for finding the workspace GID), the keymap (or a link to `?`), theming with an example user theme, and the CLI.
- Check that the crates.io name is free.

**Acceptance criteria:**
- [ ] CI is green on a PR. Pushing a `v*` tag produces a release with 4 binaries.
- [ ] Following only the README on a clean machine gets a teammate from zero to their Inbox.

**Verification:**
- [ ] Manual: a teammate follows the README end to end

**Dependencies:** T13, T14, and the open question on package name and Homebrew

**Files likely touched:** `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `README.md`, `Cargo.toml`

**Estimated scope:** M

## Checkpoint D: v0.1 released
- [ ] All spec acceptance criteria are met. `v0.1.0` is tagged and binaries are attached.
- [ ] 3 teammates are dogfooding (the spec's success metric).
