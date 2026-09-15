//! Parent/child hierarchy rules.
//!
//! Two invariants hold the task graph together, and re-parenting is the only
//! operation that can break either one:
//!
//! 1. **The parent graph is acyclic.** A cycle makes any recursive walk
//!    non-terminating. `descendants` here is cycle-safe by construction, but
//!    callers that write their own walk are not, so a cycle must never be
//!    written in the first place.
//! 2. **Every task is reachable from a tree root.** `TaskTree::build` nests a
//!    child only under an *epic* and treats a non-epic with no parent as
//!    standalone. A task parented to a non-epic is therefore in neither
//!    bucket: it renders nowhere.
//!
//! A refusal protects invariant 1 and cannot be overridden. A warning names a
//! way invariant 2 degrades and is overridable, because "this task is only
//! visible via `cn show`" is a real thing to want and not corruption.

use std::collections::{HashMap, HashSet};

use super::task::{Task, TaskStatus, TaskType};

/// Every task reachable downward from `root`, excluding `root` itself.
///
/// Terminates on a cyclic graph: an id already in the output set is never
/// pushed back onto the stack.
pub fn descendants(universe: &[Task], root: &str) -> HashSet<String> {
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for t in universe {
        if let Some(p) = t.parent.as_deref() {
            children.entry(p).or_default().push(t.id.as_str());
        }
    }
    let mut out = HashSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(kids) = children.get(id) {
            for &k in kids {
                if out.insert(k.to_string()) {
                    stack.push(k);
                }
            }
        }
    }
    out
}

/// A re-parent that would break the acyclic invariant, or that names a task
/// which does not exist. Never overridable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReparentRefusal {
    /// The task does not exist.
    #[error("task {0} not found")]
    TaskNotFound(String),
    /// The proposed parent does not exist.
    #[error("parent {0} not found")]
    ParentNotFound(String),
    /// A task cannot be its own parent.
    #[error("task {0} cannot be its own parent")]
    SelfParent(String),
    /// The proposed parent sits below the task, so the move would close a loop.
    #[error("{new_parent} is below {id}, so re-parenting {id} under it would make a cycle")]
    Cycle { id: String, new_parent: String },
}

/// A re-parent that is legal but costs visibility or reads as a mistake.
/// Overridable with `--force`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReparentWarning {
    /// The tree and graph views nest children only under epics, so this task
    /// will render in neither the epic groups nor the standalone list.
    ParentNotEpic { new_parent: String, kind: TaskType },
    /// Epic under epic: the tree renders one level of nesting, so the moved
    /// epic's own children lose their grouping.
    NestedEpic { id: String, children: usize },
    /// Re-parenting something already finished is usually a mis-typed id.
    TaskDone(String),
}

impl std::fmt::Display for ReparentWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParentNotEpic { new_parent, kind } => write!(
                f,
                "{new_parent} is a {kind}, not an epic — the tree and graph views \
                 nest children only under epics, so this task will not appear in either"
            ),
            Self::NestedEpic { id, children } => write!(
                f,
                "{id} is an epic with {children} children — nesting an epic under \
                 another epic renders one level, so those children lose their grouping"
            ),
            Self::TaskDone(id) => write!(f, "{id} is already done"),
        }
    }
}

