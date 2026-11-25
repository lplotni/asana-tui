# Configuration

[asn](../README.md) › Configuration

Run `asn` (or `asn init`) and answer two questions: a command that prints your [personal access token](https://app.asana.com/0/my-apps), and your workspace, which `asn` lists for you. The answers go to `~/.config/asn/config.toml` (or `$XDG_CONFIG_HOME/asn/config.toml`), readable only by you. `asn init` again changes them.

The token itself is never stored: keep it in a password manager or the keychain and give `asn` the command that reads it, e.g. `op read op://Private/Asana/token`, `pass show asana/pat`, or on macOS `security find-generic-password -s asn -w` (after `security add-generic-password -s asn -a asana -w`).

Every setting can also come from an environment variable, which wins over the file:

| File key | Variable | |
|---|---|---|
| | `ASANA_TOKEN` | The token itself (wins over any token command) |
| `token_cmd` | `ASANA_TOKEN_CMD` | A command that prints the token |
| `workspace` | `ASANA_WORKSPACE` | Workspace GID (digits only) |
| `search` | `ASN_SEARCH` | How `S` and `asn search` find tasks: `tasks` (default) uses Asana's full-text [task search](https://developers.asana.com/reference/searchtasksforworkspace) (names and descriptions; paid plans only); `typeahead` matches task names like the web search box and works on every plan |
| `theme` | `ASN_THEME` | `terminal` (default, follows your terminal's colors), `dark`, `light`, or a path to a theme file ([example](../themes/terminal.toml)) |
| `animations` | `ASN_ANIMATIONS` | A completed board card is struck through before it leaves: `true` (default) or `false` in the file, `on` or `off` in the variable |
| `refresh` | `ASN_REFRESH` | Seconds between refreshes while the terminal is in front: 5 to 60, default 5. In the background, `asn` refreshes once a minute. See [Refresh and rate limits](#refresh-and-rate-limits) |
| | `NO_COLOR` | Disables colors |

```toml
# ~/.config/asn/config.toml
workspace = "1200000000000001"
token_cmd = "op read op://Private/Asana/token"
search = "typeahead"
animations = false
```

CLI subcommands read the same file but never ask questions, so scripts behave the same with or without it.

## Refresh and rate limits

The TUI shows your own changes as soon as Asana confirms them (usually 2–5 s). Everything else (other people's changes, the `asn` CLI, the Claude Code skill, the web app) arrives with the next refresh. While the terminal is in front, that is every `refresh` seconds, **5 by default**. When your terminal reports focus, `asn` refreshes only once a minute while it's in the background, and catches up at once when you come back.

Each refresh costs requests, and Asana limits them per token: **150 a minute on the free plan, 1,500 on paid plans (Enterprise included).** The `asn` CLI and the Claude Code skill count against the same limit when they use the same token. One refresh is 1 (Inbox) + 3 + ⌈tasks / 100⌉ (the board on screen) + 3 (the task shown) requests:

| Board | Requests per refresh | Per minute at 5 s (default) | at 10 s | at 30 s |
|---|---|---|---|---|
| 100 tasks | 8 | 96 | 48 | 16 |
| 300 tasks | 10 | 120 | 60 | 20 |
| 500 tasks | 12 | 144 | 72 | 24 |
| 1,000 tasks | 17 | 204 | 102 | 34 |

- **Free plan:** 5 s fits boards up to about 300 tasks, with a little room for the CLI. For larger boards, or heavy CLI use alongside, set `refresh = 10` or more.
- **Paid plans:** 5 s fits any board.
- **Over the limit:** Asana answers 429. `asn` waits as Asana asks and tries once more. If that fails too, that refresh fails with a `Refresh failed: …` message, the board keeps what it shows, and the next refresh tries again. A CLI command run at the same moment can fail the same way.
