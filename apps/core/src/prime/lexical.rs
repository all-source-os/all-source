//! Property-text scoring, used when recall has query text but no query vector.
//!
//! Node ids are uuids and node text lives nowhere else, so a node's
//! `properties` are the only substrate a text query can match against. Values
//! are split on identifier boundaries as well as whitespace: a graph built by
//! Hound stores `call_search` and `HostedPrime`, and a query of "search" or
//! "hosted prime" has to reach them.
//!
//! This is deliberately not a ranking model. It exists so that recall without
//! an embedder returns the right neighbourhood to expand from, rather than
//! nothing at all.

use serde_json::Value;

use super::types::Node;

/// Property keys that identify a node rather than describe it. A term matching
/// here counts for more than the same term buried in a description.
const NAME_KEYS: [&str; 6] = ["name", "title", "path", "label", "subject", "file"];

/// Weight applied to a term matched outside [`NAME_KEYS`].
const BODY_WEIGHT: f64 = 0.6;

/// Split query text into lowercase search terms.
pub fn terms(text: &str) -> Vec<String> {
    let mut out: Vec<String> = split_identifier(text);
    out.sort();
    out.dedup();
    out
}

/// Score `node` against `terms`, in `0.0..=1.0`. Zero means nothing matched.
///
/// The score is the mean per-term best match, so a node matching two of four
/// terms cannot outrank one matching all four merely by being wordier.
pub fn score(node: &Node, terms: &[String]) -> f64 {
    if terms.is_empty() {
        return 0.0;
    }

    let mut name_tokens: Vec<String> = split_identifier(&node.node_type);
    let mut body_tokens: Vec<String> = Vec::new();
    collect(&node.properties, None, &mut name_tokens, &mut body_tokens);
    for label in &node.labels {
        body_tokens.extend(split_identifier(label));
    }

    let total: f64 = terms
        .iter()
        .map(|t| {
            let name_hit = best_hit(t, &name_tokens);
            let body_hit = best_hit(t, &body_tokens) * BODY_WEIGHT;
            name_hit.max(body_hit)
        })
        .sum();

    total / terms.len() as f64
}

/// How well `term` matches any of `tokens`: exact 1.0, prefix 0.75, substring
/// 0.5, otherwise 0.0.
fn best_hit(term: &str, tokens: &[String]) -> f64 {
    let mut best = 0.0f64;
    for tok in tokens {
        let hit = if tok == term {
            1.0
        } else if tok.starts_with(term) || term.starts_with(tok.as_str()) {
            0.75
        } else if tok.contains(term) || term.contains(tok.as_str()) {
            0.5
        } else {
            0.0
        };
        if hit > best {
            best = hit;
        }
        if best >= 1.0 {
            break;
        }
    }
    best
}

/// Walk a property value, routing strings into the name or body bucket by the
/// key that led to them. `key` is `None` for values inside an array.
fn collect(value: &Value, key: Option<&str>, name: &mut Vec<String>, body: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            let bucket = if key.is_some_and(|k| NAME_KEYS.contains(&k)) {
                name
            } else {
                body
            };
            bucket.extend(split_identifier(s));
        }
        Value::Object(map) => {
            for (k, v) in map {
                collect(v, Some(k.as_str()), name, body);
            }
        }
        Value::Array(items) => {
            for v in items {
                collect(v, key, name, body);
            }
        }
        // Numbers and booleans carry no text a query can match.
        _ => {}
    }
}

