//! Measures how long Asana takes to show a write (spec F28): in the board's
//! task list (what a board reload sees), in the task itself (what a detail
//! reload sees) and in the project's events.
//!
//!     cargo run --release --example latency -- <project-gid> [--runs 3] [--every 2]
//!
//! Use a throwaway board with at least two sections. Each run makes a task
//! named `asn latency probe …`, then creates, moves, assigns (to you), sets
//! the board's first single-select field on, and completes it, through the
//! same functions as `asn`. Every task it made is completed at the end, and
//! an open one an interrupted run left behind is completed at the start.

use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use asn::api::projects::BoardTask;
use asn::api::tasks::{Membership, Task};
use asn::api::{ASANA_API_URL, ApiError, Client};
use asn::{config, core};
use clap::Parser;
use serde::Deserialize;

/// Name prefix of the probe's tasks.
const PREFIX: &str = "asn latency probe";
/// After this long a read that still doesn't show a write is a timeout.
const GIVE_UP: Duration = Duration::from_secs(300);
/// The event fields the checks use.
const EVENT_FIELDS: &str = "action,resource,parent,change.field,created_at";
/// The writes, in the order each run makes them.
const WRITES: [&str; 5] = ["create", "move", "assign", "field", "complete"];

/// Measures how long Asana takes to show a write (spec F28).
#[derive(Parser)]
struct Args {
    /// The throwaway board's project GID.
    project: String,
    /// How many tasks to run the writes on.
    #[arg(long, default_value_t = 3)]
    runs: usize,
    /// Seconds between two reads of the same kind.
    #[arg(long, default_value_t = 2.0)]
    every: f64,
}

/// A write, as the reads should come to show it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Write {
    /// Created, and put in `section`.
    Create {
        section: String,
    },
    /// Moved to `section`.
    Move {
        section: String,
    },
    /// Assigned to the user `user`.
    Assign {
        user: String,
    },
    /// The single-select `field` set to `option`.
    Field {
        field: String,
        option: String,
    },
    Complete,
}

impl Write {
    fn label(&self) -> &'static str {
        match self {
            Write::Create { .. } => "create",
            Write::Move { .. } => "move",
            Write::Assign { .. } => "assign",
            Write::Field { .. } => "field",
            Write::Complete => "complete",
        }
    }
}

/// Whether the board's open tasks show `write` to `task`.
fn list_shows(tasks: &[BoardTask], project: &str, task: &str, write: &Write) -> bool {
    let card = tasks.iter().find(|t| t.gid == task);
    match (write, card) {
        (Write::Complete, card) => card.is_none(),
        (_, None) => false,
        (Write::Create { section } | Write::Move { section }, Some(card)) => {
            in_section(&card.memberships, project, section)
        }
        (Write::Assign { user }, Some(card)) => {
            card.assignee.as_ref().is_some_and(|a| a.gid == *user)
        }
        (Write::Field { field, option }, Some(card)) => card.option_of(field) == Some(option),
    }
}

/// Whether the task, as `GET /tasks/{gid}` returns it, shows `write`.
fn task_shows(task: &Task, project: &str, write: &Write) -> bool {
    match write {
        Write::Create { section } | Write::Move { section } => {
            in_section(&task.memberships, project, section)
        }
        Write::Assign { user } => task.assignee.as_ref().is_some_and(|a| a.gid == *user),
        Write::Field { field, option } => task
            .custom_fields
            .iter()
            .any(|f| f.gid == *field && f.enum_value.as_ref().is_some_and(|v| v.gid == *option)),
        Write::Complete => task.completed,
    }
}

fn in_section(memberships: &[Membership], project: &str, section: &str) -> bool {
    memberships
        .iter()
        .any(|m| m.project.gid == project && m.section.as_ref().is_some_and(|s| s.gid == section))
}

/// One event of the project's event stream.
#[derive(Debug, Deserialize)]
struct Event {
    action: String,
    resource: Gid,
    #[serde(default)]
    parent: Option<Gid>,
    #[serde(default)]
    change: Option<Change>,
}

