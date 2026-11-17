# ADR-009: Releases are tag-driven, with the Homebrew formula in the repo

## Status
Accepted

## Date
2026-11-16 (recorded 2026-11-17)

## Context
Users install with Homebrew or download a binary. Releasing by hand is error-prone (four targets, checksums, a formula to update).

## Decision
- Bump `version` in `Cargo.toml` (and `Cargo.lock`), commit `chore: release vX.Y.Z`, push `main`, then push an annotated tag `vX.Y.Z` (message `vX.Y.Z`).
- `.github/workflows/release.yml` builds `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`, attaches the archives and `.sha256` files to a GitHub release, and commits the updated `Formula/asn.rb` (`scripts/formula.sh`).
- The repo is its own tap: `brew tap lplotni/asn https://github.com/lplotni/asana-tui`.

## Alternatives Considered
- **A separate `homebrew-asn` tap repository.** The conventional layout, but one more repo to keep in sync.
- **crates.io.** Possible later, and `cargo install --git` covers Rust users for now.

## Consequences
- The release and formula jobs run only when all four builds succeed. In v0.2.0, the macOS arm64 build couldn't get a GitHub runner ("capacity constraints") and nothing was published. `gh run rerun <id> --failed` fixed it.
- If that keeps happening, cross-compiling `aarch64-apple-darwin` on the Intel macOS runner would remove the dependency. That's a workflow change, so ask first.
- The release binaries aren't code-signed (macOS quarantine note in the README).
- After a release the workflow commits to `main`, so pull before the next push.
