---
title: "AllSource Documentation"
status: CURRENT
last_updated: 2026-02-08
---

# AllSource Documentation Hub

Welcome to the AllSource documentation. This hub provides comprehensive guides, architecture documentation, and operational resources for the AllSource event sourcing platform.

---

## Quick Navigation

- [Prime TOON control-value integrity (2026-10-28)](./plans/2026-10-28-prime-toon-control-values.md): accepted encoder repair, retained-memory reproduction and native Codex/Claude acceptance.
- [Prime control-value native verification](./reports/2026-10-28-prime-toon-values-verification.md): frozen baseline/candidate execution, independent real-host QA and retained-memory compatibility.
- [Prime control-value delivery request](./reference/prime-toon-values-request.md): accepted application job, compatibility boundary and publication authority.

- [Prime TOON native workflow qualification](./reports/2026-10-27-prime-toon-native-verification.md): current-source failures, retained-memory interoperability, real Codex/Claude calls and durable native ownership.

- [Prime TOON key integrity plan](./plans/2026-10-27-prime-toon-key-quoting.md) and [native application request](./reference/prime-toon-keys-request.md): preserve retained memory property names through independently decoded MCP responses.

- [Latest-state views and consumer-access gate, run end to end (2026-10-27)](./evidence/2026-10-27-latest-state-views/README.md): PR #366 on local Core, Query Service and gateway: view fold, conditional reads, sync, tenant isolation, restart, the consumer gate, and the TypeScript and Go SDK gateway 401 with its fix.

- [Chronis native workflow qualification](./reports/2026-10-27-chronis-native-verification.md): rebuilt baseline failures, candidate CLI lifecycle success, actual Codex/Claude calls and installed-toolkit provenance.

- [Chronis TOON integrity plan](./plans/2026-10-27-chronis-toon-escaping.md) and [native application request](./reference/chronis-toon-request.md): preserve exact task data while escaping structural output, qualified through real Codex and Claude.

- [Legacy registered key authority compatibility](./plans/2026-10-26-legacy-key-authority-compatibility.md): admit retained version-one prefixes while preserving completeness and terminal revocation.

- [Prime sync interval validation](./plans/2026-10-25-prime-sync-interval-validation.md): reject zero before store startup and qualify actual native-host MCP delivery.

- [Tenant polling defence](./plans/2026-10-24-tenant-polling-defense.md): per-tenant admission, conditional reads, change detection and caches on both sides, phased so Longhand is never starved.
- [Bounded event-prefix reads](./plans/2026-10-24-bounded-prefix-auth-reads.md): authentication lookups without whole-tenant hydration.
- [Shared tenant read model](./plans/2026-10-24-shared-tenant-read-model.md): bounded feeds, atomic checkpoints, conditional reads and rollout requirements.
- [Event feed without whole-tenant history](./plans/2026-10-25-feed-without-whole-history.md): narrow feed index, then a durable per-tenant feed log, so feed and read-model commit cost stops growing with history.

- [Bounded retained entity reads](./plans/2026-10-24-bounded-retained-entity.md): memory-bounded archive reads and verification requirements.
- [JWT crypto backend](./plans/2026-10-24-jwt-crypto-backend.md): remove the vulnerable transitive dependency while preserving token compatibility.

- [AllSource LinkedIn company page](./brand/LINKEDIN_COMPANY_PAGE.md)
- [AllSource and WolvenTech keyword demand (2026-10-01)](./seo/2026-10-01-allsource-wolventech-keyword-demand.md): treg/DataForSEO volumes, related keywords, intent exclusions and targeting priorities.
- [Search-content implementation (2026-10-01)](./seo/2026-10-01-search-content-implementation.md): page intent, content changes, technical verification and measurement limits for both sites.
- [Directory and registry listings (2026-10-21)](./outreach/2026-10-21-directory-listings.md): product facts for listings, MCP registry and ClawHub routes, curated directories and the submission log.

