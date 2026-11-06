# Prime against HASH: gap status, 2026-11-06

Status of the seven gaps found when AllSource Prime was compared with HASH (hash.ai) on 2026-11-04, after #388, #389 and the branch `feat/prime-hash-gaps`. Every AllSource row was read on the code; every HASH row was re-checked against the `hashintel/hash` main branch on 2026-11-06 by one research pass and one independent refutation pass per claim.

## What is deployed

Neither #388 nor #389 is live.

| Service | Running image | Built from |
|---|---|---|
| `allsource-core` (Fly) | `allsource-core-alpine`, v0.27.0 | `16dd13eb`, 2026-10-30 |
| `allsource-prime` (Fly) | `allsource-prime:sha-fe105449` | `fe105449`, 2026-10-24 |

The Prime image has not built on `main` since #389: the `Build prime` job of Docker Build failed on `019b7e57` and `7deb3617`, both in the `Cache Prime Core library` step, where `cargo build --release -p allsource-core --lib` hit its 260 s timeout (exit 124). The Dockerfile copied all of `apps/core/` before compiling anything, so every Core change recompiled the whole dependency graph inside that one step. This branch moves the dependency build onto manifests and stub sources in three resumable phases, so a source change no longer invalidates them.

Read with `fly image show -a allsource-core` and `fly image show -a allsource-prime`.

## The seven gaps

| Gap | HASH (verified) | AllSource after this branch |
|---|---|---|
| Valid time beside record time | Both axes stored on `entity_temporal_metadata`; the store and the legacy `POST /entities/query` pin either axis | Done in #388: `valid_from`/`valid_to` beside record time, queries and folds on both axes |
| Typed node, edge and property kinds | Writes validated against SemType entity, property and data types by default (`draft` is an opt-in relaxation). Link types are entity types in a link position, not a fourth kind | Typed kinds from #389. A tenant that never set a mode is now under `warn`: nothing is refused, every violation is recorded on the event. HASH refuses by default; AllSource refuses only under `strict` |
| Who wrote each fact | Actor always taken from the authenticated account (`create.rs:418`), never from the caller. Sources and confidence optional | Hosted node and edge creates and updates now record `recorded_by`, the user the Control Plane authenticated, beside the caller's claimed `actor`. The caller cannot set it through Prime. Deletes, kind, enforcement, projection and embed writes do not record it yet |
| Semantic and hybrid search | `POST /entities/search` over pgvector with OpenAI `text-embedding-3-large` | Core routes from #389. Checked against a running Core: hybrid answers keyword-only, semantic answers 503 because no Core image is built with an embedding model. See "Still open" |
| URL or document to typed entities | Browser plugin sends web pages only; the worker writes the page URL into each inferred property's provenance. OpenAI models only | An `extraction` write must now name its `run_id` and a non-empty `derived_from`, on every write path. Fetching a URL stays with the calling agent |
| Tenant scope in hosted mode | Per-web policies in Cedar (`policies.cedarschema`), evaluated in process, with User, Machine and AI principals | Done in #389: hosted Prime refuses a blank tenant and one that differs from the authenticated tenant. No per-principal policy inside a tenant |
| OpenAPI | Public REST v1 | Done in #389: OpenAPI 3.1 for Core and Prime, contract tests fail on an undocumented route |

HASH ships no MCP server: its Linear and Notion MCP servers were removed in `2ed2383b7a` (2026-08-10). HASH's graph, API and frontend are AGPL-3.0; the repository default is MIT or Apache-2.0.

## What this branch changes

- **Prime image build.** `apps/prime-mcp/Dockerfile` builds dependencies from manifests and empty stand-ins in `builder-deps-warm`, `builder-deps-resume` and `builder-deps`; a phase that reaches its deadline exits 0 and the next resumes. Real sources are copied and touched only in `builder-core`. `docker-build.yml` runs one step per phase, each importing its own previous cache and the phase before it.
- **`warn` by default.** `EnforcementMode::default()` is `Warn`. Hosted edge writes require live endpoints only when the relation has a kind or the tenant is `strict`, so `warn` refuses nothing.
- **`recorded_by`.** A new `Assertion` field the server sets. The Control Plane forwards `X-User-Id` beside `X-Tenant-Id` under `PRIME_API_KEY`; Prime binds it only after the credential matches and stamps it on every hosted node and edge create and update, with an `agent` assertion when the caller sent none.
- **Extraction contract.** `Assertion::validate` refuses `source: extraction` without `run_id` or without a non-blank `derived_from` entry. `doc_extract` already sent both.
- **Search over real HTTP.** `test/mcp_server_elixir/integration/live_core_search_test.exs` ingests an event through the Elixir client and finds it through hybrid search on a running Core, and checks semantic search answers for the model that Core was built with. CI runs it against `allsource-core-community:latest` as a service container.

## Still open

- **Semantic search on hosted Core** (`water`). The Core image builds `enterprise,analytics` on Alpine (musl). `prime-vectors` pulls `fastembed`, whose ONNX runtime is distributed as glibc binaries, so the Alpine image cannot simply turn it on. Options to spike: a glibc runtime image for Core, the `prime-remote-embed` path without the in-process model, or serving semantic search from Prime, which already embeds.
- **Refusing by default** (`rock` once `warn` has run). `warn` records what `strict` would refuse. Reading the recorded `validation_warnings` per tenant says which tenants can move to `strict` without a refused write.
- **`recorded_by` on every write** (`rock`). `prime_forget`, edge deletes, kind registration, enforcement changes, projection definitions and embeddings go through `HostedPrime` methods that take no metadata, so they record no principal.
- **Raw ingest can forge provenance** (`sand`). A tenant member can post a `prime.node.created` event straight to Core through the Control Plane's event ingest, with any `metadata.assertion`, including `recorded_by`. It stays inside that tenant. `recorded_by` is trustworthy only for writes made through Prime; closing this means Core or the Control Plane refusing `prime.*` events on raw ingest.
- **Per-principal authorization inside a tenant** (`sand`). HASH has it (Cedar policies per web, team and role); AllSource scopes by tenant only. `recorded_by` gives the principal a policy would key on.
- **Deploying.** The Prime image builds again only after this merges; promotion to Fly is the `Promote Fly Image` workflow, run by hand.

## Behaviour changes

- Tenants and embedded stores that never set a mode now record `validation_warnings` on writes that break a registered kind or use an unregistered type. The mode change refuses no write that was accepted before. This replaces the decision in `docs/reference/prime-typed-provenance-request.md` that every store starts `open`.
- A tenant explicitly set to `warn` no longer has hosted edges to missing endpoints refused.
- Any caller sending `source: extraction` without `run_id` and `derived_from` is now refused with the field named.

## What was not checked

- The Docker phases on a GitHub runner. Every phase was built locally on arm64 with 12 cores (dependencies 1m41s, Core library 1m09s, binary 23s) and the binary ran `--help`; the 4-core amd64 runner is unmeasured until Docker Build runs on `main`.
- The new CI step itself; the live test was run locally against `allsource-core-community:sha-7deb361`, the image CI pins, not inside the Actions service container.
- A Control Plane deploy carrying `X-User-Id`. Until it ships, hosted writes record no `recorded_by`.
- HASH's hosted product beyond the repository, and hash.ai documentation pages that returned 404.
