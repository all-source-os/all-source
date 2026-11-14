//! `shape.toml` holds numbers only: no tenant names, no payload text.

use core_heap_replay::{
    scan::repo_root,
    shape::{Shape, check_numbers_only, load_shape},
};

#[test]
fn shape_has_no_string_values() {
    let path = repo_root().join("tooling/core-heap-replay/shape.toml");
    let text = std::fs::read_to_string(&path).expect("read shape.toml");
    let value: toml::Value = toml::from_str(&text).expect("parse shape.toml");
    let strings = check_numbers_only(&value);
    assert!(
        strings.is_empty(),
        "string values in shape.toml: {strings:?}"
    );
}

#[test]
fn shape_pins_the_production_counts() {
    let shape: Shape =
        load_shape(&repo_root().join("tooling/core-heap-replay/shape.toml")).expect("load shape");
    assert_eq!(shape.tenants.counts.len(), 6, "six hydrated tenants");
    assert_eq!(
        shape.tenants.counts[0], 201_785,
        "largest pinned at Longhand"
    );
    assert_eq!(shape.tenants.counts.iter().copied().max(), Some(201_785));
    assert_eq!(shape.payload.mean_bytes, 3_892);
}
