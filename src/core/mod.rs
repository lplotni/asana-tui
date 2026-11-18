//! Operations shared by the CLI and the TUI.

pub mod mention;
pub mod resolve;

use crate::api::attachments::Attachment;
use crate::api::notifications::{MARK_BATCH, Notification};
use crate::api::projects::BoardTask;
use crate::api::search::FoundTask;
use crate::api::tasks::{Named, Task, TaskStory, Text};
use crate::api::{ApiError, Client, Page};
use crate::config::SearchMode;
use mention::Mention;

/// Page size requested from Asana. Pages can still come back shorter or empty.
const PAGE_SIZE: u32 = 100;

/// Fetches up to `limit` notifications, newest first, following `next_page`
/// until Asana reports the end. `unread_only` filters on our side because the
/// API has no unread filter.
pub async fn inbox(
    client: &Client,
    workspace: &str,
    limit: usize,
    unread_only: bool,
) -> Result<Vec<Notification>, ApiError> {
    let (mut found, _) = inbox_pages(client, workspace, limit, None, |n| {
        !unread_only || !n.is_read
    })
    .await?;
    found.truncate(limit);
    Ok(found)
}

/// Fetches pages starting at `offset` until at least `target` notifications
/// matching `keep` were collected or the Inbox ends. Returns them with the
/// offset to continue from (`None` at the end). Empty pages are skipped.
pub async fn inbox_pages(
    client: &Client,
    workspace: &str,
    target: usize,
    offset: Option<&str>,
    keep: impl Fn(&Notification) -> bool,
) -> Result<(Vec<Notification>, Option<String>), ApiError> {
    let mut found = Vec::new();
    let mut offset = offset.map(str::to_owned);
    loop {
        let page = client
            .notifications_page(workspace, PAGE_SIZE, offset.as_deref())
            .await
            .map_err(workspace_hint)?;
        found.extend(page.data.into_iter().filter(|n| keep(n)));
        offset = page.next_page.map(|next| next.offset);
        if offset.is_none() || found.len() >= target {
            return Ok((found, offset));
        }
    }
}

/// A task with all its comments (oldest first) and attachments.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TaskDetail {
    pub task: Task,
    pub comments: Vec<TaskStory>,
    /// System stories (activity) not shown as comments.
    pub activity_count: usize,
    pub attachments: Vec<Attachment>,
    /// Why the comments couldn't be loaded. Asana refuses the stories of
    /// some tasks (e.g. with replies the public API can't represent); the
    /// rest of the task is still shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comments_error: Option<String>,
}

/// Fetches the task, every page of its stories and every page of its
/// attachments at the same time.
pub async fn task_detail(client: &Client, gid: &str) -> Result<TaskDetail, ApiError> {
    let (task, stories, attachments) = tokio::join!(
        client.task(gid),
        all_pages(|offset| async move { client.stories_page(gid, offset.as_deref()).await }),
        all_pages(|offset| async move { client.attachments_page(gid, offset.as_deref()).await }),
    );
    let task = task?;
    let (stories, comments_error) = match stories {
        Ok(stories) => (stories, None),
        Err(err) => (Vec::new(), Some(err.to_string())),
    };
    let (comments, activity): (Vec<TaskStory>, Vec<TaskStory>) =
        stories.into_iter().partition(TaskStory::is_comment);
    Ok(TaskDetail {
        task,
        comments,
        activity_count: activity.len(),
        attachments: attachments?,
        comments_error,
    })
}

/// Adds a comment to a task and returns it as Asana stored it.
/// Mentions in `text` make it rich text; without any it stays plain.
pub async fn comment(
    client: &Client,
    task_gid: &str,
    text: &str,
    mentions: &[Mention],
) -> Result<TaskStory, ApiError> {
    match mention::rich_text(text, mentions) {
        Some(html) => client.add_comment(task_gid, Text::Rich(&html)).await,
        None => client.add_comment(task_gid, Text::Plain(text)).await,
    }
}

