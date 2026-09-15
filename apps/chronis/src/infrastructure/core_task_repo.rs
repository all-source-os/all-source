use std::{collections::HashSet, sync::Arc};

use allsource_core::embedded::{IngestEvent, Query};
use serde_json::json;

use super::backend::CoreBackend;
use crate::domain::{
    error::{ChronError, CoreError},
    repository::{TaskDetail, TaskEdit, TaskRepository, TimelineEntry},
    task::{Task, TaskStatus, TaskType},
};

pub struct CoreTaskRepository {
    backend: Arc<CoreBackend>,
}

impl CoreTaskRepository {
    pub fn new(backend: Arc<CoreBackend>) -> Self {
        Self { backend }
    }

    pub fn backend(&self) -> &CoreBackend {
        &self.backend
    }

    pub fn backend_arc(&self) -> &Arc<CoreBackend> {
        &self.backend
    }

    fn value_to_task(&self, value: &serde_json::Value) -> Result<Task, ChronError> {
        serde_json::from_value::<Task>(value.clone())
            .map_err(|e| CoreError(format!("failed to deserialize task: {e}")).into())
    }

    fn all_tasks_raw(&self) -> Vec<serde_json::Value> {
        let all = self
            .backend
            .projection("chronis_tasks", "__all")
            .unwrap_or_else(|| json!({ "tasks": [] }));
        all["tasks"].as_array().cloned().unwrap_or_default()
    }

    pub fn list_tasks_archived(&self) -> Result<Vec<Task>, ChronError> {
        let raw = self.all_tasks_raw();
        let filtered: Vec<_> = raw
            .into_iter()
            .filter(|t| t["archived"].as_bool().unwrap_or(false))
            .collect();
        filtered.iter().map(|v| self.value_to_task(v)).collect()
    }

    pub fn list_tasks_all(&self, status: Option<&str>) -> Result<Vec<Task>, ChronError> {
        let raw = self.all_tasks_raw();
        let filtered: Vec<_> = if let Some(status) = status {
            raw.into_iter()
                .filter(|t| t["status"].as_str().unwrap_or("") == status)
                .collect()
        } else {
            raw
        };
        filtered.iter().map(|v| self.value_to_task(v)).collect()
    }
}

impl TaskRepository for CoreTaskRepository {
    fn get_task(&self, id: &str) -> Result<Task, ChronError> {
        let value = self
            .backend
            .projection("chronis_tasks", id)
            .ok_or_else(|| ChronError::TaskNotFound(id.to_string()))?;
        self.value_to_task(&value)
    }

    fn list_tasks(&self, status: Option<&str>) -> Result<Vec<Task>, ChronError> {
        let raw = self.all_tasks_raw();
        let filtered: Vec<_> = raw
            .into_iter()
            .filter(|t| {
                if let Some(status) = status {
                    t["status"].as_str().unwrap_or("") == status
                } else {
                    true
                }
            })
            .filter(|t| !t["archived"].as_bool().unwrap_or(false))
            .collect();
        filtered.iter().map(|v| self.value_to_task(v)).collect()
    }

    fn ready_tasks(&self) -> Result<Vec<Task>, ChronError> {
        let raw = self.all_tasks_raw();

        let done_ids: HashSet<String> = raw
            .iter()
            .filter(|t| t["status"].as_str() == Some("done"))
            .filter_map(|t| t["id"].as_str().map(String::from))
            .collect();

        let ready: Vec<_> = raw
            .into_iter()
            .filter(|t| !t["archived"].as_bool().unwrap_or(false))
            .filter(|t| t["status"].as_str() == Some("open"))
            .filter(|t| {
                let blocked_by = t["blocked_by"].as_array();
                match blocked_by {
                    None => true,
                    Some(deps) => deps
                        .iter()
                        .all(|d| d.as_str().is_some_and(|id| done_ids.contains(id))),
                }
            })
            .collect();

        ready.iter().map(|v| self.value_to_task(v)).collect()
    }

    fn children_of(&self, parent_id: &str) -> Result<Vec<Task>, ChronError> {
        let raw = self.all_tasks_raw();
        raw.into_iter()
            .filter(|t| t["parent"].as_str() == Some(parent_id))
            .map(|v| self.value_to_task(&v))
            .collect()
    }

    async fn create_task(
        &self,
        id: &str,
        title: &str,
        priority: &str,
        blocked_by: &[String],
        task_type: TaskType,
        parent: Option<&str>,
        description: Option<&str>,
    ) -> Result<(), ChronError> {
        // INVARIANT (#194): a `task.created` event must never target an
        // already-projected entity_id. If it did, the projection would drop
        // the new task's title/description/type/parent (silent data loss) and
        // the follow-up `task.dependency.added` events would leak onto the
        // existing record as phantom blockers (silent corruption).
        //
        // We guard BEFORE any ingest, so a collision fails loudly and emits
        // ZERO events — the existing record is left completely untouched. We
        // do not mutate or rewrite past events; this is a read-check guard.
        if self.get_task(id).is_ok() {
            return Err(ChronError::IdAlreadyTaken(id.to_string()));
        }

        let mut payload = json!({
            "title": title,
            "priority": priority,
            "task_type": task_type.to_string(),
        });
        if let Some(parent) = parent {
            payload["parent"] = json!(parent);
        }
        if let Some(desc) = description {
            payload["description"] = json!(desc);
        }

        self.backend
            .ingest(IngestEvent {
                entity_id: id,
                event_type: "task.created",
                payload,
                metadata: None,
                tenant_id: None,
            })
            .await?;

        for dep in blocked_by {
            self.backend
                .ingest(IngestEvent {
                    entity_id: id,
                    event_type: "task.dependency.added",
                    payload: json!({ "depends_on": dep }),
                    metadata: None,
                    tenant_id: None,
                })
                .await?;
        }

        Ok(())
    }

