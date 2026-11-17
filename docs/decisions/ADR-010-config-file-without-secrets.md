# ADR-010: A config file for settings; the token still never touches disk

## Status
Accepted (supersedes [ADR-003](ADR-003-environment-only-configuration.md))

## Date
2026-11-17

## Context
ADR-003 kept all configuration in environment variables. The settings have grown (`ASANA_WORKSPACE`, `ASANA_TOKEN_CMD`, `ASN_SEARCH`, `ASN_THEME`, and now an animation switch), and a new user has to find a workspace GID with `curl` before `asn` starts at all. ADR-003's reason against a file was that it adds a place for secrets to leak. That reason holds for the token, not for the other settings. See [SPEC-board.md](../SPEC-board.md), F22.

## Decision
- Settings may live in `$XDG_CONFIG_HOME/asn/config.toml` (default `~/.config/asn/config.toml`).
- **The token never goes in the file.** The file holds `token_cmd`, a command that prints it. A `token` key is rejected with an error, not ignored.
- **Environment variables win over the file**, and the file wins over the defaults. Existing setups, scripts and CI keep working unchanged. `NO_COLOR` stays environment-only, as its convention requires.
- `asn` writes the file only through the setup interview (`asn init`, or the TUI's first run when no file exists). It never rewrites it on its own, and it never overwrites an existing file without asking. The file is created with mode 0600.
- Unknown keys are an error, so a typo doesn't silently fall back to a default.
- Everything else from ADR-003 stands: the token is never printed, logged or written to disk, the editor and browser don't inherit it, and session state (recents, caches, drafts) stays in memory.

## Alternatives Considered
- **Keep environment variables only (ADR-003).** It works, but setup needs `curl` and a shell profile edit, and five or more variables are hard to discover.
- **Allow a literal token in the file (with 0600).** Convenient, but it puts the one secret on disk. A command that reads from a password manager or the macOS keychain costs one line.
- **File only, env deprecated.** One source of truth, but it breaks every current setup for no gain.

## Consequences
- Error messages name both the variable and the file key (e.g. `ASANA_WORKSPACE` / `workspace`).
- CLI subcommands never prompt. They read the file if it exists, and otherwise behave as before.
- Setup can look up workspaces with the token (`GET /workspaces`), so the `curl` step goes away.
