---
title: "AllSource Documentation"
status: CURRENT
last_updated: 2026-11-24
---

# AllSource Documentation

Index of the live documents under `docs/`. Archived material is listed in [ARCHIVE_INDEX.md](ARCHIVE_INDEX.md) and is not indexed here.

## Conventions

- **Dated documents** use `YYYY-MM-DD-slug.md` under `plans/`, `reports/` and `research/`. Decision records under `decisions/` use the same form.
- **Reference documents** under `current/`, `guides/`, `runbooks/` and `contracts/` keep a descriptive name without a date and are updated in place.
- **ADRs** are `adr/NNN-slug.md`, numbered in sequence. `adr/README.md` is the index.
- **Evidence** is kept in `evidence/<YYYY-MM-DD>-<slug>/` with a `README.md` and the raw outputs it cites. Reports link to the bundle.
- **Archive**: a superseded or completed document moves to `archive/<YYYY-MM-DD>/` and keeps its original path below that directory. `ARCHIVE_INDEX.md` records the reason and the successor for each move.
- **Status** is stated in each document, in frontmatter `status:` or a `Status:` line.
- **Links** are relative. Each link must resolve to a file in the repository.

---

## Getting started

- [Quick Start](QUICK_START.md): get started with AllSource in under five minutes using curl.
- [Quick Start guide](guides/QUICK_START.md): add `allsource-core` to a Rust project and run a first store.
- [Glossary](GLOSSARY.md): definitions of the terms used across the project.
- [Vision](VISION.md): product vision, version 1.0.
- [Monorepo structure](MONOREPO_STRUCTURE.md): the authoritative layout rules for `apps/`, `crates/`, `sdks/`, `packages/` and `docs/`.
- [Self-hosting](self-hosting/README.md): running the Community Edition as a single-node event store with an API gateway.
- [Changelog](CHANGELOG.md): release notes for AllSource Chronos.
- [Embedded Prime integration](guides/EMBEDDED_PRIME_INTEGRATION.md): using Prime as an embedded library with graph, vectors and tiered recall.
- [allsource-mcp](guides/ALLSOURCE_MCP.md): local MCP server that reads Core WAL and Parquet files for debugging.
- [Claude Code skills](guides/CLAUDE_CODE_SKILLS.md): repository skills for testing, release and diagnostics.
- [Claude Desktop setup](guides/mcp-server/CLAUDE_DESKTOP_SETUP.md): connecting the Elixir MCP server to Claude Desktop.
- [MCP tools quick reference](guides/mcp-server/QUICK_REFERENCE.md): the MCP tools on one card.
- [Prime agent prompt](guides/PRIME_AGENT_PROMPT.md): system prompt text that teaches an agent to use Prime as persistent memory.
- [Registered client keys](guides/REGISTERED_CLIENT_KEYS.md): generating a P-256 key, computing the `kid`, registering it, and configuring SDKs and allsource-mcp.
- [Demos](demos/README.md): Prime agent memory in 60 seconds, without Docker.
- [Recording the chronis demo](demos/RECORDING.md): steps to record the chronis demo.
- [Decision-history restart proof](demos/decision-history-proof.md): an offline event-sourcing example.
- [Event-store restart proof](demos/event-store-restart-proof.md): rebuilding inventory from events after a restart, on a synthetic workload.
- [Demo accounts](demo/DEMO_ACCOUNTS.md): how demo tenants work end to end and how the daily reset runs.
- Demo scripts: [prime-mcp-test.sh](demos/prime-mcp-test.sh) and [tui-demo.sh](demos/tui-demo.sh).

---

## Architecture and decisions

### Architecture Decision Records

- [ADR index](adr/README.md): all 21 ADRs.
- [ADR-001](adr/001-embedded-core-library.md): embedded Core library API (accepted).
- [ADR-002](adr/002-crash-safe-compaction.md): crash-safe token compaction.
- [ADR-003](adr/003-batch-ingestion-single-lock.md): batch ingestion with a single lock acquisition.
- [ADR-004](adr/004-projection-backfill.md): projection backfill on registration.
- [ADR-005](adr/005-postgresql-removal-event-sourced-metadata.md): PostgreSQL removal and event-sourced metadata.
- [ADR-006](adr/006-server-side-projections-fold-on-read.md): server-side projections with fold-on-read (partly superseded by ADR-021).
- [ADR-007](adr/007-domain-value-objects.md): domain value objects for type safety.
- [ADR-008](adr/008-vector-search-fastembed.md): vector search with fastembed, pure Rust.
- [ADR-009](adr/009-simd-json-zero-copy-deserialization.md): simd-json zero-copy deserialization.
- [ADR-010](adr/010-native-arm64-ci.md): native ARM64 CI runners.
- [ADR-011](adr/011-prime-entity-id-enum.md): typed `EntityId` enum for Prime.
- [ADR-012](adr/012-prime-directed-adjacency.md): unified `DirectedAdjacencyProjection`.
- [ADR-013](adr/013-prime-atomic-deletion.md): atomic node deletion via batch ingestion.
- [ADR-014](adr/014-prime-facade-decomposition.md): Prime facade decomposition into sub-API modules (accepted, not implemented).
- [ADR-015](adr/015-prime-vector-index-generation.md): generation counter for `VectorIndexProjection`.
- [ADR-016](adr/016-prime-eventual-consistency.md): eventual consistency model for Prime.
- [ADR-017](adr/017-chronis-prime-integration.md): Chronis and Prime integration.
- [ADR-018](adr/018-embedded-eager-parquet-hydration.md): eager Parquet hydration on embedded boot.
- [ADR-019](adr/019-compile-time-edition-gating.md): edition gating at compile time, not at runtime.
- [ADR-020](adr/020-prime-stateless-over-core.md): Prime as a stateless engine over Core.
- [ADR-021](adr/021-views-folded-by-query-service.md): views folded by the Query Service and stored in Core.

### Decisions

