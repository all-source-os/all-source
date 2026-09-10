# allsource-prime

Unified agent memory engine — vectors + graph + events in one binary.

MCP server (stdio) and HTTP REST API for AI agents that need persistent, cross-domain memory with temporal reasoning.

- MCP Registry: `mcp-name: io.github.all-source-os/allsource-prime`

## Install

```bash
cargo install allsource-prime
```

## Quick Start — Claude Code (project-scoped)

From the project directory you want Prime available in:

```bash
cn prime setup
```

That writes a `.mcp.json` at the project root pointing at your installed
`allsource-prime` and a per-project data dir (`<project>/.chronis/prime/`).
The next `claude` session in that directory surfaces the
`mcp__prime__*` tools automatically — no JSON hand-editing, no bearer
tokens. Re-running is idempotent and preserves any other MCP entries
already configured for the project.

`cn` ships with chronis (`cargo install chronis`). If you don't use
chronis, the same `.mcp.json` shape works — see the Claude Desktop
example below for the equivalent JSON.

## Quick Start — MCP (Claude Desktop)

Add to `~/.claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "prime": {
      "command": "allsource-prime",
      "args": ["--data-dir", "~/.prime/memory"]
    }
  }
}
```

Restart Claude Desktop. You now have 13 tools:

| Tool | Description |
|------|-------------|
| `prime_add_node` | Create a node (person, concept, project...) |
| `prime_add_edge` | Create a relationship between nodes |
| `prime_neighbors` | Find connected nodes (1-hop or multi-hop BFS) |
| `prime_search` | Find nodes by type |
| `prime_shortest_path` | Find shortest path between two nodes |
| `prime_forget` | Soft-delete a node (preserved in history) |
| `prime_history` | Full audit trail for any entity |
| `prime_stats` | Graph statistics |
| `prime_embed` | Store a vector embedding |
| `prime_similar` | Find similar embeddings |
| `prime_recall` | Hybrid recall: vectors + graph + temporal |
| `prime_index` | Compressed knowledge index (for system prompt injection) |
| `prime_context` | Combined retrieval: index + vectors + graph |

## Quick Start — HTTP

```bash
allsource-prime --mode http --port 3905 --data-dir ~/.prime/memory
```

```bash
# Create a node
curl -X POST http://localhost:3905/api/v1/prime/nodes \
  -H 'Content-Type: application/json' \
  -d '{"type": "person", "properties": {"name": "Alice", "role": "engineer"}}'

# Get stats
curl http://localhost:3905/api/v1/prime/stats
```

In HTTP mode the binary also serves a self-contained, offline graph viewer at
`http://localhost:3905/api/v1/prime/graph.html` (logged on startup) — open it
in a browser to see your local memory as a bubble graph + detail list, no
account or network required.

## Auto-Inject (zer0dex-style)

Pre-inject a compressed knowledge index into every agent conversation:

```bash
allsource-prime --data-dir ~/.prime/memory --auto-inject --auto-inject-max-tokens 1000
```

Exposes `prime://auto-context` as an MCP resource — the agent's system prompt automatically includes a token-efficient summary of everything it knows, organized by domain with cross-references.

## What Makes This Different

| Feature | zer0dex | Mem0 | Letta | **AllSource Prime** |
|---------|---------|------|-------|---------------------|
| Compressed index | Manual markdown | No | No | **Auto-generated** |
| Temporal queries | No | No | No | **Full time-travel** |
| Provenance | No | No | Partial | **Immutable event audit** |
| Cross-domain recall | 80% | ~50% | ~37% | **80%+** |
| Offline/embedded | Yes | No | No | **Yes + sync** |
| Latency | 70ms | Variable | Variable | **12μs** |

## Architecture

