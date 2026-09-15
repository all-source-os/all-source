use std::sync::Arc;

use allsource_core::embedded::{Config, EmbeddedCore};
use chronis::{
    application::{
        create_task::{CreateTaskInput, create_task_with_id_gen},
        reparent_task::reparent_task,
    },
    domain::{
        error::ChronError,
        repository::{TaskEdit, TaskRepository},
        task::{TaskStatus, TaskType},
    },
    infrastructure::{
        backend::CoreBackend, core_task_repo::CoreTaskRepository, projection::TaskProjection,
    },
};

async fn setup() -> CoreTaskRepository {
    let config = Config::builder()
        .single_tenant(true)
        .build()
        .expect("config");
    let core = EmbeddedCore::open(config).await.expect("core");
    let core = Arc::new(core);
    core.inner()
        .register_projection_with_backfill(
            &(Arc::new(TaskProjection::new()) as Arc<dyn allsource_core::application::Projection>),
        )
        .expect("projection");
    let backend = Arc::new(CoreBackend::new_embedded(core));
    CoreTaskRepository::new(backend)
}

#[tokio::test]
async fn create_and_list_task() {
    let repo = setup().await;
    repo.create_task(
        "t-0001",
        "Write tests",
        "p1",
        &[],
        TaskType::Task,
        None,
        None,
    )
    .await
    .unwrap();

    let tasks = repo.list_tasks(None).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "t-0001");
    assert_eq!(tasks[0].title, "Write tests");
    assert_eq!(tasks[0].priority.to_string(), "p1");
    assert_eq!(tasks[0].status.to_string(), "open");
    assert_eq!(tasks[0].task_type, TaskType::Task);
}

#[tokio::test]
async fn full_lifecycle_create_claim_done() {
    let repo = setup().await;
    repo.create_task(
        "t-0001",
        "Build feature",
        "p0",
        &[],
        TaskType::Feature,
        None,
        None,
    )
    .await
    .unwrap();

    repo.claim_task("t-0001", "agent-1").await.unwrap();
    let task = repo.get_task("t-0001").unwrap();
    assert_eq!(task.status.to_string(), "in-progress");
    assert_eq!(task.claimed_by.as_deref(), Some("agent-1"));

    repo.complete_task("t-0001", Some("shipped")).await.unwrap();
    let task = repo.get_task("t-0001").unwrap();
    assert_eq!(task.status.to_string(), "done");
    assert_eq!(task.done_reason.as_deref(), Some("shipped"));
}

