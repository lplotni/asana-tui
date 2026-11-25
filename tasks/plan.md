# Implementation Plan: instant new cards (F32)

Source of truth: [`docs/SPEC-board.md`](../docs/SPEC-board.md) F32. Tasks with acceptance criteria: [`todo.md`](todo.md). Earlier plans: [`v0.1/`](v0.1/plan.md), [`v0.2-v0.4/`](v0.2-v0.4/plan.md).

## Overview

After `Tab q` and Enter in lanes, the card took about 5 s to appear: three writes in a row (create, top of the column, lane option), then a reload. F32a drops the third write by sending the lane's option with the create. F32b shows a local card at once and turns it into the real one when Asana confirms.

## Architecture Decisions

- **The option goes in the create body** (`custom_fields`), not in a parallel write: one request fewer, and no half-made task when the field is refused.
- **The local card lives in the board's own data** (`BoardView`), marked by its local GID (`local-N`). That's the convention comments being posted already use. No overlay is re-applied on each load: F30's stale-load rule already keeps loads that may predate the create off the screen, and the reload after the confirm brings the real card.
- **Task keys check the selected card**, and a pending card isn't a selected task (`App::selected_card`), so no detail load or write can carry a local GID.

## Task List

- [x] T53: Send the lane's option with `POST /tasks` (F32a)
- [ ] T54: Show the new card at once, as `creating…` (F32b)

### Checkpoint: instant
- [ ] fmt, clippy and tests pass
- [ ] Rex on the throwaway board: the card shows within 0.3 s of Enter, and `creating…` is gone within the confirm time plus a reload

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Asana refuses `custom_fields` on create for a lane option | Med: a create in a lane fails | The options come from the project's own field settings, so they belong to the project. A refusal shows `Task not created`, and the name is kept. The throwaway board has no single-select field, so a real lanes board is the final check. |
| A local GID reaches a write or a detail load | High | A pending card isn't a selected task; app tests press every task key on it. |