    async fn claim_task(&self, id: &str, agent_id: &str) -> Result<(), ChronError> {
        let task = self.get_task(id)?;
        if task.status != TaskStatus::Open {
            return Err(ChronError::InvalidTransition {
                id: id.to_string(),
                current: task.status.to_string(),
                action: "claim".to_string(),
            });
        }

        self.backend
            .ingest(IngestEvent {
                entity_id: id,
                event_type: "workflow.claimed",
                payload: json!({ "agent_id": agent_id }),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    async fn release_task(
        &self,
        id: &str,
        agent_id: &str,
        reason: Option<&str>,
    ) -> Result<(), ChronError> {
        let task = self.get_task(id)?;
        if task.status != TaskStatus::InProgress {
            return Err(ChronError::NotClaimed(id.to_string()));
        }

        let mut payload = json!({ "agent_id": agent_id });
        if let Some(holder) = task.claimed_by {
            payload["released_from"] = json!(holder);
        }
        if let Some(reason) = reason {
            payload["reason"] = json!(reason);
        }

        self.backend
            .ingest(IngestEvent {
                entity_id: id,
                event_type: "workflow.released",
                payload,
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    async fn complete_task(&self, id: &str, reason: Option<&str>) -> Result<(), ChronError> {
        let task = self.get_task(id)?;
        if task.status == TaskStatus::Done {
            return Err(ChronError::AlreadyDone(id.to_string()));
        }

        let mut payload = json!({});
        if let Some(reason) = reason {
            payload["reason"] = json!(reason);
        }

        self.backend
            .ingest(IngestEvent {
                entity_id: id,
                event_type: "workflow.step.completed",
                payload,
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    async fn edit_task(&self, id: &str, edit: &TaskEdit) -> Result<(), ChronError> {
        // Editing a non-existent task is an error, not a silent create: a
        // `task.updated` against an unknown entity_id would be a no-op in the
        // projection (the field-setters all guard on get_mut), so fail loudly
        // here rather than ingest an event that changes nothing.
        let _ = self.get_task(id)?;

        let mut payload = json!({});
        if let Some(ref title) = edit.title {
            payload["title"] = json!(title);
        }
        if let Some(ref description) = edit.description {
            payload["description"] = json!(description);
        }
        if let Some(priority) = edit.priority {
            payload["priority"] = json!(priority.to_string());
        }
        if let Some(task_type) = edit.task_type {
            payload["task_type"] = json!(task_type.to_string());
        }

        self.backend
            .ingest(IngestEvent {
                entity_id: id,
                event_type: "task.updated",
                payload,
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    async fn reparent_task(&self, id: &str, new_parent: Option<&str>) -> Result<(), ChronError> {
        let _ = self.get_task(id)?;

        // `None` serializes to a present-and-null `parent`, which the
        // projection reads as "clear it" rather than "leave it alone".
        self.backend
            .ingest(IngestEvent {
                entity_id: id,
                event_type: "task.updated",
                payload: json!({ "parent": new_parent }),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    async fn approve_task(&self, id: &str) -> Result<(), ChronError> {
        let task = self.get_task(id)?;
        if task.status == TaskStatus::Done {
            return Err(ChronError::AlreadyDone(id.to_string()));
        }

        self.backend
            .ingest(IngestEvent {
                entity_id: id,
                event_type: "workflow.approval.granted",
                payload: json!({}),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    async fn add_dependency(&self, task_id: &str, blocker_id: &str) -> Result<(), ChronError> {
        self.get_task(task_id)?;
        self.get_task(blocker_id)?;

        self.backend
            .ingest(IngestEvent {
                entity_id: task_id,
                event_type: "task.dependency.added",
                payload: json!({ "depends_on": blocker_id }),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    async fn remove_dependency(&self, task_id: &str, blocker_id: &str) -> Result<(), ChronError> {
        self.get_task(task_id)?;

        self.backend
            .ingest(IngestEvent {
                entity_id: task_id,
                event_type: "task.dependency.removed",
                payload: json!({ "depends_on": blocker_id }),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    async fn archive_task(&self, id: &str) -> Result<(), ChronError> {
        self.get_task(id)?;

        self.backend
            .ingest(IngestEvent {
                entity_id: id,
                event_type: "task.archived",
                payload: json!({}),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    async fn unarchive_task(&self, id: &str) -> Result<(), ChronError> {
        self.get_task(id)?;

        self.backend
            .ingest(IngestEvent {
                entity_id: id,
                event_type: "task.unarchived",
                payload: json!({}),
                metadata: None,
                tenant_id: None,
            })
            .await?;

        Ok(())
    }

    async fn get_task_detail(&self, id: &str) -> Result<TaskDetail, ChronError> {
        let task = self.get_task(id)?;

        let events = self.backend.query(Query::new().entity_id(id)).await?;

        let timeline = events
            .iter()
            .map(|ev| TimelineEntry {
                timestamp: ev.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
                event_type: ev.event_type.clone(),
            })
            .collect();

        Ok(TaskDetail { task, timeline })
    }
}
