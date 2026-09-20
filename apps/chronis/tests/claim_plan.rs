//! `cn claim` on a task someone else holds must say so.
//!
//! It used to skip any non-open task in silence and exit 0, so a claim that
//! never happened read as success. A session then worked a task still held by
//! a dead session's claim, which is how two sessions end up on one task.

use chronis::{
    application::claim_task::plan_claim,
    domain::{
        error::ChronError,
        task::{Priority, Task, TaskStatus, TaskType},
    },
};

fn task(id: &str, status: TaskStatus, claimed_by: Option<&str>) -> Task {
    Task {
        id: id.to_string(),
        title: format!("task {id}"),
        priority: Priority::P1,
        status,
        task_type: TaskType::Task,
        parent: None,
        claimed_by: claimed_by.map(str::to_string),
        blocked_by: vec![],
        created_at: None,
        done_reason: None,
        done_at: None,
        awaiting_approval: None,
        approved: None,
        approved_at: None,
        description: None,
        archived: false,
    }
}

#[test]
fn claiming_a_held_task_names_the_holder_instead_of_passing_silently() {
    let held = task(
        "t-10f997",
        TaskStatus::InProgress,
        Some("claude:b68b0105"),
    );

    let err = plan_claim("t-10f997", std::slice::from_ref(&held)).unwrap_err();

    match err {
        ChronError::AlreadyClaimed { id, holder } => {
            assert_eq!(id, "t-10f997");
            assert_eq!(holder, "claude:b68b0105");
            assert!(
                err_text(&ChronError::AlreadyClaimed { id, holder }).contains("cn release"),
                "the message must say how to take the task back"
            );
        }
        other => panic!("expected AlreadyClaimed, got {other:?}"),
    }
}

#[test]
fn a_done_target_reports_the_status_rather_than_a_holder() {
    let done = task("t-0002", TaskStatus::Done, None);

    match plan_claim("t-0002", std::slice::from_ref(&done)).unwrap_err() {
        ChronError::InvalidTransition { id, current, .. } => {
            assert_eq!(id, "t-0002");
            assert_eq!(current, "done");
        }
        other => panic!("expected InvalidTransition, got {other:?}"),
    }
}

#[test]
fn an_open_target_is_claimed() {
    let open = task("t-0003", TaskStatus::Open, None);

    let plan = plan_claim("t-0003", std::slice::from_ref(&open)).unwrap();

    assert_eq!(plan.to_claim, ["t-0003"]);
    assert!(plan.skipped.is_empty());
}

/// Claiming an epic must not fail because one child is busy — but the caller
/// still has to be told which children it did not get.
#[test]
fn a_busy_cascade_child_is_skipped_and_reported() {
    let tasks = vec![
        task("t-epic", TaskStatus::Open, None),
        task("t-child-a", TaskStatus::Open, None),
        task("t-child-b", TaskStatus::InProgress, Some("claude:other")),
        task("t-child-c", TaskStatus::Done, None),
    ];

    let plan = plan_claim("t-epic", &tasks).unwrap();

    assert_eq!(plan.to_claim, ["t-epic", "t-child-a"]);
    let skipped: Vec<(&str, TaskStatus)> = plan
        .skipped
        .iter()
        .map(|s| (s.id.as_str(), s.status))
        .collect();
    assert_eq!(
        skipped,
        [
            ("t-child-b", TaskStatus::InProgress),
            ("t-child-c", TaskStatus::Done)
        ]
    );
    assert_eq!(plan.skipped[0].held_by.as_deref(), Some("claude:other"));
}

fn err_text(err: &ChronError) -> String {
    err.to_string()
}
