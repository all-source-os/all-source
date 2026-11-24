# Prime typed writes and provenance request

The user authorized `feat/prime-typed-provenance` and one AllSource PR, delivered through the native Allstack plan → build → verify → ship flow with Codex and Claude Code. Source: a comparison of what HASH (hash.ai) ships for AI users against AllSource, 2026-11-04, with every AllSource claim below re-read on `origin/main`.

Out of scope: a second time axis (valid time). Another thread owns it on `feat/bitemporal-time-axes` (plan `docs/plans/2026-11-04-bitemporal-time-axes.md` there, no code commits yet). That thread owns **time semantics**, not files. Shared files (`facade.rs`, `hosted.rs`, `declarative.rs`, `types.rs`, `tools.rs`) may be edited here. Do not change: `as_of` filtering, `Observation.observed_at` and what feeds it, event or node timestamps, `get_node_as_of`, `neighbors_as_of`, `history`, `timeline`, `diff`, recall recency, or any `valid_*`/`recorded_*` field. Store the assertion under its own `metadata.assertion` key and leave the payload shape alone, so a later `valid_at` can sit beside it without a merge decision.

## Coordinator decisions

- **Import priority:** `Import` derives `source_priority` 500: above an agent write (100), below a user correction (1000). An explicit `source_priority` on the assertion overrides the derived value.
- **Enforcement default:** every store, new or existing, starts `warn` (changed from `open`; see [the 2026-11-06 gap status](../reports/2026-11-06-prime-hash-gap-status.md)). `strict` and `warn` are set explicitly per store and tenant by a persisted `prime.enforcement.set` event (MCP and HTTP), and the latest such event wins on replay. No newness detection.
- **Hosted tenant:** the tenant is the one bound to the authenticated credential that the hosted path already resolves (`HostedPrime` tenant, `prime-mcp` hosted mode with `CORE_URL` + `PRIME_API_KEY`). A caller-supplied tenant parameter never establishes scope. Embedded unscoped reads keep current behavior.
- **Legacy provenance:** events written before this change report `source: unknown`, not a defaulted agent.

## Complete application job

An agent writes facts into Prime through MCP or HTTP, embedded or hosted, and gets back:

1. A refusal naming the field, the expected type and the received value when the write breaks the declared type of the node or edge.
2. For any field of any node, the event that set it, who asserted it (user, agent, extraction, import), the model and run that produced it, what it was derived from, and its confidence. A user correction outranks an AI extraction under `highest_priority` even when the extraction is newer.
3. Semantic and hybrid search through the Elixir MCP tools that answer from Core instead of a 404.
4. No advertised MCP tool that only returns "not yet implemented".
5. In hosted mode, no read that silently spans every tenant.

## Findings on origin/main

### Provenance is plumbed but never written

- `Observation` carries `source_priority`, `specificity_score`, `source_event_id` (`apps/core/src/prime/projections/declarative.rs:74-92`).
- `Prime::observations` reads priority and specificity from event metadata (`apps/core/src/prime/facade.rs` `observations`).
- Every node and edge write ingests `metadata: None`: embedded `add_node`, `update_node`, `add_edge` (`facade.rs:456`, `:505`, `:856`), the conversation-scoped writers (`facade.rs:2471`, `:2499`, `:2542`) and hosted (`apps/core/src/prime/hosted.rs:121`, `:199`, `:222`, `:328`, `:351`).
- No writer sets `source_priority`. `highest_priority` therefore degrades to `last_write` in production; the passing test `highest_priority_user_correction_beats_ai_extraction` constructs observations by hand.
- Confidence exists only as free JSON in `prime_embed` metadata (`apps/prime-mcp/src/tools.rs:234`).
- Optional LLM document extraction records `prime.extraction.usage` (`apps/prime-mcp/src/doc_extract.rs`) but stamps no run id on the nodes it creates.

### Types are required-fields only

- `apps/core/src/prime/schema.rs` checks only `{"required": [...]}` presence; no property types, enums, ranges, versions, edge endpoint types or cardinality.
- A type with no schema accepts anything (`schema.rs` `validate_node`).
- Embedded writes validate (`facade.rs:442`, `:498`, `:832`). Hosted writes (`hosted.rs` `add_node`, `add_edge`, `update_node`) validate nothing.
- Schema registration ingests with `tenant_id: None` (`facade.rs:389`).
- No `prime_*` MCP tool registers or lists a schema.
- `configure_exclusive` (`facade.rs:406`) is a runtime call, not part of a declared edge type.

### Search tools call routes Core does not define

- `apps/mcp-server-elixir/lib/mcp_server_elixir/infrastructure/core_client.ex:289` GETs `/api/v1/search/semantic`; `:325` POSTs `/api/v1/search/hybrid`. No Core router defines either path.
- `apps/core/src/infrastructure/search/hybrid_search_engine.rs` and `application/use_cases/semantic_search.rs` exist with tests; only `/api/v1/prime/vectors/search` (`apps/core/src/infrastructure/web/prime_api.rs:62`) is routed.