- [Complete Android and iOS delivery](decisions/2026-09-21-complete-mobile-delivery.md): the decision date and scope of complete mobile delivery.
- [Chat delivery amendment](decisions/2026-09-22-chat-delivery.md): prioritises OpenAI/ChatGPT and Claude integrations, customers first.
- [Tenant clients use registered ES256 keys](decisions/2026-11-07-tenant-clients-registered-es256-keys.md): accepted; clients sign 60-second assertions with a key registered on `clientkey:<tenant>`.

### Current architecture

- [Tenant architecture](current/TENANT_ARCHITECTURE.md): tenant management on event-sourced system streams in Core, with a stateless Query Service.
- [Billing architecture](current/BILLING_ARCHITECTURE.md): billing state, current as of 2026-03-26, with the planned event-sourced migration.
- [Production data flow (C4)](current/PROD_DATA_FLOW_C4.md): production data flow across the services.
- [Prime C4 architecture](current/prime-c4-architecture.md): Prime architecture diagrams in Mermaid.
- [Storage format](current/STORAGE_FORMAT.md): how events move from ingest to durable storage and how to read the files.

---

## Contracts and reference

### API reference

| Component | Path | Role |
|-----------|------|------|
| Core | [apps/core](../apps/core/README.md) | Rust event store: WAL, Parquet and DashMap, with the Prime graph, vector and recall modules |
| Query Service | [apps/query-service](../apps/query-service/README.md) | Elixir/Phoenix read plane and API gateway, stateless over Core |
| Control Plane | [apps/control-plane](../apps/control-plane/README.md) | Go service for auth, onboarding, billing and administration |
| Auth | [apps/auth](../apps/auth/) | Rust auth service (better-auth adapter for AllSource) |
| Prime MCP | [apps/prime-mcp](../apps/prime-mcp/README.md) | Rust MCP server for Prime, over stdio and HTTP |
| MCP server | [apps/mcp-server-elixir](../apps/mcp-server-elixir/README.md) | Elixir MCP server |
| Web | [apps/web](../apps/web/README.md) | Next.js marketing site and customer dashboard |
| Admin | [apps/admin](../apps/admin/) | Next.js admin console |
| Chronis | [apps/chronis](../apps/chronis/README.md) | Event-sourced task CLI (`cn`) |

Client SDKs live under `sdks/`: [Rust](../sdks/rust/README.md), Go (`sdks/go/`), Python (`sdks/python-client/`), TypeScript (`sdks/typescript/`) and the shared `sdks/contract/`.

### Wire contracts

- [Latest-state read contract](contracts/latest-state/README.md): entity state without history, ETag and 304, event feed, read models, views, latest and changes.
- [Read-cost contract](contracts/read-cost.md): caller and read-id headers, stream classes, the `/api/v1/read-costs` and `/api/v1/streams?with_bytes` endpoints, and the MCP cost tools.
- [Email event contract](contracts/email-events/README.md): the durable `email.*` Core events, with examples and JSON schemas in the same folder.
- [GEO event contract](contracts/geo-events/README.md): the durable `geo.*` Core events, with `discovery-sources.json`, examples and schemas.

### Reference

- [API reference](current/API_REFERENCE.md): the three core services, their ports and endpoints.
- [Query Service API reference](allsource-qs-api-reference.md): endpoints verified for building `QueryServiceClient`, version 0.30.0.
- [Authentication chain](current/AUTH_CHAIN.md): `X-API-Key` authentication across Core, the Query Service and clients.
- [Event store features](current/EVENT_STORE_FEATURES.md): the event store's capabilities.
- [Query patterns](current/QUERY_PATTERNS.md): common query patterns, saga orchestration and entity deduplication for consumers.
- [Real-time streaming](current/websocket-streaming.md): event streaming over Phoenix Channels.
- [Glossary](GLOSSARY.md) and [monorepo structure](MONOREPO_STRUCTURE.md) are listed under Getting started.

### Use cases

- [Custom projections with `ProjectionWorker`](use-cases/custom-projections.md): long-lived read models fed by live events through Core's durable-consumer protocol.
- [Agent persistence use cases](use-cases/AGENT_PERSISTENCE_USE_CASES.md): AllSource as the database for agents.
- [AI agents syncing tasks to a team tenant](use-cases/AGENT_TEAM_AUTH.md): agent authentication for a team tenant.

---

## Operations and runbooks

### Deployment and releases

- [Docker images](docker-images.md): the official images for the AllSource services.
- [Docker deployment](deployment/DOCKER.md): container images and configuration.
- [`api.all-source.xyz` subdomain setup](deployment/API_SUBDOMAIN_SETUP.md): pointing the public API subdomain at the gateway.
- [Release guide](guides/RELEASE.md): the release workflow for the monorepo.
- [Release procedure](guides/RELEASE_PROCEDURE.md): cutting a new release of the monorepo.
- [Branch protection](guides/BRANCH_PROTECTION.md): branch protection settings.
- [Quality gates setup](guides/QUALITY_GATES_SETUP.md): the quality gates configured for CI and pre-push.
- [Quality gates](current/QUALITY_GATES.md): the CI and quality enforcement standards for Core.
- [Performance guide](current/PERFORMANCE.md): optimisation and tuning.
- [Dependency audit](current/DEPENDENCY_AUDIT.md): the compile-time baseline and dependency audit.
- [Storage layout migration](migrations/STORAGE_LAYOUT_FLAT_TO_TENANT.md): flat to tenant-partitioned Parquet layout, ready since 2026-04-26.
- [Manual and external actions](manual-and-external-actions.md): actions that need account authority, publication or a third-party response, with the evidence each one must leave.

### Configuration and access

- [OAuth and secrets](guides/OAUTH_SECRETS.md): configuring the Google and GitHub OAuth secrets for the stack.
- [WebSocket configuration](guides/WEBSOCKET_CONFIGURATION.md): real-time event streaming between services.
- [Troubleshooting](guides/TROUBLESHOOTING.md): common issues, with diagnostic commands and steps.
- [Projection debugging](current/PROJECTION_DEBUGGING.md): verifying projection state against raw events and fixing stale projections.
- [Analytics accuracy](ANALYTICS_ACCURACY.md): how the web analytics loads and what it sends.