| | Link | Description |
|---|------|-------------|
| 🚀 | [Quick Start](./guides/QUICK_START.md) | Get up and running in minutes |
| 📖 | [Architecture](./current/CLEAN_ARCHITECTURE.md) | Clean architecture principles |
| ⚡ | [Performance](./current/PERFORMANCE.md) | Optimization strategies |
| 🔒 | [Security](./current/TENANT_ARCHITECTURE.md) | Multi-tenant security model |
| 🛠️ | [Quality Gates](./current/QUALITY_GATES.md) | CI/CD quality standards |
| 🗺️ | [Roadmap](./roadmaps/) | Future plans and progress |

---

## What's New

**February 2026**
- Updated documentation hub with modern navigation
- Consolidated quality gates documentation

**November 2025**
- Added multi-tenant architecture security documentation
- Quality gates implementation and enforcement
- Critical bug fixes documented

**October 2025**
- Clean Architecture refactoring complete (Phase 1.5)
- Performance optimization guide published
- Query Service roadmap established

---

## Status Legend

| Marker | Status | Meaning |
|--------|--------|---------|
| ✅ | CURRENT | Active, up-to-date documentation |
| ⚠️ | NEEDS_REVIEW | May be outdated, review before use |
| 📦 | ARCHIVED | Historical reference, superseded by newer docs |

---

## Documentation by Audience

### Getting Started

For new users looking to understand and run AllSource.

| Document | Description | Status |
|----------|-------------|--------|
| [Quick Start](./guides/QUICK_START.md) | Installation and first run | ✅ |
| [Demo Walkthrough](./guides/DEMO.md) | Hands-on demonstration | ✅ |
| [MCP Server Setup](./guides/mcp-server/CLAUDE_DESKTOP_SETUP.md) | Claude Desktop integration | ✅ |

---

### Core Concepts

For understanding the system architecture and design decisions.

| Document | Description | Status |
|----------|-------------|--------|
| [Clean Architecture](./current/CLEAN_ARCHITECTURE.md) | Layered architecture guide | ✅ |
| [SOLID Principles](./current/SOLID_PRINCIPLES.md) | Design principles applied | ✅ |
| [Tenant Architecture](./current/TENANT_ARCHITECTURE.md) | Multi-tenancy and isolation | ✅ |
| [Tenant egress and prepaid credits](./plans/2026-10-22-tenant-egress-prepaid-design.md) | Proposed allowances, durable hard stops, payment verification and activation requirements | Local implementation |
| [Event Store Features](./current/EVENT_STORE_FEATURES.md) | Core event sourcing capabilities | ✅ |
| [Architecture Optimization](./current/ARCHITECTURE_OPTIMIZATION.md) | System-level optimizations | ✅ |
| [OCC version index](./research/2026-10-04-occ-version-index.md) | Why entity versions must be read, never counted; the eviction and memory defects that follow | ✅ |
| [Strict hydration and version-index reuse](./research/2026-10-08-strict-hydration-version-index.md) | Reuse complete archive validation before conditional writes; failure and eviction checks | ✅ |
| [Latest-state reads](./research/2026-10-27-latest-state-reads.md) | Why consumers pull history to get current state through every SDK, the gateway, Query Service and MCP; snapshot-plus-tail views as the target | ✅ |
| [ADR-021: views folded by the Query Service](./adr/021-views-folded-by-query-service.md) | The Query Service folds declarative views from the event feed into Core read-model rows; Core validates, stores and serves them and never applies a reducer | Accepted |
| [Latest-state read contract](./contracts/latest-state/README.md) | Wire contract for entity state without history, ETag/304, event feed and read models, view definitions, latest, latestMany and changes | ✅ |
| [SEO evidence baseline](./reports/2026-10-20-seo-evidence-baseline.md) | Pages score 100/100 and take 2 organic clicks a month: DA 2, 3 referring domains, 12 ranked keywords | ✅ |

---

### Guides

How-to documentation for common tasks.

