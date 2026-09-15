use crate::domain::task::{Priority, Task, TaskStatus, TaskType};

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn type_badge(task_type: TaskType) -> &'static str {
    match task_type {
        TaskType::Task => r#"<span class="type-badge type-task">task</span>"#,
        TaskType::Epic => r#"<span class="type-badge type-epic">epic</span>"#,
        TaskType::Bug => r#"<span class="type-badge type-bug">bug</span>"#,
        TaskType::Feature => r#"<span class="type-badge type-feature">feat</span>"#,
    }
}

pub fn pri_badge(task: &Task) -> String {
    let class = match task.priority {
        Priority::P0 => "pri-p0",
        Priority::P1 => "pri-p1",
        Priority::P2 => "pri-p2",
        Priority::P3 => "pri-p3",
    };
    format!(
        r#"<span class="pri-badge {class}">{}</span>"#,
        task.priority
    )
}

pub fn task_row_html(task: &Task) -> String {
    let status_class = crate::presentation::shared::status_css_class(task.status);
    let claimed = task.claimed_by.as_deref().unwrap_or("-");
    let blocked = if task.blocked_by.is_empty() {
        String::from("-")
    } else {
        task.blocked_by.join(", ")
    };
    let id = &task.id;
    let title = html_escape(&task.title);
    let status = task.status.to_string();

    // data-* attributes give the client-side column sort (index.html) stable,
    // machine-readable keys so it sorts by semantic value rather than scraping
    // badge text out of the rendered cells. data-blocked is "1" when the task
    // has any blocker, "0" otherwise — the cell itself shows the blocker ids.
    let blocked_flag = if task.blocked_by.is_empty() { "0" } else { "1" };
    let mut s = String::new();
    s.push_str("<tr class=\"task-row\" data-status=\"");
    s.push_str(&status);
    s.push_str("\" data-pri=\"");
    s.push_str(&task.priority.to_string());
    s.push_str("\" data-type=\"");
    s.push_str(&task.task_type.to_string());
    s.push_str("\" data-blocked=\"");
    s.push_str(blocked_flag);
    s.push_str("\" hx-get=\"/partials/task-detail/");
    s.push_str(id);
    s.push_str("\" hx-target=\"#detail-pane\" hx-swap=\"innerHTML\">\n");
    s.push_str("  <td>");
    s.push_str(id);
    s.push_str("</td>\n  <td>");
    s.push_str(type_badge(task.task_type));
    s.push_str("</td>\n  <td>");
    s.push_str(&title);
    s.push_str("</td>\n  <td>");
    s.push_str(&pri_badge(task));
    s.push_str("</td>\n  <td><span class=\"");
    s.push_str(status_class);
    s.push_str("\">");
    s.push_str(&status);
    s.push_str("</span></td>\n  <td>");
    s.push_str(&html_escape(claimed));
    s.push_str("</td>\n  <td>");
    s.push_str(&html_escape(&blocked));
    s.push_str("</td>\n  <td class=\"action-group\">\n");
    s.push_str("    <button class=\"btn btn-sm btn-claim\" hx-post=\"/api/tasks/");
    s.push_str(id);
    s.push_str("/claim\" hx-swap=\"none\">claim</button>\n");
    s.push_str("    <button class=\"btn btn-sm btn-done\" hx-post=\"/api/tasks/");
    s.push_str(id);
    s.push_str("/done\" hx-swap=\"none\">done</button>\n");
    s.push_str("    <button class=\"btn btn-sm btn-approve\" hx-post=\"/api/tasks/");
    s.push_str(id);
    s.push_str("/approve\" hx-swap=\"none\">approve</button>\n");
    s.push_str("  </td>\n</tr>");
    s
}

