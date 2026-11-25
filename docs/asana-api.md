# How asn talks to Asana

[asn](../README.md) › How asn talks to Asana

`asn` uses Asana's public [REST API](https://developers.asana.com/reference/rest-api-reference) (v1.0, `https://app.asana.com/api/1.0`) over HTTPS with JSON, authenticated with your personal access token as a `Bearer` header. Every request asks only for the fields it shows (`opt_fields`), and lists are read page by page via `next_page.offset`.

| What | Endpoint |
|---|---|
| Setup (`asn init`) | `GET /workspaces` |
| Inbox | `GET /users/me/notifications`, `POST /users/me/notifications/markRead` (and `markUnread`) |
| Task, description, completion, assignee | `GET /tasks/{task}`, `PUT /tasks/{task}` |
| Comments and reactions | `GET /tasks/{task}/stories`, `POST /tasks/{task}/stories` (`html_text` when there's an @-mention, as for descriptions with `html_notes`) |
| People (`asn users`, @-mentions, names in `asn task assign`) | `GET /workspaces/{workspace}/typeahead?resource_type=user` |
| Attachments | `GET /attachments?parent={task}` |
| Task search (`S`, `asn search`) | `GET /workspaces/{workspace}/tasks/search`, or `GET /workspaces/{workspace}/typeahead?resource_type=task` with `ASN_SEARCH=typeahead` |
| Add to project | `GET /workspaces/{workspace}/typeahead`, `GET /projects/{project}/sections`, `POST /tasks/{task}/addProject` |
| Move to a section | `GET /projects/{project}/sections`, `POST /sections/{section}/addTask` |
| New task (`asn project new`) | `POST /tasks`, then `POST /sections/{section}/addTask` |
| Boards (`P`, `b`, `asn board`) | `GET /projects/{project}`, `GET /projects/{project}/sections`, `GET /projects/{project}/tasks?completed_since=now`; projects are found (`P`, `asn projects`, project names) with `GET /workspaces/{workspace}/typeahead` |
| Images | the attachment's short-lived `download_url`, fetched from the file host without your token |

The Asana web app runs on its own internal API, so it can show things the public API can't; those gaps are listed below.

## Known limitations

These come from Asana's public API (most from the [Notifications API](https://forum.asana.com/t/new-notifications-api/1159934)), not from `asn`:

- **No archive.** The API can only mark notifications read or unread. `e` marks an item read, and it stays in the list, unlike `E` in the web Inbox, which removes it. Archive in the web app if you want an item gone: archived items disappear from `asn` at its next refresh.
- **No grouping.** The web Inbox can bundle several notifications about one task into one card; `asn` lists them one by one.
- **Polling, not push.** There are no webhooks for notifications, so new items appear on refresh (`R`, or every 5 s while the terminal is in front).
- **Descriptions are edited as plain text.** Saving keeps @-mentions but replaces other formatting (links, lists, bold); `asn` asks first when a description has any. Edit formatted descriptions on the web.
- **Mentions look like comments.** The API doesn't mark @mentions separately.
- **Some tasks' comments can't be loaded.** For some tasks (likely those with replies to comments), Asana refuses the comment list with "unsupported parent in the public API: CommentStory". `asn` still shows the rest of the task and says why the comments are missing; read them on the web.
- **Task search needs a paid plan.** On other plans Asana answers 402; set `ASN_SEARCH=typeahead` to search task names instead. Results are the 50 most relevant, and new or renamed tasks can take up to a minute to become searchable.
- **Boards show open, top-level tasks only.** Completed tasks and subtasks aren't on the board, and cards can't be reordered within a column; `m` moves them between sections.
- **Image links expire.** An attachment's `download_url` works for a short time only; if an image fails after the task was open a while, refresh (`R`).