### Monitoring and cost

- [Alerts](operations/ALERTS.md): the alerts defined in the repository, their thresholds and escalation.
- [Cold tier](operations/COLD_TIER.md): archiving events past retention to S3 or R2, with the environment variables and IAM policy.
- [Business dashboards](runbooks/BUSINESS_DASHBOARD.md): the five dashboards in Fly's managed Grafana.
- [Bandwidth alerts](runbooks/BANDWIDTH_ALERTS.md): control-plane edge-output thresholds and private email delivery.
- [Read cost](runbooks/READ_COST.md): finding the costliest callers and streams with `read_costs`, `stream_sizes` and `cap_check`.
- [SLO and SLA](runbooks/SLO_SLA.md): a stub; targets and alerting are still to be written.
- [PostHog analytics](runbooks/POSTHOG_ANALYTICS.md): web acquisition and product-UX analytics on PostHog Cloud EU.
- [Dashboard data audit](checklists/dashboard-data-audit.md): audit of the hosted dashboard data for one tenant.
- [Admin console health](runbooks/ADMIN_HEALTH.md): verifying that every admin page returns real data.
- [Fleet health and recovery](runbooks/FLEET_HEALTH_RECOVERY.md): the operator surface catalogue for fleet health and recovery.
- [Diagnosing pipeline data visibility](runbooks/DIAGNOSING_PIPELINE_DATA_VISIBILITY.md): when stored data does not show in a downstream surface.
- [Partnerships workspace](runbooks/PARTNERSHIPS.md): the admin workspace for commercial channels.
- [Proactive-comms efficiency engine](runbooks/COMMS_EFFICIENCY_ENGINE.md): the control-plane and admin comms engine.

### Core

- [Deploying Core](runbooks/CORE_DEPLOY.md): one machine on one volume; preflight, deploy window, checkpoint verify and rollback.
- [System checkpoints](runbooks/SYSTEM_CHECKPOINTS.md): Core boots from a CRC-sealed checkpoint of the system store and replays the WAL after it.
- [Core memory and compaction](runbooks/CORE_MEMORY_AND_COMPACTION.md): how Core bounds its memory, what refuses work, and the memory tunables.
- [Bounded archive output and recovery](runbooks/BOUNDED_ARCHIVE_REPACK.md): finalised output bounds, preserved originals, crash recovery and rollback.
- [Latest-state views](runbooks/LATEST_STATE_VIEWS.md): turning views on, folder counters and the gateway changes lane.
- [Consumer access gate](runbooks/CONSUMER_ACCESS_GATE.md): gateways deploy blue-green and take traffic after a canary of each credential kind.
- [Control-plane CORS](runbooks/CONTROL_PLANE_CORS.md): CORS for the Go control plane on Fly app `allsource-control-plane`.
- [Fly image promotion](runbooks/FLY_IMAGE_PROMOTION.md): copying tested CI images into app-scoped Fly registries.
- [Fly frontends](runbooks/FLY_FRONTENDS.md): deploying the web and admin Next.js containers to Fly.
- [Prime multi-tenant ingest](runbooks/PRIME_MULTI_TENANT_INGEST.md): one writer and many readers, and how to spot a second writer.
- [Hosted Prime sync](runbooks/HOSTED_PRIME_SYNC.md): verifying hosted Prime sync with the native skills.

### Keys, plans and metering

- [Client keys and the legacy sunset](runbooks/CLIENT_KEY_ROTATION.md): registering, rotating and revoking a tenant client's ES256 key.
- [Revoking legacy Core keys](runbooks/REVOKE_LEGACY_CORE_KEYS.md): classifying, rotating and purging old API keys.
- [Customer remote connections](runbooks/CUSTOMER_REMOTE_CONNECTIONS.md): configuration for remote customer connections, disabled pending host and rollout gates.
- [Customer replay approval](runbooks/CUSTOMER_REPLAY_APPROVAL.md): the default-off replay approval gate.
- [Hosted extraction metering](runbooks/HOSTED_EXTRACTION_METERING.md): metering and hard gate for hosted Hound extraction; code-complete, parked, not live.
- [Pricing and billing cutover](runbooks/PRICING_BILLING_CUTOVER.md): cutover from the legacy four-tier model.
- [Retire the free plan](runbooks/RETIRE_FREE_PLAN.md): the operator procedure for migrating existing free tenants.
- [SEO evidence setup](runbooks/SEO_EVIDENCE_SETUP.md): every measurement source behind `tooling/seo-evidence` and what unblocks each one.
- [GEO measurement](runbooks/GEO_MEASUREMENT.md): the four built layers of GEO measurement.

### Self-hosting

- [Self-hosting Community Edition](self-hosting/README.md): single-node event store, Apache 2.0.

---

## Plans in progress

### Strategy and delivery plans

- [AllSource bet](BET.md): the active bet, its contract and shape versions, and the chat delivery gate.
- [Impact plan](IMPACT_PLAN.md): the outcome plan toward the BET gate of three qualifying hosted customers.
- [Value delivery plan](VALUE_DELIVERY_PLAN.md): internal accountability plan; customer value not yet reconciled.
- [Consolidated roadmap](roadmaps/2026-02-15_CONSOLIDATED_ROADMAP.md): the product roadmap, updated 2026-11-23.

### Plans

- CTO referral homepage: [implementation plan](plans/2026-11-26-cto-landing-plan.md), [browser walkthrough with screenshots](reports/2026-11-26-cto-homepage.html), and [independent code review](plans/2026-11-26-cto-landing-code-review.md).
- [Engineering harness capabilities and marketplace routes across the AllSource suite](reports/2026-11-26-harness-suite-marketplaces.md).
- AllSource Event History: [self-service brief](plans/2026-11-26-event-history-brief.md), [workspace connection and evidence design](plans/2026-11-26-event-history-design.md), [independent review A](plans/2026-11-26-event-history-design-review-a.md), and [independent review B](plans/2026-11-26-event-history-design-review-b.md).
  [Reviewer deployment and host acceptance cases](plans/2026-11-26-event-history-verification.md).
  [Implementation review](plans/2026-11-26-event-history-code-review.md).