/// People, projects and tasks matching `query`, in that order, to
/// @-mention: three typeahead searches at the same time.
pub async fn search_mentions(
    client: &Client,
    workspace: &str,
    query: &str,
) -> Result<Vec<mention::Target>, ApiError> {
    let (people, projects, tasks) = tokio::try_join!(
        client.typeahead_users(workspace, query),
        client.typeahead_projects(workspace, query),
        client.typeahead_tasks(workspace, query),
    )?;
    let target =
        |kind: &'static str, gid: String, name: String| mention::Target { kind, gid, name };
    Ok(people
        .into_iter()
        .map(|p| target("person", p.gid, p.name))
        .chain(
            projects
                .into_iter()
                .map(|p| target("project", p.gid, p.name)),
        )
        .chain(tasks.into_iter().map(|t| target("task", t.gid, t.name)))
        .collect())
}

/// People in the workspace whose name matches `query` (Asana's typeahead).
pub async fn search_users(
    client: &Client,
    workspace: &str,
    query: &str,
) -> Result<Vec<Named>, ApiError> {
    client.typeahead_users(workspace, query).await
}

/// Projects matching `query` (Asana's typeahead; works on every plan).
pub async fn search_projects(
    client: &Client,
    workspace: &str,
    query: &str,
) -> Result<Vec<Named>, ApiError> {
    client.typeahead_projects(workspace, query).await
}

/// Tasks in the workspace matching `query`, using the `ASN_SEARCH` backend.
/// A blank query finds nothing (task search would return every task).
pub async fn search_tasks(
    client: &Client,
    workspace: &str,
    query: &str,
    mode: SearchMode,
) -> Result<Vec<FoundTask>, ApiError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    match mode {
        SearchMode::Tasks => client
            .search_tasks(workspace, query)
            .await
            .map_err(search_plan_hint),
        SearchMode::Typeahead => client.typeahead_tasks(workspace, query).await,
    }
}

/// Task search needs a paid plan; Asana answers 402 otherwise.
fn search_plan_hint(err: ApiError) -> ApiError {
    with_hint(
        err,
        &[402],
        "set ASN_SEARCH=typeahead to search task names instead",
    )
}

/// The sections of a project, in board order.
pub async fn sections(client: &Client, project_gid: &str) -> Result<Vec<Named>, ApiError> {
    client.sections(project_gid).await
}

/// A project's open tasks as board columns, in the project's section order.
#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub project: Named,
    pub columns: Vec<Column>,
}

/// One section of a board and its tasks in board order. `section` is `None`
/// for the leading column of tasks whose section isn't known.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub section: Option<Named>,
    pub tasks: Vec<BoardTask>,
}

/// Loads the project, its sections and every page of its open tasks at the
/// same time, and groups the tasks into columns.
pub async fn board(client: &Client, project_gid: &str) -> Result<Board, ApiError> {
    let (project, sections, tasks) = tokio::try_join!(
        client.project(project_gid),
        client.sections(project_gid),
        all_pages(|offset| async move {
            client
                .project_tasks_page(project_gid, offset.as_deref())
                .await
        }),
    )?;
    Ok(group_board(project, sections, tasks))
}

/// One column per section, keeping empty ones. Each task goes to its section
/// in *this* project (a task can be in several). Tasks without a known
/// section go to a leading column that exists only when it has tasks.
fn group_board(project: Named, sections: Vec<Named>, tasks: Vec<BoardTask>) -> Board {
    let mut columns: Vec<Column> = sections
        .into_iter()
        .map(|section| Column {
            section: Some(section),
            tasks: Vec::new(),
        })
        .collect();
    let mut unsectioned = Vec::new();
    for task in tasks {
        let section_gid = task
            .memberships
            .iter()
            .find(|m| m.project.gid == project.gid)
            .and_then(|m| m.section.as_ref())
            .map(|s| s.gid.clone());
        let column = section_gid.and_then(|gid| {
            columns
                .iter_mut()
                .find(|c| c.section.as_ref().is_some_and(|s| s.gid == gid))
        });
        match column {
            Some(column) => column.tasks.push(task),
            None => unsectioned.push(task),
        }
    }
    if !unsectioned.is_empty() {
        columns.insert(
            0,
            Column {
                section: None,
                tasks: unsectioned,
            },
        );
    }
    Board { project, columns }
}