| Document | Description | Status |
|----------|-------------|--------|
| [WebSocket Configuration](./guides/WEBSOCKET_CONFIGURATION.md) | Real-time event streaming setup | ✅ |
| [Release Guide](./guides/RELEASE.md) | Create releases with `make release` | ✅ |
| [Quality Gates Setup](./guides/QUALITY_GATES_SETUP.md) | Configure CI/CD quality checks | ✅ |
| [Branch Protection](./guides/BRANCH_PROTECTION.md) | Git workflow protection | ✅ |
| [MCP Demo Script](./guides/mcp-server/DEMO_SCRIPT.md) | MCP demonstration walkthrough | ✅ |
| [MCP Quick Reference](./guides/mcp-server/QUICK_REFERENCE.md) | MCP tool reference card | ✅ |
| [Claude Code Skills](./guides/CLAUDE_CODE_SKILLS.md) | Automated testing, release, and diagnostics skills | ✅ |
| [Embedded Prime Integration](./guides/EMBEDDED_PRIME_INTEGRATION.md) | Use Prime as an embedded library in Tauri / Rust apps — graph, HNSW vectors, tiered recall | ✅ |
| [Allstack CLI and recovery](./reference/allstack.md) | Repository setup and recovery for the local Allstack workflow: Core keeps its history, Chronis its tasks and claims, Prime its recall | ✅ |
| [Allstack native skills](./reference/allstack-skills.md) | The `allstack-*` skills for Codex and Claude Code, and what each is for | ✅ |
| [Allstack workflow catalog](./reference/allstack-catalog.md) | The `delivery` and `bugfix` workflows in `workflows/catalog.toml` and how to start one | ✅ |

---

### API Reference

Developer documentation for service integration.

| Service | README | Details |
|---------|--------|---------|
| Rust Core | [apps/core](../apps/core/README.md) | Event sourcing engine |
| Control Plane | [apps/control-plane](../apps/control-plane/README.md) | Cluster management (Go) |
| Query Service | [apps/query-service](../apps/query-service/README.md) | Read-side queries (Elixir) |
| MCP Server | [apps/mcp-server](../apps/mcp-server/README.md) | Model Context Protocol |
| Web App | [apps/web](../apps/web/README.md) | Web interface |

---

### Operations

For DevOps and platform engineers.

| Document | Description | Status |
|----------|-------------|--------|
| [Alerts](./operations/ALERTS.md) | Repo-defined alarms, their thresholds and escalation | ✅ |
| [Bandwidth alerts](./runbooks/BANDWIDTH_ALERTS.md) | Production edge-output thresholds, private email delivery, and operator checks | Active; firing and recovery verified |
| [Business dashboards](./runbooks/BUSINESS_DASHBOARD.md) | Business questions, availability, engineering detail, egress attribution and the traffic & outage report on fly-metrics.net: tenant cost vs plan, failed reads, deployment fit, which caller sends the bytes, outage signals, event markers, and the alerts behind them | ✅ |
| [Production traffic report (2026-10-24)](./reports/2026-10-24-production-traffic.html) | Measured bandwidth by app, region and tenant; animated system diagram; six bottlenecks with charts from raw Fly and service metrics; the 2026-10-24/25 outage (12 h of failed reads) with timeline, cause and what is still open | ✅ |
| [Control-plane egress root cause (2026-10-24)](./reports/2026-10-24-control-plane-egress-rca.md) | 66 GB/day alert: Longhand control-plane polling over the public edge, egress-guard throttling since 15:40Z, fix sites | ✅ |
| [Control-plane tenant outage (2026-10-25)](./reports/2026-10-25-control-plane-tenant-outage.html) | Measured 11:53–12:25Z: ~9% of Longhand reads refused with 429 and sign-in failing intermittently; full outage fixed; #355 not slower (gzip skews the release check); `team_queue.snapshot.chunk` is 81% of the tenant's history | ✅ |
| [Client access outage (2026-10-26)](./reports/2026-10-26-client-access-outage.html) | Longhand's key refused for 2 h 29 m (18:56–21:25Z) and again for 8 m (22:44–22:52Z) by the API-key enforcement deploys; Core unreachable 6 m during a redeploy; timeline, cause, state, actions | ✅ |
| [Why a deploy denied clients (2026-10-26)](./reports/2026-10-26-auth-deploy-denial-diagnosis.html) | The auth path that refused unregistered keys, which credentials it refuses, why no signal showed it, how to diagnose a denial spike, and ranked safeguards so no deploy denies a client without a named rule | ✅ |
| [Fly image promotion](./runbooks/FLY_IMAGE_PROMOTION.md) | Copy tested private CI images into app-scoped Fly registries before deployment | Operator runbook |
| [Consumer access gate](./runbooks/CONSUMER_ACCESS_GATE.md) | Gateways deploy bluegreen and take traffic only after one canary of every credential kind customers hold reads through the new machine | Operator runbook |
| [Latest-state views](./runbooks/LATEST_STATE_VIEWS.md) | Turning views on (`ALLSOURCE_VIEWS`, admin `CORE_API_KEY` for the Query Service folder, optional `ALLSOURCE_FEED_DURABLE_LOG`), defining the ops-status view, folder counters and the gateway changes lane | Operator runbook |
| [Cold tier](./operations/COLD_TIER.md) | Archiving events past retention to S3/R2, env vars and IAM policy | ✅ |
| [Core memory and compaction](./runbooks/CORE_MEMORY_AND_COMPACTION.md) | Checking compaction and retention actually run, reading the two hydration refusals, memory tunables | ✅ |
| [Bounded archive repack](./runbooks/BOUNDED_ARCHIVE_REPACK.md) | Finalized output bounds, preserved originals, crash recovery and rollback commands | ✅ |
| [Revoking legacy Core keys](./runbooks/REVOKE_LEGACY_CORE_KEYS.md) | Classifying, rotating and purging old API keys | ✅ |
| [Prime multi-tenant ingest](./runbooks/PRIME_MULTI_TENANT_INGEST.md) | One writer, many readers; spotting an accidental second writer | ✅ |
| [Prime sync interval verification (2026-10-26)](./reports/2026-10-26-prime-sync-interval-verification.md) | A zero sync interval is refused before the store opens; positive intervals still serve MCP tools and deliver events | ✅ |
| [SEO evidence setup](./runbooks/SEO_EVIDENCE_SETUP.md) | Every measurement source behind `tooling/seo-evidence`, and the exact condition that unblocks each | ⚠️ |
| [Docker Deployment](./deployment/DOCKER.md) | Container images and configuration | ✅ |
| [Quality Gates](./current/QUALITY_GATES.md) | CI/CD enforcement standards | ✅ |
| [Performance Guide](./current/PERFORMANCE.md) | Optimization and tuning | ✅ |
| [Docker Images](./docker-images.md) | Container configuration | ✅ |
| [Troubleshooting](./guides/TROUBLESHOOTING.md) | Common issues and solutions | ✅ |
| [Test Coverage Plan](./testing/control-plane-coverage-plan.md) | Control plane testing | ✅ |