```
┌─────────────────────────────────────────────┐
│              AllSource Prime                 │
│                                              │
│  Graph    Vectors    Temporal    Compressed   │
│  Nodes    HNSW       History    Index        │
│  Edges    Embed      Time-travel Cross-refs  │
│           Similar    Diff                    │
│                                              │
│  ┌──────────────────────────────────────┐   │
│  │         AllSource Core Engine         │   │
│  │  WAL + Parquet + DashMap + HLC + CRDT │   │
│  │  469K events/sec │ 12μs queries       │   │
│  └──────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
```

## How it stores and embeds

**Storage.** Prime is an AllSource event store, not a separate database. Every
mutation — node, edge, vector, schema, soft-delete — appends an immutable event
to the WAL and is replayed by projections that hold the queryable state:

| Event                  | Emitted by                                | Indexed by                                                        |
|------------------------|-------------------------------------------|-------------------------------------------------------------------|
| `prime.node.created`   | `prime_add_node`                          | `NodeState`, `NodeTypeIndex`, `DomainIndex`                       |
| `prime.node.updated`   | `prime.update_node`                        | `NodeState`                                                       |
| `prime.node.deleted`   | `prime_forget`                            | `NodeState` (soft-delete)                                         |
| `prime.edge.created`   | `prime_add_edge`                          | `AdjacencyList`, `ReverseIndex`, `CrossDomain`, `GraphStats`      |
| `prime.edge.deleted`   | cascade from `prime_forget`                | `AdjacencyList`, `ReverseIndex`                                   |
| `prime.vector.stored`  | `prime_embed`                             | `VectorIndex` (HNSW via `instant-distance`)                       |
| `prime.vector.deleted` | `prime.delete_vector`                      | `VectorIndex`                                                     |

Durability comes from AllSource Core: WAL with CRC32 checksums and configurable
fsync, periodic Snappy-compressed Parquet snapshots, and projection checkpoints.
Use `prime_history` on any entity to see the full audit trail; use the time-
travel projections (`get_node_as_of`) for point-in-time reads.