#[derive(Debug, Deserialize)]
struct Gid {
    gid: String,
}

#[derive(Debug, Deserialize)]
struct Change {
    field: String,
}

/// Whether `event` is the one `write` to `task` causes. Asana's docs: a task
/// added to a project or section is `added` with that parent, and a changed
/// field is `changed` with `change.field`.
fn event_shows(event: &Event, task: &str, write: &Write) -> bool {
    if event.resource.gid != task {
        return false;
    }
    let changed = |field: &str| {
        event.action == "changed" && event.change.as_ref().is_some_and(|c| c.field == field)
    };
    match write {
        Write::Create { .. } => event.action == "added",
        Write::Move { section } => {
            event.action == "added" && event.parent.as_ref().is_some_and(|p| p.gid == *section)
        }
        Write::Assign { .. } => changed("assignee"),
        Write::Field { .. } => changed("custom_fields"),
        Write::Complete => changed("completed"),
    }
}

/// One page of `GET /projects/{gid}/events`. A 412 (no or an expired sync
/// token) has no events, only a fresh token.
#[derive(Debug, Deserialize)]
struct EventPage {
    #[serde(default)]
    data: Vec<Event>,
    sync: String,
    #[serde(default)]
    has_more: bool,
}

/// The project's event stream, read with plain requests (no `api` method is
/// added for a feature asn doesn't use).
struct Events {
    http: reqwest::Client,
    url: String,
    token: String,
    sync: Option<String>,
}

impl Events {
    fn new(token: &str, project: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            url: format!("{ASANA_API_URL}/projects/{project}/events"),
            token: token.to_owned(),
            sync: None,
        }
    }

    /// The events since the last call. The first call only gets a sync token.
    async fn next(&mut self) -> Result<Vec<Event>, String> {
        let mut events = Vec::new();
        loop {
            let mut query = vec![("opt_fields", EVENT_FIELDS.to_owned())];
            if let Some(sync) = &self.sync {
                query.push(("sync", sync.clone()));
            }
            let response = self
                .http
                .get(&self.url)
                .query(&query)
                .bearer_auth(&self.token)
                .send()
                .await
                .map_err(|_| "events: request failed".to_owned())?;
            let status = response.status().as_u16();
            if status == 429 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            if status != 200 && status != 412 {
                return Err(format!("events: Asana answered {status}"));
            }
            let page: EventPage = response
                .json()
                .await
                .map_err(|_| "events: unexpected response".to_owned())?;
            if status == 412 && self.sync.is_some() {
                eprintln!("events: the sync token expired; events since may be missing");
            }
            self.sync = Some(page.sync);
            events.extend(page.data);
            if !page.has_more {
                return Ok(events);
            }
        }
    }
}

/// How long each read took to show one write.
#[derive(Debug, Clone, PartialEq)]
struct Sample {
    write: &'static str,
    confirm: Duration,
    list: Option<Duration>,
    task: Option<Duration>,
    events: Option<Duration>,
}

/// The reads after a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Read {
    List,
    Task,
    Events,
}

const READS: [Read; 3] = [Read::List, Read::Task, Read::Events];

struct Probe {
    client: Client,
    events: Events,
    project: String,
    every: Duration,
    samples: Vec<Sample>,
}

impl Probe {
    /// Reads, staggered, each read every `every`, until each shows `write`
    /// to `task` or [`GIVE_UP`] passes; then keeps the sample.
    async fn measure(&mut self, task: &str, write: &Write, confirm: Duration) {
        let confirmed = Instant::now();
        let mut seen: [Option<Duration>; 3] = [None; 3];
        let step = self.every / 3;
        for (turn, read) in READS.iter().cycle().enumerate() {
            if seen.iter().all(Option::is_some) || confirmed.elapsed() >= GIVE_UP {
                break;
            }
            let slot = turn % READS.len();
            if seen[slot].is_none() {
                match self.shows(*read, task, write).await {
                    Ok(true) => seen[slot] = Some(confirmed.elapsed()),
                    Ok(false) => {}
                    Err(message) => eprintln!("{read:?}: {message}"),
                }
            }
            tokio::time::sleep(step).await;
        }
        let sample = Sample {
            write: write.label(),
            confirm,
            list: seen[0],
            task: seen[1],
            events: seen[2],
        };
        eprintln!(
            "{:<9} confirmed in {} ms; list {}, task {}, events {}",
            sample.write,
            sample.confirm.as_millis(),
            secs(sample.list),
            secs(sample.task),
            secs(sample.events),
        );
        self.samples.push(sample);
    }

