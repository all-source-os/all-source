//! MCP tool definitions and dispatch for Prime.
//!
//! Each tool maps to a Prime facade method. Tool descriptions are written for
//! AI agent consumption — they explain *when* to use each tool.

use std::sync::OnceLock;

use allsource_core::prime::{Prime, recall::RecallEngine, types::Retrieval};
use serde_json::{Value, json};

use crate::wire::{Fields, Page, node_row, paged};

/// Encoding for the text payload of MCP tool results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultFormat {
    /// Pretty-printed JSON.
    Json,
    /// TOON — compact, token-efficient (see the `toon` module).
    Toon,
}

static RESULT_FORMAT: OnceLock<ResultFormat> = OnceLock::new();

/// Set the tool-result payload format. Called once at startup; later calls
/// are ignored. Defaults to [`ResultFormat::Json`] if never set.
pub fn set_result_format(fmt: ResultFormat) {
    let _ = RESULT_FORMAT.set(fmt);
}

fn result_format() -> ResultFormat {
    RESULT_FORMAT.get().copied().unwrap_or(ResultFormat::Json)
}

/// Whether the running process is shipping `prime.*` events to a remote
/// `AllSource` tenant (so the web dashboard's Memory tab can see them).
///
/// Recorded once at startup by `main` and surfaced through `prime_stats` so an
/// agent — and the human reading its output — can tell at a glance whether
/// memory is local-only or actually reaching the hosted dashboard. The whole
/// point is that a silent local-only mode is no longer possible to mistake for
/// a working sync.
#[derive(Clone, Debug)]
pub struct SyncStatus {
    /// `true` when `--sync-to` + `--api-key` were supplied and the push loop
    /// is running.
    pub enabled: bool,
    /// Remote base URL events are shipped to, when sync is enabled.
    pub remote_url: Option<String>,
}

static SYNC_STATUS: OnceLock<SyncStatus> = OnceLock::new();

/// Record the resolved sync status. Called once at startup; later calls are
/// ignored.
pub fn set_sync_status(status: SyncStatus) {
    let _ = SYNC_STATUS.set(status);
}

fn sync_status() -> SyncStatus {
    SYNC_STATUS.get().cloned().unwrap_or(SyncStatus {
        enabled: false,
        remote_url: None,
    })
}

/// Build the `sync` object embedded in `prime_stats` output. Mirrors the
/// startup log so an agent can read back whether its writes will appear in the
/// `AllSource` dashboard. Also reused by the HTTP `/api/v1/prime/stats` handler.
pub fn sync_status_json() -> Value {
    let status = sync_status();
    if status.enabled {
        json!({
            "enabled": true,
            "remote_url": status.remote_url,
            "note": "prime.* events are syncing to your AllSource tenant — \
                     writes will appear on the dashboard Memory tab.",
        })
    } else {
        json!({
            "enabled": false,
            "remote_url": Value::Null,
            "note": "LOCAL-ONLY: writes are NOT syncing to AllSource and will \
                     NOT appear on the dashboard Memory tab. Relaunch prime with \
                     --sync-to https://api.all-source.xyz and --api-key <key> to enable sync.",
        })
    }
}

