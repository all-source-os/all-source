# Embedded Prime Integration Guide

How to use AllSource Prime as an embedded library for agent memory — graph, vectors, and tiered recall — without running a separate server.

## Table of Contents

1. [Feature Flags](#feature-flags)
2. [Initialization](#initialization)
3. [Migration from EmbeddedCore](#migration-from-embeddedcore)
4. [VectorIndexProjection Usage](#vectorindexprojection-usage)
5. [Local Embeddings with fastembed](#local-embeddings-with-fastembed)
6. [RecallEngine and Tiered Retrieval](#recallengine-and-tiered-retrieval)
7. [Hybrid Recall (Graph + Vectors)](#hybrid-recall-graph--vectors)
8. [Configuration Reference](#configuration-reference)

---

## Feature Flags

Add `allsource-core` with the features you need:

```toml
[dependencies]
allsource-core = { version = "0.30.2", features = ["prime-full", "prime-recall"] }
```

| Feature | What it enables | Compile cost |
|---------|----------------|-------------|
| `prime` | Graph engine (nodes, edges, schema, projections) | Low |
| `prime-vectors` | HNSW vector index + `fastembed` + `instant-distance` | Medium (fastembed model download on first run) |
| `prime-full` | `prime` + `prime-vectors` | Medium |
| `prime-recall` | RecallEngine with L0/L1/L2 tiers + index compression | Low (adds `reqwest` for optional Ollama LLM) |
| `vector-search` | Standalone vector search (used by `prime-vectors` internally) | Medium |

For a Tauri app replacing O(n) cosine search with HNSW + tiered recall:

```toml
allsource-core = { version = "0.30.2", features = ["prime-full", "prime-recall"] }
```

---

## Initialization

### `Prime::open(path)` — Durable, persistent

```rust
use allsource_core::prime::Prime;

let prime = Prime::open("/path/to/app/data/prime").await?;

// All data persists across restarts via WAL + Parquet.
// Graph projections are rebuilt from WAL on startup.
```

In a Tauri app:

```rust
use allsource_core::prime::Prime;
use std::sync::Arc;
use tauri::Manager;

#[tauri::command]
async fn init(app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let prime_dir = data_dir.join("prime");
    let prime = Prime::open(&prime_dir).await.map_err(|e| e.to_string())?;
    app.manage(Arc::new(prime));
    Ok(())
}
```

### `Prime::open_in_memory()` — Testing only

```rust
let prime = Prime::open_in_memory().await?;
// No persistence. Data lost on drop.
```

### Graceful shutdown

```rust
prime.shutdown().await?; // Flushes WAL, Parquet checkpoint
```

---

## Migration from EmbeddedCore

### Architecture

Prime **wraps** EmbeddedCore — it is not a replacement. When you call `Prime::open()`, it internally creates an `EmbeddedCore` with graph-specific merge strategies and registers 8+ graph projections on it.

```
Prime
 └── EmbeddedCore (accessible via prime.core())
      └── EventStore (WAL + Parquet + DashMap)
           └── ProjectionManager
                ├── 8 graph projections (node_state, adjacency, etc.)
                ├── 1 vector projection (if prime-vectors enabled)
                └── your domain projections (registered separately)
```

### Can I wrap my existing `EmbeddedCore` instance?

**No.** `Prime::from_core(core)` exists internally but is **private** — it
is not part of the public API. `Prime::open(path)` always constructs a
fresh `EmbeddedCore` with graph-specific merge strategies and registers
Prime's projections on it. If you have an existing `EmbeddedCore` you
want to reuse, you have two options:

- **Option A (recommended)** — replace your `EmbeddedCore::open` call with
  `Prime::open` and access the underlying core via `prime.core()` for
  any raw event operations you still need.
- **Option C (isolated)** — keep your existing `EmbeddedCore` for domain
  events and run a separate `Prime` instance in a different data
  directory for agent memory. You can't share an event stream this way,
  but the two instances are fully independent.

### Option A: Prime wraps a fresh Core (recommended for new projects)

```rust
use allsource_core::embedded::{IngestEvent, Query};

let prime = Prime::open(&data_dir).await?;

// Access the underlying EmbeddedCore for raw event operations:
let core = prime.core();
core.ingest(IngestEvent {
    entity_id: "workflow:123",
    event_type: "workflow.started",
    payload: serde_json::json!({ "status": "running" }),
    metadata: None,
    tenant_id: None,
}).await?;
let events = core.query(Query::new().entity_id("workflow:123")).await?;
```

### Option B: Register domain projections alongside Prime's

If you have existing domain projections (e.g., 39 projections in Longhand), register them on the same underlying store:

```rust
use allsource_core::application::services::projection::Projection;

let prime = Prime::open(&data_dir).await?;
let store = prime.core().inner(); // Arc<EventStore>

// Register your existing domain projections
let my_proj = Arc::new(MyWorkflowProjection::new());
store.register_projection_with_backfill(
    &(Arc::clone(&my_proj) as Arc<dyn Projection>)
);

// Both Prime graph projections and your domain projections
// now process the same event stream.
```

A projection that persists its own state can resume instead of replaying the
whole history on every boot. Store `BackfillPosition::of(&event)` for the last
event it applied, next to its state, and pass it back on the next start:

```rust
use allsource_core::BackfillPosition;

let after: Option<BackfillPosition> = load_checkpoint()?; // your storage
store.register_projection_with_backfill_from_for(
    &(Arc::clone(&activity_log) as Arc<dyn Projection>),
    after,
    &["activity.logged"],
)?;
```

Only matching events ordered after `after` by `(timestamp, version, id)` are
replayed. Events ingested while the backfill runs are delivered once, after it.
The type filter applies to the backfill only; live events still reach
`process()` unfiltered.

### Option C: Separate data directories

If you want complete isolation between your domain events and Prime's graph events:

```rust
// Your existing EmbeddedCore for domain events
let domain_core = EmbeddedCore::open(
    Config::builder().data_dir(&domain_dir).build()?
).await?;

// Separate Prime instance for agent memory
let prime = Prime::open(&prime_dir).await?;
```

This keeps event streams separate but means you can't query across them in a single call.

### What changes from raw EmbeddedCore

| Before (EmbeddedCore) | After (Prime) |
|----------------------|--------------|
| `core.ingest(event)` | `prime.add_node(type, props)` / `prime.add_edge(...)` |
| Manual O(n) cosine search | `prime.vector_search(query, top_k)` — HNSW O(log n) |
| Custom projection for entity state | `prime.get_node(id)` — O(1) via NodeStateProjection |
| No graph traversal | `prime.neighbors(id, depth)` — BFS with direction filter |
| No recall API | `recall.context(query)` — tiered retrieval |

---

## VectorIndexProjection Usage

### Storing embeddings

Prime stores embeddings as events. The vector goes in event metadata; text and user metadata go in the payload.

```rust
// Generate embedding externally (fastembed, OpenAI, Ollama, etc.)
let embedding: Vec<f32> = generate_embedding("skill output text")?;

// Store in Prime
prime.embed("output-42", Some("skill output text"), embedding).await?;

// With metadata
prime.embed_with_metadata(
    "output-42",
    Some("skill output text"),
    embedding,
    Some(serde_json::json!({
        "project_id": "proj-1",
        "skill": "summarize",
        "created_at": "2026-03-26T12:00:00Z"
    })),
).await?;
```

**Event stored:** `prime.vector.stored` with entity ID `vec:output-42`.

### Querying by similarity

```rust
// Direct vector search — returns top K by cosine similarity
let query_vec: Vec<f32> = generate_embedding("revenue analysis")?;
let results = prime.vector_search(&query_vec, 5);

for result in &results {
    println!("{}: score={:.3} text={:?}", result.id, result.score, result.text);
}
```

### Find similar to an existing embedding

```rust
// Find the 5 most similar vectors to a stored embedding
let similar = prime.similar("output-42", 5)?;
```

### Cross-domain search

Over-fetches 3x, groups by domain, round-robin interleaves so results span multiple domains:

```rust
let results = prime.vector_search_cross_domain(&query_vec, 5);
```

### How the HNSW index works

- **Batch rebuild, not incremental** — the `instant-distance` crate is build-once. The index is lazily rebuilt when a search is performed and the generation counter has advanced.
- **Generation counter** — avoids TOCTOU races. Each `embed()` / delete increments the counter; `search()` checks if it needs to rebuild.
- **Cosine distance** — `distance = 1.0 - cosine_similarity`. Results are converted back to similarity scores (0.0–1.0).
- **Config** — `ef_construction: 100`, `ef_search: 100` (defaults). Higher values = more accurate but slower.

---

## Local Embeddings with fastembed

The `vector-search` feature (pulled in by `prime-vectors`) includes `fastembed` — a pure-Rust embedding library with no Python dependency.

### Using the built-in VectorSearchEngine

```rust
use allsource_core::infrastructure::search::vector_search_engine::{
    VectorSearchEngine, VectorSearchEngineConfig,
};

let config = VectorSearchEngineConfig {
    model_name: "AllMiniLML6V2".to_string(), // 384 dimensions
    embedding_dimensions: 384,
    ..Default::default()
};

// Note: constructor is `with_config(config)`, not `new(config)`.
// `VectorSearchEngine::new()` takes no arguments and uses defaults.
let engine = VectorSearchEngine::with_config(config)?;

// Single-shot embedding. `embed_text` takes `&str` and returns `Vec<f32>`.
let embedding: Vec<f32> = engine.embed_text("skill output text")?;
```

### Direct fastembed usage

```rust
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

// fastembed 4.x uses a builder pattern, not a struct literal. Note the
// enum variant name is `AllMiniLML6V2` (all caps LM), not AllMiniLmL6V2.
let model = TextEmbedding::try_new(
    InitOptions::new(EmbeddingModel::AllMiniLML6V2)
        .with_show_download_progress(true),
)?;

// Batch embedding — returns `Vec<Vec<f32>>`.
let embeddings = model.embed(vec!["skill output text"], None)?;
// embeddings[0] is Vec<f32> with 384 dimensions
```

### Model selection for short-form content (100–500 chars)

The enum variants are re-exported from `fastembed::EmbeddingModel`; use
them directly rather than passing string names (the string table is only
used by `VectorSearchEngineConfig.model_name` for serialization).

| Model variant | Dimensions | Speed | Quality for short text | Cache size |
|---------------|-----------|-------|----------------------|-----------|
| **`AllMiniLML6V2`** (default) | 384 | Fast | Good | ~23 MB |
| `AllMiniLML12V2` | 384 | Medium | Better | ~33 MB |
| `BGESmallENV15` | 384 | Fast | Good | ~33 MB |

**Recommendation for skill outputs (100–500 chars):** `AllMiniLML6V2` is the default and a good fit. It's optimized for short passages, fast enough for real-time use, and the 384-dimension vectors keep memory usage low.

The model files are downloaded on first use and cached in `.fastembed_cache/` (excluded from git via Core's Cargo.toml).

### Replacing your O(n) cosine search

Before:
```rust
// O(n) brute force over ALL outputs
let query_embedding = embedding::generate_embedding(skill_prompt, &embed_config).await?;
for output in all_outputs {
    let similarity = cosine_similarity(&query_embedding, &output.embedding);
    candidates.push((similarity, output.content));
}
candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
candidates.truncate(5);
```

After:
```rust
// O(log n) HNSW search. fastembed uses a builder pattern (see note above).
let model = TextEmbedding::try_new(
    InitOptions::new(EmbeddingModel::AllMiniLML6V2),
)?;
let query_vec = model.embed(vec![skill_prompt], None)?.remove(0);
let results = prime.vector_search(&query_vec, 5);
```

---

## RecallEngine and Tiered Retrieval

The RecallEngine provides three retrieval tiers with increasing cost and context size.

### Setup

```rust
use allsource_core::prime::recall::{RecallEngine, RecallContextQuery, RecallDeps};
use allsource_core::prime::recall::types::{ContextTier, IndexConfig};

// Preferred: share Prime's projections (enables L0/L1 tiers)
let recall = RecallEngine::with_deps(prime.recall_deps(), &IndexConfig::default());
```

### L0: Stats only (~100–200 tokens)

O(1) retrieval. Returns graph statistics — total nodes, edges, types, domains.

```rust
let ctx = recall.context(RecallContextQuery {
    query: "what do I know?".to_string(),
    tier: ContextTier::L0,
    ..Default::default()
}).await;

// ctx.stats — Some(PrimeStats { total_nodes, total_edges, ... })
// ctx.token_count — ~100-200
```

Use case: orientation queries, "do I have any data about X?"

### L1: Conversation context (~500–1500 tokens)

Stats + recent conversation-scoped nodes + 1-hop edges. No vector search.

```rust
let ctx = recall.context(RecallContextQuery {
    query: "what was that project name?".to_string(),
    tier: ContextTier::L1,
    conversation_id: Some("conv-abc".to_string()),
    ..Default::default()
}).await;

// ctx.stats + ctx.nodes (recent from this conversation) + ctx.edges (1-hop)
```

Use case: follow-up questions within the same session.

### L2: Compressed index (~1000–2000 tokens) — current behaviour

L2 is the default tier and produces a compressed markdown summary of
the graph — domain counts, type distributions, example entities, and
cross-references. It is generated by a deterministic heuristic unless
you configure an LLM backend via `IndexConfig.llm_endpoint`.

```rust
let ctx = recall.context(RecallContextQuery {
    query: "revenue analysis across projects".to_string(),
    tier: ContextTier::L2,  // default
    top_k: 5,
    include_index: true,
    ..Default::default()
}).await;

// ctx.index     — compressed markdown summary of all knowledge
// ctx.stats     — graph statistics
// ctx.vectors   — currently empty (see note below)
// ctx.nodes     — currently empty (see note below)
// ctx.edges     — currently empty (see note below)
// ctx.token_count — ~1000–2000
```

> ⚠️ **L2 does not currently fan out to vector search or graph
> expansion** — see the `TODO: vector search integration` in
> `apps/core/src/prime/recall/api.rs::context_l2`. Today L2 returns
> the compressed markdown index plus stats. If you need vector
> similarity results or graph-expanded neighbours as part of the same
> call, call `Prime::recall(RecallQuery)` directly — see
> [Hybrid Recall](#hybrid-recall-graph--vectors) below. A future
> release will wire L2 through `Prime::vector_search` so that
> `RecallContextQuery` returns both the compressed index and the hit
> list in one call.

If you want the compressed index as a drop-in replacement for your
O(n) cosine loop's formatted output, L2 works today:

```rust
// Replaces ~70 lines of brute-force cosine search with a single call
// that returns a markdown summary instead of raw hits. For ranked
// hits, use `prime.vector_search(&query_vec, 5)` or
// `prime.recall(RecallQuery { vector: Some(query_vec), .. })` —
// see the section below.
let ctx = recall.context(RecallContextQuery {
    query: skill_prompt.to_string(),
    tier: ContextTier::L2,
    top_k: 5,
    ..Default::default()
}).await;
let context_string = ctx.index;
```

### With LLM-powered index compression (optional)

The compressed index can be generated by a heuristic (default) or an LLM (Ollama):

```rust
let config = IndexConfig {
    max_tokens: 1000,
    llm_endpoint: Some("http://localhost:11434/api/generate".to_string()),
    llm_model: Some("mistral:7b".to_string()),
    refresh_interval_events: 100,  // Rebuild every 100 events
    refresh_interval_seconds: 300, // Or every 5 minutes
};

let recall = RecallEngine::with_deps(prime.recall_deps(), &config);
```

Without an LLM endpoint, the index is a deterministic heuristic markdown summary — domain counts, type distributions, example entities, cross-references. This works well and has zero latency.

---

## Hybrid Recall (Graph + Vectors)

For full hybrid scoring that combines cosine similarity, graph proximity, and temporal recency:

```rust
use allsource_core::prime::types::RecallQuery;

let result = prime.recall(RecallQuery {
    vector: Some(query_vec),
    text: Some("revenue analysis".to_string()),
    depth: 1,           // BFS expansion depth from vector hits
    top_k: 10,
    similarity_weight: 0.5,
    proximity_weight: 0.3,
    recency_weight: 0.2,
    ..RecallQuery::default()
}).await?;

// result.nodes — scored and ranked by hybrid score
// result.vectors — raw vector search results
// result.edges — edges discovered during graph expansion
```

**Scoring formula:**
```
score = sw * cosine_similarity + pw * 1/(1 + depth) + rw * exp_decay(age)
```

Weights are normalized to sum to 1.0. Vector hits seed the search at depth 0 (max proximity), then BFS expands through the graph.

---

## Configuration Reference

### IndexConfig

| Field | Default | Description |
|-------|---------|-------------|
| `max_tokens` | 1000 | Token budget for compressed index (adaptive 500–2000) |
| `llm_endpoint` | `None` | Ollama endpoint for LLM compression (e.g., `http://localhost:11434/api/generate`) |
| `llm_model` | `None` | LLM model name (e.g., `mistral:7b`). Falls back to `"mistral"` |
| `refresh_interval_events` | 100 | Rebuild index every N new events |
| `refresh_interval_seconds` | 300 | Rebuild index every M seconds |

### VectorIndexConfig

| Field | Default | Description |
|-------|---------|-------------|
| `ef_construction` | 100 | HNSW build-time accuracy. Higher = better recall, slower build |
| `ef_search` | 100 | HNSW search-time accuracy. Higher = better recall, slower search |

### RecallContextQuery

| Field | Default | Description |
|-------|---------|-------------|
| `query` | `""` | Natural language query string |
| `agent_id` | `None` | Scope to a specific agent |
| `top_k` | 5 | Max vector results |
| `as_of` | `None` | Time-travel: only knowledge that existed at this timestamp |
| `include_index` | `true` | Include compressed index in response |
| `max_tokens` | `None` | Truncate response to this token budget |
| `tier` | `L2` | Retrieval tier: `L0`, `L1`, or `L2` |
| `conversation_id` | `None` | Scope L1 to a specific conversation |

### RecallQuery (hybrid recall)

| Field | Default | Description |
|-------|---------|-------------|
| `vector` | `None` | Query embedding vector |
| `text` | `None` | Query text |
| `depth` | 1 | BFS expansion depth from vector hits |
| `top_k` | 10 | Number of results |
| `similarity_weight` | 0.5 | Weight for cosine similarity |
| `proximity_weight` | 0.3 | Weight for graph proximity |
| `recency_weight` | 0.2 | Weight for temporal recency |

---

## Complete Example: Tauri App with Prime

```rust
use allsource_core::prime::Prime;
use allsource_core::prime::recall::{RecallContextQuery, RecallEngine};
use allsource_core::prime::recall::types::{ContextTier, IndexConfig};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::Manager;

/// App state shared across commands. Prime holds its own Arc internally so
/// it's cheap to clone; `TextEmbedding` is not `Clone` and its `embed`
/// call takes `&mut self`, so wrap it in a Mutex.
struct AppState {
    prime: Prime,
    recall: RecallEngine,
    embedder: Mutex<TextEmbedding>,
}

async fn build_state(data_dir: std::path::PathBuf) -> Result<AppState, Box<dyn std::error::Error>> {
    let prime = Prime::open(data_dir.join("prime")).await?;
    let recall = RecallEngine::with_deps(prime.recall_deps(), &IndexConfig::default());
    let embedder = TextEmbedding::try_new(
        InitOptions::new(EmbeddingModel::AllMiniLML6V2),
    )?;
    Ok(AppState {
        prime,
        recall,
        embedder: Mutex::new(embedder),
    })
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    // Tauri's `setup` runs on a blocking thread — use `block_on` on the
    // current runtime handle.
    let state = tokio::runtime::Handle::current().block_on(build_state(data_dir))?;
    app.manage(Arc::new(state));
    Ok(())
}

/// Ingest a skill output with an embedding.
#[tauri::command]
async fn store_output(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    text: String,
    project_id: String,
) -> Result<(), String> {
    // Scope the lock so we drop it before the await.
    let embedding = {
        let mut embedder = state.embedder.lock();
        embedder
            .embed(vec![text.as_str()], None)
            .map_err(|e| e.to_string())?
            .remove(0)
    };

    state
        .prime
        .embed_with_metadata(
            &id,
            Some(&text),
            embedding,
            Some(serde_json::json!({ "project_id": project_id })),
        )
        .await
        .map_err(|e| e.to_string())
}

/// Retrieve cross-project context — uses `Prime::recall` to get both the
/// compressed index (via `RecallEngine`) and ranked vector hits in one
/// place, since `RecallEngine::context(L2)` does not yet fan out to
/// `vector_search` (see the L2 section above).
#[tauri::command]
async fn retrieve_context(
    state: tauri::State<'_, Arc<AppState>>,
    skill_prompt: String,
) -> Result<String, String> {
    // 1. Embed the prompt.
    let query_vec = {
        let mut embedder = state.embedder.lock();
        embedder
            .embed(vec![skill_prompt.as_str()], None)
            .map_err(|e| e.to_string())?
            .remove(0)
    };

    // 2. Compressed markdown summary of the graph (L2).
    let ctx = state
        .recall
        .context(RecallContextQuery {
            query: skill_prompt.clone(),
            tier: ContextTier::L2,
            top_k: 5,
            ..Default::default()
        })
        .await;

    // 3. Ranked hits via HNSW — O(log n) instead of O(n).
    let hits = state.prime.vector_search(&query_vec, 5);
    let hit_summary = hits
        .iter()
        .map(|h| {
            format!(
                "- ({:.3}) {}",
                h.score,
                h.text.as_deref().unwrap_or(&h.id)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!("{}\n\n## Relevant outputs\n{}", ctx.index, hit_summary))
}
```

This replaces ~70 lines of brute-force O(n) cosine similarity with
O(log n) HNSW search, eliminates the external embedding API dependency,
and adds a compressed-index summary of the overall graph. When the L2
fan-out is wired up (see the L2 section), the explicit
`prime.vector_search` call above will collapse back into a single
`recall.context(...)` call.