- CTO referral homepage: [research brief](plans/2026-11-26-cto-landing-brief.md), [proposed copy and buyer test](plans/2026-11-26-cto-landing-design.md), [review A](plans/2026-11-26-cto-landing-design-review-a.md), [review B](plans/2026-11-26-cto-landing-design-review-b.md), and [source evidence](evidence/2026-11-26-cto-landing/README.md).

- [Prime as a Folk and Notion replacement](plans/2026-06-10-prime-workspace-crm-design.md): design for a Prime workspace CRM.
- [Dashboard route-family improvement](plans/2026-08-13-dashboard-autoresearch-design.md): one operational question and one next action per dashboard route.
- [Dashboard and onboarding UX](plans/2026-08-13-dashboard-onboarding-ux.md): moving a signed-in user from an empty tenant to one verified event.
- [Projection workspace redesign](plans/2026-08-13-projections-workspace-design.md): the projections page and its empty state.
- [Public-page clarity autoresearch](plans/2026-08-13-public-pages-autoresearch-design.md): homepage, platform and comparison page clarity.
- [Replay Studio design](plans/2026-08-13-replay-studio-design.md): the Replay Studio feature.
- [Replay Studio analysis and SDK](plans/2026-08-14-replay-studio-analysis-sdk-design.md): analysis and SDK design for Replay Studio.
- [Design partner acquisition campaign](plans/2026-08-27-design-partner-campaign-design.md): recruiting five AI-agent builders who need durable memory (approved).
- [Public SEO remediation](plans/2026-08-27-public-seo-remediation-autoresearch-design.md): code-addressable findings from the 27 August 2026 SEO audit.
- [Website brand-system migration](plans/2026-08-27-website-brand-system-design.md): the brand-system change for the website.
- [Homepage reliability autoresearch](plans/2026-09-02-homepage-reliability-autoresearch-design.md): homepage reliability loop.
- [Event-store SEO design](plans/2026-09-10-allsource-event-store-seo-design.md): SEO design for the event-store pages.
- [GitHub contribution motion](plans/2026-09-11-github-contribution-motion-design.md): contribution motion design.
- [Agent-run evidence and replay](plans/2026-09-14-agent-run-evidence-design.md): an evidence layer over existing Core events, with SDK and MCP capture first.
- [Optional decision intelligence](plans/2026-09-23-jev-decision-intelligence-design.md): offline proof, optional Choice adapter and stored-label filtering prototype.
- [Customer investigation and review contract v1](plans/2026-09-26-customer-agent-contract.md): the customer-agent contract. It adds no route or MCP tool.
- [Customer connection grants](plans/2026-09-26-customer-agent-grants.md): persistence and verification of grants; access not yet completed.
- [Plug and Play referral offer](plans/2026-09-26-plug-and-play-offer-design.md): a referral offer of 12 paid months.
- [Agent-run evidence capture and read](plans/2026-09-27-agent-run-evidence-contract.md): internal capture and read implementation under an existing epic.
- [Restricted evidence transport](plans/2026-09-27-customer-evidence-bindings.md): binding the comparison workflow to the Query Service.
- [Customer source references and pending reviews](plans/2026-09-27-customer-evidence-reviews.md): source references and pending comparison reviews.
- [Remote customer connection authorization](plans/2026-09-27-customer-remote-oauth-design.md): authorization design for remote customer connections.
- [Customer review work bounds](plans/2026-09-27-customer-review-work-bounds-design.md): bounds on customer review work.
- [Durable review query admission](plans/2026-09-27-durable-review-query-admission-design.md): admission for durable review queries.
- [Metered customer evidence workflow](plans/2026-09-27-metered-evidence-workflow-design.md): source sharing and pending reviews under metering.
- [Partnerships workspace](plans/2026-09-27-partnerships-design.md): the admin workspace for commercial channel exploration.
- [Product evidence selection and review](plans/2026-09-27-product-evidence-review-design.md): evidence consent and review in the agent connections settings.
- [Product-only approval of a rebuild plan](plans/2026-09-27-product-replay-approval-design.md): approval of a tenant projection rebuild plan.
- [Query Service query-admission client](plans/2026-09-27-query-admission-client.md): the canonical query-admission client.
- [Bounded cache warming](plans/2026-09-27-shared-archive-warmup-design.md): cache warming for strict HTTP operations.
- [Strict archive work](plans/2026-09-27-strict-archive-read-design.md): bounded input and HTTP isolation for strict archive reads.
- [Durable identity for projection replay](plans/2026-09-27-tracked-replay-design.md): a minimal execution journal for tenant projection replay.
- [Tenant egress allowances](plans/2026-10-22-tenant-egress-prepaid-design.md): allowances, prepaid overage and hard stops; implemented locally, values proposed for owner review.
- [Tenant polling defense](plans/2026-10-24-tenant-polling-defense.md): per-tenant admission, conditional reads and caches on both sides.
- [Event feed without whole-tenant history](plans/2026-10-25-feed-without-whole-history.md): a narrow feed index and a durable per-tenant feed log.
- [Native delivery CI prerequisites](plans/2026-10-28-native-ci-prerequisites.md): accepted CI prerequisites for native delivery.
- [Core restarts without refused clients](plans/2026-11-01-core-zero-downtime-restart.md): phased fix for Core deploys that refuse clients; proposed.
- [Bitemporal time axes](plans/2026-11-04-bitemporal-time-axes.md): a second time axis (valid time) next to record time; accepted 2026-11-04.
- [Fly cost reduction](plans/2026-11-08-fly-cost-reduction.md): steps to reduce the Fly org's monthly cost, justified by measured activity.
- [Core memory bound: brief](plans/2026-11-11-core-memory-bound-brief.md): why Core needs 16 GB today and the staged path to 8 GB.
- [AllSource outreach comparison](plans/2026-11-11-outreach-comparison.md): proposed outreach workflows, review controls and measurement.
- [Core memory bound: design](plans/2026-11-12-core-memory-bound-design.md): S0 heap attribution, the S1 ledger, and the measurement harness.
- [Core memory bound: design review record](plans/2026-11-12-core-memory-bound-design-review.md): the twenty fresh-context review rounds and their outcome.
- [Core memory bound: review round 1](plans/2026-11-12-core-memory-bound-design-review-r1.md): review round record.
- [Core memory bound: review round 10](plans/2026-11-12-core-memory-bound-design-review-r10.md): review round record.
- [Core memory bound: review round 11 delta](plans/2026-11-12-core-memory-bound-design-review-r11-delta.md): review delta record.
- [Core memory bound: review round 16 delta](plans/2026-11-12-core-memory-bound-design-review-r16-delta.md): review delta record.
- [Core memory bound: review round 17](plans/2026-11-12-core-memory-bound-design-review-r17.md): review round record.
- [Core memory bound: plan](plans/2026-11-12-core-memory-bound-plan.md): quality gates, the testing approach and the stories from manifest commits through the S1 run.

