# ADR-008: Boards are a second top-level screen of columns, backed by a pure `BoardView`

## Status
Accepted

## Date
2026-11-17

## Context
v0.2 adds project boards for glancing at status across many projects (50–300 tasks each), with the aim of replacing the web board. See [SPEC-board.md](../SPEC-board.md).

## Decision
- **Layout:** one column per section, cards of two lines (name, then initials · due date with `!` when overdue). Columns are at least 28 cells, `h`/`l` page when they don't fit, and the hidden counts are shown in the header.
- **Navigation:** the board and the Inbox are peer screens (`Screen::{Inbox, Board}`). `Tab i` and `P` switch between them, and each keeps its own state.
- **Code:** all board navigation, paging, selection by GID, search, and card removal live in the pure `tui/board.rs` (`BoardView`).
- **Actions:** task actions reuse the existing task-GID seam (`selected_task_gid()`), so comment, edit, add to project, move, open, complete and assign work on cards unchanged.
- **Data:** `core::board` loads the project, its sections and its open tasks (`completed_since=now`) at the same time. Cards are placed by their membership *in this project*, which assumes Asana's task order is board order. That was confirmed on real projects (A1).

## Alternatives Considered
- **A stacked, grouped list with the detail pane beside it.** Denser and cheaper, because it reuses the Inbox layout, but it doesn't look like a board. A `v` toggle between the two was deferred.
- **One request per section** (`GET /sections/{gid}/tasks`). Kept as the fallback if A1 had failed.
- **Overlaying the board on the Inbox.** Simpler, but switching would reload the board.

## Consequences
- Completed tasks and subtasks don't show on boards, and cards can't be reordered (`m` moves them between sections).
- New board behaviour goes in `BoardView` first, with unit tests, and only then into `app.rs`.
