# asn

Your Asana Inbox in the terminal, with vim keys and a scriptable CLI.

![asn showing an Inbox with read and unread notifications next to the selected task](docs/screenshot.svg)

> Early development (v0.1). The Inbox, read state and task details work; commenting, description editing and adding to projects are next ([spec](docs/SPEC.md), [plan](tasks/plan.md)).

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

`asn` opens the Inbox. It shows the same notifications as the web Inbox, newest first, and the selected task's details: status, assignee, dates, projects, tags, custom fields, description and every comment.

| Keys | |
|---|---|
| `j` `k` `gg` `G` `Ctrl-d` `Ctrl-u` | Move (with counts: `5j`, `12G`) |
| `l` / `Enter`, `h` / `Esc` | Focus the task (then the same keys scroll it) / back to the Inbox |
| `o` | Open the task in the browser |
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
asn task <task-gid>                     # a task with its description and comments (--json too)
```

`asn read --unread <gid>…` marks notifications unread. Exit codes: `0` ok, `1` error, `2` token problem.

## Known limitations

These come from Asana's [Notifications API](https://forum.asana.com/t/new-notifications-api/1159934), not from `asn`:

- **No archive.** The API can only mark notifications read or unread. `e` marks an item read, and it stays in the list, unlike `E` in the web Inbox, which removes it. Archive in the web app if you want an item gone.
- **Archived items may still show.** The API has no archived flag, so items you archived on the web may still appear in `asn`.
- **No grouping.** The web Inbox can bundle several notifications about one task into one card; `asn` lists them one by one.
- **Polling, not push.** There are no webhooks for notifications, so new items appear on refresh (`R` or every 60 s).
- **Mentions look like comments.** The API doesn't mark @mentions separately.

## Develop

```sh
cargo test
cargo run --example screenshot > docs/screenshot.svg   # regenerate the screenshot (made-up data)
```
