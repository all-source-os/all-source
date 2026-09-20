use fuzzy_matcher::skim::SkimMatcherV2;

use crate::domain::{
    repository::{TaskDetail, TaskRepository},
    task::{Task, TaskStatus},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    Kanban,
    Graph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KanbanColumn {
    Open,
    InProgress,
    Done,
}

impl KanbanColumn {
    pub fn next(self) -> Self {
        match self {
            Self::Open => Self::InProgress,
            Self::InProgress => Self::Done,
            Self::Done => Self::Done,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Open => Self::Open,
            Self::InProgress => Self::Open,
            Self::Done => Self::InProgress,
        }
    }
}

pub struct App<R: TaskRepository> {
    pub repo: R,
    pub tasks: Vec<Task>,
    pub selected_index: usize,
    pub view: View,
    pub input_mode: InputMode,
    pub search_query: String,
    pub kanban_column: KanbanColumn,
    pub kanban_indices: [usize; 3], // per-column selection
    pub detail: Option<TaskDetail>,
    pub status_message: Option<String>,
    pub status_filter: Option<TaskStatus>,
    pub should_quit: bool,
    fuzzy_matcher: SkimMatcherV2,
}

impl<R: TaskRepository> App<R> {
    pub fn new(repo: R) -> Self {
        Self {
            repo,
            tasks: Vec::new(),
            selected_index: 0,
            view: View::Dashboard,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            kanban_column: KanbanColumn::Open,
            kanban_indices: [0; 3],
            detail: None,
            status_message: None,
            status_filter: None,
            should_quit: false,
            fuzzy_matcher: SkimMatcherV2::default(),
        }
    }

    pub fn refresh(&mut self) {
        match self.repo.list_tasks(None) {
            Ok(tasks) => {
                self.tasks = tasks;
                // Clamp selection to filtered list
                let count = self.filtered_tasks().len();
                if count > 0 && self.selected_index >= count {
                    self.selected_index = count - 1;
                }
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    pub fn filtered_tasks(&self) -> Vec<&Task> {
        use fuzzy_matcher::FuzzyMatcher;

        let iter: Box<dyn Iterator<Item = &Task>> = match self.status_filter {
            Some(status) => Box::new(self.tasks.iter().filter(move |t| t.status == status)),
            None => Box::new(self.tasks.iter()),
        };

        if self.search_query.is_empty() {
            return iter.collect();
        }

        let matcher = &self.fuzzy_matcher;
        let query = &self.search_query;
        let mut scored: Vec<(i64, &Task)> = iter
            .filter_map(|t| {
                let id_score = matcher.fuzzy_match(&t.id, query).unwrap_or(0);
                let title_score = matcher.fuzzy_match(&t.title, query).unwrap_or(0);
                let desc_score = t
                    .description
                    .as_deref()
                    .and_then(|d| matcher.fuzzy_match(d, query))
                    .unwrap_or(0);
                let best = id_score.max(title_score).max(desc_score);
                if best > 0 { Some((best, t)) } else { None }
            })
            .collect();
        scored.sort_by_key(|a| std::cmp::Reverse(a.0));
        scored.into_iter().map(|(_, t)| t).collect()
    }

    pub fn tasks_by_status(&self, status: TaskStatus) -> Vec<&Task> {
        self.tasks.iter().filter(|t| t.status == status).collect()
    }

    pub fn select_next(&mut self) {
        let count = self.filtered_tasks().len();
        if count > 0 {
            self.selected_index = (self.selected_index + 1).min(count - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn set_filter(&mut self, filter: Option<TaskStatus>) {
        self.status_filter = filter;
        self.selected_index = 0;
        self.detail = None;
    }

    pub fn toggle_view(&mut self) {
        self.view = match self.view {
            View::Dashboard => View::Kanban,
            View::Kanban => View::Graph,
            View::Graph => View::Dashboard,
        };
        self.detail = None;
    }

    /// Get the currently focused task (works for both views)
    pub fn focused_task_id(&self) -> Option<String> {
        match self.view {
            View::Dashboard | View::Graph => self
                .filtered_tasks()
                .get(self.selected_index)
                .map(|t| t.id.clone()),
            View::Kanban => {
                let status = match self.kanban_column {
                    KanbanColumn::Open => TaskStatus::Open,
                    KanbanColumn::InProgress => TaskStatus::InProgress,
                    KanbanColumn::Done => TaskStatus::Done,
                };
                let col_tasks = self.tasks_by_status(status);
                let idx = self.kanban_indices[self.kanban_column as usize];
                col_tasks.get(idx).map(|t| t.id.clone())
            }
        }
    }

    pub fn kanban_select_next(&mut self) {
        let status = match self.kanban_column {
            KanbanColumn::Open => TaskStatus::Open,
            KanbanColumn::InProgress => TaskStatus::InProgress,
            KanbanColumn::Done => TaskStatus::Done,
        };
        let count = self.tasks_by_status(status).len();
        let idx = &mut self.kanban_indices[self.kanban_column as usize];
        if count > 0 {
            *idx = (*idx + 1).min(count - 1);
        }
    }

    pub fn kanban_select_prev(&mut self) {
        let idx = &mut self.kanban_indices[self.kanban_column as usize];
        *idx = idx.saturating_sub(1);
    }

    pub async fn load_detail(&mut self) {
        if let Some(id) = self.focused_task_id() {
            match self.repo.get_task_detail(&id).await {
                Ok(detail) => self.detail = Some(detail),
                Err(e) => self.status_message = Some(format!("Error: {e}")),
            }
        }
    }

    pub async fn claim_focused(&mut self) {
        if let Some(id) = self.focused_task_id() {
            let agent = crate::infrastructure::agent_id();
            match self.repo.claim_task(&id, &agent).await {
                Ok(()) => {
                    self.status_message = Some(format!("Claimed {id}"));
                    self.refresh();
                }
                Err(e) => self.status_message = Some(format!("Error: {e}")),
            }
        }
    }

    pub async fn complete_focused(&mut self) {
        if let Some(id) = self.focused_task_id() {
            match self.repo.complete_task(&id, None).await {
                Ok(()) => {
                    self.status_message = Some(format!("Completed {id}"));
                    self.refresh();
                }
                Err(e) => self.status_message = Some(format!("Error: {e}")),
            }
        }
    }

    pub async fn approve_focused(&mut self) {
        if let Some(id) = self.focused_task_id() {
            match self.repo.approve_task(&id).await {
                Ok(()) => {
                    self.status_message = Some(format!("Approved {id}"));
                    self.refresh();
                }
                Err(e) => self.status_message = Some(format!("Error: {e}")),
            }
        }
    }

    pub fn copy_focused(&mut self) {
        let task = match self.focused_task_id().and_then(|id| {
            self.filtered_tasks()
                .into_iter()
                .find(|t| t.id == id)
                .cloned()
        }) {
            Some(t) => t,
            None => {
                self.status_message = Some("No task selected".into());
                return;
            }
        };

        let mut md = format!("## {} — {}\n\n", task.id, task.title);
        md.push_str(&format!("- **Type:** {}\n", task.task_type));
        md.push_str(&format!("- **Priority:** {}\n", task.priority));
        md.push_str(&format!("- **Status:** {}\n", task.status));
        if let Some(ref parent) = task.parent {
            md.push_str(&format!("- **Parent:** {parent}\n"));
        }
        if let Some(ref claimed) = task.claimed_by {
            md.push_str(&format!("- **Claimed:** {claimed}\n"));
        }
        if !task.blocked_by.is_empty() {
            md.push_str(&format!(
                "- **Blocked by:** {}\n",
                task.blocked_by.join(", ")
            ));
        }
        if let Some(ref desc) = task.description {
            md.push_str(&format!("\n{desc}\n"));
        }

        match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&md)) {
            Ok(()) => self.status_message = Some(format!("Copied {} to clipboard", task.id)),
            Err(e) => self.status_message = Some(format!("Copy failed: {e}")),
        }
    }

    pub fn export_tasks(&mut self) {
        let now = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("chronis_export_{now}.md");
        let md = crate::presentation::shared::export_markdown(&self.tasks);

        match std::fs::write(&filename, &md) {
            Ok(()) => {
                self.status_message =
                    Some(format!("Exported {} tasks to {filename}", self.tasks.len()))
            }
            Err(e) => self.status_message = Some(format!("Export failed: {e}")),
        }
    }
}