---

## Roadmaps & Planning

| Document | Description | Status |
|----------|-------------|--------|
| [Comprehensive Roadmap](./roadmaps/2025-10-22_COMPREHENSIVE_ROADMAP.md) | v1.0 to v2.0 vision | ✅ |
| [Query Service Roadmap](./roadmaps/query-service-roadmap.md) | Query service evolution | ✅ |
| [MCP v2 Enhancements](./roadmaps/mcp-v2-enhancements.md) | MCP protocol improvements | ✅ |
| [Vector Embedding Design](./roadmaps/FUTURE_VECTOR_EMBEDDING_DESIGN.md) | Future AI/ML integration | ✅ |
| [LanceDB Comparison](./roadmaps/ALLSOURCE_VS_LANCEDB_COMPARISON.md) | Technology comparison | ✅ |

---

## Archive Index

Historical documentation preserved for reference. Each archived document notes what superseded it.

### Supersession Reference

| Archived Document | Superseded By | Date |
|-------------------|---------------|------|
| `archive/2025-10-22_CLEAN_ARCHITECTURE_FULL.md` | [current/CLEAN_ARCHITECTURE.md](./current/CLEAN_ARCHITECTURE.md) | 2025-10-22 |
| `archive/2025-10-22_SOLID_PRINCIPLES_FULL.md` | [current/SOLID_PRINCIPLES.md](./current/SOLID_PRINCIPLES.md) | 2025-10-22 |
| `archive/2025-10-22_PERFORMANCE_FULL.md` | [current/PERFORMANCE.md](./current/PERFORMANCE.md) | 2025-10-22 |
| `archive/2025-11-04/ARCHITECTURE_OPTIMIZATION_v1.md` | [current/ARCHITECTURE_OPTIMIZATION.md](./current/ARCHITECTURE_OPTIMIZATION.md) | 2025-11-04 |
| `archive/2025-11-04/QUERY_SERVICE_ROADMAP_v1.md` | [roadmaps/query-service-roadmap.md](./roadmaps/query-service-roadmap.md) | 2025-11-04 |
| `archive/2025-10-21_ROADMAP.md` | [roadmaps/2025-10-22_COMPREHENSIVE_ROADMAP.md](./roadmaps/2025-10-22_COMPREHENSIVE_ROADMAP.md) | 2025-10-22 |