/// Return MCP tool definitions (for `tools/list`).
pub fn tool_definitions() -> Value {
    json!([
        // ─── Graph CRUD ─────────────────────────────────────────────
        {
            "name": "prime_add_node",
            "description": "Create a node in the knowledge graph. Use whenever you learn a new fact, meet a new entity, or discover a concept. Always include a 'domain' property to enable cross-domain reasoning. Pair with prime_embed to make the node searchable by meaning, and prime_add_edge to connect it to existing knowledge.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "type": { "type": "string", "description": "Node type: 'person', 'concept', 'project', 'metric', 'decision', 'event', 'insight'" },
                    "properties": { "type": "object", "description": "Node data. Always include 'name' and 'domain'. Example: {\"name\": \"Alice\", \"role\": \"engineer\", \"domain\": \"engineering\"}" }
                },
                "required": ["type", "properties"]
            }
        },
        {
            "name": "prime_add_edge",
            "description": "Create a directed relationship between two nodes. Use when you discover how entities connect: causes, depends_on, works_on, impacts, requires. Cross-domain edges (connecting nodes in different domains) are especially valuable — they power the compressed index's cross-reference section.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Source node entity_id (format: node:{type}:{id})" },
                    "target": { "type": "string", "description": "Target node entity_id" },
                    "relation": { "type": "string", "description": "Relationship type: 'works_on', 'impacts', 'requires', 'depends_on', 'causes', 'authored', 'manages'" },
                    "properties": { "type": "object", "description": "Optional edge properties (e.g. {\"since\": \"2026-01\"})" },
                    "weight": { "type": "number", "description": "Optional confidence/strength (0.0-1.0)" }
                },
                "required": ["source", "target", "relation"]
            }
        },
        // ─── Graph Queries ───────────────────────────────────────────
        {
            "name": "prime_neighbors",
            "description": "Explore the graph around a node. Use to find what's connected to an entity: who works on a project, what a person manages, what depends on a service. Set depth > 1 for multi-hop exploration (e.g. 'who are the teammates of Alice's manager?'). Prefer this over prime_search when you already know a starting node.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "node_id": { "type": "string", "description": "Starting node entity_id" },
                    "relation": { "type": "string", "description": "Filter to edges of this type only" },
                    "direction": { "type": "string", "enum": ["incoming", "outgoing", "both"], "description": "Edge direction (default: both)" },
                    "depth": { "type": "integer", "description": "BFS depth: 1 = immediate neighbors, 2+ = multi-hop (default: 1)" },
                    "limit": { "type": "integer", "description": "Max rows to return (default: 50, max: 500)" },
                    "offset": { "type": "integer", "description": "Rows to skip — use the 'next_offset' from the previous call (default: 0)" },
                    "fields": { "type": "string", "enum": ["summary", "full"], "description": "'summary' returns {id, type, name, at}; 'full' (default at depth 1) returns every property. Depth > 1 defaults to 'summary' because the frontier grows fast." }
                },
                "required": ["node_id"]
            }
        },
        {
            "name": "prime_search",
            "description": "List nodes of a given type. Use for broad queries like 'list all projects' or 'show me every person'. For semantic queries ('find things related to X'), use prime_recall instead. Returns at most 50 rows by default and reports the true 'total' — on a code graph a type can hold thousands, so read 'total' and page with 'offset' rather than raising 'limit'. Each row's 'id' is the entity_id every other tool accepts, so drill into a row with prime_neighbors or prime_history instead of asking for 'fields: full' over the whole type.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "type": { "type": "string", "description": "Node type to search for (e.g. 'person', 'project')" },
                    "limit": { "type": "integer", "description": "Max rows to return (default: 50, max: 500)" },
                    "offset": { "type": "integer", "description": "Rows to skip — use the 'next_offset' from the previous call (default: 0)" },
                    "fields": { "type": "string", "enum": ["summary", "full"], "description": "'summary' (default) returns {id, type, name, at}; 'full' returns every property and is much larger" }
                },
                "required": ["type"]
            }
        },
        {
            "name": "prime_shortest_path",
            "description": "Find how two entities are connected through the graph. Returns the chain of nodes linking them. Use to answer 'how does X relate to Y?' when you need the specific path of relationships.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Start node entity_id" },
                    "to": { "type": "string", "description": "End node entity_id" },
                    "relation": { "type": "string", "description": "Restrict path to this relation type only" }
                },
                "required": ["from", "to"]
            }
        },
        // ─── Memory Lifecycle ────────────────────────────────────────
        {
            "name": "prime_forget",
            "description": "Soft-delete a node and all its edges. The node becomes invisible to queries but its full history is preserved — use prime_history to see what was forgotten and why. Use when knowledge is outdated or incorrect. Prefer updating over forgetting when the entity still exists but facts changed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "node_id": { "type": "string", "description": "Node entity_id to forget" }
                },
                "required": ["node_id"]
            }
        },
        {
            "name": "prime_history",
            "description": "Get the complete audit trail for any entity: every creation, update, and deletion with timestamps. Use to answer 'when did I learn this?', 'what changed?', or 'who was responsible before?' Returns events in chronological order.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity_id": { "type": "string", "description": "Any entity_id: node, edge, or vector" }
                },
                "required": ["entity_id"]
            }
        },
        {
            "name": "prime_stats",
            "description": "Quick overview of memory state: total nodes, edges, types, relations, AND sync status. Call this at conversation start to orient yourself. The 'sync' field tells you whether your writes are reaching the AllSource dashboard ('enabled': true) or are stranded local-only ('enabled': false) — if local-only, warn the user that nothing will appear at all-source.xyz/dashboard/memory until they relaunch prime with --sync-to and --api-key. Low cost, no parameters needed.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        // ─── Compressed Index & Recall ───────────────────────────────
        {
            "name": "prime_index",
            "description": "Get your compressed knowledge index — a token-efficient markdown summary of everything you know, organized by domain with cross-references. Call this FIRST at the start of every conversation to orient yourself. The index shows: which domains exist, how many facts per domain, and which domains are connected. Use it as navigational scaffolding before searching for specifics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Scope to a specific agent's knowledge" }
                }
            }
        },
        {
            "name": "prime_context",
            "description": "Combined retrieval with tiered depth control. L0: stats only (~100 tokens, use for orientation). L1: recent conversation context (~500-1500 tokens, use for follow-ups in same conversation). L2: full hybrid recall with compressed index + vectors + graph expansion (~2000-5000 tokens, use for cross-domain questions or new topics). Default: L2.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language question" },
                    "agent_id": { "type": "string", "description": "Scope to a specific agent" },
                    "top_k": { "type": "integer", "description": "Max vector results (default: 5, L2 only)" },
                    "include_index": { "type": "boolean", "description": "Prepend compressed index excerpt (default: true, L2 only)" },
                    "max_tokens": { "type": "integer", "description": "Cap total response tokens" },
                    "tier": { "type": "string", "enum": ["L0", "L1", "L2"], "description": "Retrieval depth. L0=stats only. L1=recent conversation context. L2=full hybrid recall. Default: L2." },
                    "conversation_id": { "type": "string", "description": "Scope L1 retrieval to this conversation's nodes. Ignored for L0/L2." }
                },
                "required": ["query"]
            }
        },
        // ─── Vector Operations ───────────────────────────────────────
        {
            "name": "prime_embed",
            "description": "Store a vector embedding for semantic search. Always pair with prime_add_node — create the node first, then embed it using the node's entity_id. This makes the node findable by meaning, not just by type or graph position. Supply 'text' alone and the server will embed it in-process via fastembed (AllMiniLML6V2, 384 dims). Supply 'vector' if you already have a precomputed embedding. At least one of 'text' or 'vector' is required.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Entity_id of the node to embed (use the entity_id returned by prime_add_node)" },
                    "text": { "type": "string", "description": "Source text. Stored alongside the vector for display in search results, and — if 'vector' is omitted — embedded server-side via the bundled fastembed model." },
                    "vector": { "type": "array", "items": { "type": "number" }, "description": "Precomputed 384-dim embedding vector. Optional when 'text' is supplied. This is also the escape hatch when the in-process embedder is unavailable (no network on first use, restrictive proxy, model-fetch failure): compute the vector yourself with any all-MiniLM-L6-v2 embedder (e.g. ~10 lines of sentence-transformers) and pass it here — semantic recall keeps working without the bundled model." },
                    "metadata": { "type": "object", "description": "Optional: tags, source URL, confidence score" }
                },
                "required": ["id"]
            }
        },
        {
            "name": "prime_similar",
            "description": "Find the most similar embeddings to a stored vector. Use to discover related knowledge: 'what else do I know that's similar to this?' Requires the target to have been embedded with prime_embed first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Entity_id of the embedded node to find similar items for" },
                    "top_k": { "type": "integer", "description": "Number of results (default: 5)" }
                },
                "required": ["id"]
            }
        },
        {
            "name": "prime_recall",
            "description": "Hybrid recall: vectors + graph + temporal recency. Your PRIMARY tool for 'what do I know about X?' questions. Finds semantically similar facts via embedding, then expands through graph connections to discover related context. Supply 'text' alone and the server embeds it in-process via fastembed (AllMiniLML6V2, 384 dims) — no client-side embedding model required. Supply 'vector' if you already have a precomputed query embedding. Set depth=0 for vector-only, depth=1+ to include graph neighbors. Every result names its 'retrieval' mode: 'semantic' means vectors ranked it, 'lexical' means the embedder was unavailable and text was matched against node properties instead, 'type_scan' means nothing matched and these are the most recent nodes of the type. A 'degraded' field appears with the reason whenever the answer is not semantic — keep using this tool, do NOT fall back to prime_search. For cross-domain questions, use prime_context instead (it adds the compressed index).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Natural language query. Embedded server-side via fastembed when 'vector' is not supplied." },
                    "vector": { "type": "array", "items": { "type": "number" }, "description": "Precomputed query embedding vector. Optional — supply 'text' instead to have the server embed for you." },
                    "node_type": { "type": "string", "description": "Filter to this node type only" },
                    "tenant_id": { "type": "string", "description": "Restrict results to nodes stamped with this tenant_id. Only needed when one Prime store holds several tenants' nodes; omit for a single-tenant store." },
                    "depth": { "type": "integer", "description": "Graph expansion hops from vector matches (0=vector-only, 1+=include neighbors, default: 1)" },
                    "top_k": { "type": "integer", "description": "Max results (default: 10)" }
                }
            }
        },
        // ─── Declarative Projections ──────────────────────────────────
        {
            "name": "prime_define_projection",
            "description": "Register or replace a declarative projection definition for an entity type. The projection tells Prime how to fold a node's observation history into a single snapshot — one merge policy per field. Closes the foundation for 'what is the current state of this entity?' without bespoke fold code. Policies: 'last_write' (most recent observed_at — for changing-over-time fields like status), 'highest_priority' (highest source_priority metadata — for identity fields where user corrections beat AI extractions), 'most_specific' (highest specificity_score metadata — when one source produces dense facts and another shallow), 'merge_array' (union all observed values — for tags, aliases). Fields not in field_policies are dropped from the snapshot.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity_type": { "type": "string", "description": "Entity type this projection applies to (e.g. 'contact', 'task')" },
                    "field_policies": {
                        "type": "object",
                        "description": "Map of field name to merge policy. Values: 'last_write' | 'highest_priority' | 'most_specific' | 'merge_array'",
                        "additionalProperties": { "type": "string", "enum": ["last_write", "highest_priority", "most_specific", "merge_array"] }
                    }
                },
                "required": ["entity_type", "field_policies"]
            }
        },
        {
            "name": "prime_list_projections",
            "description": "List every projection definition registered in this process. Use to introspect what's defined before calling prime_define_projection — you can see the existing field policies and decide whether to extend or replace.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "prime_project_node",
            "description": "Compute a node's snapshot by folding its event history through the registered projection definition for its entity type. Returns the merged field values according to each field's merge policy. Returns an error if no projection is defined for the node's entity type — register one first with prime_define_projection. The fold is deterministic: same observations always produce the same snapshot.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "node_id": { "type": "string", "description": "Node entity_id in 'node:{type}:{id}' format" }
                },
                "required": ["node_id"]
            }
        },
        {
            "name": "prime_node_provenance",
            "description": "Answer 'where did this field's value come from?' for a single field on a node. Returns the source event_id, timestamp, value, and the merge policy that picked it. Surfaces the audit trail without manual event-history scanning. Returns an error for fields whose policy is 'merge_array' (no single source — value is a union of many events). Requires a projection defined for the node's entity type.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "node_id": { "type": "string", "description": "Node entity_id in 'node:{type}:{id}' format" },
                    "field": { "type": "string", "description": "Field name to query provenance for" }
                },
                "required": ["node_id", "field"]
            }
        },
        // ─── Entity Templates ──────────────────────────────────────────
        {
            "name": "prime_list_templates",
            "description": "List bundled entity-type templates (person, contact, organization, task, decision, transaction, meeting, document). Call this FIRST when starting to model a new domain — pick one whose shape matches what you're trying to remember, then use prime_load_template to read its full property + relation schema. Templates are descriptive guides, not enforced contracts; you can still create any shape via prime_add_node.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "prime_load_template",
            "description": "Load a single entity-type template by name. Returns the full schema: entity_type, description, properties (with types + required flags), and suggested_relations (typical edges for this entity type). Use the returned shape when calling prime_add_node so the node matches the template's convention.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Template name (e.g. 'person', 'task', 'decision'). List available names with prime_list_templates." }
                },
                "required": ["name"]
            }
        },
        // ─── AI Inbox ──────────────────────────────────────────────────
        {
            "name": "inbox_recall_thread",
            "description": "Ground a reply in real history: return an email thread's interaction nodes (in/outbound messages) plus each one's graph neighbors (the people it involves). Pass the conversation_id (== an interaction node's conversation_id / a thread node's conversation_id). Read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "conversation_id": { "type": "string", "description": "Thread / conversation id shared by the email's interaction nodes." }
                },
                "required": ["conversation_id"]
            }
        },
        {
            "name": "inbox_draft",
            "description": "Persist a reviewed reply draft as a durable email.drafted event in Core (never auto-sends). Compose the body yourself — ground it first with inbox_recall_thread. Returns a draft_id for a later send. Requires sync enabled (--sync-to/--api-key).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "Conversation id the draft replies into." },
                    "body": { "type": "string", "description": "The drafted reply text." },
                    "intent": { "type": "string", "description": "Short description of what the reply is meant to do." },
                    "in_reply_to": { "type": "string", "description": "Optional provider message id being replied to." },
                    "subject": { "type": "string", "description": "Optional subject (inherits the thread otherwise)." },
                    "to": { "type": "array", "description": "Optional recipients [{name?, email}]." },
                    "by": { "type": "string", "enum": ["claude", "human"], "description": "Who composed it (default claude)." }
                },
                "required": ["thread_id", "body", "intent"]
            }
        },
        // ─── Hound (code graph) ────────────────────────────────────────
        {
            "name": "hound_ingest",
            "description": "Prime Hound: extract a codebase into the graph. Walk a directory (or a single file) with Tree-sitter — on-device, no LLM, no upload — and fold every function/type/trait/module into nodes with `defines` and `calls` edges, each confidence-tagged (EXTRACTED = AST-certain, INFERRED, AMBIGUOUS). Set `embed: true` to also embed each symbol (in-process, no LLM) so the graph is searchable by meaning via prime_recall — the hybrid-retrieval edge a flat graph file can't offer. Runs where your code is (local stdio mode). If sync is enabled the graph also reaches your AllSource tenant. Phase 1 parses Rust; more languages follow. Call this first, then hound_report / hound_impact / prime_recall.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory (or file) to extract. Honors .gitignore; skips hidden dirs." },
                    "embed": { "type": "boolean", "description": "Also embed each file/symbol for semantic recall (default false; runs the embedder once per node)." },
                    "rebuild": { "type": "boolean", "description": "Delete the existing code graph first so a re-ingest replaces rather than duplicates it (default false). Use when re-ingesting the same repo." }
                },
                "required": ["path"]
            }
        },
        {
            "name": "hound_impact",
            "description": "Prime Hound: blast radius of a change. Given a function (by name, or a node:function:<id> wire id), return everything that transitively CALLS it — the code that could break if you change it — up to `depth` hops. Computed from the code graph's incoming `calls` edges. Use before refactoring or deleting a function. A bare name may resolve to several definitions; each is reported separately.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "Function name (e.g. 'authenticate') or a node:function:<id> wire id." },
                    "depth": { "type": "integer", "description": "Transitive caller hops to follow (default 2)." }
                },
                "required": ["target"]
            }
        },
        {
            "name": "hound_report",
            "description": "Prime Hound: a structural summary of the code graph — node counts by type, edge counts by relation, the EXTRACTED/INFERRED/AMBIGUOUS confidence breakdown, and the top 'god nodes' (highest-degree symbols, the architectural hubs). Call after hound_ingest to orient on an unfamiliar codebase. Pass markdown:true for a rendered GRAPH_REPORT.md.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "top": { "type": "integer", "description": "How many god-nodes to return (default 10)." },
                    "markdown": { "type": "boolean", "description": "Return the rendered GRAPH_REPORT.md instead of JSON (default false)." }
                }
            }
        },
        {
            "name": "hound_pr_impact",
            "description": "Prime Hound: triage a pull request by blast radius. Pass the files it changes (e.g. `git diff --name-only main..HEAD`); returns a review queue of the changed symbols ranked by how many functions transitively CALL them — review the most depended-upon first. A change to an architectural hub is high-risk; a change nothing depends on is safe. Run hound_ingest first so the graph reflects the current code.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "files": { "type": "array", "items": { "type": "string" }, "description": "Changed file paths, e.g. the output of `git diff --name-only main..HEAD`." },
                    "depth": { "type": "integer", "description": "Transitive caller hops to follow (default 2)." },
                    "top": { "type": "integer", "description": "Max review-queue entries to return (default 20)." }
                },
                "required": ["files"]
            }
        },
        {
            "name": "hound_ingest_docs",
            "description": "Prime Hound: extract a knowledge graph from documentation (markdown/text). Code is parsed locally with Tree-sitter; prose has no AST, so it goes to your configured LLM, which returns the entities and relationships it describes — these fold into the SAME graph as nodes/edges, embedded for semantic recall and tagged INFERRED. Requires an LLM endpoint (PRIME_LLM_ENDPOINT, OpenAI-compatible, e.g. a local Ollama). Set embed:true for hybrid recall. Use alongside hound_ingest (code) to graph a whole repo.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory or file of docs (.md/.markdown/.txt/.rst)." },
                    "embed": { "type": "boolean", "description": "Embed each extracted entity for semantic recall (default false)." }
                },
                "required": ["path"]
            }
        },
        {
            "name": "hound_export",
            "description": "Prime Hound: export the code graph to an interchange format — cypher (Neo4j/FalkorDB import), graphml (Gephi/yEd), mermaid (a docs diagram), obsidian (a vault of linked notes), or wiki (an index + per-node markdown pages). Single-file formats return `content`; obsidian/wiki return a `files` array of {path, content} to write to disk.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "format": { "type": "string", "description": "cypher | graphml | mermaid | obsidian | wiki" },
                    "mermaid_max": { "type": "integer", "description": "For mermaid, cap the diagram to the N busiest nodes (default 60)." }
                },
                "required": ["format"]
            }
        }
    ])
}