/// Adds the task to a project (and optionally a section), keeping the
/// projects it's already in.
pub async fn add_to_project(
    client: &Client,
    task_gid: &str,
    project_gid: &str,
    section_gid: Option<&str>,
) -> Result<(), ApiError> {
    client.add_project(task_gid, project_gid, section_gid).await
}

/// Marks the task complete (or, not `completed`, incomplete again).
pub async fn set_completed(
    client: &Client,
    task_gid: &str,
    completed: bool,
) -> Result<(), ApiError> {
    client.set_completed(task_gid, completed).await
}

/// Assigns the task to a user GID, `"me"`, or (`None`) nobody; returns
/// whom Asana has it assigned to now.
pub async fn assign(
    client: &Client,
    task_gid: &str,
    assignee: Option<&str>,
) -> Result<Option<Named>, ApiError> {
    client.set_assignee(task_gid, assignee).await
}

/// Assigns the task to `who`: a user GID, `me`, `none`, or a person's name
/// (F19). A name that doesn't resolve to exactly one person is returned as
/// [`resolve::Unresolved`], and the task is left as it is.
pub async fn assign_to(
    client: &Client,
    workspace: &str,
    task_gid: &str,
    who: &str,
) -> Result<Result<Option<Named>, resolve::Unresolved>, ApiError> {
    let assignee = match who {
        "none" => None,
        "me" => Some("me".to_owned()),
        gid if resolve::is_gid(gid) => Some(gid.to_owned()),
        name => {
            let found = search_users(client, workspace, name).await?;
            match resolve::pick("person", name, found) {
                Ok(person) => Some(person.gid),
                Err(unresolved) => return Ok(Err(unresolved)),
            }
        }
    };
    assign(client, task_gid, assignee.as_deref()).await.map(Ok)
}

/// A task made by [`create_task`], and why it isn't in the section asked
/// for, if it isn't. The task exists either way.
#[derive(Debug)]
pub struct CreatedTask {
    pub task: Named,
    pub section_error: Option<ApiError>,
}

/// Creates an unassigned task in the project, then puts it in `section_gid`
/// (at the top) if given. Only a failure to create is an error: a task that
/// exists but couldn't be put in the section is returned with that error, so
/// nobody creates it twice.
pub async fn create_task(
    client: &Client,
    project_gid: &str,
    section_gid: Option<&str>,
    name: &str,
) -> Result<CreatedTask, ApiError> {
    let task = client.create_task(project_gid, name).await?;
    let section_error = match section_gid {
        Some(section) => client.add_task_to_section(section, &task.gid).await.err(),
        None => None,
    };
    Ok(CreatedTask {
        task,
        section_error,
    })
}

/// Moves the task to another section of a project it's already in.
pub async fn move_to_section(
    client: &Client,
    task_gid: &str,
    section_gid: &str,
) -> Result<(), ApiError> {
    client.add_task_to_section(section_gid, task_gid).await
}

