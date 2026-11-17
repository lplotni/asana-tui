# ADR-003: Configuration and credentials come only from the environment

## Status
Accepted

## Date
2026-11-16 (recorded 2026-11-17)

## Context
`asn` needs a Personal Access Token and a workspace. Credentials on disk are a liability, and the target users already keep secrets in password managers. See [SPEC.md](../SPEC.md), Configuration.

## Decision
- Configuration is environment variables only: `ASANA_TOKEN`, or `ASANA_TOKEN_CMD` (a command that prints the token, run once at startup), plus `ASANA_WORKSPACE`, `ASN_SEARCH`, `ASN_THEME` and `NO_COLOR`.
- `asn` writes nothing to disk except an explicit recovery file for unsaved description text.
- The token is never printed, logged, or put in an error message. The editor and browser processes don't inherit it. Requests time out (10 s to connect, 30 s in total).
- Session state (recent boards, caches, drafts) lives in memory only.

## Alternatives Considered
- **A config file** (`~/.config/asn`). More convenient for many settings, but it adds a place for secrets to leak, and there are few settings.
- **OAuth.** Needs a registered app and a redirect host, which brings no gain for a personal-token tool.
- **Persisted recent boards** (`ASN_PROJECTS` or a state file). Considered for F11 and rejected: session recents were enough.

## Consequences
- Exit code 2 means a token problem, and 1 any other error.
- Invalid GIDs (`ASANA_WORKSPACE`, `--project`) are rejected before any request is made.
- Anything that would need persistence has to be argued against this ADR first.