/// Dispatch a tool call to the Prime facade or Recall engine.
pub async fn call_tool(prime: &Prime, recall: &RecallEngine, name: &str, args: &Value) -> Value {
    match name {
        "prime_add_node" => call_add_node(prime, args).await,
        "prime_add_edge" => call_add_edge(prime, args).await,
        "prime_neighbors" => call_neighbors(prime, args),
        "prime_search" => call_search(prime, args),
        "prime_shortest_path" => call_shortest_path(prime, args),
        "prime_forget" => call_forget(prime, args).await,
        "prime_history" => call_history(prime, args).await,
        "prime_stats" => call_stats(prime),
        "prime_index" => call_index(recall).await,
        "prime_context" => call_context(prime, recall, args).await,
        "prime_embed" => call_embed(prime, args).await,
        "prime_similar" => call_similar(prime, args),
        "prime_recall" => call_recall(prime, args).await,
        "prime_list_templates" => call_list_templates(),
        "prime_load_template" => call_load_template(args),
        "prime_define_projection" => call_define_projection(prime, args).await,
        "prime_list_projections" => call_list_projections(),
        "prime_project_node" => call_project_node(prime, args).await,
        "prime_node_provenance" => call_node_provenance(prime, args).await,
        "inbox_recall_thread" => call_inbox_recall_thread(prime, args),
        "inbox_draft" => call_inbox_draft(args).await,
        "hound_ingest" => call_hound_ingest(prime, args).await,
        "hound_ingest_docs" => call_hound_ingest_docs(prime, args).await,
        "hound_impact" => call_hound_impact(prime, args),
        "hound_report" => call_hound_report(prime, args),
        "hound_pr_impact" => call_hound_pr_impact(prime, args),
        "hound_export" => call_hound_export(prime, args),
        _ => tool_error(&format!("Unknown tool: {name}")),
    }
}

// ─── Declarative projections + provenance dispatch ──────────────────────
//
// These call into the in-memory projection registry (see
// `crate::projection_registry`) and the pure fold/provenance functions in
// `allsource_core::prime::projections`. The MCP layer's job is just
// argument parsing, projection lookup, and result shaping — the
// interesting logic lives in Core's declarative module.

async fn call_define_projection(prime: &Prime, args: &Value) -> Value {
    use allsource_core::prime::projections::{MergePolicy, ProjectionDef};
    use std::collections::BTreeMap;

    let Some(entity_type) = args.get("entity_type").and_then(Value::as_str) else {
        return tool_error("missing required argument: entity_type");
    };
    let Some(policies_map) = args.get("field_policies").and_then(Value::as_object) else {
        return tool_error("missing required argument: field_policies (object)");
    };

    let mut field_policies = BTreeMap::new();
    for (field, value) in policies_map {
        let Some(policy_str) = value.as_str() else {
            return tool_error(&format!(
                "field_policies.{field} must be a string (one of last_write, highest_priority, most_specific, merge_array)"
            ));
        };
        let policy: MergePolicy = match serde_json::from_value(json!(policy_str)) {
            Ok(p) => p,
            Err(_) => {
                return tool_error(&format!(
                    "field_policies.{field}: unknown policy '{policy_str}' — valid: last_write, highest_priority, most_specific, merge_array"
                ));
            }
        };
        field_policies.insert(field.clone(), policy);
    }

    let def = ProjectionDef {
        entity_type: entity_type.to_string(),
        field_policies,
    };

    // Persistence FIRST, cache AFTER. If the event ingest fails, the cache
    // stays consistent with the (unchanged) event log. The cache is just a
    // read-side accelerator over the durable source of truth.
    if let Err(e) = prime.define_projection(&def).await {
        return tool_error(&format!("failed to persist projection definition: {e}"));
    }
    let replaced = crate::projection_registry::upsert(def);
    if replaced {
        tracing::warn!(
            entity_type = entity_type,
            "prime_define_projection replaced an existing definition (full history retained in event log)"
        );
    }
    tool_result(json!({
        "entity_type": entity_type,
        "replaced": replaced,
        "persisted": true,
    }))
}

fn call_list_projections() -> Value {
    let defs = crate::projection_registry::list();
    let payload: Vec<Value> = defs
        .iter()
        .map(|d| {
            let policies: serde_json::Map<String, Value> = d
                .field_policies
                .iter()
                .map(|(field, policy)| {
                    let policy_str = serde_json::to_value(policy)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default();
                    (field.clone(), Value::String(policy_str))
                })
                .collect();
            json!({
                "entity_type": d.entity_type,
                "field_policies": policies,
            })
        })
        .collect();
    tool_result(json!({ "projections": payload }))
}

/// Parse a node `entity_id` of the form `node:{type}:{id}` and return the
/// type segment. The format is documented in every node-related tool's
/// description; this is the inverse used by the projection lookup path.
fn entity_type_from_node_id(node_id: &str) -> Option<&str> {
    let mut parts = node_id.splitn(3, ':');
    let prefix = parts.next()?;
    if prefix != "node" {
        return None;
    }
    let entity_type = parts.next()?;
    let _id = parts.next()?;
    Some(entity_type)
}

async fn call_project_node(prime: &Prime, args: &Value) -> Value {
    use allsource_core::prime::projections::fold;

    let Some(node_id) = args.get("node_id").and_then(Value::as_str) else {
        return tool_error("missing required argument: node_id");
    };
    let Some(entity_type) = entity_type_from_node_id(node_id) else {
        return tool_error(&format!(
            "node_id must be in 'node:{{type}}:{{id}}' format, got: {node_id}"
        ));
    };
    let Some(def) = crate::projection_registry::get(entity_type) else {
        return tool_error(&format!(
            "no projection defined for entity_type '{entity_type}' — register one with prime_define_projection first"
        ));
    };
    let observations = match prime.observations(node_id).await {
        Ok(o) => o,
        Err(e) => return tool_error(&format!("failed to load observations: {e}")),
    };
    let snapshot = fold(&observations, &def);
    tool_result(json!({
        "entity_type": snapshot.entity_type,
        "fields": snapshot.fields,
        "observation_count": observations.len(),
    }))
}

