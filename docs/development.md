# Development

[asn](../README.md) › Development

```sh
cargo test
cargo run --example screenshot > docs/screenshot.svg            # regenerate the screenshots (made-up data)
cargo run --example screenshot -- board > docs/board.svg
cargo run --example screenshot -- lanes > docs/lanes.svg
```

How quickly changes show up, measured on a throwaway board of your own (pass its GID, never commit it):

```sh
cargo run --release --example latency -- <project-gid>   # Asana: confirm time, and when each read shows a write
cargo build --release && scripts/tui-latency.sh <project-gid>   # the TUI, driven in a Rex pane
```

The probe creates, moves, assigns, sets a field on and completes tasks named `asn latency probe …`, and completes them afterwards. The harness runs inside a Rex terminal (it needs `REX_SESSION`) and does the same through the TUI's keys, watching each change for two minutes in case it flicks back.

Why it's built this way: [architecture decisions](decisions/README.md). Specs: [v0.1](SPEC.md), [v0.2–v0.4 and boards](SPEC-board.md), [Claude Code](SPEC-skill.md). Plans: [v0.1](../tasks/v0.1/plan.md), [v0.2–v0.4](../tasks/v0.2-v0.4/plan.md).

Releases: push a `v*` tag. CI builds the four binaries, attaches them to a GitHub release, and commits the updated `Formula/asn.rb`.
