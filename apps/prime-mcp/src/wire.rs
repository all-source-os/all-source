//! Wire shaping and windowing for node-bearing tool results.
//!
//! Two invariants live here, both of which an MCP client depends on.
//!
//! - **`id` is the entity id** — `node:{type}:{uuid}`. [`Node::id`] holds only
//!   the uuid segment, and no tool accepts that as input: `prime_neighbors`,
//!   `prime_forget`, `prime_history` and `prime_embed` all key on the wire
//!   form. A row carrying the bare uuid cannot be drilled into, so a client
//!   given a large result has to keep all of it.
//! - **A [`Fields::Summary`] row is flat and all-scalar**, which is what lets
//!   `toon::encode` take its tabular path. A nested `properties` falls back to
//!   list form and compresses nothing.

use allsource_core::prime::{EntityId, Node};
use serde_json::{Map, Value, json};

/// Rows returned when the caller names no `limit`.
///
/// 50 summary rows is roughly 1.5k tokens.
pub const DEFAULT_LIMIT: usize = 50;

/// Ceiling on an explicit `limit`, so `limit: 100000` cannot reinstate an
/// unbounded result.
pub const MAX_LIMIT: usize = 500;

/// How much of a node goes on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fields {
    /// `{id, type, name, at}` — flat, all-scalar, TOON-tabular.
    Summary,
    /// `{id, type, properties}` — the full property bag.
    Full,
}

impl Fields {
    /// Read the `fields` argument, falling back to the tool's own default.
    pub fn from_args(args: &Value, default: Self) -> Self {
        match args.get("fields").and_then(Value::as_str) {
            Some("full") => Self::Full,
            Some("summary") => Self::Summary,
            _ => default,
        }
    }
}

/// Shape a node for the wire.
pub fn node_row(n: &Node, fields: Fields) -> Value {
    let id = EntityId::node(&n.node_type, n.id.as_str()).to_wire();
    match fields {
        Fields::Summary => json!({
            "id": id,
            "type": n.node_type,
            "name": display_name(n),
            "at": at_hint(n),
        }),
        Fields::Full => json!({
            "id": id,
            "type": n.node_type,
            "properties": n.properties,
        }),
    }
}

/// The first property a human would recognise the node by.
fn display_name(n: &Node) -> String {
    for key in ["name", "title", "path", "label", "subject", "summary"] {
        if let Some(s) = n.properties.get(key).and_then(Value::as_str)
            && !s.is_empty()
        {
            return s.to_string();
        }
    }
    String::new()
}

/// Where the node came from, in one scalar: `file:line` for code, otherwise the
/// date it last changed. A colon needs no quoting in TOON, so this stays
/// tabular.
fn at_hint(n: &Node) -> String {
    if let (Some(file), Some(line)) = (
        n.properties.get("file").and_then(Value::as_str),
        n.properties.get("line").and_then(Value::as_u64),
    ) {
        return format!("{file}:{line}");
    }
    n.updated_at.format("%Y-%m-%d").to_string()
}

/// A `limit`/`offset` window over a list-shaped result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    pub limit: usize,
    pub offset: usize,
}

impl Page {
    pub fn from_args(args: &Value) -> Self {
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_LIMIT, |v| (v as usize).clamp(1, MAX_LIMIT));
        let offset = args
            .get("offset")
            .and_then(Value::as_u64)
            .map_or(0, |v| v as usize);
        Self { limit, offset }
    }

    /// Apply the window, returning the slice and the count before it was cut.
    pub fn apply<T>(self, items: Vec<T>) -> (Vec<T>, usize) {
        let total = items.len();
        let rows = items
            .into_iter()
            .skip(self.offset)
            .take(self.limit)
            .collect();
        (rows, total)
    }
}