/// Moves the task to `section`: a section GID, or a section name (exact, or
/// the only section containing it) among the task's projects (F19). Returns
/// where the task is now (`Project › Section`, if Asana reports it). A name
/// that doesn't resolve to one section is returned as
/// [`resolve::Unresolved`], with candidates labelled `Project › Section`, and
/// nothing is moved.
pub async fn move_to(
    client: &Client,
    task_gid: &str,
    section: &str,
) -> Result<Result<Option<String>, resolve::Unresolved>, ApiError> {
    let section_gid = if resolve::is_gid(section) {
        section.to_owned()
    } else {
        // Sections of the task's projects whose name contains the name asked
        // for, each with its `Project › Section` label.
        let task = client.task(task_gid).await?;
        let wanted = section.trim().to_lowercase();
        let mut found: Vec<(Named, String)> = Vec::new();
        for membership in &task.memberships {
            for candidate in sections(client, &membership.project.gid).await? {
                if candidate.name.to_lowercase().contains(&wanted) {
                    let label = format!("{} › {}", membership.project.name, candidate.name);
                    found.push((candidate, label));
                }
            }
        }
        let names = found.iter().map(|(s, _)| s.clone()).collect();
        match resolve::pick("section", section, names) {
            Ok(picked) => picked.gid,
            Err(mut unresolved) => {
                for candidate in &mut unresolved.candidates {
                    if let Some((_, label)) = found.iter().find(|(s, _)| s.gid == candidate.gid) {
                        candidate.name.clone_from(label);
                    }
                }
                return Ok(Err(unresolved));
            }
        }
    };
    move_to_section(client, task_gid, &section_gid).await?;
    // The move is done; where the task is now is only reported, so a failed
    // read must not turn it into an error (and a retry).
    let place = client.task(task_gid).await.ok().and_then(|task| {
        task.memberships
            .iter()
            .find(|m| m.section.as_ref().is_some_and(|s| s.gid == section_gid))
            .map(crate::api::tasks::Membership::place)
    });
    Ok(Ok(place))
}

