# ADR-005: Poll and cache in memory; no local mirror

## Status
Accepted

## Date
2026-11-16, extended for boards 2026-11-17 (recorded 2026-11-17)

## Context
Asana's public API has no push for notifications. The free plan allows about 150 requests a minute. Boards need fresher data than a one-off load, but this is a status glance, not a collaborative editor.

## Decision
- **Inbox:** polled every 60 s and on `R`. Task details load after the cursor rests for 250 ms and are cached per task GID.
- **Boards:** only the board on screen is polled, with the same 60 s tick. A load is 2 + ⌈tasks / 100⌉ requests.
- **Overlapping loads:** a reload asked for while one is running is queued and runs right after. This guarantees that a new or moved card is in the next result.
- Boards opened this session stay in memory with their selection.

## Alternatives Considered
- **A local SQLite mirror kept in sync with the Events API.** Instant switching and offline use, but it's real infrastructure (sync tokens, invalidation, migrations) for a glance tool. Deferred in SPEC.md and SPEC-board.md.
- **Polling every board opened.** Wastes requests on boards nobody is looking at.

## Consequences
- Data can be up to 60 s old. `R` refreshes now.
- Request use stays far below the free-plan limit.
- If live sync is ever needed, this is the ADR to supersede.