/// Check a proposed re-parent. `new_parent` is `None` for a detach to the root.
///
/// Returns the warnings the caller must clear with `--force`, or the refusal
/// that no flag overrides. Emits no events and touches no state.
pub fn check_reparent(
    universe: &[Task],
    id: &str,
    new_parent: Option<&str>,
) -> Result<Vec<ReparentWarning>, ReparentRefusal> {
    let task = universe
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| ReparentRefusal::TaskNotFound(id.to_string()))?;

    let mut warnings = Vec::new();

    if let Some(parent_id) = new_parent {
        if parent_id == id {
            return Err(ReparentRefusal::SelfParent(id.to_string()));
        }
        let parent = universe
            .iter()
            .find(|t| t.id == parent_id)
            .ok_or_else(|| ReparentRefusal::ParentNotFound(parent_id.to_string()))?;

        if descendants(universe, id).contains(parent_id) {
            return Err(ReparentRefusal::Cycle {
                id: id.to_string(),
                new_parent: parent_id.to_string(),
            });
        }

        if parent.task_type != TaskType::Epic {
            warnings.push(ReparentWarning::ParentNotEpic {
                new_parent: parent_id.to_string(),
                kind: parent.task_type,
            });
        }

        if task.task_type == TaskType::Epic {
            let children = universe
                .iter()
                .filter(|t| t.parent.as_deref() == Some(id))
                .count();
            if children > 0 {
                warnings.push(ReparentWarning::NestedEpic {
                    id: id.to_string(),
                    children,
                });
            }
        }
    }

    if task.status == TaskStatus::Done {
        warnings.push(ReparentWarning::TaskDone(id.to_string()));
    }

    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::task::Priority;

    fn task(id: &str, kind: TaskType, parent: Option<&str>) -> Task {
        Task {
            id: id.into(),
            title: id.into(),
            priority: Priority::P2,
            status: TaskStatus::Open,
            task_type: kind,
            parent: parent.map(String::from),
            claimed_by: None,
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

    fn epic(id: &str, parent: Option<&str>) -> Task {
        task(id, TaskType::Epic, parent)
    }

    #[test]
    fn moving_under_an_epic_is_clean() {
        let universe = vec![epic("e1", None), task("t1", TaskType::Task, None)];
        assert_eq!(check_reparent(&universe, "t1", Some("e1")), Ok(vec![]));
    }

    #[test]
    fn detaching_to_the_root_is_clean() {
        let universe = vec![epic("e1", None), task("t1", TaskType::Task, Some("e1"))];
        assert_eq!(check_reparent(&universe, "t1", None), Ok(vec![]));
    }

    #[test]
    fn self_parent_is_refused() {
        let universe = vec![task("t1", TaskType::Task, None)];
        assert_eq!(
            check_reparent(&universe, "t1", Some("t1")),
            Err(ReparentRefusal::SelfParent("t1".into()))
        );
    }

    #[test]
    fn parenting_under_own_descendant_is_refused() {
        // e1 -> t1 -> t2; moving e1 under t2 would close the loop.
        let universe = vec![
            epic("e1", None),
            task("t1", TaskType::Task, Some("e1")),
            task("t2", TaskType::Task, Some("t1")),
        ];
        assert_eq!(
            check_reparent(&universe, "e1", Some("t2")),
            Err(ReparentRefusal::Cycle {
                id: "e1".into(),
                new_parent: "t2".into()
            })
        );
    }

    #[test]
    fn a_missing_task_or_parent_is_refused() {
        let universe = vec![task("t1", TaskType::Task, None)];
        assert_eq!(
            check_reparent(&universe, "nope", Some("t1")),
            Err(ReparentRefusal::TaskNotFound("nope".into()))
        );
        assert_eq!(
            check_reparent(&universe, "t1", Some("nope")),
            Err(ReparentRefusal::ParentNotFound("nope".into()))
        );
    }

    #[test]
    fn a_non_epic_parent_warns_about_invisibility() {
        let universe = vec![
            task("t1", TaskType::Task, None),
            task("t2", TaskType::Task, None),
        ];
        assert_eq!(
            check_reparent(&universe, "t1", Some("t2")),
            Ok(vec![ReparentWarning::ParentNotEpic {
                new_parent: "t2".into(),
                kind: TaskType::Task
            }])
        );
    }

    #[test]
    fn an_epic_with_children_warns_about_nesting() {
        let universe = vec![
            epic("e1", None),
            epic("e2", None),
            task("t1", TaskType::Task, Some("e2")),
        ];
        assert_eq!(
            check_reparent(&universe, "e2", Some("e1")),
            Ok(vec![ReparentWarning::NestedEpic {
                id: "e2".into(),
                children: 1
            }])
        );
    }

    #[test]
    fn a_childless_epic_nests_without_warning() {
        let universe = vec![epic("e1", None), epic("e2", None)];
        assert_eq!(check_reparent(&universe, "e2", Some("e1")), Ok(vec![]));
    }

    #[test]
    fn a_done_task_warns_even_when_detaching() {
        let mut done = task("t1", TaskType::Task, Some("e1"));
        done.status = TaskStatus::Done;
        let universe = vec![epic("e1", None), done];
        assert_eq!(
            check_reparent(&universe, "t1", None),
            Ok(vec![ReparentWarning::TaskDone("t1".into())])
        );
    }

    #[test]
    fn descendants_terminates_on_a_cycle() {
        // Already-cyclic data must not hang the check that exists to prevent it.
        let universe = vec![
            task("a", TaskType::Task, Some("b")),
            task("b", TaskType::Task, Some("a")),
        ];
        let found = descendants(&universe, "a");
        assert_eq!(found, HashSet::from(["a".to_string(), "b".to_string()]));
    }
}