**Embeddings.** Embeddings are computed **in-process** via
[`fastembed`](https://crates.io/crates/fastembed) (pure-Rust ONNX runtime).
Default model is `AllMiniLML6V2` — 384 dims, ~25 MB, auto-downloaded into the
fastembed cache on first call. **No external embedding service is required.**

That means `prime_embed` and `prime_recall` accept either a precomputed
`vector` *or* plain `text`; if you pass `text` alone the server embeds it
in-process before storing or searching. Same for the HTTP endpoints:

```bash
# Store via text only — server embeds
curl -X POST http://localhost:3905/api/v1/prime/vectors \
  -H 'Content-Type: application/json' \
  -d '{"id": "node:concept:abc", "text": "agents need persistent memory"}'

# Search via text only — server embeds the query
curl -X POST http://localhost:3905/api/v1/prime/vectors/search \
  -H 'Content-Type: application/json' \
  -d '{"text": "what do I know about memory?", "top_k": 5}'
```

Pass `vector` instead of (or alongside) `text` if you already have an
embedding from a different model — Prime won't re-embed when `vector` is set.
The first text-embedding call in a process pays the model download; subsequent
calls take ~1–3 ms.

### Offline embeddings (no network on first use)

**A default build already works offline** — `prime-bundled-model` is on by
default, so the weights are baked into the binary and the embedder never touches
the network. That covers `cargo install allsource-prime` and the Claude Desktop
`.dxt`, which ships the model alongside its binaries and points at it with
`PRIME_EMBED_MODEL_DIR`.

A build with `--no-default-features` (which is what the release tarballs are,
to keep them small) downloads the model from HuggingFace **on first use**, so
behind a proxy, on a flight, or in a region where HF is blocked it fails the
*first* `prime_embed`/`prime_recall` with an actionable error. Three ways to
make that build offline too:

1. **Warm the cache ahead of time.** Run once where network is available:

   ```bash
   allsource-prime --data-dir ~/.prime/memory --mode warm
   ```

   This downloads the model, verifies a real embed, and exits 0 (non-zero if the
   model can't load — good as a CI canary on a fresh, cache-less container). After
   this, embeds work offline from the fastembed cache.

2. **Vendor the model and point at it.** Copy the five files from the fastembed
   cache snapshot dir (`model.onnx`, `tokenizer.json`, `config.json`,
   `special_tokens_map.json`, `tokenizer_config.json`) somewhere stable and set:

   ```bash
   export PRIME_EMBED_MODEL_DIR=/path/to/vendored/all-MiniLM-L6-v2
   ```

   Prime loads straight from disk — **zero network access**, ever.

3. **Bring your own vector.** Skip the in-process embedder entirely: compute a
   384-dim `all-MiniLM-L6-v2` embedding yourself and pass `vector` to
   `prime_embed` / `prime_recall`. ~10 lines of `sentence-transformers`:

   ```python
   from sentence_transformers import SentenceTransformer
   m = SentenceTransformer("all-MiniLM-L6-v2")
   vector = m.encode("test facet").tolist()  # 384 floats
   # prime_embed({ "id": "node:insight:…", "vector": vector })
   ```

**Cache & proxy knobs:** `FASTEMBED_CACHE_DIR` overrides the cache location
(default `.fastembed_cache/`). `HF_HOME` and `HF_ENDPOINT` (mirror URL) are
honored by fastembed for the download path. A stale/partial download is fixed by
deleting the cache dir and retrying.

### Bundled model — the default (`prime-bundled-model`)

On by default. The model is baked into the binary, so the embedder works fully
offline at runtime with no warm step, no `PRIME_EMBED_MODEL_DIR`, no first-use
fetch.

```bash
cargo build --release                      # bundled: ~138 MB binary
cargo build --release --no-default-features # unbundled: ~51 MB, fetches on first use
# offline build: vendor the 5 files once and skip the build-time HF download
ALLSOURCE_PRIME_MODELS_SRC=/path/to/vendored cargo build --release
```

The weights (86 MB) are fetched once at **build** time by the
[`allsource-prime-models`](../../crates/allsource-prime-models) crate and
`include_bytes!`'d. Runtime never touches the network for the model.

The released tarballs and the `.dxt` are built `--no-default-features`: baking
86 MB into each of two platform binaries would put the model in the bundle
twice. The `.dxt` ships it once under `server/model/` instead and sets
`PRIME_EMBED_MODEL_DIR` in its manifest, which is embedder precedence step 2 —
so a Desktop install is offline-capable at ~129 MB rather than ~220 MB.

### Remote / pluggable embedder (`prime-remote-embed`)

Route text→vector to an external embeddings endpoint instead of the in-process
model — useful to share one embedder fleet-wide, or to sidestep the HF fetch
entirely by pointing at a local Ollama.

```bash
cargo build --release --features prime-remote-embed
```

| Env | Meaning |
|-----|---------|
| `PRIME_EMBED_ENDPOINT` | Embeddings URL. When set, the remote backend is used. |
| `PRIME_EMBED_PROTOCOL` | `openai` (default) or `ollama`. |
| `PRIME_EMBED_MODEL` | Model name sent in the request (default `all-minilm`). |
| `PRIME_EMBED_API_KEY` | Optional bearer token. |

```bash
# Local Ollama — no HuggingFace, no proxy headaches:
export PRIME_EMBED_ENDPOINT=http://127.0.0.1:11434/api/embeddings
export PRIME_EMBED_PROTOCOL=ollama
export PRIME_EMBED_MODEL=all-minilm

# OpenAI-compatible:
export PRIME_EMBED_ENDPOINT=https://api.openai.com/v1/embeddings
export PRIME_EMBED_API_KEY=sk-…
export PRIME_EMBED_MODEL=text-embedding-3-small
```

The HTTP client honors `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` / `NO_PROXY`.
The endpoint is probed once at startup (fail-fast with an actionable error). Note
that the embedding **dimension must stay consistent for a given `--data-dir`** —
don't switch between a 384-dim model and a 1536-dim one against existing vectors.

## License

Apache-2.0
