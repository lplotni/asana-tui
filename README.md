# asn

Your Asana Inbox and project boards in the terminal, with vim keys and a scriptable CLI.

![asn showing an Inbox with read and unread notifications next to the selected task](docs/screenshot.svg)

I built `asn` to get a simple terminal UI for the way I work with Asana:

- **The Inbox:** go through notifications, read the task, comment, and mark them read.
- **Projects as boards:** see where every task of a project stands, section by section, and act on any card.

Every action is also a small CLI command for scripts, and an `asn` skill lets Claude Code read and update your tasks while you code.

## Install

```sh
brew tap lplotni/asn https://github.com/lplotni/asana-tui
brew install lplotni/asn/asn
```

Or download a binary for macOS (Apple Silicon, Intel) or Linux (x86_64, ARM) from [Releases](https://github.com/lplotni/asana-tui/releases), or build it with `cargo install --git https://github.com/lplotni/asana-tui`. The release binaries aren't code-signed, so macOS blocks one downloaded in a browser: allow it with `xattr -d com.apple.quarantine ./asn`.

## Get started

```sh
asn                           # the first run asks for a command that prints your token, and your workspace
asn --project <project-gid>   # open a project's board
```

The bottom line shows the keys that work where you are, and `?` lists them all. `P` opens a project's board, and `Tab i` goes back to the Inbox.

## Documentation

| | |
|---|---|
| [Using the TUI](docs/tui.md) | The Inbox, task details, boards and swimlanes, and every key |
| [CLI](docs/cli.md) | The commands for scripts: `asn inbox`, `asn task`, `asn board` and the rest |
| [Configuration](docs/configuration.md) | The token, settings and environment variables, refresh and Asana's rate limits |
| [Claude Code](docs/claude-code.md) | The `asn` skill: how to install it, and the rules it follows |
| [How asn talks to Asana](docs/asana-api.md) | The endpoints it calls, and what Asana's public API can't do |
| [Development](docs/development.md) | Tests, screenshots, latency tools, releases, specs and design decisions |

## Versions

- **v0.4:** boards and the Inbox refresh every 5 s while the terminal is in front (once a minute behind it; see `refresh`), and saving a description no longer fails after another change to the task.
- **v0.3:** swimlanes and single-select fields on boards, a config file set up with `asn init`, names instead of GIDs in the CLI, and the `asn` skill for Claude Code.
- **v0.2:** project boards, creating, completing and assigning tasks, and @-mentions.
- **v0.1:** the Inbox, read state, task details, comments, description editing and adding to projects.

## License

[MIT](LICENSE)
