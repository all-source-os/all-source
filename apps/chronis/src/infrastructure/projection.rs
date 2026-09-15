use allsource_core::{
    application::services::projection::Projection, domain::entities::Event, error::Result,
};
use dashmap::DashMap;
use serde_json::{Value, json};

/// Projection that aggregates task lifecycle events into queryable task state.
///
/// Handles: task.created, task.updated, task.dependency.added, task.dependency.removed,
/// workflow.claimed (first-write-wins), workflow.released, workflow.step.completed,
/// workflow.approval.requested, workflow.approval.granted
pub struct TaskProjection {
    states: DashMap<String, Value>,
}

impl Default for TaskProjection {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskProjection {
    pub fn new() -> Self {
        Self {
            states: DashMap::new(),
        }
    }
}

impl Projection for TaskProjection {
    fn name(&self) -> &str {
        "chronis_tasks"
    }

    fn process(&self, event: &Event) -> Result<()> {
        let entity_id = event.entity_id_str().to_string();
        let event_type = event.event_type_str();
        let payload = &event.payload;

        match event_type {
            "task.created" => {
                // Only insert if the task doesn't already exist.
                // Duplicate task.created events arrive via sync — they must not
                // overwrite state that was built from later events (e.g. archived,
                // done, claimed). See issue #125.
                if self.states.contains_key(&entity_id) {
                    return Ok(());
                }

                let title = payload
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("untitled")
                    .to_string();
                let priority = payload
                    .get("priority")
                    .and_then(|v| v.as_str())
                    .unwrap_or("p2")
                    .to_string();
                let task_type = payload
                    .get("task_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("task")
                    .to_string();
                let parent = payload
                    .get("parent")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let description = payload
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let mut state = json!({
                    "id": entity_id,
                    "title": title,
                    "priority": priority,
                    "status": "open",
                    "task_type": task_type,
                    "claimed_by": null,
                    "blocked_by": [],
                    "created_at": event.timestamp().to_rfc3339(),
                });
                if let Some(p) = parent {
                    state["parent"] = json!(p);
                }
                if let Some(d) = description {
                    state["description"] = json!(d);
                }
                self.states.insert(entity_id, state);
            }
            "task.updated" => {
                // Last-write-wins per field; only the fields present in the
                // payload change (see `cn task edit` / TaskEdit).
                if let Some(mut state) = self.states.get_mut(&entity_id) {
                    if let Some(title) = payload.get("title") {
                        state["title"] = title.clone();
                    }
                    if let Some(priority) = payload.get("priority") {
                        state["priority"] = priority.clone();
                    }
                    if let Some(description) = payload.get("description") {
                        state["description"] = description.clone();
                    }
                    if let Some(task_type) = payload.get("task_type") {
                        state["task_type"] = task_type.clone();
                    }
                    // Present-and-null clears the parent; absent leaves it
                    // alone. `get` distinguishes the two, `as_str` would not.
                    if let Some(parent) = payload.get("parent") {
                        state["parent"] = parent.clone();
                    }
                }
            }
            "task.dependency.added" => {
                if let Some(mut state) = self.states.get_mut(&entity_id) {
                    let dep_id = payload.get("depends_on").cloned().unwrap_or(json!(null));
                    if let Some(arr) = state["blocked_by"].as_array_mut()
                        && !arr.contains(&dep_id)
                    {
                        arr.push(dep_id);
                    }
                }
            }
            "task.dependency.removed" => {
                if let Some(mut state) = self.states.get_mut(&entity_id)
                    && let Some(dep_id) = payload.get("depends_on")
                    && let Some(arr) = state["blocked_by"].as_array_mut()
                {
                    arr.retain(|v| v != dep_id);
                }
            }
            "workflow.claimed" => {
                if let Some(mut state) = self.states.get_mut(&entity_id) {
                    // First-write-wins: only accept claim if still open
                    let status = state.get("status").and_then(|s| s.as_str()).unwrap_or("");
                    if status == "open" {
                        state["status"] = json!("in-progress");
                        if let Some(agent) = payload.get("agent_id") {
                            state["claimed_by"] = agent.clone();
                        }
                    }
                }
            }
            "workflow.released" => {
                if let Some(mut state) = self.states.get_mut(&entity_id) {
                    // Only an in-progress task goes back to the pool; a
                    // release replayed after `done` must not reopen it.
                    let status = state.get("status").and_then(|s| s.as_str()).unwrap_or("");
                    if status == "in-progress" {
                        state["status"] = json!("open");
                        state["claimed_by"] = json!(null);
                    }
                }
            }
            "workflow.step.completed" => {
                if let Some(mut state) = self.states.get_mut(&entity_id) {
                    state["status"] = json!("done");
                    if let Some(reason) = payload.get("reason") {
                        state["done_reason"] = reason.clone();
                    }
                    state["done_at"] = json!(event.timestamp().to_rfc3339());
                }
            }
            "workflow.approval.requested" => {
                if let Some(mut state) = self.states.get_mut(&entity_id) {
                    state["awaiting_approval"] = json!(true);
                }
            }
            "workflow.approval.granted" => {
                if let Some(mut state) = self.states.get_mut(&entity_id) {
                    state["awaiting_approval"] = json!(false);
                    state["approved"] = json!(true);
                    state["approved_at"] = json!(event.timestamp().to_rfc3339());
                }
            }
            "task.archived" => {
                if let Some(mut state) = self.states.get_mut(&entity_id) {
                    state["archived"] = json!(true);
                }
            }
            "task.unarchived" => {
                if let Some(mut state) = self.states.get_mut(&entity_id) {
                    state["archived"] = json!(false);
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn get_state(&self, entity_id: &str) -> Option<Value> {
        if entity_id == "__all" {
            let tasks: Vec<Value> = self
                .states
                .iter()
                .map(|entry| {
                    let mut task = entry.value().clone();
                    task["id"] = json!(entry.key().clone());
                    task
                })
                .collect();
            Some(json!({ "tasks": tasks }))
        } else {
            self.states.get(entity_id).map(|v| {
                let mut task = v.clone();
                task["id"] = json!(entity_id);
                task
            })
        }
    }

    fn clear(&self) {
        self.states.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use allsource_core::domain::entities::Event;
    use uuid::Uuid;

    fn make_event(entity_id: &str, event_type: &str, payload: Value) -> Event {
        Event::reconstruct_from_strings(
            Uuid::new_v4(),
            event_type.to_string(),
            entity_id.to_string(),
            "default".to_string(),
            payload,
            chrono::Utc::now(),
            None,
            1,
        )
    }

    #[test]
    fn test_archive_survives_duplicate_created_event() {
        // Regression test for #125: cn sync pulls task.created events from remote
        // that overwrite the local archived state.
        let proj = TaskProjection::new();

        // 1. Task is created locally
        proj.process(&make_event(
            "t-abc",
            "task.created",
            json!({"title": "Test Task", "priority": "p1", "task_type": "task"}),
        ))
        .unwrap();

        // 2. Task is archived locally
        proj.process(&make_event("t-abc", "task.archived", json!({})))
            .unwrap();

        // Verify archived
        let state = proj.get_state("t-abc").unwrap();
        assert_eq!(state["archived"], json!(true));

        // 3. Sync pulls the SAME task.created event from remote (duplicate)
        proj.process(&make_event(
            "t-abc",
            "task.created",
            json!({"title": "Test Task", "priority": "p1", "task_type": "task"}),
        ))
        .unwrap();

        // MUST still be archived — the duplicate task.created must not wipe archive state
        let state = proj.get_state("t-abc").unwrap();
        assert_eq!(
            state["archived"],
            json!(true),
            "task.created must not overwrite archived state (issue #125)"
        );
    }

    #[test]
    fn test_archive_survives_out_of_order_events() {
        // Simulate sync pulling events in non-chronological order:
        // local: created → claimed → done → archived
        // sync pulls: created (again), claimed (again)
        let proj = TaskProjection::new();

        proj.process(&make_event(
            "t-xyz",
            "task.created",
            json!({"title": "Build Feature", "priority": "p0", "task_type": "task"}),
        ))
        .unwrap();
        proj.process(&make_event(
            "t-xyz",
            "workflow.claimed",
            json!({"agent_id": "agent-1"}),
        ))
        .unwrap();
        proj.process(&make_event(
            "t-xyz",
            "workflow.step.completed",
            json!({"reason": "done"}),
        ))
        .unwrap();
        proj.process(&make_event("t-xyz", "task.archived", json!({})))
            .unwrap();

        // Verify: done + archived
        let state = proj.get_state("t-xyz").unwrap();
        assert_eq!(state["status"], json!("done"));
        assert_eq!(state["archived"], json!(true));

        // Sync pulls duplicate created + claimed events
        proj.process(&make_event(
            "t-xyz",
            "task.created",
            json!({"title": "Build Feature", "priority": "p0", "task_type": "task"}),
        ))
        .unwrap();
        proj.process(&make_event(
            "t-xyz",
            "workflow.claimed",
            json!({"agent_id": "agent-1"}),
        ))
        .unwrap();

        // MUST preserve done + archived status
        let state = proj.get_state("t-xyz").unwrap();
        assert_eq!(
            state["archived"],
            json!(true),
            "sync replay must not un-archive"
        );
        assert_eq!(
            state["status"],
            json!("done"),
            "sync replay must not revert done status"
        );
    }

    #[test]
    fn test_first_created_event_works_normally() {
        // Ensure the fix doesn't break normal task creation
        let proj = TaskProjection::new();

        proj.process(&make_event(
            "t-new",
            "task.created",
            json!({"title": "New Task", "priority": "p2", "task_type": "epic"}),
        ))
        .unwrap();

        let state = proj.get_state("t-new").unwrap();
        assert_eq!(state["title"], json!("New Task"));
        assert_eq!(state["priority"], json!("p2"));
        assert_eq!(state["task_type"], json!("epic"));
        assert_eq!(state["status"], json!("open"));
        assert_eq!(state["archived"], json!(null)); // not archived
    }
}
