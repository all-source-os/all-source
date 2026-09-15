use crate::domain::{
    error::ChronError, hierarchy::check_reparent, repository::TaskRepository, task::Task,
};

/// Move a task under a new parent, or to the root when `new_parent` is `None`.
///
/// Refusals are never overridable; warnings are cleared by `force`. No event is
/// emitted unless the checks pass, so a rejected re-parent leaves no trace.
pub async fn reparent_task(
    repo: &impl TaskRepository,
    universe: &[Task],
    id: &str,
    new_parent: Option<&str>,
    force: bool,
) -> Result<Vec<String>, ChronError> {
    let warnings = check_reparent(universe, id, new_parent)?;

    if !warnings.is_empty() && !force {
        return Err(ChronError::ReparentNeedsForce {
            id: id.to_string(),
            warnings,
        });
    }

    repo.reparent_task(id, new_parent).await?;
    Ok(warnings.iter().map(ToString::to_string).collect())
}
