use serde::{Deserialize, Serialize};

use super::{
    error::ChronError,
    task::{Priority, Task, TaskType},
};

/// Fields to overwrite on an existing task via `cn task edit`. A `None` field
/// is left unchanged. Emitted as a single `task.updated` event carrying only
/// the present fields; the projection applies last-write-wins per field.
#[derive(Debug, Default, Clone)]
pub struct TaskEdit {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<Priority>,
    pub task_type: Option<TaskType>,
}

impl TaskEdit {
    /// True when no field is set — editing with nothing to change is rejected
    /// (`ChronError::NothingToEdit`) so we never emit an empty event.
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.priority.is_none()
            && self.task_type.is_none()
    }
}

/// Timeline entry for a task event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub timestamp: String,
    pub event_type: String,
}

/// Task with its event timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetail {
    pub task: Task,
    pub timeline: Vec<TimelineEntry>,
}

pub trait TaskRepository: Send + Sync {
    fn get_task(&self, id: &str) -> Result<Task, ChronError>;
    fn list_tasks(&self, status: Option<&str>) -> Result<Vec<Task>, ChronError>;
    fn ready_tasks(&self) -> Result<Vec<Task>, ChronError>;

    #[allow(clippy::too_many_arguments)]
    fn create_task(
        &self,
        id: &str,
        title: &str,
        priority: &str,
        blocked_by: &[String],
        task_type: TaskType,
        parent: Option<&str>,
        description: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), ChronError>> + Send;

    fn children_of(&self, parent_id: &str) -> Result<Vec<Task>, ChronError>;

    fn claim_task(
        &self,
        id: &str,
        agent_id: &str,
    ) -> impl std::future::Future<Output = Result<(), ChronError>> + Send;

    /// Hand a claimed task back to the pool. `agent_id` is the releasing
    /// actor, recorded for provenance; it need not match the claim holder,
    /// because the holder is usually a session that no longer exists.
    fn release_task(
        &self,
        id: &str,
        agent_id: &str,
        reason: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), ChronError>> + Send;

    fn complete_task(
        &self,
        id: &str,
        reason: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), ChronError>> + Send;

    fn edit_task(
        &self,
        id: &str,
        edit: &TaskEdit,
    ) -> impl std::future::Future<Output = Result<(), ChronError>> + Send;

    /// Move `id` under `new_parent`, or to the root when `new_parent` is
    /// `None`. Assumes the caller has already cleared
    /// [`crate::domain::hierarchy::check_reparent`].
    fn reparent_task(
        &self,
        id: &str,
        new_parent: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), ChronError>> + Send;

    fn approve_task(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<(), ChronError>> + Send;

    fn add_dependency(
        &self,
        task_id: &str,
        blocker_id: &str,
    ) -> impl std::future::Future<Output = Result<(), ChronError>> + Send;

    fn remove_dependency(
        &self,
        task_id: &str,
        blocker_id: &str,
    ) -> impl std::future::Future<Output = Result<(), ChronError>> + Send;

    fn get_task_detail(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<TaskDetail, ChronError>> + Send;

    fn archive_task(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<(), ChronError>> + Send;

    fn unarchive_task(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<(), ChronError>> + Send;
}
