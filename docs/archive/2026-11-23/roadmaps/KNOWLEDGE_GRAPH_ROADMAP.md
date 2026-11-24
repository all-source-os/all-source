---
title: "AllSource Knowledge Graph — Feature Roadmap"
status: DRAFT
last_updated: 2026-03-01
category: roadmap
proposal: "../proposals/KNOWLEDGE_GRAPH_SERVICE.md"
---

# AllSource Knowledge Graph — Feature Roadmap

**Date**: 2026-03-01
**Version**: 1.0

---

## Summary

This roadmap sequences the AllSource Knowledge Graph from foundational primitives through competitive feature parity and into differentiated capabilities that no competitor offers. Features are organized into milestones, each producing a usable release.

The competitive landscape (Zep/Graphiti, Mem0, Letta, FalkorDB, Cognee) is documented in the [proposal](../proposals/KNOWLEDGE_GRAPH_SERVICE.md#10-competitive-analysis). Every feature below is tagged with whether it's **table stakes** (competitors have it), **parity** (matches best-in-class), or **differentiated** (only AllSource offers it).

---

## Milestone 1: Foundation — Embeddable Graph Primitives

**Goal**: A working knowledge graph that can be used as a Rust library. No HTTP, no MCP — just the core data model and projections.

**Competitive position**: Exceeds all competitors on write latency and embeddability from day one.

| # | Feature | Type | Priority | Detail |
|---|---------|------|----------|--------|
| 1.1 | `KnowledgeGraph` facade | Foundation | P0 | Wraps `EmbeddedCore`, registers graph projections, configures merge strategies |
| 1.2 | Node CRUD | Table stakes | P0 | `add_node`, `get_node`, `update_node`, `delete_node` with `graph.node.*` events |
| 1.3 | Edge CRUD | Table stakes | P0 | `add_edge`, `get_edge`, `delete_edge`, `list_edges` with `graph.edge.*` events |
| 1.4 | `AdjacencyListProjection` | Foundation | P0 | Outgoing edges per node — O(1) neighbor lookup |
| 1.5 | `ReverseIndexProjection` | Foundation | P0 | Incoming edges per node — O(1) reverse traversal |
| 1.6 | `NodeStateProjection` | Foundation | P0 | Current merged state per node (fold create + update events) |
| 1.7 | `NodeTypeIndexProjection` | Foundation | P0 | Nodes grouped by type — O(1) "all persons" lookup |
| 1.8 | `GraphStatsProjection` | Foundation | P1 | Aggregate counts: nodes, edges, per-type, per-relation |
| 1.9 | Node labels | Table stakes | P1 | Multiple labels per node (e.g., `["employee", "engineer"]`) with label index |
| 1.10 | Edge weights | Table stakes | P1 | Optional `f64` weight on edges for weighted traversals |

**Exit criteria**: `cargo test --features knowledge-graph` passes 30+ tests covering CRUD, projection consistency, concurrent access, and merge strategy behavior.

**New files**:
```
apps/core/src/kg/mod.rs
apps/core/src/kg/graph.rs
apps/core/src/kg/types.rs
apps/core/src/kg/projections.rs
apps/core/tests/knowledge_graph.rs
```

---

## Milestone 2: Graph Traversal

**Goal**: Multi-hop queries, shortest path, subgraph extraction. This is where a knowledge graph becomes useful beyond a key-value store.

**Competitive position**: Matches FalkorDB/Neo4j traversal capabilities but over projection-backed indexes (no separate DB).

| # | Feature | Type | Priority | Detail |
|---|---------|------|----------|--------|
| 2.1 | `neighbors(node, relation?, direction?)` | Table stakes | P0 | 1-hop outgoing/incoming/both with optional relation filter |
| 2.2 | `neighbors_within(node, depth, relation?)` | Table stakes | P0 | BFS up to N hops, returns nodes + edges + depth |
| 2.3 | `shortest_path(source, target, max_depth?)` | Table stakes | P0 | Unweighted BFS; weighted Dijkstra when edges have weights |
| 2.4 | `subgraph(center, depth)` | Parity | P0 | Ego network extraction — all nodes and edges within N hops |
| 2.5 | `connected_components()` | Parity | P1 | Detect isolated subgraphs |
| 2.6 | `degree_centrality(node)` | Parity | P2 | In-degree + out-degree as a centrality measure |
| 2.7 | Cycle detection | Parity | P2 | Detect circular dependencies in directed graphs |

**Exit criteria**: Traversal benchmarks on a 10K-node graph: `neighbors` < 20μs, `shortest_path` < 50ms, `subgraph(depth=3)` < 5ms.

---

## Milestone 3: Temporal Graph Queries

**Goal**: Time-travel, audit trails, graph diffs. This is AllSource's primary differentiator — no competitor does this natively.

**Competitive position**: **Differentiated.** Graphiti has bi-temporal metadata but can't reconstruct past graph states. Nobody else has temporal queries at all.

| # | Feature | Type | Priority | Detail |
|---|---------|------|----------|--------|
| 3.1 | `neighbors_as_of(node, timestamp)` | **Differentiated** | P0 | Graph state at a past timestamp — replay events up to cutoff |
| 3.2 | `node_history(node)` | **Differentiated** | P0 | Full audit trail: every create, update, delete with who/when/why |
| 3.3 | `edge_history(edge)` | **Differentiated** | P0 | Same for edges — when was this relationship created/modified? |
| 3.4 | `diff(t1, t2)` | **Differentiated** | P1 | `GraphDiff { nodes_added, nodes_removed, edges_added, edges_removed, properties_changed }` |
| 3.5 | `timeline(node)` | **Differentiated** | P1 | Chronological event stream for an entity, suitable for rendering a timeline UI |
| 3.6 | `graph_at(timestamp)` | **Differentiated** | P2 | Full graph snapshot reconstruction at a point in time |

**Exit criteria**: `node_history` returns correct provenance chain. `neighbors_as_of` matches independently verified past state. `diff` correctly identifies all mutations between two timestamps.

---

## Milestone 4: HTTP API + MCP Tools

**Goal**: Standalone service mode (port 3905) and AI agent integration via MCP.

**Competitive position**: Matches Graphiti/Mem0 on MCP integration, exceeds on self-hosted simplicity (single binary, no external DBs).

| # | Feature | Type | Priority | Detail |
|---|---------|------|----------|--------|
| 4.1 | REST API for all graph operations | Table stakes | P0 | `POST/GET/PATCH/DELETE` for nodes, edges, traversals, temporal queries |
| 4.2 | `kg_add_node` MCP tool | Parity | P0 | Create node via MCP — agent-driven graph construction |
| 4.3 | `kg_add_edge` MCP tool | Parity | P0 | Create edge via MCP |
| 4.4 | `kg_query_neighbors` MCP tool | Parity | P0 | Traversal via MCP |
| 4.5 | `kg_search_nodes` MCP tool | Parity | P0 | Search by type, label, property filter |
| 4.6 | `kg_shortest_path` MCP tool | Parity | P1 | Path finding via MCP |
| 4.7 | `kg_node_history` MCP tool | **Differentiated** | P1 | Audit trail via MCP — no competitor exposes this |
| 4.8 | `McpToolTracker` integration | **Differentiated** | P1 | Auto-emit `mcp.tool.*` events for every KG operation |
| 4.9 | Docker image | Table stakes | P1 | `ghcr.io/all-source-os/chronos-kg` |
| 4.10 | OpenAPI spec | Table stakes | P2 | Full API documentation |

**Exit criteria**: HTTP API passes integration tests. MCP tools work with Claude Code. Docker image starts and serves requests.

---

## Milestone 5: Contradiction Detection + Edge Invalidation

**Goal**: Automatically detect when new information contradicts existing knowledge. Inspired by Graphiti's edge invalidation, but event-sourced.

**Competitive position**: **Parity with Graphiti**, but implemented without LLM calls on the write path.

| # | Feature | Type | Priority | Detail |
|---|---------|------|----------|--------|
| 5.1 | `ContradictionProjection` | Parity | P0 | Detects conflicting edges for same `(subject, predicate)` pair. E.g., "Alice works at X" vs "Alice works at Y" |
| 5.2 | `graph.edge.invalidated` event | Parity | P0 | Emitted when a newer edge supersedes an older one for the same subject-predicate |
| 5.3 | Configurable invalidation rules | **Differentiated** | P1 | Per-relation-type rules: `supersedes` (latest wins), `accumulates` (all valid), `manual` (flag for review) |
| 5.4 | Contradiction audit trail | **Differentiated** | P1 | Full history of what was invalidated, when, and by what evidence |

**Example**:
```rust
// Agent learns new info
kg.add_edge(alice, company_x, "works_at", json!({"since": "2024"})).await?;
// Later, agent learns updated info
kg.add_edge(alice, company_y, "works_at", json!({"since": "2026"})).await?;

// ContradictionProjection detects conflict:
//   "works_at" is configured as `supersedes`
//   → emits graph.edge.invalidated for alice→company_x
//   → alice→company_y is the current valid edge
//   → full audit trail preserved (alice DID work at company_x, now works at company_y)
```

---

## Milestone 6: Schema Enforcement

**Goal**: Validate node and edge types against registered JSON Schemas. Prevents malformed graph data at ingestion time.

**Competitive position**: Table stakes for production use. Graphiti has implicit schemas via Pydantic models; AllSource uses explicit JSON Schema.

| # | Feature | Type | Priority | Detail |
|---|---------|------|----------|--------|
| 6.1 | `register_node_schema(type, json_schema)` | Table stakes | P0 | Define required/optional properties per node type |
| 6.2 | `register_edge_schema(relation, json_schema)` | Table stakes | P0 | Define required/optional properties per edge type |
| 6.3 | Validation on ingest | Table stakes | P0 | Reject malformed nodes/edges with clear error messages |
| 6.4 | Schema evolution | Parity | P1 | Backward/forward/full compatibility modes (reuse Core's schema system) |
| 6.5 | Schema inference from existing data | Parity | P2 | Analyze existing nodes of a type, suggest a JSON Schema |

---

## Milestone 7: Semantic Search + Hybrid Retrieval

**Goal**: Vector embeddings on nodes for similarity queries. Combined with structural traversal for hybrid retrieval — the pattern that Mem0 and Cognee use, but without external vector DBs.

**Competitive position**: **Parity with Mem0/Cognee** on hybrid retrieval, **differentiated** by not requiring a separate vector DB.

| # | Feature | Type | Priority | Detail |
|---|---------|------|----------|--------|
| 7.1 | `embed_node(node, vector)` | Parity | P0 | Attach embedding vector to node (stored as event metadata) |
| 7.2 | `similar_to(node, top_k)` | Parity | P0 | Find semantically similar nodes via cosine distance |
| 7.3 | `vector_search(vector, top_k)` | Parity | P0 | Direct vector query across all embedded nodes |
| 7.4 | Hybrid query: `search(text, relation?, depth?)` | Parity | P1 | Vector similarity + graph traversal in single query |
| 7.5 | Auto-embedding pipeline | Parity | P2 | Optional: call embedding API on node creation, attach vector automatically |
| 7.6 | `context_for(conversation_id, top_k)` | Parity (Mem0) | P2 | Retrieve relevant subgraph for current conversation — semantic + structural |

**Note on 7.5**: Auto-embedding puts an LLM/embedding API on the write path (like Graphiti). This should be optional and async — never block writes on external API calls. Default behavior is explicit `embed_node()` calls.

---

## Milestone 8: Community Detection + Graph Analytics

**Goal**: Identify clusters, compute centrality, detect patterns. Inspired by Graphiti's community nodes (Leiden algorithm).

**Competitive position**: **Parity with Graphiti** on community detection, **differentiated** by incremental updates via projections.

| # | Feature | Type | Priority | Detail |
|---|---------|------|----------|--------|
| 8.1 | `CommunityProjection` | Parity (Graphiti) | P1 | Leiden/Louvain clustering, maintained incrementally as edges arrive |
| 8.2 | `communities()` | Parity | P1 | List detected communities with member nodes |
| 8.3 | `node_community(node)` | Parity | P1 | Which community does this node belong to? |
| 8.4 | PageRank projection | Parity | P2 | Incremental PageRank over graph structure |
| 8.5 | Betweenness centrality | Parity | P2 | Identify bridge nodes connecting communities |
| 8.6 | Temporal community evolution | **Differentiated** | P2 | How did communities change over time? (uses temporal queries from M3) |

---

## Milestone 9: Agent Memory Primitives

**Goal**: Higher-level APIs specifically for AI agent memory use cases. These build on the graph primitives but add agent-specific semantics.

**Competitive position**: **Parity with Mem0/Letta** on memory management, **differentiated** by event-sourced provenance and offline sync.

| # | Feature | Type | Priority | Detail |
|---|---------|------|----------|--------|
| 9.1 | `add_episode(text, source, metadata)` | Parity (Graphiti) | P0 | Batch operation: parse structured input into nodes + edges |
| 9.2 | `memory_decay(strategy)` | Parity (Mem0) | P1 | Configurable relevance decay: exponential, linear, or access-based. Implemented as a `RelevanceProjection` |
| 9.3 | `compact(entity)` | Parity (Letta) | P1 | Agent-triggered merge of redundant/similar nodes into one (emits `graph.node.merged` event) |
| 9.4 | `recall(query, top_k)` | Parity (Mem0) | P1 | Unified recall: combines semantic similarity + structural neighbors + temporal recency |
| 9.5 | Conversation scope | Parity (Mem0) | P1 | Associate graph mutations with a `conversation_id` for session-aware memory |
| 9.6 | User/agent scope | Parity (Mem0) | P2 | Tenant-level isolation for per-user or per-agent memory graphs |
| 9.7 | Memory importance scoring | Parity (Letta) | P2 | Heuristic scoring of memory importance based on access frequency, edge count, recency |

---

## Milestone 10: Batch Import/Export + Interop

**Goal**: Bulk operations and interchange formats for migration from/to other graph systems.

| # | Feature | Type | Priority | Detail |
|---|---------|------|----------|--------|
| 10.1 | `import_batch(nodes, edges)` | Table stakes | P0 | Atomic bulk import with progress reporting |
| 10.2 | CSV/JSON import | Table stakes | P1 | Common interchange formats |
| 10.3 | GraphML export | Parity | P1 | Export subgraph as GraphML (import into Neo4j, Gephi, etc.) |
| 10.4 | Cypher export | Parity | P2 | Export as Cypher CREATE statements |
| 10.5 | Neo4j migration tool | Differentiated | P2 | Import from Neo4j dump files |
| 10.6 | Graphiti migration | Differentiated | P2 | Import episodes/nodes from a Graphiti Neo4j instance |

---

## Milestone 11: Offline Sync for Distributed Graphs

**Goal**: Multiple embedded KG instances build knowledge independently and merge via CRDT sync. This is AllSource's strongest differentiator — no competitor supports this.

**Competitive position**: **Fully differentiated.** No competitor offers offline graph construction with automatic merge.

| # | Feature | Type | Priority | Detail |
|---|---------|------|----------|--------|
| 11.1 | KG-aware merge strategies | **Differentiated** | P0 | Pre-configured CRDT strategies for graph events (FWW for node.created, LWW for node.updated, AppendOnly for edge.created) |
| 11.2 | `kg.sync(remote)` | **Differentiated** | P0 | High-level sync API that wraps `SyncClient` with graph-specific conflict reporting |
| 11.3 | Sync conflict report | **Differentiated** | P1 | `SyncReport { nodes_pushed, nodes_pulled, edges_pushed, edges_pulled, contradictions_detected }` |
| 11.4 | Merge preview | **Differentiated** | P2 | `kg.preview_sync(remote)` — show what would change without applying |
| 11.5 | Selective sync | **Differentiated** | P2 | Sync only specific node types or subgraphs |

---

## Release Mapping

| Release | Milestones | Tagline |
|---------|-----------|---------|
| **v0.13.0** | M1 + M2 | "Embeddable knowledge graph with traversal" |
| **v0.14.0** | M3 + M4 | "Temporal queries + HTTP API + MCP tools" |
| **v0.15.0** | M5 + M6 | "Contradiction detection + schema enforcement" |
| **v0.16.0** | M7 + M8 | "Semantic search + community detection" |
| **v0.17.0** | M9 | "Agent memory primitives" |
| **v0.18.0** | M10 + M11 | "Batch import/export + offline sync" |

---

## Priority Legend

- **P0** — Required for the milestone to ship. Blocks the release.
- **P1** — Should ship with the milestone. Can be deferred to next milestone if needed.
- **P2** — Nice to have. Ship when ready, doesn't block.

## Feature Type Legend

- **Foundation** — Core infrastructure that other features build on
- **Table stakes** — Every graph DB has this. Must have to be taken seriously.
- **Parity** — Matches best-in-class competitor for this specific feature
- **Differentiated** — Only AllSource offers this. Key selling point.

---

## What's NOT on this Roadmap

- **Custom query language** (Cypher, Gremlin, SPARQL) — defer until user demand justifies parser complexity. Rust builder API + REST covers 90% of use cases.
- **Graph neural networks** — out of scope; export to PyG/DGL if needed
- **Graph visualization UI** — frontend concern for dashboard team, not the KG engine
- **Multi-leader graph sync** — single-leader CRDT sync first; multi-leader adds complexity without clear demand
- **LLM-on-write-path entity extraction** — explicitly avoided as a design principle. Agents call structured APIs; we don't second-guess their input with another LLM call.
