use crate::domain::{
    error::ChronError,
    repository::TaskRepository,
    task::{Task, TaskStatus},
};

pub async fn claim_task(
    repo: &impl TaskRepository,
    id: &str,
    agent_id: &str,
) -> Result<(), ChronError> {
    repo.claim_task(id, agent_id).await
}

/// A cascade member that will not be claimed, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedClaim {
    pub id: String,
    pub status: TaskStatus,
    pub held_by: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaimPlan {
    pub to_claim: Vec<String>,
    pub skipped: Vec<SkippedClaim>,
}

/// Decide what a claim touches.
///
/// `target` is the id the caller named; the rest of `tasks` are its cascade
/// descendants. A descendant that is already in progress or done is skipped,
/// because claiming an epic must not fail on one busy child. The TARGET is
/// different: passing over the task the caller named, and saying nothing,
/// reports success for work that was never claimed — a stale claim then looks
/// like a live one.
pub fn plan_claim(target: &str, tasks: &[Task]) -> Result<ClaimPlan, ChronError> {
    let mut plan = ClaimPlan::default();

    for task in tasks {
        if task.status == TaskStatus::Open {
            plan.to_claim.push(task.id.clone());
            continue;
        }

        if task.id != target {
            plan.skipped.push(SkippedClaim {
                id: task.id.clone(),
                status: task.status,
                held_by: task.claimed_by.clone(),
            });
            continue;
        }

        return Err(match task.status {
            TaskStatus::InProgress => ChronError::AlreadyClaimed {
                id: task.id.clone(),
                holder: task
                    .claimed_by
                    .clone()
                    .unwrap_or_else(|| "another session".to_string()),
            },
            status => ChronError::InvalidTransition {
                id: task.id.clone(),
                current: status.to_string(),
                action: "claim".to_string(),
            },
        });
    }

    Ok(plan)
}