async fn call_node_provenance(prime: &Prime, args: &Value) -> Value {
    use allsource_core::prime::projections::provenance_for_field;

    let Some(node_id) = args.get("node_id").and_then(Value::as_str) else {
        return tool_error("missing required argument: node_id");
    };
    let Some(field) = args.get("field").and_then(Value::as_str) else {
        return tool_error("missing required argument: field");
    };
    let Some(entity_type) = entity_type_from_node_id(node_id) else {
        return tool_error(&format!(
            "node_id must be in 'node:{{type}}:{{id}}' format, got: {node_id}"
        ));
    };
    let Some(def) = crate::projection_registry::get(entity_type) else {
        return tool_error(&format!(
            "no projection defined for entity_type '{entity_type}' — register one with prime_define_projection first"
        ));
    };
    let observations = match prime.observations(node_id).await {
        Ok(o) => o,
        Err(e) => return tool_error(&format!("failed to load observations: {e}")),
    };
    match provenance_for_field(&observations, &def, field) {
        Some(p) => tool_result(json!({
            "field": p.field,
            "value": p.value,
            "source_event_id": p.source_event_id,
            "source_event_at": p.source_event_at,
            "merge_policy_applied": p.merge_policy_applied,
        })),
        None => tool_error(&format!(
            "no provenance available for field '{field}' on {node_id} \
             (field may be missing, not in the projection, or have a merge_array policy which has no single source)"
        )),
    }
}

fn call_list_templates() -> Value {
    tool_result(crate::templates::list_templates_summary())
}