pub fn kanban_card_html(task: &Task) -> String {
    let id = &task.id;
    let title = html_escape(&task.title);
    let claimed = task.claimed_by.as_deref().unwrap_or("");

    let mut s = String::new();
    s.push_str("<div class=\"kanban-card\" hx-get=\"/partials/task-detail/");
    s.push_str(id);
    s.push_str("\" hx-target=\"#detail-pane\" hx-swap=\"innerHTML\">\n");

    // Header: ID + type + priority
    s.push_str("  <div class=\"card-header\">");
    s.push_str("<span class=\"card-id\">");
    s.push_str(id);
    s.push_str("</span> ");
    s.push_str(type_badge(task.task_type));
    s.push(' ');
    s.push_str(&pri_badge(task));
    s.push_str("</div>\n");

    // Title
    s.push_str("  <div class=\"card-title\">");
    s.push_str(&title);
    s.push_str("</div>\n");

    // Description preview
    if let Some(ref desc) = task.description {
        let preview: String = desc.chars().take(80).collect();
        let preview = html_escape(&preview);
        s.push_str("  <div class=\"card-desc\">");
        s.push_str(&preview);
        if desc.len() > 80 {
            s.push_str("...");
        }
        s.push_str("</div>\n");
    }

    // Meta: claimed + blocked
    s.push_str("  <div class=\"card-meta\">");
    if !claimed.is_empty() {
        s.push_str("<span class=\"card-claimed\">@");
        s.push_str(&html_escape(claimed));
        s.push_str("</span>");
    }
    if !task.blocked_by.is_empty() {
        s.push_str("<span class=\"card-blocked\">blocked by ");
        s.push_str(&html_escape(&task.blocked_by.join(", ")));
        s.push_str("</span>");
    }
    s.push_str("</div>\n");

    // Actions
    s.push_str("  <div class=\"card-actions\">\n");
    s.push_str("    <button class=\"btn btn-sm btn-claim\" hx-post=\"/api/tasks/");
    s.push_str(id);
    s.push_str("/claim\" hx-swap=\"none\">claim</button>\n");
    s.push_str("    <button class=\"btn btn-sm btn-done\" hx-post=\"/api/tasks/");
    s.push_str(id);
    s.push_str("/done\" hx-swap=\"none\">done</button>\n");
    s.push_str("    <button class=\"btn btn-sm btn-approve\" hx-post=\"/api/tasks/");
    s.push_str(id);
    s.push_str("/approve\" hx-swap=\"none\">approve</button>\n");
    s.push_str("  </div>\n</div>");
    s
}

pub fn graph_node_html(task: &Task, depth: usize) -> String {
    let node_class = match task.status {
        TaskStatus::Open => "node-open",
        TaskStatus::InProgress => "node-progress",
        TaskStatus::Done => "node-done",
    };

    let icon = if task.task_type == TaskType::Epic {
        "&#9670;" // diamond
    } else if !task.blocked_by.is_empty() && task.status != TaskStatus::Done {
        "&#9676;" // dotted circle
    } else {
        "&#9679;" // filled circle
    };

    let claimed = task
        .claimed_by
        .as_deref()
        .map(|c| format!(" <span class=\"card-claimed\">@{}</span>", html_escape(c)))
        .unwrap_or_default();

    let connector = if depth > 0 {
        "&#9500;&#9472;&#9472; "
    } else {
        ""
    };
    let id = &task.id;
    let title = html_escape(&task.title);
    let pri = task.priority;
    let status = task.status;

    let mut s = String::new();
    s.push_str("<div class=\"graph-node depth-");
    s.push_str(&depth.to_string());
    s.push(' ');
    s.push_str(node_class);
    s.push_str("\" hx-get=\"/partials/task-detail/");
    s.push_str(id);
    s.push_str("\" hx-target=\"#detail-pane\" hx-swap=\"innerHTML\" style=\"cursor:pointer\">\n");
    s.push_str("  <span class=\"graph-connector\">");
    s.push_str(connector);
    s.push_str("</span><span class=\"graph-icon\">");
    s.push_str(icon);
    s.push_str("</span>\n  <span class=\"graph-id\">");
    s.push_str(id);
    s.push_str("</span>\n  <span class=\"graph-title\">");
    s.push_str(&title);
    s.push_str("</span>\n  <span style=\"color:#565f89;font-size:12px\">[");
    s.push_str(&pri.to_string());
    s.push_str(", ");
    s.push_str(&status.to_string());
    s.push_str("]</span>");
    s.push_str(&claimed);
    s.push_str("\n</div>\n");
    s
}