    async fn shows(&mut self, read: Read, task: &str, write: &Write) -> Result<bool, String> {
        match read {
            Read::List => {
                let tasks = board_tasks(&self.client, &self.project)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(list_shows(&tasks, &self.project, task, write))
            }
            Read::Task => {
                let found = self.client.task(task).await.map_err(|e| e.to_string())?;
                Ok(task_shows(&found, &self.project, write))
            }
            Read::Events => {
                let events = self.events.next().await?;
                for event in events.iter().filter(|e| e.resource.gid == task) {
                    if !event_shows(event, task, write) {
                        eprintln!("events: other event for the task: {event:?}");
                    }
                }
                Ok(events.iter().any(|e| event_shows(e, task, write)))
            }
        }
    }
}

/// Every page of the board's open tasks, with the board's own query.
async fn board_tasks(client: &Client, project: &str) -> Result<Vec<BoardTask>, ApiError> {
    let mut tasks = Vec::new();
    let mut offset: Option<String> = None;
    loop {
        let page = client
            .project_tasks_page(project, offset.as_deref())
            .await?;
        tasks.extend(page.data);
        match page.next_page {
            Some(next) => offset = Some(next.offset),
            None => return Ok(tasks),
        }
    }
}

/// `0.4` seconds, or `>300` for a read that never showed the write.
fn secs(value: Option<Duration>) -> String {
    value.map_or_else(
        || format!(">{}", GIVE_UP.as_secs()),
        |d| format!("{:.1}", d.as_secs_f64()),
    )
}

/// `median/max` of `values`, a timeout counting as the longest.
fn med_max(mut values: Vec<Option<Duration>>, show: impl Fn(Option<Duration>) -> String) -> String {
    values.sort_by_key(|v| v.unwrap_or(Duration::MAX));
    match (values.get(values.len() / 2), values.last()) {
        (Some(median), Some(max)) => format!("{}/{}", show(*median), show(*max)),
        _ => "n/a".to_owned(),
    }
}

/// The result table: per write, the median and maximum over the runs.
fn table(samples: &[Sample]) -> String {
    let mut out = format!(
        "{:<9}{:>14}{:>14}{:>14}{:>14}\n{:<9}{:>14}{:>14}{:>14}{:>14}\n",
        "write",
        "confirm-ms",
        "list-s",
        "task-s",
        "events-s",
        "",
        "med/max",
        "med/max",
        "med/max",
        "med/max",
    );
    let ms = |d: Option<Duration>| d.map_or_else(String::new, |d| d.as_millis().to_string());
    for write in WRITES {
        let rows: Vec<&Sample> = samples.iter().filter(|s| s.write == write).collect();
        let column = |pick: fn(&Sample) -> Option<Duration>,
                      show: &dyn Fn(Option<Duration>) -> String| {
            med_max(rows.iter().map(|s| pick(s)).collect(), show)
        };
        out.push_str(&format!(
            "{write:<9}{:>14}{:>14}{:>14}{:>14}\n",
            column(|s| Some(s.confirm), &ms),
            column(|s| s.list, &secs),
            column(|s| s.task, &secs),
            column(|s| s.events, &secs),
        ));
    }
    out
}

/// The time of day in UTC, `14:03:22Z`, to tell the probe's tasks apart.
fn clock() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!(
        "{:02}:{:02}:{:02}Z",
        secs / 3600 % 24,
        secs / 60 % 60,
        secs % 60
    )
}