fn call_load_template(args: &Value) -> Value {
    let Some(name) = args.get("name").and_then(Value::as_str) else {
        return tool_error("missing required argument: name");
    };
    match crate::templates::load_template(name) {
        Some(t) => tool_result(t),
        None => tool_error(&format!(
            "unknown template: {name} — call prime_list_templates to see what's available"
        )),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn tool_result(content: Value) -> Value {
    let text = match result_format() {
        // A leading `format: toon` line tells the consuming model how to read
        // the payload; it is itself a valid TOON field, so the result stays
        // well-formed TOON end to end.
        ResultFormat::Toon => format!("format: toon\n{}", crate::toon::encode(&content)),
        ResultFormat::Json => serde_json::to_string_pretty(&content).unwrap_or_default(),
    };
    json!({
        "content": [{
            "type": "text",
            "text": text
        }]
    })
}

fn tool_error(msg: &str) -> Value {
    json!({
        "isError": true,
        "content": [{
            "type": "text",
            "text": msg
        }]
    })
}

// ─── AI Inbox tools ────────────────────────────────────────────────────

/// `node:{type}:{id}` entity id for a graph node.
fn node_eid(node_type: &str, id: &str) -> String {
    allsource_core::prime::EntityId::node(node_type, id).to_string()
}

/// `inbox_recall_thread`: a thread's interaction nodes + each one's neighbors.
fn call_inbox_recall_thread(prime: &Prime, args: &Value) -> Value {
    let Some(conversation_id) = args.get("conversation_id").and_then(Value::as_str) else {
        return tool_error("missing 'conversation_id'");
    };

    let thread = prime.nodes_by_type("thread").into_iter().find(|n| {
        n.properties.get("conversation_id").and_then(Value::as_str) == Some(conversation_id)
    });

    let mut interactions = Vec::new();
    for n in prime.nodes_by_type("interaction") {
        if n.properties.get("conversation_id").and_then(Value::as_str) != Some(conversation_id) {
            continue;
        }
        let eid = node_eid(&n.node_type, n.id.as_str());
        let neighbors: Vec<Value> = prime
            .neighbors(&eid, None, allsource_core::prime::Direction::Both)
            .iter()
            .map(|nb| node_row(nb, Fields::Full))
            .collect();
        interactions.push(json!({
            "id": eid,
            "message_id": n.properties.get("message_id"),
            "direction": n.properties.get("direction"),
            "subject": n.properties.get("subject"),
            "snippet": n.properties.get("snippet"),
            "from": n.properties.get("from"),
            "to": n.properties.get("to"),
            "at": n.properties.get("at"),
            "neighbors": neighbors,
        }));
    }

    let thread_json = thread.map(
        |t| json!({ "id": node_eid(&t.node_type, t.id.as_str()), "properties": t.properties }),
    );

    tool_result(json!({
        "conversation_id": conversation_id,
        "thread": thread_json,
        "count": interactions.len(),
        "interactions": interactions,
    }))
}

/// Build an `email.drafted` (`entity_id` = `draft_id`, payload, metadata) from tool
/// args. Pure + testable; the remote write is separate. Requires `thread_id`,
/// body, intent (see docs/contracts/email-events/schema/email.drafted.schema.json).
fn build_drafted_event(args: &Value) -> Result<(String, Value, Value), String> {
    let thread_id = args
        .get("thread_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing 'thread_id'".to_string())?;
    let body = args
        .get("body")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing 'body'".to_string())?;
    let intent = args
        .get("intent")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing 'intent'".to_string())?;
    let by = match args.get("by").and_then(Value::as_str) {
        Some("human") => "human",
        _ => "claude",
    };

    let draft_id = format!("draft_{}", uuid::Uuid::new_v4());
    let mut payload = json!({
        "thread_id": thread_id,
        "body": body,
        "intent": intent,
        "by": by,
        "drafted_at": chrono::Utc::now().to_rfc3339(),
    });
    for key in ["in_reply_to", "subject", "to"] {
        if let Some(v) = args.get(key) {
            payload[key] = v.clone();
        }
    }
    let metadata = json!({ "draft_id": draft_id });
    Ok((draft_id, payload, metadata))
}

/// `inbox_draft`: persist a reviewed reply as a durable email.drafted Core event.
async fn call_inbox_draft(args: &Value) -> Value {
    let (draft_id, payload, metadata) = match build_drafted_event(args) {
        Ok(t) => t,
        Err(e) => return tool_error(&e),
    };
    if !crate::core_writer::is_configured() {
        return tool_error(
            "inbox_draft needs remote sync: start the server with --sync-to and --api-key",
        );
    }
    match crate::core_writer::ingest_event("email.drafted", &draft_id, &payload, &metadata).await {
        Ok(event_id) => {
            tool_result(json!({ "draft_id": draft_id, "event_id": event_id, "status": "drafted" }))
        }
        Err(e) => tool_error(&format!("failed to write draft: {e}")),
    }
}

async fn call_add_node(prime: &Prime, args: &Value) -> Value {
    let node_type = args
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let properties = args.get("properties").cloned().unwrap_or(json!({}));

    match prime.add_node(node_type, properties).await {
        Ok(id) => {
            let entity_id = allsource_core::prime::EntityId::node(node_type, id.as_str()).to_wire();
            tool_result(json!({ "node_id": id.as_str(), "entity_id": entity_id }))
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_add_edge(prime: &Prime, args: &Value) -> Value {
    let Some(source) = args.get("source").and_then(Value::as_str) else {
        return tool_error("missing 'source'");
    };
    let Some(target) = args.get("target").and_then(Value::as_str) else {
        return tool_error("missing 'target'");
    };
    let Some(relation) = args.get("relation").and_then(Value::as_str) else {
        return tool_error("missing 'relation'");
    };
    let properties = args.get("properties").cloned();
    let weight = args.get("weight").and_then(serde_json::Value::as_f64);

    let result = if let Some(w) = weight {
        prime
            .add_edge_weighted(source, target, relation, w, properties)
            .await
    } else {
        prime.add_edge(source, target, relation, properties).await
    };

    match result {
        Ok(id) => tool_result(json!({ "edge_id": id.as_str() })),
        Err(e) => tool_error(&e.to_string()),
    }
}

fn call_neighbors(prime: &Prime, args: &Value) -> Value {
    let Some(node_id) = args.get("node_id").and_then(Value::as_str) else {
        return tool_error("missing 'node_id'");
    };
    let relation = args.get("relation").and_then(Value::as_str);
    let direction = match args.get("direction").and_then(Value::as_str) {
        Some("incoming") => allsource_core::prime::Direction::Incoming,
        Some("outgoing") => allsource_core::prime::Direction::Outgoing,
        _ => allsource_core::prime::Direction::Both,
    };
    let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(1) as usize;

    let page = Page::from_args(args);

    if depth <= 1 {
        let fields = Fields::from_args(args, Fields::Full);
        let (nodes, total) = page.apply(prime.neighbors(node_id, relation, direction));
        let nodes_json: Vec<Value> = nodes.iter().map(|n| node_row(n, fields)).collect();
        tool_result(paged("nodes", nodes_json, page, total))
    } else {
        let fields = Fields::from_args(args, Fields::Summary);
        let (results, total) =
            page.apply(prime.neighbors_within(node_id, depth, relation, direction));
        let nodes_json: Vec<Value> = results
            .iter()
            .map(|(n, d)| {
                let mut row = node_row(n, fields);
                row["depth"] = json!(d);
                row
            })
            .collect();
        tool_result(paged("nodes", nodes_json, page, total))
    }
}

fn call_search(prime: &Prime, args: &Value) -> Value {
    let Some(node_type) = args.get("type").and_then(Value::as_str) else {
        return tool_error("missing 'type'");
    };
    let fields = Fields::from_args(args, Fields::Summary);
    let page = Page::from_args(args);
    let (nodes, total) = page.apply(prime.nodes_by_type(node_type));
    let nodes_json: Vec<Value> = nodes.iter().map(|n| node_row(n, fields)).collect();
    tool_result(paged("nodes", nodes_json, page, total))
}

fn call_shortest_path(prime: &Prime, args: &Value) -> Value {
    let Some(from) = args.get("from").and_then(Value::as_str) else {
        return tool_error("missing 'from'");
    };
    let Some(to) = args.get("to").and_then(Value::as_str) else {
        return tool_error("missing 'to'");
    };
    let relation = args.get("relation").and_then(Value::as_str);

    match prime.shortest_path(from, to, relation) {
        Some(path) => {
            let path_json: Vec<Value> = path.iter().map(|n| node_row(n, Fields::Full)).collect();
            tool_result(json!({ "path": path_json }))
        }
        None => tool_result(json!({ "path": null, "message": "No path found" })),
    }
}

async fn call_forget(prime: &Prime, args: &Value) -> Value {
    let Some(node_id) = args.get("node_id").and_then(Value::as_str) else {
        return tool_error("missing 'node_id'");
    };

    match prime.delete_node(node_id).await {
        Ok(()) => tool_result(json!({ "deleted": true })),
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_history(prime: &Prime, args: &Value) -> Value {
    let Some(entity_id) = args.get("entity_id").and_then(Value::as_str) else {
        return tool_error("missing 'entity_id'");
    };

    match prime.history(entity_id).await {
        Ok(entries) => {
            let events_json: Vec<Value> = entries
                .iter()
                .map(|e| {
                    json!({
                        "type": e.event_type,
                        "timestamp": e.timestamp.to_rfc3339(),
                        "payload": e.payload,
                    })
                })
                .collect();
            tool_result(json!({ "events": events_json }))
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

/// One line telling the agent how much of the store semantic recall can see.
///
/// Returns `None` at full coverage so a healthy store costs no tokens.
pub fn recall_coverage_note(total_nodes: usize, unembedded: usize) -> Option<String> {
    if unembedded == 0 || total_nodes == 0 {
        return None;
    }
    let embedded = total_nodes.saturating_sub(unembedded);
    Some(format!(
        "{embedded} of {total_nodes} nodes are embedded — prime_recall cannot see the other \
         {unembedded}. They are embedded in the background; call prime_embed to prioritise one."
    ))
}

fn call_stats(prime: &Prime) -> Value {
    let stats = prime.stats();
    let unembedded = prime.count_nodes_missing_vectors();
    tool_result(json!({
        "total_nodes": stats.total_nodes,
        "total_edges": stats.total_edges,
        "deleted_nodes": stats.deleted_nodes,
        "deleted_edges": stats.deleted_edges,
        "event_count": stats.event_count,
        "nodes_by_type": stats.nodes_by_type,
        "edges_by_relation": stats.edges_by_relation,
        // A node with no vector is unreachable by prime_recall, and an
        // unembedded store answers identically to an empty one.
        "nodes_without_vectors": unembedded,
        "recall_coverage": recall_coverage_note(stats.total_nodes, unembedded),
        // Surface sync state so the agent can tell whether what it just wrote
        // will reach the AllSource dashboard or is stranded local-only.
        "sync": sync_status_json(),
    }))
}

// =========================================================================
// Hound (code graph) tools
// =========================================================================
//
// Hound is a local-first capability: ingest runs where the source lives, so
// these are served on the embedded (stdio/local-http) path only. The hosted
// dispatch returns a redirect error (see `hosted_dispatch::call_tool_hosted`).
// Deep analytics (community detection, full GRAPH_REPORT.md, git-diff PR
// impact) are Phase 3; these are the working Phase-1 versions over the live
// graph projections.

/// `hound_ingest`: extract a codebase into the local Prime graph via `Tree-sitter`.
async fn call_hound_ingest(prime: &Prime, args: &Value) -> Value {
    let Some(path) = args.get("path").and_then(Value::as_str) else {
        return tool_error("missing 'path' — the directory (or file) to extract");
    };
    let embed = args.get("embed").and_then(Value::as_bool).unwrap_or(false);
    let rebuild = args
        .get("rebuild")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match crate::hound::ingest(prime, std::path::Path::new(path), embed, rebuild).await {
        Ok(s) => tool_result(json!({
            "files": s.files,
            "nodes": s.nodes,
            "edges": s.edges,
            "defines": s.defines,
            "calls": s.calls,
            "ambiguous": s.ambiguous,
            "unresolved": s.unresolved,
            "embedded": s.embedded,
            "deleted_stale": s.deleted_stale,
            "sync": sync_status_json(),
        })),
        Err(e) => tool_error(&e.to_string()),
    }
}

/// `hound_ingest_docs`: LLM-extract a knowledge graph from prose docs.
async fn call_hound_ingest_docs(prime: &Prime, args: &Value) -> Value {
    let Some(path) = args.get("path").and_then(Value::as_str) else {
        return tool_error("missing 'path' — the directory (or file) of docs to extract");
    };
    let embed = args.get("embed").and_then(Value::as_bool).unwrap_or(false);
    match crate::doc_extract::extract_docs(prime, std::path::Path::new(path), embed).await {
        Ok(s) => tool_result(json!({
            "files": s.files,
            "chunks": s.chunks,
            "entities": s.entities,
            "relationships": s.relationships,
            "embedded": s.embedded,
            "skipped": s.skipped,
            "usage": {
                "llm_calls": s.llm_calls,
                "prompt_tokens": s.prompt_tokens,
                "completion_tokens": s.completion_tokens,
                "total_tokens": s.total_tokens,
            },
            "sync": sync_status_json(),
        })),
        Err(e) => tool_error(&e.to_string()),
    }
}

/// `hound_impact`: transitive callers of a function — the blast radius of a change.
fn call_hound_impact(prime: &Prime, args: &Value) -> Value {
    let Some(target) = args.get("target").and_then(Value::as_str) else {
        return tool_error("missing 'target' — a function name or a node:function:<id> wire id");
    };
    let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(2) as usize;

    // Resolve the target to one or more function nodes. A bare name can match
    // several definitions across files, so impact is reported per match.
    let resolved: Vec<(String, Value)> = if target.starts_with("node:") {
        match prime.get_node(target) {
            Some(n) => vec![(target.to_string(), n.properties)],
            None => return tool_error(&format!("no node found for wire id '{target}'")),
        }
    } else {
        prime
            .nodes_by_type("function")
            .into_iter()
            .filter(|n| n.properties.get("name").and_then(Value::as_str) == Some(target))
            .map(|n| {
                let wire =
                    allsource_core::prime::EntityId::node("function", n.id.as_str()).to_wire();
                (wire, n.properties)
            })
            .collect()
    };

    if resolved.is_empty() {
        return tool_result(json!({
            "target": target,
            "matches": 0,
            "results": [],
            "message": "no function with that name in the graph — run hound_ingest first?",
        }));
    }

    let results: Vec<Value> = resolved
        .iter()
        .map(|(wire, props)| {
            // Incoming `calls` edges, transitively = who depends on this fn.
            let callers = prime.neighbors_within(
                wire,
                depth,
                Some("calls"),
                allsource_core::prime::Direction::Incoming,
            );
            let impacted: Vec<Value> = callers
                .iter()
                .filter(|(_, d)| *d > 0) // drop the target itself (depth 0)
                .map(|(n, d)| {
                    json!({
                        "id": allsource_core::prime::EntityId::node(&n.node_type, n.id.as_str()).to_wire(),
                        "name": n.properties.get("name").and_then(Value::as_str),
                        "file": n.properties.get("file").and_then(Value::as_str),
                        "line": n.properties.get("line"),
                        "depth": d,
                    })
                })
                .collect();
            json!({
                "target": wire,
                "target_name": props.get("name").and_then(Value::as_str),
                "target_file": props.get("file").and_then(Value::as_str),
                "impacted_count": impacted.len(),
                "impacted": impacted,
            })
        })
        .collect();

    tool_result(json!({
        "target": target,
        "depth": depth,
        "matches": results.len(),
        "results": results,
    }))
}

/// `hound_report`: structural summary of the code graph — counts, confidence,
/// PageRank-ranked god-nodes, and communities. Pass `markdown: true` for the
/// rendered `GRAPH_REPORT.md` instead of the JSON payload.
fn call_hound_report(prime: &Prime, args: &Value) -> Value {
    let top = args.get("top").and_then(Value::as_u64).unwrap_or(10) as usize;
    let markdown = args
        .get("markdown")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let g = prime.full_graph(None, None, None);
    let data = crate::report::compute(&g, top);
    if markdown {
        tool_result(json!({ "markdown": data.to_markdown() }))
    } else {
        tool_result(data.to_json())
    }
}

/// `hound_pr_impact`: rank a PR's changed files by blast radius (transitive callers).
fn call_hound_pr_impact(prime: &Prime, args: &Value) -> Value {
    let Some(files_val) = args.get("files").and_then(Value::as_array) else {
        return tool_error(
            "missing 'files' — an array of changed file paths (e.g. `git diff --name-only main..HEAD`)",
        );
    };
    let files: Vec<String> = files_val
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    if files.is_empty() {
        return tool_error("'files' is empty — pass the PR's changed file paths");
    }
    let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(2) as usize;
    let top = args.get("top").and_then(Value::as_u64).unwrap_or(20) as usize;
    let g = prime.full_graph(None, None, None);
    tool_result(crate::pr::pr_impact(&g, &files, depth).to_json(top))
}

/// `hound_export`: serialize the code graph to an interchange format.
fn call_hound_export(prime: &Prime, args: &Value) -> Value {
    let Some(format) = args.get("format").and_then(Value::as_str) else {
        return tool_error(&format!(
            "missing 'format' — one of: {}",
            crate::export::format_keys()
        ));
    };
    let mermaid_max = args
        .get("mermaid_max")
        .and_then(Value::as_u64)
        .unwrap_or(60) as usize;
    let g = prime.full_graph(None, None, None);

    if let Some(text) = crate::export::render_text(&g, format, mermaid_max) {
        tool_result(json!({ "format": format, "content": text }))
    } else if let Some(files) = crate::export::render_files(&g, format) {
        let files_json: Vec<Value> = files
            .into_iter()
            .map(|(path, content)| json!({ "path": path, "content": content }))
            .collect();
        tool_result(json!({ "format": format, "files": files_json }))
    } else {
        tool_error(&format!(
            "unknown format '{format}' — choose one of: {}",
            crate::export::format_keys()
        ))
    }
}

// =========================================================================
// Recall tools
// =========================================================================

async fn call_index(recall: &RecallEngine) -> Value {
    let index = recall.index().await;
    tool_result(json!({
        "index": index.markdown,
        "token_count": index.token_count,
        "domains": index.domains,
        "cross_references": index.cross_references,
        "last_updated": index.last_updated.to_rfc3339(),
    }))
}

async fn call_embed(prime: &Prime, args: &Value) -> Value {
    let Some(id) = args.get("id").and_then(Value::as_str) else {
        return tool_error("missing 'id'");
    };
    let text = args.get("text").and_then(Value::as_str);

    let vector: Vec<f32> = if let Some(arr) = args.get("vector").and_then(|v| v.as_array()) {
        arr.iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect()
    } else if let Some(t) = text {
        match prime.embed_text(t) {
            Ok(v) => v,
            Err(e) => return tool_error(&format!("server-side embedding failed: {e}")),
        }
    } else {
        return tool_error("missing 'vector' or 'text' — supply at least one");
    };

    let metadata = args.get("metadata").cloned();

    match prime.embed_with_metadata(id, text, vector, metadata).await {
        Ok(()) => tool_result(json!({ "stored": true, "id": id })),
        Err(e) => tool_error(&e.to_string()),
    }
}

fn call_similar(prime: &Prime, args: &Value) -> Value {
    let Some(id) = args.get("id").and_then(Value::as_str) else {
        return tool_error("missing 'id'");
    };
    let top_k = args
        .get("top_k")
        .and_then(Value::as_u64)
        .map_or(5, |v| v as usize);

    match prime.similar(id, top_k) {
        Ok(results) => {
            let results_json: Vec<Value> = results
                .iter()
                .map(|r| json!({ "id": r.id, "score": r.score, "text": r.text }))
                .collect();
            tool_result(json!({ "results": results_json }))
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn call_recall(prime: &Prime, args: &Value) -> Value {
    use allsource_core::prime::types::RecallQuery;

    let text = args.get("text").and_then(Value::as_str).map(String::from);
    let node_type = args
        .get("node_type")
        .and_then(Value::as_str)
        .map(String::from);

    // An unavailable embedder is not a reason to answer nothing: `text` is
    // still a retrieval input, so fall through with no vector and let recall
    // score lexically. The reason travels to the client on the result.
    let mut embed_failure: Option<String> = None;
    let vector: Option<Vec<f32>> = if let Some(arr) = args.get("vector").and_then(|v| v.as_array())
    {
        Some(
            arr.iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect(),
        )
    } else if let Some(ref t) = text {
        match prime.embed_text(t) {
            Ok(v) => Some(v),
            Err(e) => {
                embed_failure = Some(e.to_string());
                None
            }
        }
    } else {
        None
    };

    if vector.is_none() && node_type.is_none() && text.is_none() {
        return tool_error(
            "missing input — supply 'text', 'vector', or 'node_type' so recall has something to search on",
        );
    }

    let query = RecallQuery {
        text,
        vector,
        node_type,
        tenant: args
            .get("tenant_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        depth: args
            .get("depth")
            .and_then(Value::as_u64)
            .map_or(1, |v| v as usize),
        top_k: args
            .get("top_k")
            .and_then(Value::as_u64)
            .map_or(10, |v| v as usize),
        ..RecallQuery::default()
    };

    match prime.recall(query).await {
        Ok(result) => {
            let fields = Fields::from_args(args, Fields::Full);
            let nodes_json: Vec<Value> = result
                .nodes
                .iter()
                .map(|sn| {
                    let mut row = node_row(&sn.node, fields);
                    row["score"] = json!(sn.score);
                    row["depth"] = json!(sn.depth);
                    row
                })
                .collect();
            let vectors_json: Vec<Value> = result
                .vectors
                .iter()
                .map(|v| {
                    json!({
                        "id": v.id,
                        "score": v.score,
                        "text": v.text,
                    })
                })
                .collect();

            let mut out = json!({
                "nodes": nodes_json,
                "vectors": vectors_json,
                "edges": result.edges.len(),
                "retrieval": result.retrieval,
            });
            if let Some(note) = degraded_note(result.retrieval, embed_failure.as_deref()) {
                out["degraded"] = json!(note);
            }
            tool_result(out)
        }
        Err(e) => tool_error(&e.to_string()),
    }
}

/// One line telling the model these results are not semantic, so it does not
/// read a lexical answer as a meaning-based one.
pub fn degraded_note(retrieval: Retrieval, embed_failure: Option<&str>) -> Option<String> {
    if !retrieval.is_degraded() {
        return None;
    }
    let how = match retrieval {
        Retrieval::Lexical => "ranked by text match against node properties, not by meaning",
        Retrieval::TypeScan => {
            "nothing matched the query text; these are the most recent nodes of \
                                this type"
        }
        _ => return None,
    };
    Some(match embed_failure {
        Some(e) => format!(
            "{how}. The embedding model is unavailable ({e}) — run `allsource-prime --mode warm` \
             once with network access, or set PRIME_EMBED_MODEL_DIR to a vendored model directory."
        ),
        None => format!("{how}. Pass 'text' with a working embedder for semantic recall."),
    })
}

async fn call_context(prime: &Prime, recall: &RecallEngine, args: &Value) -> Value {
    use allsource_core::prime::recall::{ContextTier, RecallContextQuery};

    let Some(query) = args.get("query").and_then(Value::as_str) else {
        return tool_error("missing 'query'");
    };
    let query = query.to_string();

    let top_k = args
        .get("top_k")
        .and_then(Value::as_u64)
        .map_or(5, |v| usize::try_from(v).unwrap_or(5));
    let max_tokens = args
        .get("max_tokens")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok());

    let tier = match args.get("tier").and_then(Value::as_str) {
        Some("L0") => ContextTier::L0,
        Some("L1") => ContextTier::L1,
        Some("L2") | None => ContextTier::L2,
        Some(other) => return tool_error(&format!("invalid tier: {other}. Use L0, L1, or L2")),
    };

    // Keep the query for the L2 vector arm below (the struct moves `query`).
    let query_for_recall = query.clone();

    let ctx_query = RecallContextQuery {
        query,
        agent_id: args
            .get("agent_id")
            .and_then(Value::as_str)
            .map(String::from),
        top_k,
        as_of: None,
        include_index: args
            .get("include_index")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        max_tokens,
        tier,
        conversation_id: args
            .get("conversation_id")
            .and_then(Value::as_str)
            .map(String::from),
    };

    let ctx = recall.context(ctx_query).await;

    // L2 is "full hybrid recall" — index + vectors + graph. The RecallEngine
    // owns the compressed index but not the vector store, so its `vectors`/`nodes`
    // are empty by design (a documented TODO in `context_l2`). Fill the vector +
    // graph arms here from the facade's working `prime.recall()` path (embeds the
    // query in-process, HNSW vector search + graph expansion). L0/L1 don't do
    // vector recall, so leave them untouched.
    let (vectors_json, nodes_json) = if matches!(tier, ContextTier::L2) {
        use allsource_core::prime::types::RecallQuery;
        match prime.embed_text(&query_for_recall) {
            Ok(vector) => {
                let rq = RecallQuery {
                    text: Some(query_for_recall.clone()),
                    vector: Some(vector),
                    top_k,
                    ..RecallQuery::default()
                };
                match prime.recall(rq).await {
                    Ok(result) => {
                        let v: Vec<Value> = result
                            .vectors
                            .iter()
                            .map(|x| json!({ "id": x.id, "score": x.score, "text": x.text }))
                            .collect();
                        let n: Vec<Value> = result
                            .nodes
                            .iter()
                            .map(|sn| {
                                let mut row = node_row(&sn.node, Fields::Full);
                                row["score"] = json!(sn.score);
                                row["depth"] = json!(sn.depth);
                                row
                            })
                            .collect();
                        (Value::Array(v), Value::Array(n))
                    }
                    // Recall failure shouldn't sink the whole context call — the
                    // index is still useful. Fall back to the engine's (empty) arms.
                    Err(_) => (json!(ctx.vectors), json!(ctx.nodes)),
                }
            }
            Err(_) => (json!(ctx.vectors), json!(ctx.nodes)),
        }
    } else {
        (json!(ctx.vectors), json!(ctx.nodes))
    };

    tool_result(json!({
        "tier": format!("{:?}", ctx.tier),
        "index": ctx.index,
        "vectors": vectors_json,
        "nodes": nodes_json,
        "edges": ctx.edges,
        "stats": ctx.stats,
        "token_count": ctx.token_count,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hound_ingest_impact_report_roundtrip() {
        // Source lives on disk (a temp dir); the Prime store is in-memory.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("lib.rs"),
            "fn helper() -> i32 { 42 }\n\
             pub fn run() -> i32 { helper() }\n\
             fn other() { run(); }\n",
        )
        .expect("write src");

        let prime = Prime::open_in_memory().await.unwrap();

        // 1. Ingest: 1 file + 3 fns = 4 nodes; defines 3; calls run→helper, other→run = 2.
        let ing = call_hound_ingest(&prime, &json!({ "path": dir.path().to_str().unwrap() })).await;
        assert_ne!(ing.get("isError"), Some(&json!(true)));
        let t = ing["content"][0]["text"].as_str().unwrap();
        assert!(t.contains("\"nodes\": 4"), "got: {t}");
        assert!(t.contains("\"calls\": 2"), "got: {t}");

        // 2. Impact of `helper`: run calls it directly (d1); other reaches it via run (d2).
        let imp = call_hound_impact(&prime, &json!({ "target": "helper", "depth": 2 }));
        let t = imp["content"][0]["text"].as_str().unwrap();
        assert!(t.contains("\"run\""), "direct caller missing: {t}");
        assert!(t.contains("\"other\""), "transitive caller missing: {t}");

        // 3. Report: type breakdown, confidence tags, and god-nodes present.
        let rep = call_hound_report(&prime, &json!({ "top": 5 }));
        let t = rep["content"][0]["text"].as_str().unwrap();
        assert!(t.contains("\"god_nodes\""), "got: {t}");
        assert!(t.contains("\"function\": 3"), "expected 3 functions: {t}");
        assert!(t.contains("EXTRACTED"), "expected confidence tags: {t}");

        // 4. Unknown function → empty result, not an error.
        let none = call_hound_impact(&prime, &json!({ "target": "does_not_exist" }));
        let t = none["content"][0]["text"].as_str().unwrap();
        assert!(t.contains("\"matches\": 0"), "got: {t}");
    }

    #[tokio::test]
    async fn hound_rebuild_reingest_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("lib.rs"), "fn a() { b(); }\nfn b() {}\n").expect("src");
        let path = json!({ "path": dir.path().to_str().unwrap(), "rebuild": true });
        let prime = Prime::open_in_memory().await.unwrap();

        // Count what queries actually see (full_graph excludes soft-deleted
        // tombstones), which is the idempotency that matters.
        let live = |p: &Prime| {
            let g = p.full_graph(None, None, None);
            (g.nodes.len(), g.edges.len())
        };

        call_hound_ingest(&prime, &path).await;
        let (n1, e1) = live(&prime); // 1 file + a + b = 3 nodes; 2 defines + 1 call = 3 edges

        // Re-ingest with rebuild → replaces, does not duplicate.
        call_hound_ingest(&prime, &path).await;
        assert_eq!(
            live(&prime),
            (n1, e1),
            "rebuild must not duplicate the live graph"
        );

        // Without rebuild, a re-ingest DOES append (the bug rebuild exists to fix).
        call_hound_ingest(&prime, &json!({ "path": dir.path().to_str().unwrap() })).await;
        assert!(
            live(&prime).0 > n1,
            "append (no rebuild) should add duplicate nodes"
        );
    }

    #[tokio::test]
    #[ignore = "needs the embedder model (downloads ~25MB on first run); run with --ignored"]
    async fn hound_embed_enables_semantic_recall() {
        // The differentiator over a flat graph file: with `embed`, a meaning-based
        // query finds the right symbol even though the query words don't appear in
        // the identifier.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("auth.rs"),
            "pub fn authenticate(user: &str) -> bool { check_password(user) }\n\
             fn check_password(_u: &str) -> bool { true }\n\
             fn render_button() {}\n",
        )
        .expect("write src");

        let prime = Prime::open_in_memory().await.unwrap();
        let ing = call_hound_ingest(
            &prime,
            &json!({ "path": dir.path().to_str().unwrap(), "embed": true }),
        )
        .await;
        let t = ing["content"][0]["text"].as_str().unwrap();
        // 1 file + 3 fns = 4 symbols embedded.
        assert!(t.contains("\"embedded\": 4"), "expected 4 embedded: {t}");

        // "login authentication" should surface authenticate(), not render_button().
        let rec = call_recall(
            &prime,
            &json!({ "text": "login authentication", "top_k": 3 }),
        )
        .await;
        let t = rec["content"][0]["text"].as_str().unwrap();
        assert!(
            t.contains("authenticate"),
            "semantic recall missed authenticate: {t}"
        );
    }

    fn find_tool<'a>(defs: &'a Value, name: &str) -> &'a Value {
        defs.as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("tool {name} not in definitions"))
    }

    #[test]
    fn prime_embed_schema_does_not_require_vector() {
        // Issue #186: agents can't precompute vectors, so the server must
        // accept `text` alone and embed it in-process. `vector` being in
        // `required` reintroduces the asymmetry.
        let defs = tool_definitions();
        let embed = find_tool(&defs, "prime_embed");
        let required = embed["inputSchema"]["required"].as_array().unwrap();
        assert!(
            required.iter().any(|v| v == "id"),
            "id should still be required"
        );
        assert!(
            !required.iter().any(|v| v == "vector"),
            "vector must NOT be required — text alone is valid input"
        );
    }

    #[test]
    fn prime_recall_schema_does_not_require_vector() {
        let defs = tool_definitions();
        let recall = find_tool(&defs, "prime_recall");
        let required = recall["inputSchema"]
            .get("required")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(
            !required.iter().any(|v| v == "vector"),
            "vector must NOT be required for prime_recall"
        );
    }

    #[test]
    fn prime_embed_description_mentions_text_only_path() {
        let defs = tool_definitions();
        let embed = find_tool(&defs, "prime_embed");
        let desc = embed["description"].as_str().unwrap();
        assert!(
            desc.contains("text"),
            "description should mention text-only embedding path"
        );
    }

    #[test]
    fn prime_recall_description_mentions_text_only_path() {
        let defs = tool_definitions();
        let recall = find_tool(&defs, "prime_recall");
        let desc = recall["description"].as_str().unwrap();
        assert!(
            desc.contains("text"),
            "description should mention text-only query path"
        );
    }

    #[test]
    fn template_tools_are_exposed() {
        let defs = tool_definitions();
        let _ = find_tool(&defs, "prime_list_templates");
        let load = find_tool(&defs, "prime_load_template");
        let required = load["inputSchema"]["required"].as_array().unwrap();
        assert!(
            required.iter().any(|v| v == "name"),
            "prime_load_template should require `name`"
        );
    }

    /// End-to-end via the public dispatch surface: `list_templates` lists every
    /// bundled template; `load_template("contact`") returns the shape an agent
    /// would use to call `prime_add_node` with the right properties + relations.
    #[test]
    fn template_dispatch_round_trip() {
        let listed = call_list_templates();
        let text = listed["content"][0]["text"].as_str().unwrap();
        // Result is JSON or TOON depending on RESULT_FORMAT; default Json.
        assert!(
            text.contains("contact") && text.contains("person") && text.contains("transaction"),
            "list_templates should mention every bundled name; got: {text}"
        );

        let loaded = call_load_template(&json!({ "name": "contact" }));
        let text = loaded["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"entity_type\": \"contact\""));
        assert!(
            text.contains("\"name\"") && text.contains("\"email\""),
            "contact template should declare name + email properties"
        );
        assert!(
            text.contains("suggested_relations"),
            "contact template should suggest relations agents can use"
        );
    }

    #[test]
    fn template_dispatch_unknown_name_errors() {
        let result = call_load_template(&json!({ "name": "nonexistent" }));
        assert_eq!(result["isError"], json!(true));
    }

    #[test]
    fn template_dispatch_missing_name_errors() {
        let result = call_load_template(&json!({}));
        assert_eq!(result["isError"], json!(true));
    }

    // ─── Projection / provenance tool surface ─────────────────────────────

    #[test]
    fn projection_tools_are_exposed() {
        let defs = tool_definitions();
        for tool_name in [
            "prime_define_projection",
            "prime_list_projections",
            "prime_project_node",
            "prime_node_provenance",
        ] {
            let _ = find_tool(&defs, tool_name);
        }
        let define = find_tool(&defs, "prime_define_projection");
        let required = define["inputSchema"]["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "entity_type"));
        assert!(required.iter().any(|v| v == "field_policies"));
    }

    #[test]
    fn entity_type_from_node_id_parses_canonical_format() {
        assert_eq!(
            entity_type_from_node_id("node:contact:abc-123"),
            Some("contact")
        );
        assert_eq!(entity_type_from_node_id("node:person:xyz"), Some("person"));
        // Type segments can contain colons inside the id portion — splitn(3)
        // means everything past the second colon stays in the id.
        assert_eq!(
            entity_type_from_node_id("node:task:id:with:colons"),
            Some("task")
        );
    }

    #[test]
    fn entity_type_from_node_id_rejects_non_node_ids() {
        assert!(entity_type_from_node_id("edge:abc").is_none());
        assert!(entity_type_from_node_id("malformed").is_none());
        assert!(entity_type_from_node_id("node:only-type").is_none()); // no id segment
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // serial test guard, single-threaded — no deadlock
    async fn call_define_projection_parses_policy_strings() {
        let _guard = crate::projection_registry::test_guard();
        crate::projection_registry::clear_for_test();
        let prime = Prime::open_in_memory().await.unwrap();
        let result = call_define_projection(
            &prime,
            &json!({
                "entity_type": "dispatch-test-1",
                "field_policies": {
                    "status": "last_write",
                    "name": "highest_priority",
                    "role": "most_specific",
                    "tags": "merge_array",
                }
            }),
        )
        .await;
        // Should be a success result, not an error
        assert_ne!(result.get("isError"), Some(&json!(true)));
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"entity_type\": \"dispatch-test-1\""));
        assert!(text.contains("\"replaced\": false"));
        // And it should actually be in the registry
        let def =
            crate::projection_registry::get("dispatch-test-1").expect("def should be registered");
        assert_eq!(def.field_policies.len(), 4);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // serial test guard, single-threaded — no deadlock
    async fn call_define_projection_rejects_unknown_policy() {
        let _guard = crate::projection_registry::test_guard();
        crate::projection_registry::clear_for_test();
        let prime = Prime::open_in_memory().await.unwrap();
        let result = call_define_projection(
            &prime,
            &json!({
                "entity_type": "dispatch-test-2",
                "field_policies": { "status": "not_a_real_policy" }
            }),
        )
        .await;
        assert_eq!(result["isError"], json!(true));
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("unknown policy"));
    }

    #[tokio::test]
    async fn inbox_recall_thread_returns_interactions_and_neighbors() {
        let prime = Prime::open_in_memory().await.unwrap();
        let payload = json!({
            "thread_id": "th1", "subject": "Re: Q3", "snippet": "hi",
            "from": { "name": "Dana", "email": "dana@acme.com" },
            "to": [{ "email": "me@all-source.xyz" }],
            "received_at": "2026-06-14T09:12:00Z"
        });
        crate::email_ingester::ingest_email_event(
            &prime,
            "email.received",
            "m1",
            "tnt1",
            &payload,
            crate::email_ingester::IngestOpts { embed: false },
        )
        .await
        .unwrap();

        let result = call_inbox_recall_thread(&prime, &json!({ "conversation_id": "th1" }));
        assert_ne!(result.get("isError"), Some(&json!(true)));
        let parsed: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(parsed["count"], json!(1));
        assert_eq!(parsed["interactions"][0]["message_id"], json!("m1"));
        assert_eq!(
            parsed["thread"]["properties"]["conversation_id"],
            json!("th1")
        );
        // neighbors include the thread + the people (from + to)
        assert!(
            parsed["interactions"][0]["neighbors"]
                .as_array()
                .unwrap()
                .len()
                >= 2
        );
    }

    // ─── Result windowing and the drill-down contract ────────────────────

    /// Add `n` symbol nodes and return the `Prime` holding them.
    async fn prime_with_symbols(n: usize) -> Prime {
        let prime = Prime::open_in_memory().await.unwrap();
        for i in 0..n {
            prime
                .add_node(
                    "function",
                    json!({ "name": format!("fn_{i}"), "file": "src/lib.rs", "line": i }),
                )
                .await
                .unwrap();
        }
        prime
    }

    fn parse_result(result: &Value) -> Value {
        assert_ne!(result.get("isError"), Some(&json!(true)), "{result}");
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[tokio::test]
    async fn search_caps_rows_but_reports_the_true_total() {
        let prime = prime_with_symbols(120).await;
        let parsed = parse_result(&call_search(&prime, &json!({ "type": "function" })));

        assert_eq!(parsed["nodes"].as_array().unwrap().len(), 50);
        assert_eq!(parsed["total"], json!(120));
        assert_eq!(parsed["next_offset"], json!(50));
        assert!(parsed["note"].as_str().unwrap().contains("offset=50"));
    }

    #[tokio::test]
    async fn search_offset_walks_to_the_end_and_the_last_page_says_it_is_last() {
        let prime = prime_with_symbols(120).await;
        let parsed = parse_result(&call_search(
            &prime,
            &json!({ "type": "function", "offset": 100 }),
        ));

        assert_eq!(parsed["nodes"].as_array().unwrap().len(), 20);
        assert_eq!(parsed["total"], json!(120));
        assert!(parsed.get("next_offset").is_none());
    }

    /// The reported symptom: a search row whose `id` no other tool accepts
    /// forces the client to keep every row it was given.
    #[tokio::test]
    async fn a_search_row_id_is_feedable_straight_back_into_neighbors() {
        let prime = prime_with_symbols(1).await;
        let parsed = parse_result(&call_search(&prime, &json!({ "type": "function" })));
        let id = parsed["nodes"][0]["id"].as_str().unwrap().to_string();

        assert!(id.starts_with("node:function:"), "not a wire id: {id}");
        assert_ne!(
            call_neighbors(&prime, &json!({ "node_id": id })).get("isError"),
            Some(&json!(true)),
            "the id search returned was rejected by prime_neighbors"
        );
    }

    #[tokio::test]
    async fn search_defaults_to_summary_rows_and_full_is_opt_in() {
        let prime = prime_with_symbols(1).await;

        let summary = parse_result(&call_search(&prime, &json!({ "type": "function" })));
        assert_eq!(summary["nodes"][0]["name"], json!("fn_0"));
        assert_eq!(summary["nodes"][0]["at"], json!("src/lib.rs:0"));
        assert!(summary["nodes"][0].get("properties").is_none());

        let full = parse_result(&call_search(
            &prime,
            &json!({ "type": "function", "fields": "full" }),
        ));
        assert_eq!(full["nodes"][0]["properties"]["name"], json!("fn_0"));
    }

    // ─── Recall degradation ──────────────────────────────────────────────

    /// A `node_type`-only recall takes no embedder path, so this pins the
    /// MCP-layer shaping (retrieval mode + banner) without a model on disk.
    /// The lexical arm itself is covered in core's `recall_degradation_tests`.
    #[tokio::test]
    async fn a_type_only_recall_reports_its_retrieval_mode_and_banner() {
        let prime = Prime::open_in_memory().await.unwrap();
        prime
            .add_node("person", json!({ "name": "Alice" }))
            .await
            .unwrap();

        let parsed = parse_result(&call_recall(&prime, &json!({ "node_type": "person" })).await);

        assert_eq!(parsed["retrieval"], json!("type_scan"));
        assert_eq!(parsed["nodes"].as_array().unwrap().len(), 1);
        assert!(
            parsed["degraded"]
                .as_str()
                .unwrap()
                .contains("nothing matched")
        );
    }

    #[tokio::test]
    async fn recall_with_no_retrieval_input_at_all_still_errors() {
        let prime = Prime::open_in_memory().await.unwrap();
        let result = call_recall(&prime, &json!({ "depth": 2 })).await;
        assert_eq!(result["isError"], json!(true));
    }

    #[test]
    fn the_degraded_note_names_the_recovery_command() {
        let note = degraded_note(Retrieval::Lexical, Some("cannot fetch model.onnx")).unwrap();
        assert!(note.contains("--mode warm"), "{note}");
        assert!(note.contains("PRIME_EMBED_MODEL_DIR"), "{note}");
        assert!(note.contains("cannot fetch model.onnx"), "{note}");
    }

    #[test]
    fn a_semantic_retrieval_produces_no_note() {
        assert!(degraded_note(Retrieval::Semantic, None).is_none());
        assert!(degraded_note(Retrieval::Empty, None).is_none());
    }

    #[test]
    fn search_schema_advertises_the_window_and_the_drill_down() {
        let defs = tool_definitions();
        let search = find_tool(&defs, "prime_search");
        let props = &search["inputSchema"]["properties"];
        for arg in ["limit", "offset", "fields"] {
            assert!(!props[arg].is_null(), "prime_search must advertise {arg}");
        }
        let desc = search["description"].as_str().unwrap();
        assert!(
            desc.contains("total") && desc.contains("offset"),
            "the description must tell the model how to page: {desc}"
        );
    }

    #[tokio::test]
    async fn inbox_recall_thread_missing_arg_errors() {
        let prime = Prime::open_in_memory().await.unwrap();
        let result = call_inbox_recall_thread(&prime, &json!({}));
        assert_eq!(result["isError"], json!(true));
    }

    #[test]
    fn build_drafted_event_builds_contract_payload() {
        let (draft_id, payload, metadata) = build_drafted_event(
            &json!({ "thread_id": "th1", "body": "thanks", "intent": "accept renewal" }),
        )
        .unwrap();
        assert!(draft_id.starts_with("draft_"));
        assert_eq!(payload["thread_id"], json!("th1"));
        assert_eq!(payload["body"], json!("thanks"));
        assert_eq!(payload["intent"], json!("accept renewal"));
        assert_eq!(payload["by"], json!("claude"));
        assert!(payload["drafted_at"].is_string());
        assert_eq!(metadata["draft_id"], json!(draft_id));
    }

    #[test]
    fn build_drafted_event_requires_body() {
        let err = build_drafted_event(&json!({ "thread_id": "t", "intent": "x" })).unwrap_err();
        assert!(err.contains("body"));
    }

    #[tokio::test]
    async fn inbox_draft_without_sync_errors() {
        // No core_writer configured in tests -> the guard returns an error.
        let result =
            call_inbox_draft(&json!({ "thread_id": "t", "body": "b", "intent": "i" })).await;
        assert_eq!(result["isError"], json!(true));
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("sync")
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // serial test guard, single-threaded — no deadlock
    async fn call_define_projection_replaces_existing_and_flags_it() {
        let _guard = crate::projection_registry::test_guard();
        crate::projection_registry::clear_for_test();
        let prime = Prime::open_in_memory().await.unwrap();
        // First definition: replaced=false
        call_define_projection(
            &prime,
            &json!({
                "entity_type": "dispatch-test-3",
                "field_policies": { "status": "last_write" }
            }),
        )
        .await;
        // Second definition for same type: replaced=true
        let result = call_define_projection(
            &prime,
            &json!({
                "entity_type": "dispatch-test-3",
                "field_policies": { "name": "highest_priority" }
            }),
        )
        .await;
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"replaced\": true"));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // serial test guard, single-threaded — no deadlock
    async fn call_list_projections_returns_every_registered_def() {
        let _guard = crate::projection_registry::test_guard();
        crate::projection_registry::clear_for_test();
        let prime = Prime::open_in_memory().await.unwrap();
        call_define_projection(
            &prime,
            &json!({
                "entity_type": "dispatch-test-4a",
                "field_policies": { "f": "last_write" }
            }),
        )
        .await;
        call_define_projection(
            &prime,
            &json!({
                "entity_type": "dispatch-test-4b",
                "field_policies": { "f": "merge_array" }
            }),
        )
        .await;
        let result = call_list_projections();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("dispatch-test-4a"));
        assert!(text.contains("dispatch-test-4b"));
        // Policy strings should be the wire-format snake_case
        assert!(text.contains("last_write"));
        assert!(text.contains("merge_array"));
    }
}
