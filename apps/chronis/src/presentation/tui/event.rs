use super::app::{App, InputMode, View};
use crate::domain::{repository::TaskRepository, task::TaskStatus};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub enum AppEvent {
    /// Fallback timer tick — fires if the backend subscription is quiet.
    Tick,
    /// Live change notification from `CoreBackend::subscribe()`.
    BackendChanged,
    Key(KeyEvent),
}

pub async fn handle_event<R: TaskRepository>(app: &mut App<R>, evt: AppEvent) {
    match evt {
        AppEvent::Tick | AppEvent::BackendChanged => {
            app.refresh();
        }
        AppEvent::Key(key) => {
            // Ctrl+C always quits
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                app.should_quit = true;
                return;
            }

            match app.input_mode {
                InputMode::Search => handle_search_input(app, key).await,
                InputMode::Normal => handle_normal_input(app, key).await,
            }
        }
    }
}

async fn handle_search_input<R: TaskRepository>(app: &mut App<R>, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.search_query.clear();
            app.selected_index = 0;
        }
        KeyCode::Enter => {
            app.input_mode = InputMode::Normal;
            // Keep search results visible, load detail of selected
            app.load_detail().await;
        }
        KeyCode::Backspace => {
            app.search_query.pop();
            app.selected_index = 0;
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
            app.selected_index = 0;
        }
        KeyCode::Down => app.select_next(),
        KeyCode::Up => app.select_prev(),
        _ => {}
    }
}

async fn handle_normal_input<R: TaskRepository>(app: &mut App<R>, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Search;
            app.search_query.clear();
            app.selected_index = 0;
        }
        KeyCode::Tab => app.toggle_view(),
        KeyCode::Char('c') => app.claim_focused().await,
        KeyCode::Char('C') => app.copy_focused(),
        KeyCode::Char('E') => app.export_tasks(),
        KeyCode::Char('d') => app.complete_focused().await,
        KeyCode::Char('a') => app.approve_focused().await,
        KeyCode::Enter => app.load_detail().await,

        // Navigation depends on view
        KeyCode::Char('j') | KeyCode::Down => match app.view {
            View::Dashboard | View::Graph => app.select_next(),
            View::Kanban => app.kanban_select_next(),
        },
        KeyCode::Char('k') | KeyCode::Up => match app.view {
            View::Dashboard | View::Graph => app.select_prev(),
            View::Kanban => app.kanban_select_prev(),
        },
        KeyCode::Char('h') | KeyCode::Left if app.view == View::Kanban => {
            app.kanban_column = app.kanban_column.prev();
        }
        KeyCode::Char('l') | KeyCode::Right if app.view == View::Kanban => {
            app.kanban_column = app.kanban_column.next();
        }
        KeyCode::Char('r') => {
            app.refresh();
            app.status_message = Some("Refreshed".to_string());
        }
        // Status filters (dashboard only)
        KeyCode::Char('1') => app.set_filter(Some(TaskStatus::Open)),
        KeyCode::Char('2') => app.set_filter(Some(TaskStatus::InProgress)),
        KeyCode::Char('3') => app.set_filter(Some(TaskStatus::Done)),
        KeyCode::Char('0') => app.set_filter(None),
        _ => {}
    }
}