#[tokio::test]
async fn claim_non_open_task_fails() {
    let repo = setup().await;
    repo.create_task("t-0001", "Task", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();
    repo.claim_task("t-0001", "a").await.unwrap();

    let err = repo.claim_task("t-0001", "b").await.unwrap_err();
    assert!(matches!(err, ChronError::InvalidTransition { .. }));
}

#[tokio::test]
async fn release_returns_claimed_task_to_pool() {
    let repo = setup().await;
    repo.create_task("t-0001", "Task", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();
    repo.claim_task("t-0001", "claude:dead0000").await.unwrap();

    repo.release_task("t-0001", "human", Some("session died"))
        .await
        .unwrap();

    let task = repo.get_task("t-0001").unwrap();
    assert_eq!(task.status, TaskStatus::Open);
    assert_eq!(task.claimed_by, None);
    assert!(repo.ready_tasks().unwrap().iter().any(|t| t.id == "t-0001"));

    repo.claim_task("t-0001", "claude:new00000").await.unwrap();
    let task = repo.get_task("t-0001").unwrap();
    assert_eq!(task.claimed_by.as_deref(), Some("claude:new00000"));

    let detail = repo.get_task_detail("t-0001").await.unwrap();
    let types: Vec<&str> = detail
        .timeline
        .iter()
        .map(|e| e.event_type.as_str())
        .collect();
    assert_eq!(
        types,
        [
            "task.created",
            "workflow.claimed",
            "workflow.released",
            "workflow.claimed"
        ]
    );
}

#[tokio::test]
async fn release_unclaimed_task_fails() {
    let repo = setup().await;
    repo.create_task("t-0001", "Task", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();

    let err = repo
        .release_task("t-0001", "human", None)
        .await
        .unwrap_err();
    assert!(matches!(err, ChronError::NotClaimed(_)));
}

#[tokio::test]
async fn release_done_task_fails() {
    let repo = setup().await;
    repo.create_task("t-0001", "Task", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();
    repo.claim_task("t-0001", "a").await.unwrap();
    repo.complete_task("t-0001", None).await.unwrap();

    let err = repo
        .release_task("t-0001", "human", None)
        .await
        .unwrap_err();
    assert!(matches!(err, ChronError::NotClaimed(_)));
    assert_eq!(repo.get_task("t-0001").unwrap().status, TaskStatus::Done);
}

#[tokio::test]
async fn reparent_moves_a_task_between_epics_and_back_to_the_root() {
    let repo = setup().await;
    repo.create_task("t-e1", "Epic one", "p1", &[], TaskType::Epic, None, None)
        .await
        .unwrap();
    repo.create_task("t-e2", "Epic two", "p1", &[], TaskType::Epic, None, None)
        .await
        .unwrap();
    repo.create_task(
        "t-kid",
        "Child",
        "p2",
        &[],
        TaskType::Task,
        Some("t-e1"),
        None,
    )
    .await
    .unwrap();

    repo.reparent_task("t-kid", Some("t-e2")).await.unwrap();
    assert_eq!(
        repo.get_task("t-kid").unwrap().parent.as_deref(),
        Some("t-e2")
    );
    assert!(repo.children_of("t-e1").unwrap().is_empty());
    assert_eq!(repo.children_of("t-e2").unwrap().len(), 1);

    repo.reparent_task("t-kid", None).await.unwrap();
    assert_eq!(repo.get_task("t-kid").unwrap().parent, None);
    assert!(repo.children_of("t-e2").unwrap().is_empty());
}

#[tokio::test]
async fn an_unrelated_edit_leaves_the_parent_alone() {
    let repo = setup().await;
    repo.create_task("t-e1", "Epic", "p1", &[], TaskType::Epic, None, None)
        .await
        .unwrap();
    repo.create_task(
        "t-kid",
        "Child",
        "p2",
        &[],
        TaskType::Task,
        Some("t-e1"),
        None,
    )
    .await
    .unwrap();

    let edit = TaskEdit {
        title: Some("Renamed".into()),
        ..Default::default()
    };
    repo.edit_task("t-kid", &edit).await.unwrap();

    let task = repo.get_task("t-kid").unwrap();
    assert_eq!(task.title, "Renamed");
    assert_eq!(task.parent.as_deref(), Some("t-e1"));
}

#[tokio::test]
async fn a_refused_reparent_emits_no_event() {
    let repo = setup().await;
    repo.create_task("t-e1", "Epic", "p1", &[], TaskType::Epic, None, None)
        .await
        .unwrap();
    repo.create_task(
        "t-kid",
        "Child",
        "p2",
        &[],
        TaskType::Task,
        Some("t-e1"),
        None,
    )
    .await
    .unwrap();

    let universe = repo.list_tasks_all(None).unwrap();
    let err = reparent_task(&repo, &universe, "t-e1", Some("t-kid"), true)
        .await
        .unwrap_err();
    assert!(matches!(err, ChronError::ReparentRefused(_)));

    assert_eq!(repo.get_task("t-e1").unwrap().parent, None);
    let detail = repo.get_task_detail("t-e1").await.unwrap();
    assert_eq!(detail.timeline.len(), 1);
    assert_eq!(detail.timeline[0].event_type, "task.created");
}

#[tokio::test]
async fn a_warned_reparent_needs_force() {
    let repo = setup().await;
    repo.create_task("t-a", "Plain", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();
    repo.create_task("t-b", "Other", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();

    let universe = repo.list_tasks_all(None).unwrap();
    let err = reparent_task(&repo, &universe, "t-a", Some("t-b"), false)
        .await
        .unwrap_err();
    assert!(matches!(err, ChronError::ReparentNeedsForce { .. }));
    assert_eq!(repo.get_task("t-a").unwrap().parent, None);

    let warnings = reparent_task(&repo, &universe, "t-a", Some("t-b"), true)
        .await
        .unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(repo.get_task("t-a").unwrap().parent.as_deref(), Some("t-b"));
}

#[tokio::test]
async fn complete_already_done_fails() {
    let repo = setup().await;
    repo.create_task("t-0001", "Task", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();
    repo.complete_task("t-0001", None).await.unwrap();

    let err = repo.complete_task("t-0001", None).await.unwrap_err();
    assert!(matches!(err, ChronError::AlreadyDone(_)));
}

#[tokio::test]
async fn ready_excludes_blocked_tasks() {
    let repo = setup().await;
    repo.create_task("t-0001", "Dep", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();
    repo.create_task(
        "t-0002",
        "Blocked",
        "p2",
        &["t-0001".to_string()],
        TaskType::Task,
        None,
        None,
    )
    .await
    .unwrap();
    repo.create_task("t-0003", "Free", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();

    let ready = repo.ready_tasks().unwrap();
    let ids: Vec<&str> = ready.iter().map(|t| t.id.as_str()).collect();
    assert!(ids.contains(&"t-0001"));
    assert!(ids.contains(&"t-0003"));
    assert!(!ids.contains(&"t-0002"));
}

#[tokio::test]
async fn blocker_resolved_unblocks_dependent() {
    let repo = setup().await;
    repo.create_task("t-0001", "Dep", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();
    repo.create_task(
        "t-0002",
        "Blocked",
        "p2",
        &["t-0001".to_string()],
        TaskType::Task,
        None,
        None,
    )
    .await
    .unwrap();

    // t-0002 should NOT be ready
    let ready = repo.ready_tasks().unwrap();
    assert!(!ready.iter().any(|t| t.id == "t-0002"));

    // Complete the blocker
    repo.complete_task("t-0001", None).await.unwrap();

    // Now t-0002 should be ready
    let ready = repo.ready_tasks().unwrap();
    assert!(ready.iter().any(|t| t.id == "t-0002"));
}

#[tokio::test]
async fn get_task_returns_timeline() {
    let repo = setup().await;
    repo.create_task("t-0001", "Task", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();
    repo.claim_task("t-0001", "human").await.unwrap();

    let detail = repo.get_task_detail("t-0001").await.unwrap();
    assert_eq!(detail.task.id, "t-0001");
    assert_eq!(detail.timeline.len(), 2);
    assert_eq!(detail.timeline[0].event_type, "task.created");
    assert_eq!(detail.timeline[1].event_type, "workflow.claimed");
}

#[tokio::test]
async fn approve_task() {
    let repo = setup().await;
    repo.create_task("t-0001", "Review", "p1", &[], TaskType::Task, None, None)
        .await
        .unwrap();
    repo.approve_task("t-0001").await.unwrap();

    let task = repo.get_task("t-0001").unwrap();
    assert_eq!(task.approved, Some(true));
}

#[tokio::test]
async fn add_dependency_post_creation() {
    let repo = setup().await;
    repo.create_task("t-0001", "Blocker", "p1", &[], TaskType::Task, None, None)
        .await
        .unwrap();
    repo.create_task("t-0002", "Dependent", "p2", &[], TaskType::Task, None, None)
        .await
        .unwrap();

    // t-0002 should be ready initially
    let ready = repo.ready_tasks().unwrap();
    assert!(ready.iter().any(|t| t.id == "t-0002"));

    // Add dependency post-creation
    repo.add_dependency("t-0002", "t-0001").await.unwrap();

    // t-0002 should no longer be ready
    let ready = repo.ready_tasks().unwrap();
    assert!(!ready.iter().any(|t| t.id == "t-0002"));

    // Remove dependency
    repo.remove_dependency("t-0002", "t-0001").await.unwrap();

    // t-0002 should be ready again
    let ready = repo.ready_tasks().unwrap();
    assert!(ready.iter().any(|t| t.id == "t-0002"));
}

#[tokio::test]
async fn epic_with_children() {
    let repo = setup().await;
    repo.create_task(
        "t-epic",
        "Auth System",
        "p0",
        &[],
        TaskType::Epic,
        None,
        None,
    )
    .await
    .unwrap();
    repo.create_task(
        "t-child1",
        "Login flow",
        "p1",
        &[],
        TaskType::Task,
        Some("t-epic"),
        None,
    )
    .await
    .unwrap();
    repo.create_task(
        "t-child2",
        "Signup flow",
        "p2",
        &[],
        TaskType::Task,
        Some("t-epic"),
        None,
    )
    .await
    .unwrap();
    // Non-child task
    repo.create_task(
        "t-other",
        "Unrelated",
        "p3",
        &[],
        TaskType::Task,
        None,
        None,
    )
    .await
    .unwrap();

    let children = repo.children_of("t-epic").unwrap();
    assert_eq!(children.len(), 2);
    let child_ids: Vec<&str> = children.iter().map(|t| t.id.as_str()).collect();
    assert!(child_ids.contains(&"t-child1"));
    assert!(child_ids.contains(&"t-child2"));

    // Verify parent is set on the child
    let child = repo.get_task("t-child1").unwrap();
    assert_eq!(child.parent.as_deref(), Some("t-epic"));
    assert_eq!(child.task_type, TaskType::Task);

    // Verify epic type
    let epic = repo.get_task("t-epic").unwrap();
    assert_eq!(epic.task_type, TaskType::Epic);
}

#[tokio::test]
async fn task_with_description() {
    let repo = setup().await;
    repo.create_task(
        "t-0001",
        "Fix login bug",
        "p1",
        &[],
        TaskType::Bug,
        None,
        Some("Users can't login with special characters in password"),
    )
    .await
    .unwrap();

    let task = repo.get_task("t-0001").unwrap();
    assert_eq!(task.task_type, TaskType::Bug);
    assert_eq!(
        task.description.as_deref(),
        Some("Users can't login with special characters in password")
    );
}

// ---------------------------------------------------------------------------
// Issue #194: `cn task create` must never merge into / mutate an existing task
// whose short ID collides. It must land on a fresh ID or fail loudly.
// ---------------------------------------------------------------------------

/// The repository guard rejects a `task.created` against an already-projected
/// entity_id BEFORE emitting any event — so no dependency leaks onto the
/// existing record.
#[tokio::test]
async fn create_task_on_taken_id_is_rejected_with_no_side_effects() {
    let repo = setup().await;
    repo.create_task("t-aaaa", "Original", "p1", &[], TaskType::Task, None, None)
        .await
        .unwrap();

    // Direct repo call with a colliding ID and a phantom blocker. The guard
    // must fire before the task.created OR the task.dependency.added is
    // ingested.
    let err = repo
        .create_task(
            "t-aaaa",
            "Impostor",
            "p0",
            &["t-bbbb".to_string()],
            TaskType::Bug,
            Some("t-cccc"),
            Some("should never persist"),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ChronError::IdAlreadyTaken(id) if id == "t-aaaa"));

    // The original record is completely untouched: title/type/priority intact,
    // and crucially NO phantom blocked_by/parent leaked in.
    let original = repo.get_task("t-aaaa").unwrap();
    assert_eq!(original.title, "Original");
    assert_eq!(original.task_type, TaskType::Task);
    assert_eq!(original.priority.to_string(), "p1");
    assert!(original.blocked_by.is_empty(), "no phantom blocker leaked");
    assert_eq!(original.parent, None, "no phantom parent leaked");
    assert_eq!(original.status.to_string(), "open");

    // The timeline shows ONLY the original task.created — the impostor's
    // events were never ingested.
    let detail = repo.get_task_detail("t-aaaa").await.unwrap();
    assert_eq!(detail.timeline.len(), 1);
    assert_eq!(detail.timeline[0].event_type, "task.created");
}

/// When the first candidate ID collides, the application layer retries with a
/// fresh ID and the new task is created under a DIFFERENT id with full content
/// intact, while the pre-existing task is unchanged.
#[tokio::test]
async fn create_task_retries_past_collision_onto_fresh_id() {
    let repo = setup().await;
    repo.create_task(
        "t-aaaa",
        "Original",
        "p1",
        &["t-dead".to_string()],
        TaskType::Epic,
        None,
        None,
    )
    .await
    .unwrap();

    // Stub generator: first hand out the TAKEN id, then a free one.
    let mut ids = vec!["t-ffff".to_string(), "t-aaaa".to_string()]; // popped from end
    let out = create_task_with_id_gen(
        &repo,
        CreateTaskInput {
            title: "Fresh task",
            priority: "p0",
            blocked_by: &["t-beef".to_string()],
            task_type: TaskType::Bug,
            parent: Some("t-aaaa"),
            description: Some("full content"),
        },
        move || ids.pop().expect("generator exhausted"),
    )
    .await
    .unwrap();

    // Landed on the fresh, DIFFERENT id.
    assert_eq!(out.id, "t-ffff");

    // New task has its full content intact.
    let fresh = repo.get_task("t-ffff").unwrap();
    assert_eq!(fresh.title, "Fresh task");
    assert_eq!(fresh.task_type, TaskType::Bug);
    assert_eq!(fresh.priority.to_string(), "p0");
    assert_eq!(fresh.parent.as_deref(), Some("t-aaaa"));
    assert_eq!(fresh.description.as_deref(), Some("full content"));
    assert_eq!(fresh.blocked_by, vec!["t-beef".to_string()]);

    // Pre-existing task is UNCHANGED: title, type, status, and its original
    // single blocker — no leakage from the impostor attempt.
    let original = repo.get_task("t-aaaa").unwrap();
    assert_eq!(original.title, "Original");
    assert_eq!(original.task_type, TaskType::Epic);
    assert_eq!(original.status.to_string(), "open");
    assert_eq!(original.blocked_by, vec!["t-dead".to_string()]);
    assert_eq!(original.parent, None);
}

/// When every candidate ID collides, create fails with a typed error and emits
/// NO events against the existing record.
#[tokio::test]
async fn create_task_exhausts_retries_and_errors_without_side_effects() {
    let repo = setup().await;
    repo.create_task("t-aaaa", "Original", "p1", &[], TaskType::Task, None, None)
        .await
        .unwrap();

    let before = repo.get_task_detail("t-aaaa").await.unwrap().timeline.len();

    // Generator always returns the taken id — every attempt collides.
    let err = create_task_with_id_gen(
        &repo,
        CreateTaskInput {
            title: "Doomed",
            priority: "p0",
            blocked_by: &["t-beef".to_string()],
            task_type: TaskType::Bug,
            parent: Some("t-aaaa"),
            description: None,
        },
        || "t-aaaa".to_string(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ChronError::IdCollisionExhausted(_)));

    // Existing record completely untouched — no phantom events at all.
    let original = repo.get_task("t-aaaa").unwrap();
    assert_eq!(original.title, "Original");
    assert!(original.blocked_by.is_empty());
    assert_eq!(original.parent, None);
    let after = repo.get_task_detail("t-aaaa").await.unwrap().timeline.len();
    assert_eq!(before, after, "no events emitted against existing record");
}