/// Why a description couldn't be saved.
#[derive(Debug, thiserror::Error)]
pub enum SaveNotesError {
    #[error("the task was changed on the web while you were editing")]
    Conflict,
    #[error(transparent)]
    Api(#[from] ApiError),
}

/// Whether Asana's `html_notes` has anything beyond plain paragraphs (links,
/// lists, bold, mentions, …) that saving plain `notes` would remove.
/// Mentions (`<a data-asana-gid=…>`) don't count: saving keeps them.
pub fn has_formatting(html_notes: &str) -> bool {
    html_notes.split('<').skip(1).any(|tag| {
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        // A `</a>` closes an `<a`, which counted already unless a mention.
        let mention = name == "a" && (closing || tag.contains("data-asana-gid="));
        !name.is_empty() && name != "body" && !mention
    })
}

/// Replaces the task's description with `text` (rich text if it has
/// mentions, else plain), unless the task was modified after
/// `expected_modified_at` (then nothing is written).
pub async fn set_notes(
    client: &Client,
    task_gid: &str,
    text: &str,
    mentions: &[Mention],
    expected_modified_at: Option<&str>,
) -> Result<(), SaveNotesError> {
    if let Some(expected) = expected_modified_at {
        let current = client.modified_at(task_gid).await?;
        if current.as_deref() != Some(expected) {
            return Err(SaveNotesError::Conflict);
        }
    }
    match mention::rich_text(text, mentions) {
        Some(html) => client.set_notes(task_gid, Text::Rich(&html)).await?,
        None => client.set_notes(task_gid, Text::Plain(text)).await?,
    }
    Ok(())
}

/// Every item of a paginated list: calls `fetch` with each page's offset
/// (`None` first) until Asana reports the end.
async fn all_pages<T, F, Fut>(mut fetch: F) -> Result<Vec<T>, ApiError>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<Page<T>, ApiError>>,
{
    let mut items = Vec::new();
    let mut offset: Option<String> = None;
    loop {
        let page = fetch(offset.take()).await?;
        items.extend(page.data);
        match page.next_page {
            Some(next) => offset = Some(next.offset),
            None => return Ok(items),
        }
    }
}

/// The Inbox call is workspace-scoped, so 403/404 almost always mean a wrong
/// `ASANA_WORKSPACE`.
fn workspace_hint(err: ApiError) -> ApiError {
    with_hint(err, &[403, 404], "check ASANA_WORKSPACE")
}

/// Appends `hint` to Asana's message when the status is one of `statuses`.
fn with_hint(err: ApiError, statuses: &[u16], hint: &str) -> ApiError {
    match err {
        ApiError::Status { status, message } if statuses.contains(&status) => ApiError::Status {
            status,
            message: format!("{message} — {hint}"),
        },
        other => other,
    }
}

/// What happened when marking notifications: the GIDs that weren't updated,
/// and the first error if a whole batch failed.
#[derive(Debug, Default)]
pub struct MarkOutcome {
    pub failed: Vec<String>,
    pub error: Option<ApiError>,
}

/// Marks notifications read (or unread) in batches of [`MARK_BATCH`]. Every
/// batch is tried: a failing batch puts its own GIDs into `failed` without
/// affecting the others. No GIDs means no requests.
pub async fn mark_read(
    client: &Client,
    workspace: &str,
    gids: &[String],
    read: bool,
) -> MarkOutcome {
    let mut outcome = MarkOutcome::default();
    for batch in gids.chunks(MARK_BATCH) {
        match client.mark_notifications(workspace, batch, read).await {
            Ok(failed) => outcome.failed.extend(failed),
            Err(err) => {
                outcome.failed.extend_from_slice(batch);
                outcome.error.get_or_insert(workspace_hint(err));
            }
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tasks::Membership;

    fn named(gid: &str, name: &str) -> Named {
        Named {
            gid: gid.into(),
            name: name.into(),
        }
    }

    /// A task that is in `memberships`, given as (project, section) GIDs.
    fn task(gid: &str, memberships: &[(&str, Option<&str>)]) -> BoardTask {
        BoardTask {
            gid: gid.into(),
            name: format!("Task {gid}"),
            assignee: None,
            due_on: None,
            due_at: None,
            memberships: memberships
                .iter()
                .map(|(project, section)| Membership {
                    project: named(project, "Project"),
                    section: section.map(|s| named(s, "Section")),
                })
                .collect(),
        }
    }

    fn gids(column: &Column) -> Vec<&str> {
        column.tasks.iter().map(|t| t.gid.as_str()).collect()
    }

    #[test]
    fn tasks_land_in_their_section_in_board_order() {
        let sections = vec![named("s1", "To do"), named("s2", "Doing")];
        let tasks = vec![
            task("t1", &[("p", Some("s2"))]),
            task("t2", &[("p", Some("s1"))]),
            task("t3", &[("p", Some("s2"))]),
        ];

        let board = group_board(named("p", "Project"), sections, tasks);

        assert_eq!(board.columns.len(), 2);
        assert_eq!(board.columns[0].section, Some(named("s1", "To do")));
        assert_eq!(gids(&board.columns[0]), ["t2"]);
        assert_eq!(gids(&board.columns[1]), ["t1", "t3"]);
    }

    #[test]
    fn a_task_in_two_projects_is_placed_by_this_projects_section() {
        let sections = vec![named("s1", "To do"), named("s2", "Doing")];
        let tasks = vec![task("t1", &[("other", Some("s1")), ("p", Some("s2"))])];

        let board = group_board(named("p", "Project"), sections, tasks);

        assert!(board.columns[0].tasks.is_empty());
        assert_eq!(gids(&board.columns[1]), ["t1"]);
    }

    #[test]
    fn empty_sections_are_kept() {
        let sections = vec![named("s1", "To do"), named("s2", "Done")];

        let board = group_board(
            named("p", "Project"),
            sections,
            vec![task("t1", &[("p", Some("s1"))])],
        );

        assert_eq!(board.columns.len(), 2);
        assert!(board.columns[1].tasks.is_empty());
    }

    #[test]
    fn tasks_without_a_known_section_go_to_a_leading_no_section_column() {
        let sections = vec![named("s1", "To do")];
        let tasks = vec![
            task("t1", &[("p", Some("unknown"))]),
            task("t2", &[("p", None)]),
            task("t3", &[("p", Some("s1"))]),
        ];

        let board = group_board(named("p", "Project"), sections, tasks);

        assert_eq!(board.columns[0].section, None);
        assert_eq!(gids(&board.columns[0]), ["t1", "t2"]);
        assert_eq!(gids(&board.columns[1]), ["t3"]);
    }

    #[test]
    fn there_is_no_no_section_column_when_every_task_has_a_section() {
        let board = group_board(
            named("p", "Project"),
            vec![named("s1", "To do")],
            vec![task("t1", &[("p", Some("s1"))])],
        );

        assert_eq!(board.columns.len(), 1);
        assert!(board.columns[0].section.is_some());
    }
}
