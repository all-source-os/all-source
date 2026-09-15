use crate::domain::{error::ChronError, repository::TaskRepository};

pub async fn release_task(
    repo: &impl TaskRepository,
    id: &str,
    agent_id: &str,
    reason: Option<&str>,
) -> Result<(), ChronError> {
    repo.release_task(id, agent_id, reason).await
}