### Stub tools are advertised

- `core_client.ex:418`, `:432` and `mcp_tools.ex` (`backup_*`) return "Backup API is not yet implemented in AllSource Core".

### Lint inherited from trunk

- `cargo clippy -p allsource-core --features prime-recall,prime-vectors --lib --tests -- -D warnings` on the pinned toolchain fails on `apps/core/src/prime/vectors/embedder.rs:220` (`manual_is_variant_and`). Fix it in this PR.

## Accepted scope

### A. One write contract (`Assertion`)

- A typed `Assertion { source: User | Agent | Extraction | Import, actor, model, run_id, derived_from, confidence }`, `confidence` validated to `0.0..=1.0`.
- Every node and edge write in `facade.rs` and `hosted.rs` accepts an `Assertion` and stores it in event **metadata**, not the payload. Existing signatures keep working with a default agent assertion.
- `source_priority` is derived from `source` unless given: user 1000, agent 100, extraction 0 (the convention at `declarative.rs:78-79`).
- `Observation` and `Provenance` carry the assertion fields; `prime_node_provenance` and the HTTP provenance route return them. Confidence does not take part in winner selection; the order-independence contract stays intact.
- `MergeArray` provenance returns its contributing events instead of nothing.
- Document extraction records a `prime.extraction.run` (model, input source, prompt digest, usage) and stamps its `run_id` and `source: extraction` on every node and edge it writes.
- MCP: `prime_add_node`, `prime_add_edge` and the update path accept optional `source`, `actor`, `model`, `run_id`, `derived_from`, `confidence`.

### B. Typed node and edge kinds

- Property kinds `string | number | integer | boolean | datetime | node_ref | enum` with `required`, `min`/`max`, `max_length`, implemented in `schema.rs` without a new JSON Schema dependency. Existing `{"required": [...]}` schemas keep validating unchanged.
- Edge kinds declare `from_types`, `to_types` and `cardinality: one | many`; `cardinality: one` drives the existing contradiction projection.
- Kinds carry a `version`; a re-registration that breaks compatibility is refused (reuse `CompatibilityMode` from `apps/core/src/application/services/schema.rs`). Node events record the kind version they were validated against. Replay never re-validates.
- A property may declare `requires_assertion: [source, confidence]`, refusing writes without them.
- Enforcement mode per store and tenant: `open` (current behavior; typed constraints not enforced), `warn` (violations recorded, write accepted), `strict` (violations and unknown types refused). Set explicitly; see coordinator decisions.
- One validator shared by embedded and hosted writes, so the two paths cannot diverge.
- Kind registration is tenant-scoped.
- Errors name the field, expected kind and received value.
- MCP `prime_define_type` and `prime_list_types`; matching HTTP routes under `/api/v1/prime/`.

### C. Search reachable from the Elixir MCP

- Either Core serves `/api/v1/search/semantic` and `/api/v1/search/hybrid` backed by the existing semantic-search use case and `HybridSearchEngine`, or the Elixir client calls routes Core already serves. The plan chooses and states why.
- A test drives the Elixir client path against a response shape Core actually returns.

### D. No stub tools

- `backup_*` and any other tool whose only body is a not-implemented message are removed from the advertised tool list (or implemented). A test asserts no listed tool returns a stub message.

### E. Tenant scoping in hosted mode

- Read the existing tenant-isolation tests first (`recall_is_tenant_isolated`, `full_graph_is_tenant_isolated`, `recall_never_returns_another_tenants_nodes`). Where a hosted read can still run with no tenant filter and span the store (`facade.rs` tenant-filter note), hosted mode refuses it.

### F. OpenAPI for Prime routes

- `/api/v1/prime/*` publishes an OpenAPI document generated from the handlers, served from a Prime route, with a test that every routed Prime path appears in it. If generation needs a new dependency, the plan names it and its cost.

## Acceptance

Tests carry BDD names and run in CI: the Core test step and the `prime-quality` job in `.github/workflows/ci.yml`.

- A user correction written before a newer extraction wins under `highest_priority`, written through the public write API, not hand-built observations; provenance returns the user's actor and source.
- Provenance for an extracted field returns model, run id, derived-from and confidence.
- Invalid confidence (`1.5`, `-0.1`) is refused.
- A wrong property type, an out-of-range value, a bad enum value, an edge between disallowed node types and a second target on a `one` edge are each refused in `strict` with a message naming the field and value; the same writes pass in `open`.
- Embedded and hosted refuse the same invalid write identically.
- A legacy `{"required": [...]}` schema still validates as before.
- A breaking kind re-registration is refused; a compatible one is accepted; old events replay.
- The Elixir semantic and hybrid client functions hit a path Core routes.
- No advertised Elixir tool returns a not-implemented message.
- A hosted tenant-less read is refused.
- Every routed Prime path appears in the OpenAPI document.
- All policy gates pass on the exact candidate commit, including the clippy fix.

Publish one verified commit and one open PR; read back its remote head. No merge, deployment, hosted credential or sync change is requested.
