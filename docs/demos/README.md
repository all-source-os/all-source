# AllSource Prime — Agent Memory in 60 Seconds

## Setup

```bash
# Build (one time)
cd apps/prime-mcp && cargo build --release
# Binary: apps/prime-mcp/target/release/allsource-prime
```

## Add to Your Agent's MCP Config

```json
{
  "mcpServers": {
    "memory": {
      "command": "/path/to/allsource-prime",
      "args": ["--data-dir", "~/.agent/memory"]
    }
  }
}
```

That's it. No Docker. No API keys. No databases. Your agent now has persistent, structured, searchable memory.

## What Your Agent Gets

**31 tools** — the agent sees these and uses them automatically:

| Tool | When to use |
|------|------------|
| `prime_add_node` | Agent learns about a new entity |
| `prime_add_edge` | Agent discovers a connection |
| `prime_neighbors` | Explore the knowledge graph |
| `prime_search` | Find all entities of a type |
| `prime_shortest_path` | How are two things connected? |
| `prime_forget` | Remove knowledge (reversible) |
| `prime_history` | Full audit trail for any entity |
| `prime_stats` | How much does the agent know? |
| `prime_embed` | Store a vector embedding |
| `prime_similar` | Find semantically similar items |
| `prime_recall` | Hybrid search (vectors + graph + time) |
| `prime_index` | Compressed knowledge summary |
| `prime_context` | Full context retrieval with index |
| `prime_update_node` | A fact about a known entity changes or is corrected |
| `prime_define_type` | Register a node or edge kind with typed properties |
| `prime_list_types` | See registered kinds before extending one |
| `prime_set_enforcement` | Choose `open`, `warn` or `strict` enforcement of kinds |
| `prime_define_projection` | Define how a node's history folds into a snapshot |
| `prime_list_projections` | See the projection definitions already registered |
| `prime_project_node` | Compute a node's current snapshot |
| `prime_node_provenance` | Find where a field's value came from |
| `prime_list_templates` | Browse the bundled entity-type templates |
| `prime_load_template` | Read one template's schema |
| `inbox_recall_thread` | Ground a reply in a thread's interactions and their neighbors |
| `inbox_draft` | Persist a reviewed reply draft as an `email.drafted` event (never sends). Requires sync |
| `hound_ingest` | Extract a codebase into the graph with Tree-sitter (Rust in phase 1). Runs on-device |
| `hound_ingest_docs` | Extract a graph from markdown or text docs. Requires `PRIME_LLM_ENDPOINT` |
| `hound_impact` | Blast radius of a function: transitive callers |
| `hound_report` | Structural summary of the code graph and its hub symbols |
| `hound_pr_impact` | Rank a PR's changed symbols by how many callers depend on them |
| `hound_export` | Export the code graph as cypher, graphml, mermaid, obsidian or wiki |

## Example Agent Session

```
User: "Research CRDTs for me"

Agent uses prime_add_node:
  → Creates node type="concept", properties={"name": "CRDT", "full_name": "Conflict-free Replicated Data Type"}
  ← node_id: "abc-123", entity_id: "node:concept:abc-123"

Agent uses prime_embed:
  → Stores embedding for "CRDTs are data structures that can be replicated across..."
  ← stored

Agent uses prime_add_node:
  → Creates node type="paper", properties={"title": "A comprehensive study of CRDTs", "year": 2011}
  ← node_id: "def-456"

Agent uses prime_add_edge:
  → source="node:paper:def-456", target="node:concept:abc-123", relation="defines"
  ← edge_id: "edge-789"

--- next day ---

User: "What do I know about distributed systems?"

Agent uses prime_context:
  → query="distributed systems", include_index=true
  ← {
      index_excerpt: "## Concepts\n- CRDT (Conflict-free Replicated Data Type)\n  → defined by paper 'A comprehensive study of CRDTs' (2011)\n",
      vectors: [{ id: "abc-123", score: 0.89, text: "CRDTs are data structures..." }],
      nodes: [{ type: "concept", name: "CRDT" }, { type: "paper", title: "A comprehensive study..." }]
    }

Agent: "Based on my research, you know about CRDTs. I found a key paper
        by Shapiro et al. (2011) that defines the concept. I learned about
        this yesterday during our research session."
```

## What Makes This Different

| | zer0dex | Mem0 | Prime |
|---|--------|------|-------|
| **Time-travel** | No | No | `prime_history` — full audit trail, `as_of` queries |
| **Graph** | No | Partial | Native — BFS, Dijkstra, subgraph extraction |
| **Provenance** | No | No | Every mutation is an immutable event |
| **Compressed index** | Manual | No | Auto-generated from graph + LLM |
| **Offline** | Local only | Cloud | Embedded + CRDT sync |
| **Performance** | ~70ms | ~200ms | ~50μs writes, ~12μs queries |
| **Forget** | Destructive | Destructive | Reversible (tombstone event) |

## Data Persistence

All data lives in `~/.agent/memory/`:
```
~/.agent/memory/
  wal/          # Write-ahead log (crash recovery)
  parquet/      # Columnar archive (long-term storage)
  projections/  # Checkpoint snapshots (fast restart)
```

Events survive crashes, restarts, and upgrades. Nothing is ever lost.
