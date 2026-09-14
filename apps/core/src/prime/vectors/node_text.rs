//! The one text a node is embedded as.
//!
//! Three call sites need to turn a node into a sentence: the write path, the
//! back-fill worker, and any operator repair script. If they compose it
//! differently the store ends up holding vectors that rank against different
//! sentences — which is worse than holding none, because nothing reports it.

use serde_json::Value;

/// Composition version, recorded alongside every vector this text produced.
///
/// Vectors embedded under different versions are not comparable. Bump this
/// when [`node_text`] changes and the back-fill re-embeds anything stamped
/// with an older one.
pub const NODE_TEXT_VERSION: u32 = 1;

/// Metadata key carrying [`NODE_TEXT_VERSION`] on a stored vector.
pub const NODE_TEXT_VERSION_KEY: &str = "node_text_version";

/// Keys that identify a node rather than describe it. They cost tokens in the
/// embedded sentence and pull unrelated nodes together — two people who share
/// a `last_engaged` date are not similar.
const SKIP_KEYS: &[&str] = &[
    "created_at",
    "updated_at",
    "last_engaged",
    "linkedin_url",
    "profile_url",
    "source_url",
    "url",
    "id",
    "uuid",
    "avatar",
    "image",
];

/// Upper bound on the composed sentence. AllMiniLML6V2 truncates at 256
/// tokens; past that the tail is embedded as silence, so spending more
/// characters buys nothing.
const MAX_LEN: usize = 1000;

/// Compose the text embedded for a node.
///
/// Leads with `type: name` because that is what a human searches for, then
/// appends every remaining readable property as `key: value`. Property order
/// follows the map's own iteration order, so the same node composes the same
/// sentence on every call.
pub fn node_text(node_type: &str, properties: &Value) -> String {
    let name = properties
        .get("name")
        .or_else(|| properties.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let mut parts = Vec::new();
    parts.push(if name.is_empty() {
        node_type.to_string()
    } else {
        format!("{node_type}: {name}")
    });

    if let Some(map) = properties.as_object() {
        for (key, value) in map {
            if key == "name" || key == "title" || SKIP_KEYS.contains(&key.as_str()) {
                continue;
            }
            let Some(rendered) = scalar_to_string(value) else {
                continue;
            };
            let rendered = rendered.trim();
            if rendered.is_empty()
                || rendered.starts_with("http://")
                || rendered.starts_with("https://")
            {
                continue;
            }
            parts.push(format!("{key}: {rendered}"));
        }
    }

    truncate_on_char_boundary(&parts.join("; "), MAX_LEN)
}

/// Render the scalars worth embedding. Nested objects and arrays are skipped:
/// their JSON punctuation embeds as noise.
fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `String::truncate` panics mid-codepoint; node properties carry plenty of
/// non-ASCII.
fn truncate_on_char_boundary(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn leads_with_type_and_name() {
        let text = node_text(
            "person",
            &json!({"name": "Ada Lovelace", "role": "Founder"}),
        );
        assert!(text.starts_with("person: Ada Lovelace"), "{text}");
        assert!(text.contains("role: Founder"), "{text}");
    }

    #[test]
    fn drops_identifiers_and_urls() {
        let text = node_text(
            "person",
            &json!({
                "name": "Ada",
                "linkedin_url": "https://example.com/ada",
                "created_at": "2026-01-01",
                "homepage": "https://ada.dev",
                "note": "writes compilers",
            }),
        );
        assert!(!text.contains("example.com"), "{text}");
        assert!(!text.contains("2026-01-01"), "{text}");
        assert!(!text.contains("ada.dev"), "{text}");
        assert!(text.contains("note: writes compilers"), "{text}");
    }

    #[test]
    fn a_node_with_no_name_still_composes() {
        let text = node_text("event", &json!({"summary": "four comments posted"}));
        assert_eq!(text, "event; summary: four comments posted");
    }

    #[test]
    fn nested_values_are_skipped() {
        let text = node_text(
            "project",
            &json!({"name": "Prime", "tags": ["a", "b"], "meta": {"k": "v"}}),
        );
        assert_eq!(text, "project: Prime");
    }

    #[test]
    fn the_same_node_composes_the_same_sentence_twice() {
        let props = json!({"name": "Ada", "b": "two", "a": "one", "c": "three"});
        assert_eq!(node_text("person", &props), node_text("person", &props));
    }

    #[test]
    fn truncation_does_not_split_a_codepoint() {
        let long = "é".repeat(MAX_LEN);
        let text = node_text("note", &json!({"name": "x", "body": long}));
        assert!(text.len() <= MAX_LEN);
        assert!(text.chars().all(|c| c != '\u{fffd}'));
    }
}