/// Render the collapsible epic→children tree (issue #196).
///
/// Epics are native `<details>` elements (zero-JS collapse). Each epic carries
/// a roll-up badge (`done/total ✓`) and, when every child is done but the epic
/// itself is not, a "stale" badge — the bookkeeping signal the issue calls out
/// as the one eating triage time. Leaves are click-to-detail like the other
/// views; data-* attributes drive the client-side filter sidebar.
pub fn tree_html(tasks: &[Task]) -> String {
    let tree = crate::presentation::shared::TaskTree::build(tasks);

    if tree.epics.is_empty() && tree.standalone.is_empty() {
        return String::from(
            "<p class=\"empty-state\">No tasks — run <code>cn task create</code> to get started</p>",
        );
    }

    let mut html = String::new();

    for group in &tree.epics {
        let total = group.children.len();
        let done = group
            .children
            .iter()
            .filter(|c| c.status == TaskStatus::Done)
            .count();
        let all_done = total > 0 && done == total;
        let stale = all_done && group.epic.status != TaskStatus::Done;

        html.push_str("<details class=\"tree-epic\" open data-status=\"");
        html.push_str(&group.epic.status.to_string());
        html.push_str("\" data-type=\"epic\" data-pri=\"");
        html.push_str(&group.epic.priority.to_string());
        html.push_str("\" data-text=\"");
        html.push_str(&html_escape(&group.epic.title.to_lowercase()));
        html.push_str("\">\n<summary class=\"tree-summary\">");
        html.push_str(&tree_node_inner(group.epic));
        html.push_str(&format!(
            "<span class=\"rollup {}\">{done}/{total} \u{2713}</span>",
            if all_done {
                "rollup-complete"
            } else {
                "rollup-partial"
            }
        ));
        if stale {
            html.push_str(
                "<span class=\"stale-badge\" title=\"all children done but epic still open — roll it up\">stale</span>",
            );
        }
        html.push_str("</summary>\n<div class=\"tree-children\">\n");
        for child in &group.children {
            html.push_str(&tree_leaf_html(child));
        }
        if group.children.is_empty() {
            html.push_str("<div class=\"tree-empty\">no children</div>\n");
        }
        html.push_str("</div>\n</details>\n");
    }

    if !tree.standalone.is_empty() {
        html.push_str("<div class=\"tree-section\">Standalone</div>\n");
        for task in &tree.standalone {
            html.push_str(&tree_leaf_html(task));
        }
    }

    html
}

/// Inline content of a tree node (id, type, priority, title, status,
/// claimant). Shared by the epic summary and the leaf row.
///
/// The leading id/type/priority badges are grouped into a fixed `.tree-meta`
/// column so the `.tree-body` (title + status + any trailing badges) flows and
/// wraps with a consistent hanging indent instead of breaking ragged to col 0.
/// `extra` carries renderer-specific trailing markup (e.g. the leaf's quiet
/// blocked-by badge) so it lives inside the wrapping body column.
fn tree_node_inner(task: &Task) -> String {
    tree_node_inner_with(task, "")
}

fn tree_node_inner_with(task: &Task, extra: &str) -> String {
    let claimed = task
        .claimed_by
        .as_deref()
        .map(|c| format!(" <span class=\"card-claimed\">@{}</span>", html_escape(c)))
        .unwrap_or_default();
    format!(
        "<span class=\"tree-meta\"><span class=\"tree-id\">{}</span>{} {}</span>\
         <span class=\"tree-body\"><span class=\"tree-title\">{}</span> \
         <span class=\"{}\">{}</span>{}{}</span>",
        html_escape(&task.id),
        type_badge(task.task_type),
        pri_badge(task),
        html_escape(&task.title),
        crate::presentation::shared::status_css_class(task.status),
        task.status,
        claimed,
        extra,
    )
}

/// A clickable leaf node (child or standalone task) — opens the detail pane.
fn tree_leaf_html(task: &Task) -> String {
    // Quiet, discoverable blocked-by badge (de-emphasised per the polish pass):
    // it lives inside the wrapping `.tree-body` so it hangs under the title
    // rather than shouting in loud red at the row's edge.
    let blocked = if task.blocked_by.is_empty() || task.status == TaskStatus::Done {
        String::new()
    } else {
        format!(
            " <span class=\"tree-blocked\" title=\"blocked by {0}\">\u{1F512} {0}</span>",
            html_escape(&task.blocked_by.join(", "))
        )
    };
    format!(
        "<div class=\"tree-leaf\" data-status=\"{}\" data-type=\"{}\" data-pri=\"{}\" data-text=\"{}\" \
         hx-get=\"/partials/task-detail/{}\" hx-target=\"#detail-pane\" hx-swap=\"innerHTML\">{}</div>\n",
        task.status,
        task.task_type,
        task.priority,
        html_escape(&task.title.to_lowercase()),
        html_escape(&task.id),
        tree_node_inner_with(task, &blocked),
    )
}

