# ADR-011: Refresh every few seconds while in front; still no Events

## Status
Accepted (extends [ADR-005](ADR-005-polling-and-in-memory-cache.md))

## Date
2026-11-24

## Context
The user reported that changes took minutes to show on a board and in the task detail, and asked for everything to be near-live. ADR-005 polled the Inbox and the board on screen every 60 s. [SPEC-board.md](../SPEC-board.md) F28 measured before changing anything, on a throwaway board:
- **Asana's reads are fresh.** The board's task list showed every write on its first read, 0.4–0.7 s after Asana confirmed it. The single-task read was just as quick.
- **Events are quicker than documented**, at about 4 s (7.2 s at most), against the docs' "within a minute (on average)".
- **Asana's write confirmation is the slow part**, at 1.8–4.9 s.

So the TUI already showed its own writes about 0.5 s after Asana confirmed them. Changes from anywhere else waited for the next 60 s poll. A false conflict made saved descriptions seem to vanish (F29a).

## Decision
- **Refresh every `refresh` seconds while the terminal is in front:** the Inbox, the board on screen and the shown detail. The setting is `refresh` or `ASN_REFRESH`, in whole seconds from 5 to 60, with a default of 5. Only the board on screen polls, as before.
- **Back off in the background.** The TUI turns on the terminal's focus reports. While it has lost focus, it refreshes once a minute. Back in front, it refreshes at once if a refresh was skipped. A terminal that never reports focus counts as in front. Focus reports are off wherever raw mode is restored.
- **No clock in `App`** (ADR-002). The timer ticks every `refresh` seconds, and the app counts ticks to decide whether a tick refreshes.
- **Overlapping loads stay queued** (ADR-005). However many ticks arrive during a slow load, one reload follows.
- **A load older than an own write isn't shown.** A load is now often running when a write is made, and its result can predate the write. A completed card came back until the next load (found at F30's Checkpoint F3). So `App` counts its own writes until Asana answers them (`tui/writes.rs`). A board or detail load that started while one was in flight, or that a write was sent or answered during, keeps the screen as it is, and a fresh load follows. A write whose answer never comes is given up after two minutes of ticks. This is the sequence-number pattern the TUI already uses for stale async results, applied to writes.

## Alternatives Considered
- **Events API (`GET /projects/{gid}/events`) as the change detector.** It needs one request per poll, plus a reload only when something changed, so it's cheaper than reloading the board. But a token expires (412), which means a full reload. Delivery is at most once, so the poll is still needed as a fallback. And it doesn't cover the Inbox. A 5 s poll meets the target within the free plan for boards up to about 300 tasks. Revisit if the request cost bites.
- **A pending-changes overlay: each own write applied on top of every load until a load confirms it.** F28 showed Asana's reads are fresh, so only loads that started before a write can miss it. Not showing those loads (above) is simpler than overlaying each change on every load.
- **A request budget instead of an interval.** People think in seconds, so the cost of each interval is documented instead ([configuration](../configuration.md#refresh-and-rate-limits), F30).
- **A faster fixed poll without focus reports.** Simpler, but it costs requests all day while nobody looks, and they come out of the same per-token limit the CLI and the Claude Code skill use.

## Consequences
- **Cost.** A refresh is 1 + 3 + ⌈tasks / 100⌉ + 3 requests, about 10 for a 300-task board, so about 120 a minute at 5 s. That is under the free plan's 150; paid plans allow 1,500. Larger boards on the free plan should set `refresh = 10` or more. Over the limit, a refresh fails with a toast, and the next one tries again.
- **Freshness.** Changes from elsewhere show within the interval, plus a load, while the terminal is in front.
- **The detail cache is short-lived.** Each refresh keeps only the shown task's detail and reloads it, so revisiting another task loads it again. An image that downloads but won't decode is tried again on each refresh of its shown task.
- **Supersede this ADR** if push (webhooks) or Events become worth their machinery.