/// Completes the probe's open tasks on the board (an interrupted run's).
async fn complete_leftovers(client: &Client, project: &str) -> Result<(), String> {
    let tasks = board_tasks(client, project)
        .await
        .map_err(|e| e.to_string())?;
    for task in tasks.iter().filter(|t| t.name.starts_with(PREFIX)) {
        client
            .set_completed(&task.gid, true)
            .await
            .map_err(|e| e.to_string())?;
        eprintln!("completed {} (left by an earlier run)", task.name);
    }
    Ok(())
}

/// One run: a new task through every write, each measured.
async fn one_run(
    probe: &mut Probe,
    run: usize,
    sections: (&str, &str),
    field: Option<&(String, String)>,
    open: &mut Vec<String>,
) -> Result<(), String> {
    let err = |e: ApiError| e.to_string();
    let name = format!("{PREFIX} {} #{run}", clock());
    let project = probe.project.clone();

    let started = Instant::now();
    let created = core::create_task(&probe.client, &project, Some(sections.0), &name, None)
        .await
        .map_err(err)?;
    let confirm = started.elapsed();
    let gid = created.task.gid;
    open.push(gid.clone());
    if let Some(e) = created.section_error {
        return Err(format!("created, but not in the first section: {e}"));
    }
    let create = Write::Create {
        section: sections.0.to_owned(),
    };
    probe.measure(&gid, &create, confirm).await;

    let started = Instant::now();
    core::move_to_section(&probe.client, &gid, sections.1)
        .await
        .map_err(err)?;
    let moved = Write::Move {
        section: sections.1.to_owned(),
    };
    probe.measure(&gid, &moved, started.elapsed()).await;

    let started = Instant::now();
    let me = probe
        .client
        .set_assignee(&gid, Some("me"))
        .await
        .map_err(err)?
        .ok_or("Asana didn't say whom the task is assigned to")?;
    probe
        .measure(&gid, &Write::Assign { user: me.gid }, started.elapsed())
        .await;

    if let Some((field, option)) = field {
        let started = Instant::now();
        probe
            .client
            .set_select_field(&gid, field, Some(option))
            .await
            .map_err(err)?;
        let set = Write::Field {
            field: field.clone(),
            option: option.clone(),
        };
        probe.measure(&gid, &set, started.elapsed()).await;
    }

    let started = Instant::now();
    probe.client.set_completed(&gid, true).await.map_err(err)?;
    open.retain(|g| *g != gid);
    probe
        .measure(&gid, &Write::Complete, started.elapsed())
        .await;
    Ok(())
}

