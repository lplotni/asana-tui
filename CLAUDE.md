# asn: Asana Inbox and project boards in the terminal

Rust, one crate (library plus the `asn` binary). The TUI (ratatui) and a scriptable CLI share one core. Public repo: `lplotni/asana-tui`, MIT.

Read first: [README](README.md) (usage), [docs/SPEC.md](docs/SPEC.md) (v0.1, F1–F8), [docs/SPEC-board.md](docs/SPEC-board.md) (v0.2–v0.4, F9–F17, F22–F31), [docs/SPEC-skill.md](docs/SPEC-skill.md) (Claude Code skill, F18–F21), and [docs/decisions/](docs/decisions/README.md) (ADRs: why it's built this way). Finished plans are archived in `tasks/v0.1/` and `tasks/v0.2-v0.4/`; the next feature starts a fresh `tasks/plan.md` and `tasks/todo.md`.

## Commands

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                       # unit, wiremock API tests, TestBackend snapshots, CLI tests
cargo run -- --project <gid>     # the TUI on a board (needs ASANA_TOKEN and ASANA_WORKSPACE)
cargo run --example screenshot > docs/screenshot.svg            # README images (made-up data)
cargo run --example screenshot -- board > docs/board.svg
cargo run --example screenshot -- lanes > docs/lanes.svg
```

- If `cargo` isn't found, rustup lives at `/opt/homebrew/opt/rustup/bin`: `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"`.
- CI runs fmt, clippy and test. All three must pass before every commit.

## Layout

| Path | What |
|---|---|
| `src/api/` | HTTP client: typed requests and responses, pagination (`next_page` until `null`), 429 retry, errors |
| `src/core/` | Operations used by both CLI and TUI. `core::mention` holds rich text and mention parsing; `core::resolve` turns names into GIDs (exact, or the only match) |
| `src/cli/` | Text and `--json` rendering for subcommands |
| `src/present/` | UI-neutral text: task detail lines, card text, date formatting |
| `src/tui/app.rs` | Pure state machine: `App::update(Msg) -> Vec<Effect>`, no I/O (ADR-002) |
| `src/tui/mod.rs` | Event loop: runs `Effect`s as tokio tasks and sends back `Msg`s |
| `src/tui/board.rs`, `picker.rs` | Pure sub-state: `BoardView` (columns or lanes, one grid of lanes × columns), and the pickers (stages) |
| `src/tui/keys.rs` | The one keymap table (`BINDINGS`). Help and hints are generated from it |
| `src/tui/ui.rs`, `ui/board.rs` | Rendering only (`ui/board.rs`: the board screen and its cards). Styles come from `Theme` roles, never colours (ADR-004) |
| `tests/` | `wiremock` tests per API area, `cli.rs` (runs the binary), `fixtures/` (anonymized) |
| `skills/asn/SKILL.md`, `.claude-plugin/` | The Claude Code skill and its plugin/marketplace manifests (F20). `tests/skill.rs` checks every `asn` command in the skill against the real help |

## Conventions

- **The CLI comes before the key binding.** A new action is a `core` function, then an `asn` subcommand, then a key (ADR-001). Keep existing CLI output and `--json` compatible.
- **State changes and I/O stay apart.** Network work is an `Effect` plus a result `Msg`. Larger state goes in a pure module, as `board.rs` does. Stale async results are dropped via sequence numbers or GID checks.
- **Keys:**
  - Add the binding to `BINDINGS`. The collision test and the help overlay pick it up automatically.
  - Add it to the right lists in `ui::key_hints`, most useful first: the line drops from the end, and `? help` always stays.
  - Hint labels are short (`edit`, `browser`, `done`).
  - A `hidden(…)` binding (the F31 easter egg) stays out of the help overlay and the hints.
- **Help overlay is full.** At 100×24 it uses all 24 rows. `the_help_overlay_shows_every_binding_on_a_100_by_24_terminal` fails if a key's description gets cut off. Keep descriptions short, and change the help layout before adding keys.
- **Snapshots:** `cargo-insta` isn't installed. Review each `src/tui/snapshots/*.snap.new` (`diff` it against the `.snap`), then accept it by removing the `assertion_line:` line and renaming it to `.snap`.
- **Text from Asana is untrusted.** The CLI passes it through `cli::terminal_safe` / `tsv_line`, which removes control characters and bidi overrides. In the TUI, ratatui drops control characters, and `ui::render` strips bidi overrides from every drawn cell at the end (`strip_bidi_controls`), whatever widget drew it. Both use `present::format::is_bidi_control`. Keep new drawing inside `ui::render`, so it passes that pass.
- **Tests and fixtures use invented GIDs and names only** (`12000000000000xx`, "Ada Lovelace"). Never put a real workspace or project GID, a real name, or a user's pasted data in the repo, commits or docs. The repo is public.

## How we work

- New features: an idea or spec (a new `F` section in `docs/SPEC-board.md`), then tasks in `tasks/todo.md` and `tasks/plan.md`, then test-first (red, then green), one task at a time.
- Commit straight to `main`, one commit per task, Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`). Tick the task in `tasks/`, and update the spec when behaviour deviates from it.
- Commit as the GitHub noreply address (git config already does this). Stage paths explicitly.
- `notes.md` is the owner's private scratch file. It's gitignored; never add it.
- Things to ask first: new dependencies, new theme roles, rebinding an existing key, anything written to disk, changing an existing `--json` contract, CI/workflow changes, pushing, tagging.
- **Releases** (ADR-009): bump `Cargo.toml` and `Cargo.lock`, commit `chore: release vX.Y.Z`, push `main`, then push the annotated tag `vX.Y.Z`. The workflow builds four targets and commits `Formula/asn.rb`, so pull afterwards. If a build is cancelled for lack of a runner, `gh run rerun <id> --failed`.

## Gotchas

- **`asn` on PATH is usually the Homebrew release**, and `target/release/asn` is only as fresh as the last `cargo build --release`. When behaviour looks stale, check which binary and when it was built.
- **Mention URLs:** in plain text a person's mention is `https://app.asana.com/…/profile/<profile-id>`, and the profile ID is *not* the user GID. Map URLs to mentions by the `<a href>` in `html_text`/`html_notes` (ADR-007).
- **Writing rich text:** use `<a data-asana-gid="…"/>` inside `<body>`, escape `& < >`, and keep `\n` as it is (confirmed on Asana). Send it only when there's a mention; otherwise send plain text.
- **Descriptions are plain text.** Saving drops non-mention formatting, and the `has_formatting` guard asks first. The description the edit started from guards against overwriting a concurrent edit: `asn` compares it with Asana's `notes` before writing. Not `modified_at`, which changes on any write, even Asana's own follow-up to one (F29a).
- **Board order relies on `GET /projects/{gid}/tasks` returning tasks in board order.** It does (confirmed, A1). Cards are placed by their membership in *this* project.
- **Boards and reloads:** a reload requested during a load is queued (`reload_queued`). Don't drop it, or a created or moved card waits for the next refresh. Refreshes come every `refresh` seconds (5 by default) while the terminal is in front, once a minute behind it (ADR-011). Asana's reads are fresh right after a write (F28), so a reload after a write shows it.
- **Tooling quirk:** `cargo fmt` reflows code, so a scripted edit made after a format run can silently miss its target. Re-read before editing, and treat compiler warnings about unused values as possible missed edits.
