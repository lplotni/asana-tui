# ADR-001: A scriptable core, and the CLI before the key binding

## Status
Accepted

## Date
2026-11-16 (recorded 2026-11-17)

## Context
`asn` is used both interactively and from scripts (`asn inbox --unread | cut -f1 | xargs asn read`). If an action exists only in the TUI, it can't be scripted, can't be tried against real Asana before its UI exists, and is harder to test. See [SPEC.md](../SPEC.md), principle 2.

## Decision
- One crate with a library and a thin binary. `src/lib.rs` exposes `api`, `core`, `cli`, `present` and `tui`, and `src/main.rs` is only the clap entry point.
- Every action is a `core::*` function first, then an `asn <subcommand>` with stable `--json` output, and only then a key binding. Work is ordered that way (each plan task lands core + CLI before TUI).

## Alternatives Considered
- **TUI only.** Less code, but nothing is scriptable, and every Asana assumption has to be checked through the UI.
- **A Cargo workspace (core / cli / tui crates).** Cleaner boundaries in theory, but there's no second consumer yet. It can be split if one appears.

## Consequences
- Risky assumptions are checked from the CLI before UI work. Examples: board order (A1) with `asn board`, and mention rich text (M1) with `asn comment -m '… @<gid>'`.
- The CLI's text and `--json` output is a public contract. Changes must stay compatible: `asn task <gid>` kept working when `asn task done` and `asn task assign` were added as subcommands beside it.
- TUI and CLI never disagree about what an action does, because both call the same `core` function.
