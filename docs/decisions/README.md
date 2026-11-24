# Architecture decisions

Why `asn` is built the way it is. Each record keeps its context and the alternatives that were rejected. Don't edit an accepted decision to change it: write a new ADR that supersedes it. The behaviour itself is specified in [SPEC.md](../SPEC.md) (v0.1) and [SPEC-board.md](../SPEC-board.md) (v0.2).

| ADR | Decision |
|---|---|
| [001](ADR-001-scriptable-core-cli-first.md) | A scriptable core, and the CLI before the key binding |
| [002](ADR-002-pure-tui-state-machine.md) | The TUI is a pure state machine; effects run in the event loop |
| [003](ADR-003-environment-only-configuration.md) | Configuration and credentials come only from the environment (superseded by 010) |
| [004](ADR-004-semantic-theme-roles.md) | Rendering uses semantic theme roles; the default follows the terminal |
| [005](ADR-005-polling-and-in-memory-cache.md) | Poll and cache in memory; no local mirror (interval changed by 011) |
| [006](ADR-006-editor-overlay-with-external-escape.md) | Write in an in-app editor overlay, with `$EDITOR` as an escape hatch |
| [007](ADR-007-plain-text-with-rich-text-for-mentions.md) | Edit as plain text; rich text only for @-mentions; read mentions by href |
| [008](ADR-008-board-as-columns-screen.md) | Boards are a second top-level screen of columns, backed by a pure `BoardView` |
| [009](ADR-009-tag-driven-release-and-homebrew-tap.md) | Releases are tag-driven, with the Homebrew formula in the repo |
| [010](ADR-010-config-file-without-secrets.md) | A config file for settings; the token still never touches disk |
| [011](ADR-011-refresh-every-few-seconds-while-in-front.md) | Refresh every few seconds while in front, once a minute behind; still no Events |
