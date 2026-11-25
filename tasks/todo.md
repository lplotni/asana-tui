# Tasks: instant new cards (F32)

Plan: [`plan.md`](plan.md). Spec: [`docs/SPEC-board.md`](../docs/SPEC-board.md) F32.

### - [x] T53: Send the lane's option with `POST /tasks` (F32a)

**Description:** `api::projects::create_task` and `core::create_task` take an optional single-select value (field and option GID) and send it as `custom_fields` in the create. The TUI's `CreateTask` effect passes the lane's, and drops the separate field write and its `lane_error`.

**Acceptance criteria:**
- [x] In a lane, the create body has `custom_fields: {field: option}`, and no `PUT /tasks/{gid}` follows.
- [x] Without a lane (columns, `asn project new`), the body is as before.
- [x] A refused create in a lane shows `Task not created: …` and keeps the name.

**Verification:** `cargo test` (wiremock in `tests/api_project.rs`, app tests), clippy, fmt.

**Files likely touched:** `src/api/projects.rs`, `src/core/mod.rs`, `src/tui/mod.rs`, `src/tui/app.rs`, `src/tui/app_tests.rs`, `tests/api_project.rs`

**Estimated scope:** S

---

### - [x] T54: Show the new card at once, as `creating…` (F32b)

**Description:** On Enter, `App` adds a local card (`local-N`) to the board at the top of the column, with the lane's option, and selects it. `CreateTask` and `TaskCreated` carry the local GID. On success the card takes the real GID; on failure it's removed. `BoardView` gets `insert_pending` and `confirm`. Task keys on a pending card only toast. The card draws `creating…` as its second line.

**Acceptance criteria:**
- [x] Enter shows the card in its column and lane, selected, with no detail load.
- [x] Task keys on it toast `Wait for the task to be created`; navigation works.
- [x] Confirm: real GID, still selected, detail loads, reload follows. Refusal: card gone, name kept.
- [x] A stale refresh during the create doesn't remove it; two creates in a row both show.

**Verification:** `cargo test` (app and UI tests), clippy, fmt; Rex on the throwaway board.

**Files likely touched:** `src/tui/board.rs`, `src/tui/app.rs`, `src/tui/app_tests.rs`, `src/tui/mod.rs`, `src/tui/ui/board.rs`, `src/tui/ui.rs`

**Estimated scope:** M
