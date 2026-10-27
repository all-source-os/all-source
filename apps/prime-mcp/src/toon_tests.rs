use super::encode;
use serde_json::json;

#[test]
fn flat_object() {
    let v = json!({ "deleted": true, "id": "node:person:1" });
    assert_eq!(encode(&v), "deleted: true\nid: node:person:1");
}

#[test]
fn tabular_array_of_uniform_objects() {
    let v = json!({
        "vectors": [
            { "id": "v1", "score": 0.92, "text": "alpha" },
            { "id": "v2", "score": 0.81, "text": "beta" },
        ]
    });
    assert_eq!(
        encode(&v),
        "vectors[2]{id,score,text}:\n  v1,0.92,alpha\n  v2,0.81,beta"
    );
}

#[test]
fn scalar_array_inline() {
    let v = json!({ "tags": ["a", "b", "c"] });
    assert_eq!(encode(&v), "tags[3]: a,b,c");
}

#[test]
fn empty_array() {
    assert_eq!(encode(&json!({ "nodes": [] })), "nodes[0]:");
}

#[test]
fn non_uniform_array_falls_back_to_list() {
    // `properties` is a nested object, so rows are not tabular.
    let v = json!({
        "nodes": [
            { "id": "n1", "properties": { "name": "Alice" } },
            { "id": "n2", "properties": { "name": "Bob" } },
        ]
    });
    assert_eq!(
        encode(&v),
        "nodes[2]:\n  - id: n1\n    properties:\n      name: Alice\n  \
         - id: n2\n    properties:\n      name: Bob"
    );
}

#[test]
fn ambiguous_strings_are_quoted() {
    let v = json!({ "a": "123", "b": "true", "c": "x,y", "d": "plain" });
    assert_eq!(encode(&v), "a: \"123\"\nb: \"true\"\nc: \"x,y\"\nd: plain");
}

#[test]
fn nested_object() {
    // serde_json maps are ordered (no `preserve_order` feature), so the
    // encoder emits keys alphabetically — deterministic regardless of
    // json! literal order.
    let v = json!({ "stats": { "total_nodes": 3, "total_edges": 1 } });
    assert_eq!(encode(&v), "stats:\n  total_edges: 1\n  total_nodes: 3");
}

#[test]
fn entity_ids_with_colons_are_not_quoted() {
    let v = json!({ "node_id": "node:person:42" });
    assert_eq!(encode(&v), "node_id: node:person:42");
}
