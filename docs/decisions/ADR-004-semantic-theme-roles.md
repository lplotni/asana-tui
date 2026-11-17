# ADR-004: Rendering uses semantic theme roles; the default follows the terminal

## Status
Accepted

## Date
2026-11-16 (recorded 2026-11-17)

## Context
Terminal users have strong colour preferences and light or dark themes, and hard-coded colours look wrong in half of them. See [SPEC.md](../SPEC.md), F8.

## Decision
- Rendering code never names a colour. It asks the `Theme` for a role (`selection`, `task_meta`, `toast_error`, …).
- The built-in themes (`terminal`, `dark`, `light`) are TOML files, parsed by the same loader as user themes.
- The default `terminal` theme uses the terminal's own 16 ANSI colours. It also queries the real foreground and background (OSC 10/11, 100 ms timeout) to blend subtle tints, falling back to a palette-only look when the terminal doesn't answer.
- `NO_COLOR` strips colour but keeps modifiers. Meaning is never carried by colour alone: the `●` unread marker, focus by border weight, and `!` on overdue cards.

## Alternatives Considered
- **Fixed palettes only.** Simpler, but they clash with most terminal themes.
- **Light/dark detection with two palettes.** Tinting relative to the queried background handles both without detection logic.

## Consequences
- New UI reuses existing roles. A new role is added only when no existing one fits, and needs a decision (CLAUDE.md, "Ask first").
- One small dependency (`terminal-colorsaurus`) handles the colour query.