### Archive Directories

| Directory | Contents |
|-----------|----------|
| [archive/apps-core-phases/](./archive/apps-core-phases/) | Core app refactoring phases (Phase 1-5) |
| [archive/2025-11-04/](./archive/2025-11-04/) | November 2025 refactoring documentation |
| [archive/2025-11-03-marketing/](./archive/2025-11-03-marketing/) | Marketing materials drafts |
| [archive/2024-11-03/](./archive/2024-11-03/) | Migration documentation |

---

## Directory Structure

```
docs/
├── current/           # ✅ Active documentation
├── archive/           # 📦 Historical documents
├── guides/            # How-to guides
│   └── mcp-server/   # MCP-specific guides
├── roadmaps/          # Planning and vision
├── operations/        # Operational guides
├── testing/           # Test plans and coverage
├── marketing/         # Marketing materials
└── x402/              # X402 protocol documentation
```

---

## Documentation Conventions

### File Naming
- Timestamped: `YYYY-MM-DD_FILENAME.md`
- Current: Descriptive name without timestamp

### Status Markers
All documents should include a status in their frontmatter or header:
- `status: CURRENT` - Actively maintained
- `status: NEEDS_REVIEW` - May need updates
- `status: ARCHIVED` - Historical only

### Linking
Use relative paths for internal links:
```markdown
[Architecture Guide](./current/CLEAN_ARCHITECTURE.md)
[Core README](../apps/core/README.md)
```

---

## Contributing

1. **Creating**: Place new docs in the appropriate directory and update this index
2. **Updating**: Update the `last_updated` date in frontmatter
3. **Archiving**: Move to `archive/` with timestamp, update supersession reference above
4. **Questions**: Create an issue with `[docs]` prefix

---

**Navigation**: [Repository Home](../README.md) | [Current Docs](./current/) | [Guides](./guides/) | [Roadmaps](./roadmaps/) | [Archive](./archive/)

- [Hosted Prime native sync verification](plans/2026-10-26-hosted-prime-sync-verification.md)

- [Hosted Prime sync verification report](reports/2026-10-26-hosted-prime-sync-verification.md)

- [Hosted Prime sync acceptance request](reference/hosted-prime-sync-request.md)

- [Hosted Prime sync verification runbook](runbooks/HOSTED_PRIME_SYNC.md)

- [Dashboard API key lifecycle repair](plans/2026-10-26-api-key-lifecycle-repair.md)
- [Managed API key lifecycle verification](reports/2026-10-26-managed-key-lifecycle-verification.md)

- [Allstack delivery plan 568bfc54-0b16-4b1a-8043-af1d3d3a5a9e](plans/allstack-568bfc54-0b16-4b1a-8043-af1d3d3a5a9e.md)


- [Allstack delivery plan 2432f3f6-c468-46ca-99ba-45fa6185deba](plans/allstack-2432f3f6-c468-46ca-99ba-45fa6185deba.md)
- [Allstack CLI and recovery](reference/allstack.md)
- [Allstack workflow catalog](reference/allstack-catalog.md)
- [Allstack native skills](reference/allstack-skills.md)

- [Allstack delivery plan b67f2a72-0d88-40b5-bf4d-efdc492f4f99](plans/allstack-b67f2a72-0d88-40b5-bf4d-efdc492f4f99.md)

- [Allstack delivery plan d8404323-55a8-4137-8e69-639179e39202](plans/allstack-d8404323-55a8-4137-8e69-639179e39202.md)

- [Allstack delivery plan 66712413-6a19-4587-a359-387994e20530](plans/allstack-66712413-6a19-4587-a359-387994e20530.md)

- [Allstack delivery plan d408c686-a3db-46f1-96a5-c5264bdd1a6d](plans/allstack-d408c686-a3db-46f1-96a5-c5264bdd1a6d.md)