### Proposals

- [Admin tenant power tool](proposals/ADMIN_TENANT_POWER_TOOL.md): design and phased build plan for active tenant help; no code yet.
- [Agent-driven Prime onboarding](proposals/AGENT_DRIVEN_PRIME_ONBOARDING.md): the onboarding gaps, which shipped, and why the remainder stopped.
- [AI inbox on AllSource](proposals/AI_INBOX_ON_ALLSOURCE.md): not planned since 2026-09-16; the P0 email wedge stays code-complete.
- [Agent memory ecosystem](proposals/ALLSOURCE_AGENT_MEMORY_ECOSYSTEM.md): strategy and blueprint, dated 2026-05-29.
- [Control-plane tenant health and recovery](proposals/CONTROL_PLANE_TENANT_HEALTH_RECOVERY.md): fleet-wide health and recovery; P0 to P3 on main.
- [Hosted tier pricing options](proposals/MAMMOTH_HOSTED_PRICING_OPTIONS.md): pricing and abuse-control options; decided 2026-05-31.
- [MCP registry publication](proposals/MCP_REGISTRY_PUBLICATION.md): publication plan for the Elixir MCP server.
- [Neotoma parity completion plan](proposals/NEOTOMA_PARITY_COMPLETION_PLAN.md): the plan to reach parity with Neotoma.
- [Per-tenant projections](proposals/PER_TENANT_PROJECTIONS.md): the design for per-tenant projections, epic `t-822210`.
- [Pluggable embedder for Prime](proposals/PLUGGABLE_EMBEDDER_DESIGN.md): implemented as v1 behind the `prime-remote-embed` feature.
- [Pricing and exposure plan](proposals/PRICING_EXPOSURE_PLAN.md): pricing and exposure plan for the developer infrastructure product.
- [Pricing plan details](proposals/PRICING_PLAN_DETAILS.md): the source of record for per-tier entitlements.
- [Hosted Prime MCP server](proposals/PRIME_HOSTED_MCP_DESIGN.md): design for a hosted Prime MCP server.
- [Prime Hound and Graphify](proposals/PRIME_HOUND_GRAPHIFY_ANALYSIS.md): analysis of offering Graphify's capabilities as a Prime extension; not yet scheduled.
- [Prime as a stateless engine over Core](proposals/PRIME_STATELESS_OVER_CORE.md): the design for Prime without its own state.
- [Prime voice file](proposals/PRIME_VOICE_FILE.md): a durable, queryable identity index; a proposal with a shipped thin layer.
- [Regional independence](proposals/REGIONAL_INDEPENDENCE.md): steps 1 and 2 of the proposal; steps 3 to 5 not planned.
- [Self-service onboarding gap analysis](proposals/SELF_SERVICE_ONBOARDING.md): gaps in self-service onboarding, dated 2026-03-27.
- [Sustainable data strategy](proposals/SUSTAINABLE_DATA_STRATEGY.md): draft proposal, 2026-04-26.
- [Team sync setup](proposals/TEAM_SYNC_SETUP.md): priority assessment for sharing chronis tasks through the hosted instance.
- [Unified auth, teams and roles](proposals/UNIFIED_AUTH_TEAMS.md): OAuth bridge, teams, and agent and human roles.
- [PRD: agent auth and x402 payments](proposals/prd-agent-auth-x402.md): agent self-registration and x402 payments in the control plane.
- [PRD: agent-run evidence and replay](proposals/prd-agent-run-evidence-and-replay.md): evidence and replay comparison for agent runs.
- [PRD: read-only Core against a foreign data directory](proposals/prd-core-read-only-foreign-data.md): replaying existing WAL and Parquet files from a foreign directory (issue #130).
- [PRD: event-sourced billing](proposals/prd-event-sourced-billing.md): billing state as events in Core.
- [PRD: `rust-perf` skill templates](proposals/prd-rust-perf-templates-and-validation.md): runnable templates and validation for the rust-perf skill.

---

## Reports and evidence

### Incident and verification reports

- [Control-plane egress root cause (2026-10-24)](reports/2026-10-24-control-plane-egress-rca.md): the 66 GB/day alert caused by control-plane polling over the public edge.
- [Production traffic report (2026-10-24)](reports/2026-10-24-production-traffic.html): measured bandwidth by app, region and tenant, the bottlenecks, and the 2026-10-24/25 outage.
- [Control-plane tenant outage (2026-10-25)](reports/2026-10-25-control-plane-tenant-outage.html): measured refusals of Longhand reads with 429, the timeline and the fix.
- [Client access outage (2026-10-26)](reports/2026-10-26-client-access-outage.html): the key refusals caused by API-key enforcement deploys, with timeline, cause and actions.
- [Deploy denial diagnosis (2026-10-26)](reports/2026-10-26-auth-deploy-denial-diagnosis.html): the auth path that refused unregistered keys and ranked safeguards.
- [Gateway config strip outage (2026-11-06)](reports/2026-11-06-gateway-config-strip-outage.md): `api.all-source.xyz` served nothing for 32.5 minutes after a config file was replaced by an empty one.
- [Infrastructure costs status (2026-11-08)](reports/2026-11-08-infra-costs-status.html): Fly machines, volumes, gateway egress and the actions that cut cost.

### Verification reports

- [Event History checkout acceptance in ChatGPT and Claude](reports/2026-11-26-event-history-acceptance.md)

- [Hosted Prime sync verification](reports/2026-10-26-hosted-prime-sync-verification.md): synthetic sync observations bound to runtime source `f1ede26f`.
- [Managed API key lifecycle verification](reports/2026-10-26-managed-key-lifecycle-verification.md): deployed gateways, legacy-key compatibility and production readback.
- [Prime sync interval verification](reports/2026-10-26-prime-sync-interval-verification.md): a zero interval is refused before the store opens; positive intervals deliver events.
- [Chronis TOON native qualification](reports/2026-10-27-chronis-native-verification.md): TOON escaping for Chronis, qualified through real Codex and Claude calls.
- [Prime TOON key integrity](reports/2026-10-27-prime-toon-native-verification.md): retained-memory property names through independently decoded MCP responses.
- [Prime TOON control-value integrity](reports/2026-10-28-prime-toon-values-verification.md): C0 control values through retained-memory reopen.
- [Prime against HASH: gap status (2026-11-06)](reports/2026-11-06-prime-hash-gap-status.md): the seven gaps against HASH, what is deployed, and what is still open.
- [Scoped local event erasure (2026-11-13)](reports/2026-11-13-scoped-erasure.md): offline rewrite boundary, synthetic verification and the remaining account-deletion coverage.
- [Ritual-full flow adoption (2026-11-11)](reports/2026-11-11-ritual-full-adoption.md): vendored ritual skills, planning rules and what was not checked.
- [Core heap attribution (2026-11-13)](reports/2026-11-13-core-heap-attribution.md): S0 jemalloc profile of the replay fixture, attributed to S1 owners.
- [Core memory S1 run ledger (2026-11-13)](reports/2026-11-13-core-memory-s1-run-ledger.md): the S1 autoresearch run on the US-011 baseline; two candidates, neither kept.
- [Core memory S1 report (2026-11-14)](reports/2026-11-14-core-memory-s1-report.md): Acceptance 4 records for S1; the cold-load band rule failed for both sets.
- [Core memory S1 overlay (2026-11-14)](reports/2026-11-14-core-memory-s1-overlay.patch): the overlay recipe referenced by the S1 report.
- [Critical bugs fixed](current/CRITICAL_BUGS_FIXED.md): bugs fixed in AllSource Core, with the notes on feature-gated backends.
- [Evidence-led investigation audit (2026-09-23)](EVIDENCE_LED_AUDIT_2026-09-23.md): the audit of the evidence-led investigation.
- [Evidence-led investigation queue (2026-09-23)](EVIDENCE_LED_BEADS_2026-09-23.md): the queue of investigations; creating it is not completing them.
- [Chat delivery readiness](CHAT_DELIVERY.md): readiness of chat delivery, contract v3, activation queued.
- [Mobile delivery](MOBILE_DELIVERY.md): complete Android and iOS delivery, decided 2026-09-21.
- [Customer agent delivery](CUSTOMER_AGENT_DELIVERY.md): customer agent contract and runtime evidence status.
- [SEO evidence baseline (2026-10-20)](reports/2026-10-20-seo-evidence-baseline.md): pages score 100/100 and receive 2 organic clicks a month.

### Ledgers and benchmarks

- [Dashboard JS weight ledger](ledger/dashboard-bundle.md): autoresearch loop over the customer dashboard client JavaScript.
- [Dashboard route-family ledger](ledger/dashboard-pages-autoresearch.md): autoresearch ledger for the dashboard routes.
- [Homepage reliability ledger](ledger/homepage-reliability-autoresearch.md): autoresearch ledger for the homepage.
- [Monorepo focus audit](ledger/monorepo-focus-audit.md): unused code, dormant features and focus, dated 2026-08-11.
- [Public page clarity ledger](ledger/public-pages-autoresearch.md): autoresearch ledger for public page clarity.
- [Public SEO remediation ledger](ledger/public-seo-remediation-autoresearch.md): autoresearch ledger for SEO remediation; external evidence gates are recorded as unknown until observed.
- [Replay Studio ledger](ledger/replay-studio-autoresearch.md): autoresearch ledger for Replay Studio.
- [Benchmark improvement guide](benchmarks/BENCHMARK_IMPROVEMENT_GUIDE.md): the optimisations tried for Prime recall, with the results and open opportunities.
- [Recall benchmark results (2026-03-23)](benchmarks/RECALL_BENCHMARK_RESULTS.md): Prime recall benchmark results.

### Evidence bundles

- [docs/evidence/](evidence/): one bundle per run, each with a `README.md`. Bundles from 2026-09-20 to 2026-09-24: search baseline, signup repair, evidence-led investigation, experiment and SEO worksheets. Bundles from 2026-09-26: customer-agent contract, grant isolation, grants, live access. Bundles from 2026-09-27: agent-run capture and evidence, archive budget audit, archive cache residency, archive worker setting, compaction input integrity, conditional archive integrity, core foundation deploy, customer connections, customer evidence bindings and reviews, customer remote HTTP, customer review work bounds, customer workspace ownership, durable query admission, isolated archive capacity, local customer connection, metered evidence workflow, product evidence workspace, product replay approval, query admission client, remote customer authorization, shared archive warmup, strict archive work, strict retained read, strict timestamp stability, team membership, tracked replay. Bundle from 2026-10-27: latest-state views.
- [Latest-state views evidence (2026-10-27)](evidence/2026-10-27-latest-state-views/README.md): view fold, conditional reads, sync, tenant isolation, restart and the consumer gate, end to end.
- Supporting data for reports: `reports/baselines/`, `reports/data/`, `reports/evidence/` and `reports/images/`.

---

## Research

- [Stoolap vs AllSource](research/stoolap-vs-allsource.md): research dossier, citable comparison material.
- [Sea of Demand](SEA_OF_DEMAND.md): demand research for hosted agent-event durability; research complete, no outreach performed.
- [Replay-debugging guide autoresearch (2026-08-25)](research/2026-08-25-organic-page-autoresearch.md): the replay-debugging guide page.
- [Design-partner page autoresearch (2026-08-27)](research/2026-08-27-design-partner-page-autoresearch.md): the design-partner page report.
- [Design-partner page simplification (2026-08-27)](research/2026-08-27-design-partner-page-simplification.md): verification of the simplified page.
- [Agent workflow feedback (2026-09-14)](research/2026-09-14-agent-workflow-feedback-report.md): feedback from a public thread and the opportunity it shows.
- [Optimistic concurrency without loading history (2026-10-04)](research/2026-10-04-occ-version-index.md): why entity versions must be read, not counted.
- [Reuse verified archive hydration (2026-10-08)](research/2026-10-08-strict-hydration-version-index.md): conditional writes after a strict archive read.
- [Memory-failure design-partner research (2026-10-19)](research/2026-10-19-memory-failure-design-partners.md): public evidence for 30 projects, qualification and prior-contact checks.
- [Latest-state reads (2026-10-27)](research/2026-10-27-latest-state-reads.md): why consumers pull history for current state, and the snapshot-plus-tail target.

---

## SEO

- [SEO assessment (2026-11-10)](seo/2026-11-10-seo-assessment.md): observed search intent and proposed content changes.
- [SEO strategy (2026-11-08)](seo/2026-11-08-allsource-seo-strategy.md): delivery, topic priorities, evidence and measurement.
- [Keyword demand with WolvenTech (2026-10-01)](seo/2026-10-01-allsource-wolventech-keyword-demand.md): measured keyword volumes, intent exclusions and targeting priorities. Data: `seo/2026-10-01-keyword-demand.csv` and `seo/2026-10-01-keyword-demand-evidence.json`.
- [Search-content implementation (2026-10-01)](seo/2026-10-01-search-content-implementation.md): page intent, content changes and verification for both sites.
- [Agent memory after compaction (2026-11-07)](seo/2026-11-07-compaction-search-evidence.md): search intent, sources and trend-data limits.
- [Editorial review (2026-11-07)](seo/2026-11-07-editorial-review/README.md): factual corrections and contextual links for product-owner review.
- [CQRS internal link draft (2026-11-07)](seo/2026-11-07-editorial-review/2026-11-07-cqrs-link.md): a local link recommendation.
- [Production review and owner handoff (2026-11-07)](seo/2026-11-07-editorial-review/2026-11-07-production-review.md): the production review of the introductory and comparison articles.
- [Schema evolution evidence (2026-11-08)](seo/2026-11-08-schema-evolution-evidence.md): replay example, source evidence and release limits.
- [Schema evolution SEO review (2026-11-09)](seo/2026-11-09-schema-review.md): proposed correction and its local HTML preview.
- [Event sourcing reference correction (2026-11-11)](seo/2026-11-11-aws-reference-review.md): the corrected reference destination and verification limits.
- [SEO audit (2026-08-29)](seo/2026-08-29-allsource-audit/www-all-source-xyz-audit-20260829-015519/VERIFIED-AUDIT-REPORT.md): verified audit of www.all-source.xyz. The raw pipeline's performance score came from a heuristic fallback and is replaced here.
- [Verified SEO action plan (2026-08-29)](seo/2026-08-29-allsource-audit/www-all-source-xyz-audit-20260829-015519/VERIFIED-ACTION-PLAN.md): action plan built on the verified audit.
- [Sitemap validation (2026-08-29)](seo/2026-08-29-allsource-audit/www-all-source-xyz-audit-20260829-015519/SITEMAP-VALIDATION-REPORT.md): Search Console sitemap check.
- SEO audit data: `seo/2026-08-29-allsource-audit/www-all-source-xyz-audit-20260829-015519/` holds the JSON results, the Lighthouse output and the screenshots.

---

## Outreach and marketing

### Outreach

- [Outreach skill verification (2026-11-11)](outreach/2026-11-11-outreach-skill-verification.md): synthetic walkthrough and page checks; no external message sent.
- [Outreach skill presentation](outreach/allsource-outreach-skill.html): the outreach skill's workflow and example prompt. Source: [SKILL.md](../skills/allsource-outreach/SKILL.md); package: [allsource-outreach.zip](outreach/allsource-outreach.zip).
- [Fintech AI decision-context discovery (2026-11-12)](outreach/2026-11-12-fintech-discovery-review.md): sourced prospects, messages sent after approval, and exclusions.
- [Qualified builder outreach ledger](outreach/qualified-builder-outreach.md): the record that prevents duplicate founder outreach.
- [Directory and registry listings (2026-10-21)](outreach/2026-10-21-directory-listings.md): product facts and submission routes for listings.
- [Live nanobot and Hermes memory evaluation (2026-10-19)](outreach/2026-10-19-live-memory-evaluation.md): real-provider results and an isolated outage check; nothing sent.
- [Memory fixes and artifact-led drafts (2026-10-19)](outreach/2026-10-19-memory-fix-audit-and-drafts.md): released fixes, remaining gaps and unsent drafts.
- [Oh-my-openagent #7183 compaction evaluation (2026-10-19)](outreach/2026-10-19-omo-compaction-evaluation.md): repeated recovery, payload bounds and ownership checks.
- [Oh-my-openagent #7183 full-plugin evaluation (2026-10-20)](outreach/2026-10-20-omo-full-plugin-evaluation.md): the loader fix and the native compaction matrix.
- [Oh-my-openagent #7183 provider-payload capture (2026-10-21)](outreach/2026-10-21-omo-provider-payload-evaluation.md): captured model inputs and counterbalanced controls.
- [Oh-my-openagent #7183 send-ready package (2026-10-21)](outreach/2026-10-21-omo-send-ready.md): the package prepared for review; no send authorised.
- [Evaluation artifacts](outreach/artifacts/): the raw outputs, configs and scripts behind the outreach evaluations.

### Marketing

- [Marketing index](marketing/README.md): the marketing folder's index.
- [Marketing strategy pack](MARKETING_STRATEGY_PACK.md): the strategy pack; `BET.md` takes precedence on conflict.
- [Design partner launch kit (2026-08-27)](marketing/design-partner-launch-kit.md): launch kit for the design-partner campaign.
- [Qualified builder outreach (2026-08-28)](marketing/2026-08-28-qualified-builder-outreach.md): outreach campaign `design_partners_2026`.
- [GEO and AEO answer-engine checklist](marketing/GEO_AEO_ENGINE_QUESTION_CHECKLIST.md): questions to put to the answer engines.
- [Answer-engine sweep (2026-08-14)](marketing/geo-answer-engine-sweep-2026-08-14.md): results immediately after deploy.
- [GEO content experiment (2026-08-14)](marketing/geo-content-experiment-2026-08-14.md): evidence before expansion.
- [GEO baseline template](marketing/geo-baseline-TEMPLATE.md): template for the layer-3 baseline; awaiting the first live run.
- [Observed model vocabulary](marketing/geo-key-terms.md): the terms the engines use; awaiting the first live run.
- [GEO remediation backlog](marketing/geo-remediation-backlog.md): wrong claims the engines make; awaiting the first live run.
- [Free-tool spec: memcheck](marketing/free-tool-memcheck-spec.md): draft spec for a future free tool; not an implementation.
- [Lifecycle email trail](marketing/lifecycle-email-trail.md): draft lifecycle emails for human review; nothing sent.

### Social and articles

- [Embedded Core complete (2026-02-28)](social/2026-02-28-embedded-core-complete.md): thread announcing the embedded Core API.
- [Chronis cold-parquet incident (2026-05-30)](social/2026-05-30-chronis-cold-parquet-incident.md): draft postmortem thread.
- [mammoth launch thread](social/mammoth-launch-x-thread.md): draft X and Show HN launch thread.
- [Prime voice file campaign](social/prime-voice-file-campaign.md): draft X, LinkedIn and Show HN campaign.
- [Twitter thread, v0.10.4](social/twitter-v0.10.4.md): published release thread.
- [Twitter thread, v0.10.5](social/twitter-v0.10.5.md): published release thread.
- [Twitter thread, v0.10.6](social/twitter-v0.10.6.md): published release thread on SDK distribution.
- [Neotoma vs AllSource (2026-05-23)](articles/neotoma-comparison.md): two designs for agent memory.
- [Oracle 26ai vs AllSource (2026-05)](articles/oracle-26ai-comparison.md): a converged database compared with a purpose-built event spine.
- [Blog drafts](drafts/blog/README.md): unpublished `.mdx` drafts, not in the web app's content directory. Drafts: [graphify-vs-allsource-prime.mdx](drafts/blog/graphify-vs-allsource-prime.mdx), [one-graph-for-code-and-agent-memory.mdx](drafts/blog/one-graph-for-code-and-agent-memory.mdx) and [prime-hound-living-knowledge-graph.mdx](drafts/blog/prime-hound-living-knowledge-graph.mdx).

---

## Sales and launch

- [Launch readiness](LAUNCH_READINESS.md): operational readiness record; the Sunday Club submission was reported on 2026-09-14, receipt not verified.
- [Launch checklist](launch/LAUNCH_CHECKLIST.md): the launch checklist, last reviewed 2026-11-24.
- [Chronis Cloud launch plan](launch/CHRONIS_CLOUD_LAUNCH_PLAN.md): manual steps to move chronis from embedded mode to hosted sync with x402 and agent registration.
- [LemonSqueezy setup](launch/LEMONSQUEEZY_SETUP.md): product and variant setup for the subscription tiers.
- [mammoth launch runbook](launch/MAMMOTH_LAUNCH_RUNBOOK.md): sequencing for the mammoth launch assets.
- [Prime voice file launch runbook](launch/PRIME_VOICE_FILE_LAUNCH.md): sequencing for the Prime voice file launch assets.
- [Sales README](sales/README.md): index of the sales and marketing artefacts.
- [SaaS evaluation summary](sales/SUMMARY.md): summary of the SaaS evaluation, dated February 2026.
- [SaaS evaluation report](sales/evaluation/SAAS_EVALUATION.md): full SaaS evaluation, dated February 2026.
- [Sales pitch deck](sales/pitch-deck/SALES_PITCH_DECK.md): a 15 to 20 minute pitch with Q&A.
- [Visual assets for sales](sales/pitch-deck/VISUAL_ASSETS.md): the visual assets for the pitch deck.
- [Video scripts](sales/videos/VIDEO_SCRIPTS.md): scripts, storyboards and recording guides for demo videos.
- [C4 architecture diagrams (Mermaid)](sales/diagrams/C4_ARCHITECTURE_MERMAID.md): C4 diagrams for Core 0.30.0. Structurizr source: [allsource.dsl](sales/diagrams/allsource.dsl).
- [Static voice file compared with Prime voice](sales/voice-file-comparison.md): a draft comparison for the `/compare` surface.
- [Brand guide](brand/BRAND_GUIDE.md): the visual and written brand rules.
- [LinkedIn company page](brand/LINKEDIN_COMPANY_PAGE.md): the company page published 2026-09-30.

---

## Archive

Superseded, completed and expired documents are in `archive/`. [ARCHIVE_INDEX.md](ARCHIVE_INDEX.md) lists each archived document with its successor and the other archive folders. The main archive is `archive/2026-11-23/`, which mirrors the original paths.

---

*Navigation: [Repository home](../README.md) | [Archive index](ARCHIVE_INDEX.md) | [ADRs](adr/README.md)*