/// Lowercase, then split on non-alphanumerics AND on camelCase humps, so
/// `call_search`, `src/lib.rs` and `HostedPrime` all yield their parts. The
/// unsplit whole is kept too, so an exact-identifier query still scores 1.0.
fn split_identifier(s: &str) -> Vec<String> {
    let lower = s.to_lowercase();
    let mut out: Vec<String> = Vec::new();

    for chunk in lower.split(|c: char| !c.is_alphanumeric()) {
        if chunk.len() >= 2 {
            out.push(chunk.to_string());
        }
    }

    let mut hump = String::new();
    for ch in s.chars() {
        if ch.is_uppercase() && !hump.is_empty() {
            if hump.len() >= 2 {
                out.push(std::mem::take(&mut hump).to_lowercase());
            } else {
                hump.clear();
            }
        }
        if ch.is_alphanumeric() {
            hump.push(ch);
        } else if hump.len() >= 2 {
            out.push(std::mem::take(&mut hump).to_lowercase());
        } else {
            hump.clear();
        }
    }
    if hump.len() >= 2 {
        out.push(hump.to_lowercase());
    }

    if lower.len() >= 2 {
        out.push(lower);
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prime::types::NodeId;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

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
    fn snake_case_identifiers_split_into_their_parts() {
        let n = node("function", json!({ "name": "call_search" }));
        assert!(score(&n, &terms("search")) > 0.0);
        assert!(score(&n, &terms("call")) > 0.0);
    }

    #[test]
    fn camel_case_identifiers_split_into_their_parts() {
        let n = node("struct", json!({ "name": "HostedPrime" }));
        assert!(score(&n, &terms("hosted")) > 0.0);
        assert!(score(&n, &terms("prime")) > 0.0);
    }

    #[test]
    fn a_prefix_term_scores_below_an_exact_one_but_above_nothing() {
        let n = node("function", json!({ "name": "parse_invoice" }));
        let partial = score(&n, &terms("invoic"));
        assert!(partial > 0.0);
        assert!(partial < score(&n, &terms("invoice")));
    }

    /// A query and any of its identifier parts both match exactly once the
    /// query is split, so both saturate — the score does not reward verbosity.
    #[test]
    fn an_identifier_query_and_its_parts_both_match_fully() {
        let n = node("function", json!({ "name": "call_search" }));
        assert_eq!(score(&n, &terms("call_search")), 1.0);
        assert_eq!(score(&n, &terms("call")), 1.0);
    }

    #[test]
    fn a_name_match_outranks_the_same_term_in_a_description() {
        let named = node("function", json!({ "name": "checkout" }));
        let described = node(
            "function",
            json!({ "name": "f", "description": "handles checkout" }),
        );
        let t = terms("checkout");
        assert!(score(&named, &t) > score(&described, &t));
        assert!(score(&described, &t) > 0.0);
    }

    #[test]
    fn matching_every_term_outranks_matching_one() {
        let both = node("function", json!({ "name": "parse_invoice" }));
        let one = node("function", json!({ "name": "parse_header" }));
        let t = terms("parse invoice");
        assert!(score(&both, &t) > score(&one, &t));
    }

    #[test]
    fn a_node_with_nothing_in_common_scores_zero() {
        let n = node(
            "person",
            json!({ "name": "Alice", "email": "a@example.com" }),
        );
        assert_eq!(score(&n, &terms("kubernetes scheduler")), 0.0);
    }

    #[test]
    fn no_terms_scores_zero_rather_than_matching_everything() {
        let n = node("person", json!({ "name": "Alice" }));
        assert_eq!(score(&n, &terms("")), 0.0);
        assert_eq!(score(&n, &[]), 0.0);
    }

    #[test]
    fn a_file_path_matches_by_any_of_its_segments() {
        let n = node("file", json!({ "path": "apps/core/src/prime/facade.rs" }));
        assert!(score(&n, &terms("facade")) > 0.0);
        assert!(score(&n, &terms("prime")) > 0.0);
    }

    #[test]
    fn nested_and_array_properties_are_searched() {
        let n = node(
            "interaction",
            json!({ "from": { "name": "Dana" }, "tags": ["invoice", "urgent"] }),
        );
        assert!(score(&n, &terms("dana")) > 0.0);
        assert!(score(&n, &terms("invoice")) > 0.0);
    }

    #[test]
    fn numbers_are_not_matchable_text() {
        let n = node("function", json!({ "name": "f", "line": 42 }));
        assert_eq!(score(&n, &terms("42")), 0.0);
    }

    #[test]
    fn the_node_type_itself_is_matchable() {
        let n = node("project", json!({ "name": "atlas" }));
        assert!(score(&n, &terms("project")) > 0.0);
    }
}