async fn run(args: Args) -> Result<(), String> {
    if args.runs == 0 || !(args.every.is_finite() && args.every > 0.0) {
        return Err("--runs and --every must be positive".to_owned());
    }
    let config = config::load().map_err(|e| e.to_string())?;
    let client = Client::new(&config.token, ASANA_API_URL);
    let project = args.project;
    let sections = core::sections(&client, &project)
        .await
        .map_err(|e| e.to_string())?;
    let [first, second, ..] = sections.as_slice() else {
        return Err("the board needs at least two sections".to_owned());
    };
    let sections = (first.gid.clone(), second.gid.clone());
    let field = core::select_fields(&client, &project)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find_map(|f| Some((f.gid, f.options.into_iter().find(|o| o.enabled)?.gid)));
    if field.is_none() {
        eprintln!("no single-select field with an enabled option: field is skipped");
    }
    complete_leftovers(&client, &project).await?;
    let mut events = Events::new(config.token.expose(), &project);
    events.next().await?;

    let mut probe = Probe {
        client,
        events,
        project,
        every: Duration::from_secs_f64(args.every),
        samples: Vec::new(),
    };
    let mut open = Vec::new();
    let mut result = Ok(());
    for run in 1..=args.runs {
        eprintln!("run {run}/{}", args.runs);
        result = one_run(
            &mut probe,
            run,
            (&sections.0, &sections.1),
            field.as_ref(),
            &mut open,
        )
        .await;
        if result.is_err() {
            break;
        }
    }
    for gid in &open {
        if let Err(e) = probe.client.set_completed(gid, true).await {
            eprintln!("couldn't complete probe task {gid}: {e}");
        }
    }
    print!("{}", table(&probe.samples));
    result
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("latency: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PROJECT: &str = "1200000000000001";
    const TASK: &str = "1200000000000010";
    const TODO: &str = "1200000000000002";
    const DOING: &str = "1200000000000003";
    const ADA: &str = "1200000000000020";
    const FIELD: &str = "1200000000000030";
    const OPTION: &str = "1200000000000031";

    fn card(section: &str, assignee: Option<&str>, option: Option<&str>) -> BoardTask {
        serde_json::from_value(json!({
            "gid": TASK,
            "name": "asn latency probe 10:00:00Z #1",
            "assignee": assignee.map(|gid| json!({"gid": gid, "name": "Ada Lovelace"})),
            "memberships": [{
                "project": {"gid": PROJECT, "name": "Scratch"},
                "section": {"gid": section, "name": "Section"}
            }],
            "custom_fields": [{"gid": FIELD, "enum_value": option.map(|gid| json!({"gid": gid}))}]
        }))
        .unwrap()
    }

    fn task(section: &str, completed: bool, option: Option<&str>) -> Task {
        serde_json::from_value(json!({
            "gid": TASK,
            "name": "asn latency probe 10:00:00Z #1",
            "completed": completed,
            "permalink_url": "https://app.asana.com/0/0/1200000000000010",
            "assignee": {"gid": ADA, "name": "Ada Lovelace"},
            "memberships": [{
                "project": {"gid": PROJECT, "name": "Scratch"},
                "section": {"gid": section, "name": "Section"}
            }],
            "custom_fields": [{
                "gid": FIELD,
                "name": "Initiative",
                "display_value": null,
                "resource_subtype": "enum",
                "enum_value": option.map(|gid| json!({"gid": gid, "name": "Checkout"}))
            }]
        }))
        .unwrap()
    }

    fn event(value: serde_json::Value) -> Event {
        serde_json::from_value(value).unwrap()
    }

    fn moved_to(section: &str) -> Write {
        Write::Move {
            section: section.to_owned(),
        }
    }

    #[test]
    fn the_list_shows_a_card_in_its_section() {
        let tasks = [card(DOING, None, None)];
        assert!(list_shows(&tasks, PROJECT, TASK, &moved_to(DOING)));
        assert!(!list_shows(&tasks, PROJECT, TASK, &moved_to(TODO)));
        let created = Write::Create {
            section: DOING.to_owned(),
        };
        assert!(list_shows(&tasks, PROJECT, TASK, &created));
        assert!(!list_shows(&[], PROJECT, TASK, &created));
    }

    #[test]
    fn the_list_shows_an_assignee_and_an_option() {
        let assigned = Write::Assign {
            user: ADA.to_owned(),
        };
        let set = Write::Field {
            field: FIELD.to_owned(),
            option: OPTION.to_owned(),
        };
        assert!(!list_shows(
            &[card(TODO, None, None)],
            PROJECT,
            TASK,
            &assigned
        ));
        assert!(list_shows(
            &[card(TODO, Some(ADA), None)],
            PROJECT,
            TASK,
            &assigned
        ));
        assert!(!list_shows(&[card(TODO, None, None)], PROJECT, TASK, &set));
        assert!(list_shows(
            &[card(TODO, None, Some(OPTION))],
            PROJECT,
            TASK,
            &set
        ));
    }

    #[test]
    fn the_list_shows_a_completion_by_leaving_the_card_out() {
        assert!(!list_shows(
            &[card(TODO, None, None)],
            PROJECT,
            TASK,
            &Write::Complete
        ));
        assert!(list_shows(&[], PROJECT, TASK, &Write::Complete));
    }

    #[test]
    fn a_membership_of_another_project_doesnt_count() {
        let tasks = [card(DOING, None, None)];
        assert!(!list_shows(
            &tasks,
            "1200000000000099",
            TASK,
            &moved_to(DOING)
        ));
    }

    #[test]
    fn the_task_read_shows_each_write() {
        let open = task(DOING, false, Some(OPTION));
        assert!(task_shows(&open, PROJECT, &moved_to(DOING)));
        assert!(!task_shows(&open, PROJECT, &moved_to(TODO)));
        assert!(task_shows(
            &open,
            PROJECT,
            &Write::Assign {
                user: ADA.to_owned()
            }
        ));
        let set = Write::Field {
            field: FIELD.to_owned(),
            option: OPTION.to_owned(),
        };
        assert!(task_shows(&open, PROJECT, &set));
        assert!(!task_shows(&task(DOING, false, None), PROJECT, &set));
        assert!(!task_shows(&open, PROJECT, &Write::Complete));
        assert!(task_shows(
            &task(DOING, true, None),
            PROJECT,
            &Write::Complete
        ));
    }

    #[test]
    fn events_match_by_task_action_and_field() {
        let added = event(json!({
            "action": "added",
            "resource": {"gid": TASK},
            "parent": {"gid": DOING}
        }));
        assert!(event_shows(
            &added,
            TASK,
            &Write::Create {
                section: TODO.to_owned()
            }
        ));
        assert!(event_shows(&added, TASK, &moved_to(DOING)));
        assert!(!event_shows(&added, TASK, &moved_to(TODO)));
        assert!(!event_shows(&added, "1200000000000011", &moved_to(DOING)));

        let assigned = event(json!({
            "action": "changed",
            "resource": {"gid": TASK},
            "parent": null,
            "change": {"field": "assignee", "action": "changed"}
        }));
        assert!(event_shows(
            &assigned,
            TASK,
            &Write::Assign {
                user: ADA.to_owned()
            }
        ));
        assert!(!event_shows(&assigned, TASK, &Write::Complete));
        let completed = event(json!({
            "action": "changed",
            "resource": {"gid": TASK},
            "change": {"field": "completed"}
        }));
        assert!(event_shows(&completed, TASK, &Write::Complete));
    }

    #[test]
    fn an_events_page_and_a_412_both_carry_a_sync_token() {
        let page: EventPage = serde_json::from_value(json!({
            "data": [{"action": "added", "resource": {"gid": TASK}, "parent": {"gid": TODO}}],
            "sync": "next",
            "has_more": false
        }))
        .unwrap();
        assert_eq!((page.data.len(), page.sync.as_str()), (1, "next"));
        let expired: EventPage = serde_json::from_value(json!({
            "errors": [{"message": "Sync token invalid or too old."}],
            "sync": "fresh"
        }))
        .unwrap();
        assert_eq!((expired.data.len(), expired.sync.as_str()), (0, "fresh"));
    }

    #[test]
    fn the_table_shows_median_and_max_with_timeouts_last() {
        let s = |confirm: u64, list: Option<u64>| Sample {
            write: "move",
            confirm: Duration::from_millis(confirm),
            list: list.map(Duration::from_millis),
            task: Some(Duration::from_millis(300)),
            events: None,
        };
        let table = table(&[s(400, Some(500)), s(300, None), s(500, Some(1500))]);
        let row = table.lines().find(|l| l.starts_with("move")).unwrap();
        let cells: Vec<&str> = row.split_whitespace().collect();
        assert_eq!(
            cells,
            ["move", "400/500", "1.5/>300", "0.3/0.3", ">300/>300"]
        );
        let field = table.lines().find(|l| l.starts_with("field")).unwrap();
        assert_eq!(
            field.split_whitespace().collect::<Vec<_>>(),
            ["field", "n/a", "n/a", "n/a", "n/a"]
        );
    }

    #[test]
    fn the_clock_is_a_time_of_day() {
        let clock = clock();
        assert_eq!(clock.len(), 9);
        assert!(clock.ends_with('Z'));
    }
}
