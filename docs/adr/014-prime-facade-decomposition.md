# ADR-014: Facade Decomposition into Sub-API Modules

**Status:** Accepted, not implemented
**Date:** 2026-03-20
**Deciders:** Architecture Review

## Context

`facade.rs` is a 2786-line God object that contains all Prime functionality in a single `impl Prime` block: node CRUD, edge CRUD, traversal, vector operations (embed/search/delete), temporal queries (history, diff, as_of, neighbors_as_of), recall, schema enforcement, contradiction detection, import/export, and compaction.

This creates several problems:

1. **Cognitive load.** A developer looking for the vector search implementation must scroll through 800+ lines of graph CRUD to find it. Methods are loosely grouped by comments but there is no structural separation.

2. **Compilation coupling.** Any change to a temporal query method triggers recompilation of the entire facade, including all graph and vector code.

3. **Feature gating complexity.** Vector methods are scattered through the file with `#[cfg(feature = "prime-vectors")]` annotations. When the feature is disabled, the dead code is still visually present and reviewed.

4. **Testing friction.** Tests for vector search share the same test module as tests for graph traversal. Test failures are harder to triage.

The `Prime` struct itself is well-designed — it holds `Arc` references to projections and an `EmbeddedCore`. The issue is that all methods are defined directly on `Prime` rather than being organized into coherent sub-APIs.

## Decision

Split the facade into sub-API modules using the borrowed sub-API pattern. The tree below is the target layout and is not implemented: `prime/` holds one `facade.rs` (166.4K), and no `graph()`, `vectors()` or `temporal()` accessors exist.

```
prime/
  facade.rs        — Prime struct, construction, shutdown, core access
  graph_api.rs     — Node CRUD, edge CRUD, traversal, neighbors
  vector_api.rs    — embed, search, delete_vector (gated by prime-vectors)
  temporal_api.rs  — history, diff, as_of, neighbors_as_of
```

Each sub-API is a zero-cost wrapper that borrows from `Prime`:

```rust
// graph_api.rs
pub struct GraphApi<'a> {
    prime: &'a Prime,
}

impl<'a> GraphApi<'a> {
    pub(crate) fn new(prime: &'a Prime) -> Self {
        Self { prime }
    }

    pub async fn add_node(&self, node_type: &str, properties: Value) -> PrimeResult<NodeId> {
        // implementation moves here from facade.rs
    }
    // ...
}
```

`Prime` exposes accessor methods that return sub-APIs:

```rust
impl Prime {
    pub fn graph(&self) -> GraphApi<'_> {
        GraphApi::new(self)
    }

    pub fn vectors(&self) -> VectorApi<'_> {
        VectorApi::new(self)
    }

    pub fn temporal(&self) -> TemporalApi<'_> {
        TemporalApi::new(self)
    }
}
```

Callers use: `prime.graph().add_node(...)`, `prime.vectors().search(...)`, `prime.temporal().as_of(...)`.

**Backward compatibility:** Deprecated wrapper methods remain on `Prime` for one release cycle:

```rust
impl Prime {
    #[deprecated(note = "Use prime.graph().add_node() instead")]
    pub async fn add_node(&self, node_type: &str, properties: Value) -> PrimeResult<NodeId> {
        self.graph().add_node(node_type, properties).await
    }
}
```

Schema enforcement, contradiction detection, recall, and import/export remain on `Prime` directly (or get their own sub-APIs in a future ADR) since they cross-cut graph and vector concerns.

## Consequences

### Positive

- **Navigability.** Each sub-API file is 200-400 lines instead of one 2800-line file. Developers can find methods by file name rather than scrolling.
- **Clean feature gating.** `vector_api.rs` is entirely behind `#[cfg(feature = "prime-vectors")]`. No scattered feature annotations in the main facade.
- **Independent compilation.** Changes to temporal query logic do not trigger recompilation of graph CRUD or vector search code.
- **Testability.** Each sub-API file has its own `#[cfg(test)]` module with focused tests. Test failures immediately indicate which subsystem is broken.
- **Zero runtime cost.** Sub-API structs are `&'a Prime` wrappers with no allocation. The compiler inlines the accessor methods.

### Negative

- **Extra indirection in caller code.** `prime.graph().add_node(...)` is slightly more verbose than `prime.add_node(...)`. In practice, callers typically operate within one domain (all graph calls or all vector calls), so they can bind `let g = prime.graph();` once.
- **Internal visibility.** Sub-API modules need access to `Prime`'s projection fields. These fields must be `pub(crate)` rather than private, slightly widening the internal API surface.
- **Deprecation period overhead.** Maintaining wrapper methods on `Prime` for one release cycle adds ~100 lines of boilerplate that must eventually be removed.

### Risks

- **Incomplete decomposition.** Some methods (like `remember()`) span graph + vector + edge concerns. These cross-cutting methods either remain on `Prime` or delegate to multiple sub-APIs. If the delegation becomes complex, the facade pattern loses its simplicity. Mitigation: `remember()` stays on `Prime` since it is explicitly a cross-cutting convenience method.
- **Breaking change for SDK-level callers.** If any external code (unlikely — `Prime` is an embedded library) uses `prime.add_node()` directly, the deprecation warnings will surface. The one-release-cycle grace period gives time to migrate.
