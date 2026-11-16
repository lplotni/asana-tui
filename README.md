# asn

Your Asana Inbox in the terminal, with vim keys and a scriptable CLI.

![asn showing an Inbox with read and unread notifications next to the selected task](docs/screenshot.svg)

> Early development (v0.1). The Inbox and read state work; task details, comments, description editing and adding to projects are next ([spec](docs/SPEC.md), [plan](tasks/plan.md)).

## Install

```sh
cargo install --git https://github.com/lplotni/asana-tui
```

## Configure

Everything comes from the environment. Nothing is written to disk.

| Variable | |
|---|---|
| `ASANA_TOKEN` | A [personal access token](https://app.asana.com/0/my-apps) |
| `ASANA_TOKEN_CMD` | …or a command that prints it, e.g. `pass show asana/pat` |
| `ASANA_WORKSPACE` | Workspace GID (digits only): `curl -H "Authorization: Bearer $ASANA_TOKEN" https://app.asana.com/api/1.0/workspaces` |
| `ASN_THEME` | `terminal` (default, follows your terminal's colors), `dark`, `light`, or a path to a theme file ([example](themes/terminal.toml)) |
| `NO_COLOR` | Disables colors |

## Use

`asn` opens the Inbox. It shows the same notifications as the web Inbox, newest first.

| Keys | |
|---|---|
| `j` `k` `gg` `G` `Ctrl-d` `Ctrl-u` | Move (with counts: `5j`, `12G`) |
| `e` / `u` / `x` | Mark read / unread / toggle (Asana's `E` and `U`) |
| `V` | Select a range, then `e` `u` `x` |
| `f` | Unread only / all |
| `R` | Refresh (also every 60 s) |
| `q` | Quit |

Every action is also a command for scripts:

```sh
asn inbox --unread                      # gid, unread marker, time, actor, action, task (tab-separated)
asn inbox --json | jq '.[0]'            # Asana's own JSON
asn inbox --unread | cut -f1 | xargs asn read      # mark everything read
```

`asn read --unread <gid>…` marks notifications unread. Exit codes: `0` ok, `1` error, `2` token problem.

## Develop

```sh
cargo test
cargo run --example screenshot > docs/screenshot.svg   # regenerate the screenshot (made-up data)
```
