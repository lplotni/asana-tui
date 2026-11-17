# asana-tui — Spec v0.1 (Inbox MVP)

## Problem Statement

How might we let terminal-native engineers process their Asana Inbox — read what changed, act on the task, reply — without opening the web app, and faster than the web app?

## Target User & Success

- **Primary user:** engineers on our team who live in a terminal and use vim. Built in the open (OSS), but our team's workflow wins every trade-off for now.
- **Success (6 weeks):** ≥ 3 teammates process their Asana Inbox in `asn` on most working days, and leave the web Inbox tab closed.

## Principles

1. **Triage, not browse.** The home screen is an inbox you work through and leave. Every screen is optimized for "see → act → next".
2. **Scriptable core, thin TUI.** Every action exists as a library function and a CLI subcommand with stable `--json` output *before* it gets a key binding. The TUI is one consumer of the core, never the only one.
3. **Vim-native.** All navigation uses vim motions. Text is written in `$EDITOR`, not in a home-grown text box.
4. **Credentials configured through the environment only.** Either the token itself or a command that prints it (for a password manager). Never written to disk, never logged.
5. **Themeable from day one.** Rendering code never names a color. Every styled element uses a named theme role, so a new theme never requires a code change.

## Recommended Direction

Ship one vertical slice: **Inbox → task detail → act**.

`asn` with no arguments opens the TUI on the Inbox (Asana Notifications API, `GET /users/me/notifications`). Selecting a notification shows the referenced task's details and comments in a side pane. From there the user can add a comment, edit the description, and add the task to a project, all without leaving the keyboard. Long-form text (comments, descriptions) opens in `$EDITOR`, which gives vim users full vim for free and keeps the TUI small.

The same operations are available as `asn <subcommand>` for scripting (`asn inbox --unread --json | jq …`). Project board views come in v0.2, built on the same core.

## Asana API Surface (MVP)

| Need | Endpoint | Notes |
|---|---|---|
| Inbox | `GET /users/me/notifications?workspace=…&limit=…&offset=…&opt_fields=…` | Newest first, cursor via `next_page.offset`. Pages can be short or even empty, and only `next_page == null` ends the list. Offsets expire after 15 min. Polling only; no webhooks or events. |
| Mark read / unread | `POST /users/me/notifications/markRead` / `markUnread` | 1–50 GIDs per call; check `data.failed_gids`. Syncs with the web Inbox. |
| Task detail | `GET /tasks/{gid}?opt_fields=name,notes,html_notes,assignee.name,due_on,completed,projects.name,memberships.section.name,permalink_url` | |
| Comments | `GET /tasks/{gid}/stories` (filter `type == comment` client-side) | |
| Add comment | `POST /tasks/{gid}/stories` `{text}` | |
| Edit description | `PUT /tasks/{gid}` `{notes}` | See the rich-text caveat below. |
| Project picker | `GET /workspaces/{gid}/typeahead?resource_type=project&query=…` | Works on all plans, unlike search. |
| Add to project | `POST /tasks/{gid}/addProject` `{project, section?}` | |

Notification fields we rely on: `gid`, `is_read`, `created_at`, `story_resource_subtype`, `story.text` (nullable), `created_by.name` (nullable), `work_objects[]` (check `resource_type` before use). Unknown `story_resource_subtype` and `priority_classification` values are displayed as-is and must never crash parsing (`#[serde(other)]` or plain strings).

## Configuration

Environment variables only. The only file `asn` ever reads is an optional theme file that the user points to with `ASN_THEME`.

