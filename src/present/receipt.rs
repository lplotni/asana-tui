//! The read receipt (`asn task --brief`, F18): a fixed-format summary of a
//! task built from its fields only. Description and comment text never
//! appear in it, so a crafted task can't change what the receipt says.

use super::detail::due_text;
use crate::core::TaskDetail;

/// The receipt's lines. The first holds the task name and is the one to
/// truncate for a narrow output.
pub fn lines(detail: &TaskDetail, now: i64) -> Vec<String> {
    let task = &detail.task;
    let mut out = vec![format!("Read from Asana: {}", task.name)];
    out.push(format!("  gid   {}   {}", task.gid, task.permalink_url));
    if task.memberships.is_empty() {
        out.push("  where no project".to_owned());
    }
    for membership in &task.memberships {
        let place = match &membership.section {
            Some(section) => format!("{} › {}", membership.project.name, section.name),
            None => membership.project.name.clone(),
        };
        out.push(format!("  where {place}"));
    }

    let mut who = vec![
        task.assignee
            .as_ref()
            .map_or("unassigned".to_owned(), |a| a.name.clone()),
    ];
    if let Some(due) = due_text(task, now) {
        who.push(format!("due {due}"));
    }
    who.push(if task.completed { "completed" } else { "open" }.to_owned());
    out.push(format!("  who   {}", who.join(" · ")));

    let description = if task.notes.trim().is_empty() {
        "no description".to_owned()
    } else {
        format!(
            "description {}",
            count(task.notes.trim_end().lines().count(), "line")
        )
    };
    let comments = if detail.comments_error.is_some() {
        "comments unavailable".to_owned()
    } else {
        count(detail.comments.len(), "comment")
    };
    let attachments = count(detail.attachments.len(), "attachment");
    out.push(format!(
        "  has   {description} · {comments} · {attachments}"
    ));
    out
}

/// "1 comment", "3 comments".
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}
