# CLI

[asn](../README.md) › CLI

Every action in the TUI is also a command, for scripts. The CLI is deliberately small: it covers the same use cases (`init`, `inbox`, `read`, `task`, `comment`, `desc`, `project add`, `project new`, `board`) and nothing more. For full access to the Asana API from the command line, use [Asana's own CLI](https://developers.asana.com/docs/asana-cli) (`npm install --global @asana-api/cli`).

```sh
asn inbox --unread                      # gid, unread marker, time, actor, action, task (tab-separated)
asn inbox --json | jq '.[0]'            # Asana's own JSON
asn inbox --unread | cut -f1 | xargs asn read      # mark everything read
asn search checkout redirect           # find tasks: gid, ✓ if completed, name (tab-separated; --json too)
asn task <task-gid>                     # a task with its description, attachments and comments (--json too)
asn task <task-gid> --brief             # a short receipt: where it is, who has it, due, counts (no description or comment text)
asn task done <task-gid> [--undo]       # mark it complete (or incomplete again)
asn task assign <task-gid> "Ada Lovelace"   # a name, a user gid or email, `me`, or `none`; prints the new assignee
asn task field <task-gid> Initiative Checkout   # set a single-select field (or `none` to clear it); prints "Initiative: Checkout"
asn comment <task-gid> -m 'Ship it'      # or `-` for stdin, or no text to open $EDITOR; prints the comment's gid
asn comment <task-gid> -m 'Thanks @1200000000000011'   # @<gid> mentions that person, task or project
asn users ada                           # people in the workspace: gid and name (tab-separated)
asn projects web                        # projects in the workspace: gid and name (tab-separated)
asn project add <task-gid> Ops [--section "In Review"]       # keeps the task's other projects; prints where it landed
asn project sections <project-gid>      # a project's sections: gid and name (tab-separated)
asn project fields "Web Platform"       # a project's single-select fields, each followed by its options
asn project move <task-gid> "In Review"      # to another section of a project it's in; prints where it went
asn project new "Web Platform" [--section Backlog] Write the launch post   # create an unassigned task; prints its gid
asn board <project-gid>                 # open tasks by section: section lines, then indented gid, name, assignee, due (--json too)
asn board <project-gid> --lanes Initiative   # the same, per lane: lane lines, then that lane's sections and tasks
asn --project <project-gid>             # open the TUI on that project's board
asn desc <task-gid> [--file PATH | -] [--force]   # replace the description (plain text; --force for formatted ones)
```

`asn read --unread <gid>…` marks notifications unread. Exit codes: `0` ok, `1` error, `2` token problem. With `--json`, tasks from `asn board` and `asn task` include their `custom_fields` (field and option GIDs, and for `asn task` the field type and options).

People, projects and sections can be given by name or by gid. A name is used only when it matches exactly (ignoring case) or matches a single candidate; sections are matched among the task's (or the named project's) own sections, and part of a section name is enough. Anything else writes nothing, exits `1`, and lists the candidates on stderr as `gid  name` lines, so you can run the command again with a gid.