/// Envelope a windowed list so a truncated answer can never read as a complete
/// one. `total` is the count before the window was applied.
pub fn paged(key: &str, rows: Vec<Value>, page: Page, total: usize) -> Value {
    let returned = rows.len();
    let mut out = Map::new();
    out.insert(key.to_string(), Value::Array(rows));
    out.insert("total".into(), json!(total));
    out.insert("returned".into(), json!(returned));
    out.insert("offset".into(), json!(page.offset));

    let next = page.offset + returned;
    if next < total {
        out.insert("next_offset".into(), json!(next));
        out.insert(
            "note".into(),
            json!(format!(
                "showing {returned} of {total} — call again with offset={next} for the next page, \
                 or narrow the query"
            )),
        );
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use allsource_core::prime::NodeId;
    use chrono::{TimeZone, Utc};

    fn node(node_type: &str, props: Value) -> Node {
        let at = Utc.with_ymd_and_hms(2026, 3, 4, 5, 6, 7).unwrap();
        Node {
            id: NodeId::from("11111111-2222-3333-4444-555555555555".to_string()),
            node_type: node_type.to_string(),
            properties: props,
            domain: None,
            labels: vec![],
            deleted: false,
            created_at: at,
            updated_at: at,
        }
    }

    #[test]
    fn summary_row_id_is_the_wire_id_not_the_bare_uuid() {
        let n = node("function", json!({ "name": "call_search" }));
        let row = node_row(&n, Fields::Summary);
        assert_eq!(
            row["id"],
            json!("node:function:11111111-2222-3333-4444-555555555555")
        );
    }

    #[test]
    fn full_row_carries_the_wire_id_too() {
        let n = node("person", json!({ "name": "Alice" }));
        let row = node_row(&n, Fields::Full);
        assert_eq!(
            row["id"],
            json!("node:person:11111111-2222-3333-4444-555555555555")
        );
        assert_eq!(row["properties"]["name"], json!("Alice"));
    }

    #[test]
    fn summary_row_is_flat_and_all_scalar() {
        let n = node(
            "function",
            json!({ "name": "f", "file": "src/a.rs", "line": 12 }),
        );
        let row = node_row(&n, Fields::Summary);
        let obj = row.as_object().expect("row is an object");
        assert!(
            obj.values().all(|v| !v.is_object() && !v.is_array()),
            "a nested value would drop toon::encode off its tabular path: {row}"
        );
        assert_eq!(row["at"], json!("src/a.rs:12"));
        assert_eq!(row["name"], json!("f"));
    }

    #[test]
    fn at_hint_falls_back_to_the_update_date() {
        let n = node("person", json!({ "name": "Alice" }));
        assert_eq!(node_row(&n, Fields::Summary)["at"], json!("2026-03-04"));
    }

    #[test]
    fn display_name_prefers_path_when_there_is_no_name() {
        let n = node("file", json!({ "path": "src/lib.rs", "language": "rust" }));
        assert_eq!(node_row(&n, Fields::Summary)["name"], json!("src/lib.rs"));
    }

    #[test]
    fn default_page_caps_at_fifty() {
        let page = Page::from_args(&json!({}));
        let (rows, total) = page.apply((0..2000).collect::<Vec<_>>());
        assert_eq!(rows.len(), DEFAULT_LIMIT);
        assert_eq!(total, 2000);
    }

    #[test]
    fn explicit_limit_is_clamped_so_it_cannot_reinstate_the_dump() {
        let page = Page::from_args(&json!({ "limit": 100_000 }));
        assert_eq!(page.limit, MAX_LIMIT);
    }

    #[test]
    fn offset_walks_the_window() {
        let page = Page::from_args(&json!({ "limit": 2, "offset": 3 }));
        let (rows, total) = page.apply(vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(rows, vec![3, 4]);
        assert_eq!(total, 6);
    }

    #[test]
    fn a_truncated_page_says_so_and_names_the_next_offset() {
        let page = Page {
            limit: 2,
            offset: 0,
        };
        let out = paged("nodes", vec![json!(1), json!(2)], page, 7);
        assert_eq!(out["total"], json!(7));
        assert_eq!(out["returned"], json!(2));
        assert_eq!(out["next_offset"], json!(2));
        assert!(out["note"].as_str().unwrap().contains("offset=2"));
    }

    /// `toon::encode` only goes tabular on uniform all-scalar rows
    /// (`toon::tabular_fields`), and `toon::tests::non_uniform_array_falls_back_to_list`
    /// pins the other direction. This is the counterpart: summary rows keep the
    /// header, full rows do not.
    #[test]
    fn summary_rows_reach_the_tabular_toon_path_and_full_rows_do_not() {
        let nodes: Vec<Node> = (0..3)
            .map(|i| {
                node(
                    "function",
                    json!({ "name": format!("fn_{i}"), "file": "a.rs", "line": i }),
                )
            })
            .collect();
        let page = Page {
            limit: 50,
            offset: 0,
        };

        let summary = paged(
            "nodes",
            nodes.iter().map(|n| node_row(n, Fields::Summary)).collect(),
            page,
            3,
        );
        let encoded = crate::toon::encode(&summary);
        assert!(
            encoded.contains("nodes[3]{at,id,name,type}:"),
            "summary rows should encode as one header + 3 rows, got:\n{encoded}"
        );

        let full = paged(
            "nodes",
            nodes.iter().map(|n| node_row(n, Fields::Full)).collect(),
            page,
            3,
        );
        assert!(
            !crate::toon::encode(&full).contains("nodes[3]{"),
            "a nested `properties` cannot be tabular"
        );
    }

    #[test]
    fn a_complete_page_carries_no_next_offset() {
        let page = Page {
            limit: 50,
            offset: 0,
        };
        let out = paged("nodes", vec![json!(1)], page, 1);
        assert!(out.get("next_offset").is_none());
        assert!(out.get("note").is_none());
    }
}