| Variable | Required | Purpose |
|---|---|---|
| `ASANA_TOKEN` | one of these two | Personal Access Token, sent as `Authorization: Bearer`. |
| `ASANA_TOKEN_CMD` | one of these two | A shell command that prints the token, e.g. `pass show asana/pat`, `op read op://Private/Asana/token`, or `security find-generic-password -s asana -w`. Used only when `ASANA_TOKEN` is unset. |
| `ASANA_WORKSPACE` | yes | Workspace GID, digits only (e.g. `1234567890123456`, not `gid:1234567890123456`). No auto-detection. |
| `EDITOR` / `VISUAL` | no | Editor for comments and descriptions. Falls back to `vi`. |
| `ASN_THEME` | no | A built-in theme name (`terminal`, `dark`, `light`) or a path to a theme TOML file. Defaults to `terminal`. See F8. |
| `NO_COLOR` | no | If set to any non-empty value, `asn` disables all colors and keeps only modifiers such as bold and reversed ([no-color.org](https://no-color.org)). This overrides `ASN_THEME`. |

**Resolving the token:** `ASANA_TOKEN` wins if it is set and non-empty. Otherwise `ASANA_TOKEN_CMD` runs once at startup with `sh -c`, before the TUI takes over the terminal, so a gpg or 1Password prompt works normally:
- Its stdin and stderr are inherited, and its stdout is captured.
- The first line of stdout, trimmed, is the token.
- If the command exits non-zero or prints nothing, `asn` exits with code 2: `ASANA_TOKEN_CMD failed (exit N)`. The command's own stderr has already appeared on the terminal, and its stdout is never echoed.

There is no timeout, because the user may be typing a passphrase.

The token is never printed, logged, written to disk, or included in error messages. Requests time out (10 s to connect, 30 s in total), so a stalled connection can't freeze refreshes. Text from Asana printed by the CLI has control characters and bidi overrides neutralized, so comments can't inject terminal escape sequences. If neither variable is set, or a 401 response comes back, `asn` exits with: `No valid Asana token — set ASANA_TOKEN or ASANA_TOKEN_CMD (create a PAT at https://app.asana.com/0/my-apps)`. If `ASANA_WORKSPACE` is missing, `asn` exits before making any request: `ASANA_WORKSPACE is not set — find your workspace GID with: curl -H "Authorization: Bearer $ASANA_TOKEN" https://app.asana.com/api/1.0/workspaces`. A value that isn't all digits is rejected before any request, and the message suggests the digits it contains: `ASANA_WORKSPACE must be a numeric GID, got "gid:1234567890123456" — did you mean 1234567890123456?`. A 403 or 404 on the workspace exits with a message naming `ASANA_WORKSPACE` as the likely cause.

## Architecture

One crate and one binary, `asn`. No workspace split until a second consumer needs one.

```
src/
  main.rs        // clap: no subcommand → TUI; otherwise CLI
  api/           // Asana HTTP client: typed requests and responses, pagination, 429 Retry-After, error mapping
  core/          // operations used by both CLI and TUI: inbox(), mark_read(), task_detail(), comment(), set_notes(), add_to_project()
  cli/           // subcommands: text and --json output
  tui/
    app.rs       // pure state machine: App + Action → App + Effects (unit-testable, no I/O)
    keys.rs      // keymap (vim + Asana aliases): key events (+ count, Tab prefix) → Action
    theme.rs     // Theme { role → Style }: loads built-ins and user TOML, applies NO_COLOR
    ui.rs        // ratatui rendering only; styles come exclusively from &Theme
  editor.rs      // suspend TUI → spawn $EDITOR on a temp file → restore
themes/          // built-in themes as TOML, embedded with include_str!
  terminal.toml  dark.toml  light.toml
```

- **Stack:** `ratatui` + `crossterm`, `tokio`, `reqwest` (rustls), `serde`, `clap`, `anyhow`, `toml` (theme files), `terminal-colorsaurus` (queries the terminal's fg/bg colors). That crate handles the raw-mode read, timeout, and known-unsupported terminals. Writing that ourselves would duplicate a lot of terminal-specific edge cases, and the cost is one small dependency. Colors are parsed with ratatui's own `Color: FromStr` (names, `#rrggbb`, 0–255 indexes), so there's no separate color library. Dev-only: `wiremock` for API tests and ratatui's `TestBackend` for rendering snapshots.
- **Concurrency:** the TUI event loop never blocks on the network. API calls run as tokio tasks and report back to the loop through a channel. The UI shows a loading state and keeps responding to keys.
- **Data:** an in-memory cache per session, keyed by task GID. No local database in the MVP.
- **Budget for the free-plan rate limit (~150 req/min):** the Inbox is polled every 60 s and on `R`. Task detail is fetched on selection after 250 ms of idle cursor (2 requests: task + stories) and cached. Writes invalidate only the affected task.

## Feature Spec — v0.1

### F1. Inbox (home view)

- Two-pane layout: the Inbox list on the left, a detail pane for the selected item on the right. Below 100 columns, the panes stack and `Enter` switches between them.
- Each row shows an unread dot, relative time, actor (`created_by.name` or "Asana"), what happened (`story_resource_subtype`, humanized), and the work object's name.
- Pagination loads the next page when the cursor reaches the last 10 rows. It keeps following `next_page` until it is `null`, even when a page is short or empty. If loading more fails because the offset has expired (offsets are valid for 15 minutes), it reloads from the first page and keeps the cursor on the same GID.
- **Read state:** `e` marks the selection read (the Asana web "archive" key), `u` marks it unread (the Asana web "move to Inbox" key), and `x` toggles. Opening an item does not mark it read. Marking read is an explicit decision, just like archiving mail. In visual mode, `V` followed by `e`, `u` or `x` acts on a range, batched in groups of up to 50 GIDs.
- **Quitting never drops a change:** after the terminal is restored, `asn` waits up to 5 s for read/unread requests still in flight. If any are still unfinished, it prints a warning to check with `asn inbox`.
- **Archive semantics:** the API has no archive, so `e` only marks the item read, and the item stays in the list, shown as read, the same as it does in the web Inbox. The `?` overlay states this next to the `e` key.
- **Filter:** by default the list shows every notification the API returns, read and unread, newest first, to match the web Inbox. `f` toggles an unread-only view, where items marked read leave the list.
- Notifications whose work object is not a task (project, goal, etc.) show compact info only (name, type; `o` opens projects in the browser — the API returns no links, so `asn` uses Asana's URL pattern for projects and says so for other types). No actions are available on them in the MVP.

### F2. Task detail

- Shows what the web task pane shows, as far as the API allows:
  - name and completion status
  - assignee
  - start and due dates (`due_at` with its UTC time), with "(overdue)" for incomplete tasks past due
  - every project with its section
  - tags and custom fields (`—` when empty)
  - the parent task, and the number of subtasks
  - the full description
  - **every** comment, oldest to newest, with author and relative time, following story pages
- System activity (moves, date changes, …) is counted ("N updates not shown") rather than listed, like the web's collapsed activity.
- Details load after the cursor rests on a task for 250 ms, and are cached per task. A refresh reloads the shown task in place and forgets the rest.
- `l`/`Enter` focus the pane, and `h`/`Esc` return to the list. While the pane is focused, `j`/`k`/`gg`/`G`/`Ctrl-d`/`Ctrl-u` scroll it.
- `o` opens `permalink_url` in the browser (the escape hatch for anything the MVP doesn't cover, e.g. subtasks and attachments). Before the details load it opens `https://app.asana.com/0/0/{gid}`. Only `https://` links are opened, and the browser process doesn't inherit `ASANA_TOKEN`.
- `asn task <gid> [--json]` prints the same content (or `{task, comments, activity_count}` as JSON).

### F3. Add comment

- `c` suspends the TUI and opens `$EDITOR` on an empty temp file. On save and quit, non-empty content is posted. An empty file or a `:cq` exit cancels.
- The comment appears optimistically. If the request fails, it's removed and an error toast is shown. The text is kept as a draft: the next `c` on that task reopens the editor with it.
- The temp file is created with mode 0600 and removed afterwards. The editor doesn't inherit `ASANA_TOKEN`. While the editor runs, `asn` stops reading the keyboard so no keystroke is taken from it.
- Comments work on tasks only; on other items `c` explains that.

### F4. Edit description

- `Tab e` opens `$EDITOR` pre-filled with the current `notes`. If the content changed after save and quit, it's sent with `PUT notes`.
- **Rich-text guard:** writing `notes` replaces `html_notes` and drops all formatting. If `html_notes` contains anything beyond plain paragraphs (links, lists, bold, mentions), the user must confirm in the TUI with `This will remove formatting. Continue? (y/N)` before saving.
- **Concurrency guard:** before writing, re-fetch `modified_at`. If it changed while the editor was open, abort and show a "task changed remotely" toast, keeping the edited text in the temp file so nothing is lost.

### F5. Add to project

- `p` opens a fuzzy picker (typeahead endpoint, queried as you type with a 200 ms debounce). In the picker, typing (including `j`/`k`) edits the query, `Ctrl-n`/`Ctrl-p` or the arrow keys move, `Enter` selects, and `Esc` cancels. This is the fzf convention, since a query field can't also use `j`/`k` for movement.
- After a project is chosen, a second picker offers its sections (`GET /projects/{gid}/sections`), with "no section" as the default.
- Adds the task to the chosen project. If the task has no project, this gives it its first one. If it already has projects, this adds another and keeps the existing ones. Projects the task already belongs to are hidden in the picker. Removing a task from a project, or moving it between projects, is out of scope.

### F6. Keymap: vim first, Asana web aliases always on

Vim is the primary keymap. Asana web Inbox shortcuts are aliases that are always enabled wherever they don't clash with vim, so people coming from the web app can keep their muscle memory. Applies to every list and pane.

| Keys | Action | Origin |
|---|---|---|
| `j` / `k`, `{count}j` | down / up | vim and Asana |
| `gg` / `G`, `{count}G` | top / bottom / line N | vim |
| `Ctrl-d` / `Ctrl-u` | half page down / up | vim |
| `h` / `l`, `Ctrl-w h/l` | focus left / right pane | vim |
| `Enter` | open / focus detail | — |
| `Esc` | back: close overlay → leave detail → (root) nothing | vim and Asana ("collapse pane") |
| `/`, `n` / `N` | filter-search the current list, next / previous match | vim |
| `V` | visual line mode (range selection) | vim |
| `e` | mark read ("archive") | Asana |
| `u` | mark unread ("move to Inbox") | Asana |
| `x` | toggle read | asn |
| `c`, `Tab c` | comment | asn, Asana |
| `Tab e` | edit description in `$EDITOR` | asn |
| `p`, `Tab p` | add to project | asn, Asana |
| `Tab i` | go to Inbox (home) | Asana |
| `o` | open in browser | asn |
| `f` | toggle unread only / all | asn |
| `R` | refresh | asn |
| `:` | command line (`:q`, `:refresh`, `:read-all`, `:unread`) | vim |
| `q` | leave visual mode or the detail pane; quit from the Inbox list | vim |
| `Ctrl-c` | quit from anywhere | asn |
| `?` | help overlay, with an Asana column showing which keys are web aliases | — |

- **`Tab` prefix:** `Tab` starts a two-key sequence, and the status bar shows `Tab-` while it waits. `Esc` or any key that isn't bound after `Tab` cancels the sequence. There's no timeout. `Tab` is free because pane focus uses `h`/`l`.
- **Asana keys deliberately not supported:**
  - `h` and `s`/`d` (Inbox/Archive tabs): `h` is vim's left, and there's no archive to show.
  - `Cmd/Ctrl-Enter` (complete): terminals can't reliably receive it.
  - `f` (follow): taken by the filter key. Follow/unfollow is not in the MVP.
  - `Tab a` and `Tab d` (assign, due date): reserved for when those actions are added. Until then they do nothing.

Keys are fixed in the MVP (no remapping) but are defined in one table in `keys.rs` with their origin. The help overlay is generated from that table, and remapping in v0.2 would use the same table.

### F7. CLI (same core, scriptable)

```
asn                               # TUI
asn inbox [--unread] [--limit N] [--json]
asn read <notif-gid>... [--unread]
asn task <task-gid> [--json]
asn comment <task-gid> [-m TEXT | -]          # stdin with "-", $EDITOR if neither
asn desc <task-gid> [--file PATH | -] [--force]  # --force skips the rich-text guard
asn project add <task-gid> <project-gid> [--section GID]
```

`--json` prints the Asana `data` objects unchanged (a stable contract inherited from Asana). Errors go to stderr. Exit codes: `0` ok, `1` API/usage error, `2` auth error. CLI text output is plain, with no colors and no themes.

### F8. Theming

- **Roles:** a theme maps a fixed set of semantic roles to styles (`fg`, `bg`, optional modifiers). The MVP roles are:
  - `text`, `text_muted`
  - `border`, `border_focused`, `title`
  - `selection`, `visual_selection`
  - `unread_marker`, `unread_text`
  - `actor`, `timestamp`, `task_meta` (assignee, due date, projects), `comment_author`
  - `statusbar`, `statusbar_mode`, `keyhint`
  - `toast_success`, `toast_error`, `picker_match`

  Rendering code asks for a role, never for a color.
- **Built-in themes:**
  - `terminal` (the default) is based on the user's current terminal theme, whatever it is (Gruvbox, Catppuccin, Solarized, and so on). It works in two layers:
    1. **Palette:** every role uses only the terminal's own colors: `reset` for default foreground and background, and the 16 ANSI colors for accents. Nothing is hard-coded, so `asn` looks like part of the terminal.
    2. **Tints:** at startup, before entering the alternate screen, `asn` asks the terminal for its actual foreground and background RGB (OSC 10/11 queries, with a 100 ms timeout). From those it derives the subtle shades that 16 ANSI colors can't express:
       - `selection` and `visual_selection` backgrounds: the background blended 12% and 20% toward the foreground
       - `statusbar` background: 6%
       - `border`: 35%

       Because the result is relative to the real background, it works for both light and dark terminal themes without any detection logic.
    - **Fallback:** if the terminal doesn't answer in time, which happens with some terminals and with tmux/screen without passthrough, `asn` uses the palette layer only. `selection` then uses `reversed`. The fallback has to look intentional, not broken.
  - `dark` and `light` use truecolor hex values for people who want a fixed look.
  - A user theme with `base = "terminal"` (the default `base`) gets the tints too, unless it sets those roles itself.

  Built-ins are TOML files embedded in the binary and parsed by the same loader as user themes, so they also serve as examples to copy.
- **User themes:** `ASN_THEME=/path/to/theme.toml`. A user file only needs to set the roles it changes. Every other role falls back to the built-in theme named in an optional `base` field (default `terminal`):

  ```toml
  base = "dark"

  [selection]
  bg = "#3c3836"
  modifiers = ["bold"]

  [unread_marker]
  fg = "yellow"
  ```

  Colors can be ANSI names (`red`, `light-blue`), `#rrggbb`, or a 256-color index (`"236"`). Modifiers are `bold`, `dim`, `italic`, `underlined` and `reversed`.
- **Validation:** the theme is loaded once at startup, before the TUI takes over the screen. An unknown role, unknown color or unreadable file stops `asn` with a message naming the file, key and line, for example `theme.toml:7: unknown role "selction"`. A typo should never silently fall back to a default.
- **NO_COLOR:** the theme is replaced by color-free styles, and modifiers stay: `selection` uses `reversed`, unread text uses `bold`, and the focused border uses `bold`. The UI remains fully usable.
- **No color-only meaning:** unread status is shown by both the `●` marker and color, and focus by the border weight (┏━ instead of ┌─) as well as color. Every theme, including `NO_COLOR`, keeps these cues.

## Key Assumptions to Validate

- [x] **The Notifications API is accessible with a PAT on our plan.** Confirmed in T0 (HTTP 200). Test: `curl` `GET /users/me/notifications` with a team member's PAT on day 1. If it's blocked, the core feature is dead, so this check comes before any Rust code.
- [ ] **Almost all Inbox items reference tasks.** T0: 10 of 10 were tasks, but that was one user with only 10 notifications. Re-check with teammates while dogfooding. Test: dump 200 notifications from 3 teammates and count `work_objects[].resource_type`. If non-tasks are above ~20%, revisit F1's "compact info only" rule.
- [x] **Plain-text descriptions are acceptable.** Decided by the user: keep plain-text editing with the confirmation (no HTML↔Markdown conversion in v0.1). T0: **4 of 8** tasks had formatting (links, lists, headings), so the guard would fire for about half of all edits. **Decide before T11** whether that is acceptable or whether F4 needs HTML↔Markdown conversion. Test: in the same dump, fetch tasks and count how many `html_notes` contain formatting. If most do, the guard fires constantly and F4 needs HTML↔Markdown conversion (a substantial addition to scope).
- [ ] **`$EDITOR` round-trips feel fast enough for comments.** Test: dogfood for a week. If short replies feel heavy, add a single-line inline prompt (`C`) in v0.2.
- [ ] **Explicit mark-read is what people want.** Test: dogfood. The alternative is to auto-mark on open.
- [x] **Archive is `E` in the web app** (confirmed by the user). `e` is therefore the mark-read alias, and editing the description moved to `Tab e`.
- [ ] **`U` moves an item back to the Inbox in the web app.** Test: try it in the web Inbox during T0.
- [x] **Our terminals answer color queries.** Rex: yes, in under 1 ms (checked in T5). Ghostty: not checked yet. Test: in Rex (and Ghostty), run `printf '\e]11;?\e\\'; read -rs -t1 -d '\' r; printf '%q\n' "$r"`. A reply like `rgb:1e1e/1e1e/2e2e` means tints will work. If there's no reply, everyone on that terminal gets the palette-only fallback.
- [ ] **60 s polling is fresh enough.** Test: ask the dogfooders. The API has no push option.

## MVP Scope

**In:** F1–F8, env-var auth, the single-workspace flow, macOS and Linux, release binaries via GitHub Releases / `cargo install`.

**Build order** (each step is usable on its own):
1. API client, `asn inbox --json`, `asn read` (validates the API assumption and is useful as a script immediately)
2. Theme loader with the three built-ins, then the TUI Inbox list with the full keymap and read actions. The theme comes first so no hard-coded color ever lands.
3. Task detail pane and `asn task`
4. Comment (F3)
5. Edit description with guards (F4)
6. Add to project (F5)

**Tests:** `api/` against `wiremock` fixtures recorded from real responses (with GIDs and names anonymized). `tui/app.rs` as pure state transitions. `keys.rs` count and multi-key parsing (`gg`, `5j`, `Ctrl-w l`, `Tab c`, `Tab` then `Esc`), plus a test that no two bindings collide. Theme tests:
- every built-in parses and defines every role
- a partial user theme falls back to its `base`
- unknown roles and colors are rejected with the line number
- `NO_COLOR` strips all colors
- tint derivation is a pure function from fg/bg RGB to styles, tested with one light and one dark pair
- a query that gets no answer falls back to the palette-only theme (tested with a fake terminal that never replies)

One `TestBackend` render snapshot per screen with the `terminal` theme, plus one with `NO_COLOR`.

## Not Doing (and Why)

- **Project and board views** come in v0.2 (board only, as decided). The Inbox slice proves the core and the keymap first.
- **Local SQLite mirror / offline** is not needed: the Inbox is polled with about 1 request per minute, and an in-memory cache covers task detail. Revisit with board views, where it matters.
- **OAuth and config files:** a PAT in an env var covers our team, and OAuth means registering an app and hosting a redirect for no MVP gain.
- **An in-TUI text editor:** `$EDITOR` is better at this than anything we'd build.
- **HTML↔Markdown description editing** waits until the plain-text assumption fails (see above).
- **Assignee, due date, completion, subtasks, attachments, and creating tasks** are not part of "process my Inbox". `o` covers them in the browser. Completion (`d`) is the likely first addition.
- **Detecting @mentions:** the API has no field for it, and the workaround (searching `html_text` for your GID) is fragile.
- **Multiple workspaces at runtime and key remapping** are config surface no one needs yet. Disabling the Asana aliases is also out: they're only bound where they don't clash with vim.
- **Theme extras:** no hot-reload, no switching themes at runtime, no theme config directory or theme gallery, no color expressions in theme files (such as `mix(bg, fg, 12%)`), and no contrast checking. One env var and one file is enough. The `terminal` default already follows the user's terminal colors. It also handles light and dark terminals by tinting relative to the queried background, so no separate light/dark switch is needed.
- **Re-querying when the terminal theme changes while `asn` is running:** the colors are queried once at startup. Restart `asn` to pick up a new terminal theme.
- **Colored CLI output:** the CLI is for pipes and scripts, so plain text is the right default.
- **Windows:** not blocked, just not tested or supported in the MVP.

## Findings from Use

- Notifications archived in the web Inbox are not returned by the API, so they disappear from `asn` at its next refresh (confirmed by the user, 2026-11-17).

## Open Questions

1. **Publishing name:** the binary is `asn` and the repo lives under GitHub user `lplotni` (both decided). Check crates.io and Homebrew for a free package name before the first release. The package name can differ from the binary name.
2. **Should a mutation mark the notification read?** For example, should commenting on a task mark its notification read? The spec says no (explicit only). Confirm while dogfooding.
