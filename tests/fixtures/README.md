# Fixtures

Real Asana API responses captured on 2026-11-16 (T0), then anonymized. Every GID, person name, task, project or section name, comment, description and URL was replaced with a consistent fake value (GIDs `1200000000000001…`, names from a fixed list, lorem text). JSON structure, field names, HTML tags, timestamps, booleans and enum values are unchanged. A leak check confirmed that no real GID, name or distinctive word survived.

| File | Source | Notes |
|---|---|---|
| `notifications_page.json` | `GET /users/me/notifications?limit=100&opt_fields=…` | 10 items, 5 unread, has `next_page` |
| `notifications_last_page_empty.json` | the `next_page` of the above | **0 items** and `next_page: null` |
| `notifications_edge_cases.json` | **synthetic** | `null` story and actor, an `app` actor, unknown subtype and priority, a non-task object, two objects, no objects |
| `task_plain.json` | `GET /tasks/{gid}?opt_fields=…` | plain `html_notes` |
| `task_rich.json` | same | headings, lists, links and bold in `html_notes` |
| `task_no_projects.json` | same | `projects: []`, so "add to project" gives it its first project |
| `stories.json` | `GET /tasks/{gid}/stories` | 1 comment and 15 system stories |
| `attachments.json` | **synthetic**, shaped like `GET /attachments?parent={gid}` | an image, a PDF, and a Google Drive link without `size` or `download_url` |
| `typeahead_projects.json` | `GET /workspaces/{gid}/typeahead?resource_type=project` | |
| `sections.json` | `GET /projects/{gid}/sections` | 8 sections |
| `mark_read_ok.json` | `POST …/markRead` | `failed_gids: []` |
| `mark_read_partial.json` | `POST …/markRead` with an unknown GID | `failed_gids: ["1"]` |
| `error_401.json` | invalid token | HTTP **401** |
| `error_404_workspace.json` | unknown workspace GID | HTTP **404** |

## Findings that affect the code

- **Pagination:**
  - A page can be shorter than `limit`. `limit=25` returned 9 items, and `limit=100` returned 10 plus a `next_page`.
  - A page can also be **empty** and still be the result of following `next_page`.
  - The only end-of-list signal is `next_page == null`.
- **Pagination offsets expire.** An offset is a JWT with `exp − iat = 900 s`. When loading more after 15 minutes of idle, start again from the first page.
- **Inbox contents:**
  - Every real notification pointed to exactly one `task` and had a `user` actor.
  - `priority_classification` was always `unclassified`.
  - Subtypes seen: `comment_added`, `story_reaction_added`, `assigned`, `added_to_project`, `marked_complete`.
- **Stories are mostly system events** (15 of 16), so the detail pane must filter for `type == "comment"`.
- **Rich descriptions:** 4 of 8 tasks have formatting beyond plain paragraphs (links, lists, headings, bold, code). The F4 rich-text guard will fire for about half of the tasks.
- **Error statuses:** an invalid token gives 401, and an unknown workspace gives 404 with `workspace: Not a recognized ID`.
- **`limit=0`** returned `{"data": [], "next_page": null}`, not a 400.

## Not checked yet (needs the user)

- Whether `U` moves an item back to the Inbox in the Asana web app.

## Terminal color query (checked in T5)

- Rex answers OSC 10/11 in under 1 ms, so the `terminal` theme gets tints. Ghostty is not checked yet.