/// Render markdown text to HTML.
pub fn render_markdown_html(text: &str) -> String {
    let mut html = String::new();
    let mut in_code_block = false;

    for line in text.lines() {
        let trimmed = line.trim_start();

        if trimmed.starts_with("```") {
            if in_code_block {
                html.push_str("</code></pre>\n");
            } else {
                html.push_str("<pre><code>");
            }
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            html.push_str(&html_escape(line));
            html.push('\n');
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("### ") {
            html.push_str("<h3>");
            html.push_str(&inline_md(&html_escape(rest)));
            html.push_str("</h3>\n");
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            html.push_str("<h2>");
            html.push_str(&inline_md(&html_escape(rest)));
            html.push_str("</h2>\n");
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            html.push_str("<h1>");
            html.push_str(&inline_md(&html_escape(rest)));
            html.push_str("</h1>\n");
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            html.push_str("<li>");
            html.push_str(&inline_md(&html_escape(&trimmed[2..])));
            html.push_str("</li>\n");
        } else if trimmed.is_empty() {
            html.push_str("<br>\n");
        } else {
            html.push_str("<p>");
            html.push_str(&inline_md(&html_escape(line)));
            html.push_str("</p>\n");
        }
    }

    if in_code_block {
        html.push_str("</code></pre>\n");
    }

    html
}

fn inline_md(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 32);
    let mut rest = text;

    while !rest.is_empty() {
        if let Some(start) = rest.find("**") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            if let Some(end) = after.find("**") {
                out.push_str("<strong>");
                out.push_str(&after[..end]);
                out.push_str("</strong>");
                rest = &after[end + 2..];
            } else {
                out.push_str(&rest[start..]);
                break;
            }
        } else if let Some(start) = rest.find('`') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            if let Some(end) = after.find('`') {
                out.push_str("<code>");
                out.push_str(&after[..end]);
                out.push_str("</code>");
                rest = &after[end + 1..];
            } else {
                out.push_str(&rest[start..]);
                break;
            }
        } else {
            out.push_str(rest);
            break;
        }
    }

    out
}

#[cfg(test)]
mod tree_tests {
    use super::*;
    use crate::domain::task::{Priority, Task, TaskStatus, TaskType};

    fn task(id: &str, tt: TaskType, status: TaskStatus, parent: Option<&str>) -> Task {
        Task {
            id: id.into(),
            title: format!("{id} title"),
            priority: Priority::P2,
            status,
            task_type: tt,
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

    #[test]
    fn empty_universe_renders_empty_state() {
        assert!(tree_html(&[]).contains("No tasks"));
    }

    #[test]
    fn epic_rolls_up_done_count_and_flags_stale() {
        // Epic still open, but both children done → stale + 2/2 complete badge.
        let tasks = vec![
            task("t-ep", TaskType::Epic, TaskStatus::Open, None),
            task("t-c1", TaskType::Task, TaskStatus::Done, Some("t-ep")),
            task("t-c2", TaskType::Bug, TaskStatus::Done, Some("t-ep")),
        ];
        let html = tree_html(&tasks);
        assert!(
            html.contains("rollup-complete"),
            "expected complete roll-up"
        );
        assert!(html.contains("2/2"), "expected 2/2 count");
        assert!(
            html.contains("stale-badge"),
            "all-children-done open epic is stale"
        );
        assert!(
            html.contains("<details"),
            "epic is a collapsible details element"
        );
        assert!(html.contains("t-c1"), "children rendered");
    }

    #[test]
    fn partial_epic_is_not_stale() {
        let tasks = vec![
            task("t-ep", TaskType::Epic, TaskStatus::Open, None),
            task("t-c1", TaskType::Task, TaskStatus::Done, Some("t-ep")),
            task("t-c2", TaskType::Task, TaskStatus::Open, Some("t-ep")),
        ];
        let html = tree_html(&tasks);
        assert!(html.contains("rollup-partial"));
        assert!(html.contains("1/2"));
        assert!(
            !html.contains("stale-badge"),
            "partially-done epic is not stale"
        );
    }

    #[test]
    fn standalone_tasks_get_their_own_section() {
        let tasks = vec![task("t-solo", TaskType::Task, TaskStatus::Open, None)];
        let html = tree_html(&tasks);
        assert!(html.contains("Standalone"));
        assert!(html.contains("tree-leaf"));
        assert!(html.contains("t-solo"));
    }
}
