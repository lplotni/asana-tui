# ADR-007: Edit as plain text; write rich text only for @-mentions; read mentions by href

## Status
Accepted

## Date
2026-11-17

## Context
Asana stores comments and descriptions twice: as plain `text`/`notes`, and as rich `html_text`/`html_notes`. Writing plain text drops formatting, and in a sample half of the real descriptions had some (SPEC.md, assumptions). @-mentions exist only in rich text, as `<a data-asana-gid="…"/>`. Plain text shows a mention as a URL. See [SPEC-board.md](../SPEC-board.md), F15.

## Decision
- **Writing:** text stays plain unless it contains a remembered `@Label`. Then `core::mention::rich_text` escapes everything else, wraps it in `<body>`, turns each label into a mention tag, and the field becomes `html_text`/`html_notes`. Line breaks are kept (confirmed on real Asana, M1).
- **Formatting guard:** before a plain save drops formatting, `asn` asks (`has_formatting`). Mentions don't count as formatting, because they're kept.
- **Reading:** the label of each mention link in the rich text replaces the matching URL in the plain text (`core::mention::readable` / `editable`). It matches the link's **`href`** first, and the GID only as a fallback.

## Alternatives Considered
- **Full HTML ↔ Markdown conversion.** It would keep all formatting, but it's a substantial converter, and the plain-text guard was accepted instead.
- **Matching mention URLs by the GID at their end.** Wrong for people: a profile URL ends in the *profile* ID, not the user GID. This shipped once and was fixed (commit `35d405f`).
- **Resolving profile URLs with `GET /users/{gid}`.** Extra requests, and the profile ID isn't the user GID anyway.

## Consequences
- Plain-text rendering keeps its layout, and only mention URLs change.
- Links that aren't mentions, and mentions without a label, show as they are.
- `--json` output stays raw.
