//! First-class, composable task filters for `cn list` (issue #195).
//!
//! All predicates AND together. An empty multi-value filter (e.g. no
//! `--status`) means "any". The filter runs over the *full* task universe —
//! not a pre-filtered slice — because `--parent`/`--depth` ancestry and
//! `--no-blockers` resolution need to see ancestor/blocker tasks that may not
//! match the filter themselves.

use std::collections::HashSet;

use crate::domain::task::{Priority, Task, TaskStatus, TaskType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClaimState {
    #[default]
    Any,
    Claimed,
    Unclaimed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArchivedScope {
    /// Exclude archived tasks (the default listing).
    #[default]
    Active,
    /// Only archived tasks.
    Only,
    /// Include archived tasks alongside active ones.
    All,
}

#[derive(Debug, Default, Clone)]
pub struct TaskFilter {
    pub statuses: Vec<TaskStatus>,
    pub priorities: Vec<Priority>,
    pub types: Vec<TaskType>,
    pub claimed_by: Option<String>,
    pub claim_state: ClaimState,
    /// Restrict to tasks under this parent. `recursive` controls whether that
    /// means direct children only or the whole subtree. The parent itself is
    /// never included — `--parent X` reads as "tasks under X".
    pub parent: Option<String>,
    pub recursive: bool,
    pub blocked_by: Option<String>,
    pub no_blockers: bool,
    pub archived: ArchivedScope,
}

/// Apply the filter to the full task universe and return the matches in the
/// universe's original order.
pub fn apply(universe: &[Task], filter: &TaskFilter) -> Vec<Task> {
    let done_ids: HashSet<&str> = universe
        .iter()
        .filter(|t| t.status == TaskStatus::Done)
        .map(|t| t.id.as_str())
        .collect();

    let parent_set: Option<HashSet<String>> = filter.parent.as_ref().map(|root| {
        if filter.recursive {
            descendants(universe, root)
        } else {
            universe
                .iter()
                .filter(|t| t.parent.as_deref() == Some(root.as_str()))
                .map(|t| t.id.clone())
                .collect()
        }
    });

    universe
        .iter()
        .filter(|t| match filter.archived {
            ArchivedScope::Active => !t.archived,
            ArchivedScope::Only => t.archived,
            ArchivedScope::All => true,
        })
        .filter(|t| filter.statuses.is_empty() || filter.statuses.contains(&t.status))
        .filter(|t| filter.priorities.is_empty() || filter.priorities.contains(&t.priority))
        .filter(|t| filter.types.is_empty() || filter.types.contains(&t.task_type))
        .filter(|t| match &filter.claimed_by {
            Some(who) => t.claimed_by.as_deref() == Some(who.as_str()),
            None => true,
        })
        .filter(|t| match filter.claim_state {
            ClaimState::Any => true,
            ClaimState::Claimed => t.claimed_by.is_some(),
            ClaimState::Unclaimed => t.claimed_by.is_none(),
        })
        .filter(|t| match &parent_set {
            Some(set) => set.contains(&t.id),
            None => true,
        })
        .filter(|t| match &filter.blocked_by {
            Some(b) => t.blocked_by.iter().any(|x| x == b),
            None => true,
        })
        .filter(|t| {
            !filter.no_blockers || t.blocked_by.iter().all(|b| done_ids.contains(b.as_str()))
        })
        .cloned()
        .collect()
}

/// All transitive descendants of `root` (children, grandchildren, …),
/// excluding `root` itself. Cycle-safe via the visited set.
use crate::domain::hierarchy::descendants;

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, parent: Option<&str>) -> Task {
        Task {
            id: id.into(),
            title: id.into(),
            priority: Priority::P2,
            status: TaskStatus::Open,
            task_type: TaskType::Task,
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

    fn universe() -> Vec<Task> {
        let mut epic = task("t-epic", None);
        epic.task_type = TaskType::Epic;
        let mut a = task("t-a", Some("t-epic"));
        a.priority = Priority::P0;
        a.status = TaskStatus::Done;
        let mut b = task("t-b", Some("t-epic"));
        b.priority = Priority::P1;
        b.claimed_by = Some("claude:abc".into());
        b.blocked_by = vec!["t-a".into()];
        let mut grandchild = task("t-c", Some("t-b"));
        grandchild.task_type = TaskType::Bug;
        let mut archived = task("t-z", None);
        archived.archived = true;
        vec![epic, a, b, grandchild, archived]
    }

    fn ids(tasks: &[Task]) -> Vec<String> {
        let mut v: Vec<String> = tasks.iter().map(|t| t.id.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn empty_filter_returns_active_only() {
        let out = apply(&universe(), &TaskFilter::default());
        // t-z is archived → excluded by default
        assert_eq!(ids(&out), vec!["t-a", "t-b", "t-c", "t-epic"]);
    }

    #[test]
    fn status_and_priority_are_anded() {
        let f = TaskFilter {
            statuses: vec![TaskStatus::Done],
            priorities: vec![Priority::P0],
            ..Default::default()
        };
        assert_eq!(ids(&apply(&universe(), &f)), vec!["t-a"]);
    }

    #[test]
    fn type_filter_ors_within_field() {
        let f = TaskFilter {
            types: vec![TaskType::Epic, TaskType::Bug],
            ..Default::default()
        };
        assert_eq!(ids(&apply(&universe(), &f)), vec!["t-c", "t-epic"]);
    }

    #[test]
    fn parent_direct_vs_recursive() {
        let direct = TaskFilter {
            parent: Some("t-epic".into()),
            recursive: false,
            ..Default::default()
        };
        assert_eq!(ids(&apply(&universe(), &direct)), vec!["t-a", "t-b"]);

        let recursive = TaskFilter {
            parent: Some("t-epic".into()),
            recursive: true,
            ..Default::default()
        };
        // grandchild t-c included; epic itself excluded
        assert_eq!(
            ids(&apply(&universe(), &recursive)),
            vec!["t-a", "t-b", "t-c"]
        );
    }

    #[test]
    fn claimed_filters() {
        let claimed = TaskFilter {
            claim_state: ClaimState::Claimed,
            ..Default::default()
        };
        assert_eq!(ids(&apply(&universe(), &claimed)), vec!["t-b"]);

        let by = TaskFilter {
            claimed_by: Some("claude:abc".into()),
            ..Default::default()
        };
        assert_eq!(ids(&apply(&universe(), &by)), vec!["t-b"]);
    }

    #[test]
    fn no_blockers_excludes_tasks_with_open_blockers() {
        // t-b is blocked by t-a; t-a is done, so t-b counts as unblocked.
        let f = TaskFilter {
            no_blockers: true,
            ..Default::default()
        };
        assert!(ids(&apply(&universe(), &f)).contains(&"t-b".to_string()));

        // Now make the blocker open → t-b should drop out.
        let mut u = universe();
        u[1].status = TaskStatus::Open; // t-a
        let out = apply(&u, &f);
        assert!(!ids(&out).contains(&"t-b".to_string()));
    }

    #[test]
    fn archived_scopes() {
        let only = TaskFilter {
            archived: ArchivedScope::Only,
            ..Default::default()
        };
        assert_eq!(ids(&apply(&universe(), &only)), vec!["t-z"]);

        let all = TaskFilter {
            archived: ArchivedScope::All,
            ..Default::default()
        };
        assert_eq!(apply(&universe(), &all).len(), 5);
    }

    #[test]
    fn blocked_by_filter() {
        let f = TaskFilter {
            blocked_by: Some("t-a".into()),
            ..Default::default()
        };
        assert_eq!(ids(&apply(&universe(), &f)), vec!["t-b"]);
    }
}
