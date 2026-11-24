---
title: "Archive Index"
status: CURRENT
last_updated: 2026-11-24
category: reference
---

# Archive Index

This index lists the archived documentation of the AllSource monorepo: the batch archived on 2026-11-23, the five deletions made in the same change, and the older archive folders. It records why each document left the live tree and what replaced it.

## Archive Organization

- **`docs/archive/<YYYY-MM-DD>/`** holds documents archived together on one date. Each file keeps its original path below that directory, so `docs/current/X.md` is archived at `docs/archive/2026-11-23/current/X.md`.
- **Date-prefixed files and folders** in `docs/archive/` (for example `2025-10-19_ARCHITECTURE.md`, `2025-11-04/`) come from the earlier archive passes listed below.
- **Other archive folders** sit next to their live section: `docs/proposals/archived/`, `docs/roadmaps/archive/`, `docs/launch/archived/`, `docs/marketing/archive/` and `apps/core/docs/archive/`.

## Archive locations

| Location | Files | Contents |
|----------|-------|----------|
| [`docs/archive/2026-11-23/`](archive/2026-11-23/) | 104 | Documents archived on 2026-11-23, grouped below by original folder |
| [`docs/archive/`](archive/) (earlier folders) | see below | Archive passes from 2024-11-03 to 2026-03-01 |

| [`docs/proposals/archived/`](proposals/archived/) | 31 | Superseded and shipped proposals and PRDs, 2025 |
| [`docs/roadmaps/archive/`](roadmaps/archive/) | 13 | Roadmaps and status reports, 2025 to 2026-02 |
| [`docs/launch/archived/`](launch/archived/) | 6 | Launch checklists and plans from earlier launch cycles |
| [`docs/marketing/archive/`](marketing/archive/) | 11 | Launch copy for the 2026-06 relaunch and pre-relaunch material |
| [`apps/core/docs/archive/`](../apps/core/docs/archive/) | 9 | Core status reports and metrics notes, 2025-10-20 and 2025-10-21 |

---

## Supersession reference (earlier archive passes)

| Archived document | Superseded by | Date archived |
|-------------------|---------------|---------------|
| `archive/2025-10-19_OLD_README.md` | root [`README.md`](../README.md) | 2025-10-19 |
| `archive/2025-10-19_ARCHITECTURE.md` | [`archive/2026-11-23/current/CLEAN_ARCHITECTURE.md`](archive/2026-11-23/current/CLEAN_ARCHITECTURE.md) (archived; the layout it describes was removed in v0.10.0) | 2025-10-19 |
| `archive/2025-10-21_ROADMAP.md` | [`roadmaps/archive/2025-10-22_COMPREHENSIVE_ROADMAP.md`](roadmaps/archive/2025-10-22_COMPREHENSIVE_ROADMAP.md) | 2025-10-21 |
| `archive/2025-10-22_CLEAN_ARCHITECTURE_FULL.md` | [`current/TENANT_ARCHITECTURE.md`](current/TENANT_ARCHITECTURE.md) and [ADR-005](adr/005-postgresql-removal-event-sourced-metadata.md) | 2025-10-22 |
| `archive/2025-10-22_SOLID_PRINCIPLES_FULL.md` | [`archive/2026-11-23/current/SOLID_PRINCIPLES.md`](archive/2026-11-23/current/SOLID_PRINCIPLES.md) (archived) | 2025-10-22 |
| `archive/2025-10-22_PERFORMANCE_FULL.md` | [`current/PERFORMANCE.md`](current/PERFORMANCE.md) | 2025-10-22 |
| `archive/2025-11-04/ARCHITECTURE_OPTIMIZATION_v1.md` | [`archive/2026-11-23/current/ARCHITECTURE_OPTIMIZATION.md`](archive/2026-11-23/current/ARCHITECTURE_OPTIMIZATION.md) (archived) | 2025-11-04 |
| `archive/2025-11-04/QUERY_SERVICE_ROADMAP_v1.md` | [`roadmaps/archive/query-service-roadmap.md`](roadmaps/archive/query-service-roadmap.md) | 2025-11-04 |
| `archive/2026-03-01-v1-historical/QUERY_SERVICE_IMPLEMENTATION_GUIDE.md` | [`current/TENANT_ARCHITECTURE.md`](current/TENANT_ARCHITECTURE.md) | 2026-03-01 |

---

## Archive directory guide (earlier passes)

### `archive/2024-11-03/`

- `MIGRATION_COMPLETE.md`: early migration completion record.

### `archive/2025-10-19_*`

- `2025-10-19_ARCHITECTURE.md`: first architecture document.
- `2025-10-19_OLD_README.md`: first project README.

### `archive/2025-10-21_*`

- `2025-10-21_COMPREHENSIVE_TEST_SUMMARY.md`, `2025-10-21_FINAL_TEST_SUMMARY.md`, `2025-10-21_UPDATED_TEST_COVERAGE_REPORT.md`, `2025-10-21_TEST_EXECUTION_LOG.md`: test coverage records.
- `2025-10-21_ROADMAP.md`: the first roadmap, superseded.

### `archive/2025-10-22_*`

- `2025-10-22_CLEAN_ARCHITECTURE_FULL.md`, `2025-10-22_PERFORMANCE_FULL.md`, `2025-10-22_SOLID_PRINCIPLES_FULL.md`: full versions of the architecture documents, before they were condensed.

### `archive/2025-11-03-marketing/`

- Launch drafts for the QUICK_START, LinkedIn, X and visual-asset material, with their progress updates, plus `TWITTER_THREAD.md`.

### `archive/2025-11-04/`

- v1 architecture, executive summary and query-service roadmap; refactor plan and completion record; session summary; storage clarification; previous documentation index and folder README.

### `archive/apps-core-phases/`

- Phases 1 to 5 of the Core application: plans, progress and completion summaries, the SierraDB implementation plan, session notes, and `control-plane-README_v0.1.0.md`.

### `archive/2026-02-16-postgres-cleanup/`

- `PRODUCTION_ARCHITECTURE.md` from the PostgreSQL removal, which made Query Service stateless.

### `archive/2026-03-01-v1-historical/`

- `FINAL_V1_REPORT.md`, `V1_SESSION_SUMMARY.md`: October 2025 session records.
- `MIGRATION_VERIFICATION.md`: the Clojure to Elixir migration verification.
- `TEST_COVERAGE_REPORT.md`: October 2025 test snapshot, superseded by CI.
- `QUERY_SERVICE_IMPLEMENTATION_GUIDE.md`: November 2025 Query Service plan, superseded by the stateless architecture.

### Single files

- `archive/2026-03-01-embedded-core-hardening.md`: the 25 findings from two review rounds on the embedded Core library (Issue #73), with the fixes.
- `archive/2026-03-01-ralph-tui-execution-history.md`: log of the ralph-tui autonomous sessions, 2026-01-30 to 2026-03-01.

---

## Archived 2026-11-23: `archive/2026-11-23/`

Each row gives the original path, the reason from the verdict record, and the document that replaces it. A reason that names a shipped surface means the document's content is in code or in a later document; the surface is listed in the last column.

### Repository root, apps and `.planning/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [ACTION-PLAN.md](archive/2026-11-23/ACTION-PLAN.md) | Stray root SEO task list, updated 2026-09-02 | `docs/seo/2026-11-08-allsource-seo-strategy.md` |
| [FULL-AUDIT-REPORT.md](archive/2026-11-23/FULL-AUDIT-REPORT.md) | Stray root audit record, 2026-09-02; outage RCA with SEO and performance items | none |
| [.planning/phases/t-bfa32a.md](archive/2026-11-23/.planning/phases/t-bfa32a.md) | Undated Replay Studio phase plan | `docs/plans/2026-08-14-replay-studio-analysis-sdk-design.md` |
| [tmp/prime-mcp-diagnosis.md](archive/2026-11-23/tmp/prime-mcp-diagnosis.md) | Phase 1 document of the `.prompts/001` Prime MCP fix | commit `8a96633`; `apps/prime-mcp/README.md` |
| [tmp/prime-mcp-verification.md](archive/2026-11-23/tmp/prime-mcp-verification.md) | Phase 3 verification transcript of the same fix | commit `d1e24063`; `apps/prime-mcp/README.md` |
| [apps/control-plane/docs/README.md](archive/2026-11-23/apps/control-plane/docs/README.md) | Stale index, last updated 2025-10-22; links to folders that do not exist | `apps/control-plane/README.md` |
| [apps/control-plane/docs/X402_PRODUCTION_PLAN.md](archive/2026-11-23/apps/control-plane/docs/X402_PRODUCTION_PLAN.md) | Four-task plan; shipped in the control plane | `apps/control-plane/internal/infrastructure/x402/remote_facilitator.go:116` |
| [apps/core/README.v0.10.0.md](archive/2026-11-23/apps/core/README.v0.10.0.md) | Version-pinned copy of the Core README at v0.10.0 | `apps/core/README.md` |
| [apps/core/docs/CHANGELOG.md](archive/2026-11-23/apps/core/docs/CHANGELOG.md) | **Open:** the verdict record says `update`, not `archive`. `RELEASE.md` names this file as the Core changelog, and `release-plz.toml` points at `apps/core/CHANGELOG.md`, which does not exist. Restore it, or archive it and repoint both files together. | none yet |
| [apps/core/docs/FEATURES.md](archive/2026-11-23/apps/core/docs/FEATURES.md) | Undated marketing-style feature list | `docs/current/EVENT_STORE_FEATURES.md` |
| [apps/core/docs/README.md](archive/2026-11-23/apps/core/docs/README.md) | Stale index, last updated 2025-10-22 | `apps/core/README.md`; [`docs/README.md`](README.md) |
| [apps/web/public/worksheets/ARTWORK.md](archive/2026-11-23/apps/web/public/worksheets/ARTWORK.md) | Image provenance note with an absolute local path | none |

### `docs/` root

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [docs/ARCHITECTURE_REVIEW.md](archive/2026-11-23/ARCHITECTURE_REVIEW.md) | Self-declared partly superseded; current-state sections describe removed PostgreSQL-backed QS and in-memory tenant stores | `docs/current/TENANT_ARCHITECTURE.md` |
| [docs/EDITOR_CONFIG_RECOMMENDATIONS.md](archive/2026-11-23/EDITOR_CONFIG_RECOMMENDATIONS.md) | Frontmatter says CURRENT; it is a 2026-02-02 proposal imported from an external editor config | none |
| [docs/MCP_SERVER_ENHANCEMENT_PLAN.md](archive/2026-11-23/MCP_SERVER_ENHANCEMENT_PLAN.md) | Marked superseded; Clojure DSL removed, MCP server now Elixir | `apps/mcp-server-elixir/README.md` |
| [docs/TOON_ADOPTION_ANALYSIS.md](archive/2026-11-23/TOON_ADOPTION_ANALYSIS.md) | February analysis recommending TOON adoption, which has shipped | `docs/plans/2026-10-27-chronis-toon-escaping.md` |
| [docs/e2e-issues.md](archive/2026-11-23/e2e-issues.md) | Undated log of dashboard E2E bugs from closed tracker items | none |

### `docs/current/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [docs/current/ARCHITECTURE_OPTIMIZATION.md](archive/2026-11-23/current/ARCHITECTURE_OPTIMIZATION.md) | Dated 2026-02-02 plan; its work shipped (Core WebSocket, QS channels, PostgreSQL out of the default build) | `docs/adr/005-postgresql-removal-event-sourced-metadata.md`; `docs/current/TENANT_ARCHITECTURE.md` |
| [docs/current/C4_ARCHITECTURE_ANALYSIS.md](archive/2026-11-23/current/C4_ARCHITECTURE_ANALYSIS.md) | 2026-03-01 C4 snapshot, replaced by the June production trace | `docs/current/PROD_DATA_FLOW_C4.md` |
| [docs/current/CLEAN_ARCHITECTURE.md](archive/2026-11-23/current/CLEAN_ARCHITECTURE.md) | Describes a `services/` tree that does not exist and lists the implemented control plane as planned | `CLAUDE.md` (Repository Structure) |
| [docs/current/POSTGRES_MIGRATION_GUIDE.md](archive/2026-11-23/current/POSTGRES_MIGRATION_GUIDE.md) | One-time migration completed for v0.10.0 | `docs/adr/005-postgresql-removal-event-sourced-metadata.md` |
| [docs/current/SOLID_PRINCIPLES.md](archive/2026-11-23/current/SOLID_PRINCIPLES.md) | v1.0 principles essay with aspirational examples and a time-boxed phase plan | none |

### `docs/plans/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [2026-08-13-settings-secret-boundary-design.md](archive/2026-11-23/plans/2026-08-13-settings-secret-boundary-design.md) | Shipped | `apps/web/src/app/api/auth/session/route.ts` |
| [2026-08-14-allsource-product-identity-design.md](archive/2026-11-23/plans/2026-08-14-allsource-product-identity-design.md) | Shipped | `apps/web/src/app/(marketing)/what-is-allsource/page.tsx` |
| [2026-08-14-capability-workbench-design.md](archive/2026-11-23/plans/2026-08-14-capability-workbench-design.md) | Shipped | `apps/web/src/components/demo/capability-workbench.tsx` |
| [2026-08-14-demo-zone-recovery-design.md](archive/2026-11-23/plans/2026-08-14-demo-zone-recovery-design.md) | Shipped | `apps/web/src/app/dashboard/demo/page.tsx` |
| [2026-08-14-stream-processing-page-design.md](archive/2026-11-23/plans/2026-08-14-stream-processing-page-design.md) | Shipped | `apps/web/src/app/(marketing)/platform/stream-processing/page.tsx` |
| [2026-08-14-use-cases-page-design.md](archive/2026-11-23/plans/2026-08-14-use-cases-page-design.md) | Shipped | `apps/web/src/app/(marketing)/use-cases/page.tsx` |
| [2026-08-25-organic-growth-page-design.md](archive/2026-11-23/plans/2026-08-25-organic-growth-page-design.md) | Shipped | `apps/web/src/app/(marketing)/event-replay-debugging/page.tsx` |
| [2026-08-27-design-partner-page-autoresearch-design.md](archive/2026-11-23/plans/2026-08-27-design-partner-page-autoresearch-design.md) | Shipped | `docs/research/2026-08-27-design-partner-page-autoresearch.md` |
| [2026-08-27-design-partner-page-simplification-design.md](archive/2026-11-23/plans/2026-08-27-design-partner-page-simplification-design.md) | Shipped | `apps/web/src/app/(marketing)/design-partners/page.tsx` |
| [2026-09-10-allsource-navigation-simplification-design.md](archive/2026-11-23/plans/2026-09-10-allsource-navigation-simplification-design.md) | Shipped | `apps/web/src/lib/config.ts` |
| [2026-10-24-bounded-prefix-auth-reads.md](archive/2026-11-23/plans/2026-10-24-bounded-prefix-auth-reads.md) | Shipped | `apps/core/src/infrastructure/web/api.rs:627` |
| [2026-10-24-bounded-retained-entity.md](archive/2026-11-23/plans/2026-10-24-bounded-retained-entity.md) | Shipped | commit `4d94cfea` |
| [2026-10-24-jwt-crypto-backend.md](archive/2026-11-23/plans/2026-10-24-jwt-crypto-backend.md) | Shipped | commit `fc45cee2`; `Cargo.toml:97` |
| [2026-10-24-shared-tenant-read-model.md](archive/2026-11-23/plans/2026-10-24-shared-tenant-read-model.md) | Shipped | commit `50abe5a7`; release `16dd13eb` |
| [2026-10-25-prime-sync-interval-validation.md](archive/2026-11-23/plans/2026-10-25-prime-sync-interval-validation.md) | Shipped and verified | `docs/reports/2026-10-26-prime-sync-interval-verification.md` |
| [2026-10-26-api-key-lifecycle-repair.md](archive/2026-11-23/plans/2026-10-26-api-key-lifecycle-repair.md) | Shipped and verified | `docs/reports/2026-10-26-managed-key-lifecycle-verification.md` |
| [2026-10-26-hosted-prime-sync-verification.md](archive/2026-11-23/plans/2026-10-26-hosted-prime-sync-verification.md) | One-time hosted proof plan | `docs/reports/2026-10-26-hosted-prime-sync-verification.md` |
| [2026-10-26-legacy-key-authority-compatibility.md](archive/2026-11-23/plans/2026-10-26-legacy-key-authority-compatibility.md) | Shipped with the managed-key authority work | `docs/reports/2026-10-26-managed-key-lifecycle-verification.md` |
| [2026-10-27-chronis-toon-escaping.md](archive/2026-11-23/plans/2026-10-27-chronis-toon-escaping.md) | Shipped | `docs/reports/2026-10-27-chronis-native-verification.md` |
| [2026-10-27-prime-toon-key-quoting.md](archive/2026-11-23/plans/2026-10-27-prime-toon-key-quoting.md) | Shipped | `docs/reports/2026-10-27-prime-toon-native-verification.md` |
| [2026-10-28-prime-toon-control-values.md](archive/2026-11-23/plans/2026-10-28-prime-toon-control-values.md) | Shipped | `docs/reports/2026-10-28-prime-toon-values-verification.md` |
| [allstack-1a9c09ae-98ab-4120-bbe0-72342e530296.md](archive/2026-11-23/plans/allstack-1a9c09ae-98ab-4120-bbe0-72342e530296.md) | Allstack delivery plan; shipped | commit `019b7e57` |
| [allstack-c6e1b5d6-e63d-4ed1-b90e-337b9bb904e5.md](archive/2026-11-23/plans/allstack-c6e1b5d6-e63d-4ed1-b90e-337b9bb904e5.md) | Earlier 26-line draft of the native CI prerequisites | `docs/plans/2026-10-28-native-ci-prerequisites.md` |
| [allstack-d3f36c01-447b-4640-904c-13571a14aaa7.md](archive/2026-11-23/plans/allstack-d3f36c01-447b-4640-904c-13571a14aaa7.md) | Earlier 32-line draft of the native CI prerequisites | `docs/plans/2026-10-28-native-ci-prerequisites.md` |
| [allstack-d408c686-a3db-46f1-96a5-c5264bdd1a6d.md](archive/2026-11-23/plans/allstack-d408c686-a3db-46f1-96a5-c5264bdd1a6d.md) | Draft combining the Prime control-value plan with a CI prerequisite section | `docs/plans/2026-10-28-prime-toon-control-values.md`; `docs/plans/2026-10-28-native-ci-prerequisites.md` |
| [allstack-eb5af30a-ba11-49c9-b3be-328a66a6f7af.md](archive/2026-11-23/plans/allstack-eb5af30a-ba11-49c9-b3be-328a66a6f7af.md) | Earlier variant of the native CI prerequisites | `docs/plans/2026-10-28-native-ci-prerequisites.md` |

### `docs/proposals/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [AGENT_AUTH_DESIGN.md](archive/2026-11-23/proposals/AGENT_AUTH_DESIGN.md) | Superseded design, 2026-03-27 | `docs/proposals/UNIFIED_AUTH_TEAMS.md`; `docs/proposals/prd-agent-auth-x402.md` |
| [better-auth-migration.md](archive/2026-11-23/proposals/better-auth-migration.md) | Shipped | `apps/auth/`; `crates/better-auth-allsource/` |
| [MEDIA_AND_SEO_CHECKLIST.md](archive/2026-11-23/proposals/MEDIA_AND_SEO_CHECKLIST.md) | Expired marketing-readiness checklist, 2026-03-24 | `apps/web/public/`; `apps/web/src/app/sitemap.ts` |
| [ROADMAP_v1.md](archive/2026-11-23/proposals/ROADMAP_v1.md) | Planning roadmap, 2026-03-27, version 0.17.2 | git tags |
| [prd-remove-lemonsqueezy-from-qs.md](archive/2026-11-23/proposals/prd-remove-lemonsqueezy-from-qs.md) | Shipped: no LemonSqueezy webhook or config left in Query Service | `apps/control-plane` (billing owner) |

### `docs/reference/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [chronis-toon-request.md](archive/2026-11-23/reference/chronis-toon-request.md) | One-shot work order | `docs/plans/2026-10-27-chronis-toon-escaping.md`; `docs/reports/2026-10-27-chronis-native-verification.md` |
| [hosted-prime-sync-request.md](archive/2026-11-23/reference/hosted-prime-sync-request.md) | One-shot verification work order | `docs/plans/2026-10-26-hosted-prime-sync-verification.md`; `docs/reports/2026-10-26-hosted-prime-sync-verification.md` |
| [prime-toon-keys-request.md](archive/2026-11-23/reference/prime-toon-keys-request.md) | Shipped work order | `docs/plans/2026-10-27-prime-toon-key-quoting.md`; `docs/reports/2026-10-27-prime-toon-native-verification.md` |
| [prime-toon-values-request.md](archive/2026-11-23/reference/prime-toon-values-request.md) | Shipped work order | `docs/plans/2026-10-28-prime-toon-control-values.md`; `docs/reports/2026-10-28-prime-toon-values-verification.md` |
| [prime-typed-provenance-request.md](archive/2026-11-23/reference/prime-typed-provenance-request.md) | Shipped work order | `docs/reports/2026-11-06-prime-hash-gap-status.md` |

### `docs/checklists/`, `docs/operations/`, `docs/testing/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [checklists/NEXT_STEPS.md](archive/2026-11-23/checklists/NEXT_STEPS.md) | Status as of 2026-03-01 with 46 unchecked items | `docs/runbooks/FLY_FRONTENDS.md`; `CLAUDE.md` (Production Deployment Topology) |
| [operations/2025-10-22_DOCUMENTATION_ORGANIZATION.md](archive/2026-11-23/operations/2025-10-22_DOCUMENTATION_ORGANIZATION.md) | Completed one-off reorganisation log; frontmatter wrongly says CURRENT | [`docs/README.md`](README.md); this index |
| [testing/control-plane-coverage-plan.md](archive/2026-11-23/testing/control-plane-coverage-plan.md) | Phase plan from a 23.1% coverage baseline | `apps/control-plane` test files and CI gates |

### `docs/roadmaps/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [ALLSOURCE_VS_TURSO_COMPARISON.md](archive/2026-11-23/roadmaps/ALLSOURCE_VS_TURSO_COMPARISON.md) | Competitive analysis for an expired launch plan, not a plan of record | `docs/launch/archived/TURSO_COMPETITIVE_LAUNCH_PLAN.md` |
| [ANALYSIS_COMPLETE_SUMMARY.md](archive/2026-11-23/roadmaps/ANALYSIS_COMPLETE_SUMMARY.md) | Dated 2025-11-04 LanceDB research summary; calls vector work future although Prime vectors shipped | `apps/core/src/prime/vectors/` |
| [CHRONOS_VS_LANCEDB_COMPARISON.md](archive/2026-11-23/roadmaps/CHRONOS_VS_LANCEDB_COMPARISON.md) | Technology comparison from 2025-11-04, not a plan | none |
| [FUTURE_VECTOR_EMBEDDING_DESIGN.md](archive/2026-11-23/roadmaps/FUTURE_VECTOR_EMBEDDING_DESIGN.md) | Research-phase roadmap; vector support shipped as a Prime module | `apps/core/src/prime/vectors/` |
| [KNOWLEDGE_GRAPH_ROADMAP.md](archive/2026-11-23/roadmaps/KNOWLEDGE_GRAPH_ROADMAP.md) | Draft feature roadmap from 2026-03-01; graph types shipped as a Prime module | `apps/core/src/prime/` |
| [LANCEDB_ANALYSIS_SUMMARY.md](archive/2026-11-23/roadmaps/LANCEDB_ANALYSIS_SUMMARY.md) | Companion to the vector embedding design; states it is not part of the current roadmap | none |

### `docs/articles/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [oracle-26ai-x-thread.md](archive/2026-11-23/articles/oracle-26ai-x-thread.md) | Campaign copy for a time-bound announcement | `docs/articles/oracle-26ai-comparison.md` |
| [zer0dex-comparison.md](archive/2026-11-23/articles/zer0dex-comparison.md) | Earlier draft of the published post | `apps/web/content/zer0dex-vs-allsource-recall.mdx` |
| [zer0dex-x-thread.md](archive/2026-11-23/articles/zer0dex-x-thread.md) | Thread promoting a product that is not shipped | `apps/web/content/zer0dex-vs-allsource-recall.mdx` |

### `docs/marketing/launch-2026-07/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [00-LAUNCH-PLAN.md](archive/2026-11-23/marketing/launch-2026-07/00-LAUNCH-PLAN.md) | Launch window (last updated 2026-07-17, four-week run) closed; most channel steps unchecked | `docs/marketing/design-partner-launch-kit.md` |
| [CHECKLISTS.md](archive/2026-11-23/marketing/launch-2026-07/CHECKLISTS.md) | Checklist for the expired launch | none |
| [copy-mcp-lists-discord.md](archive/2026-11-23/marketing/launch-2026-07/copy-mcp-lists-discord.md) | Launch copy for the expired campaign | none |
| [copy-newsletters.md](archive/2026-11-23/marketing/launch-2026-07/copy-newsletters.md) | Launch pitches for the expired campaign | none |
| [copy-product-hunt.md](archive/2026-11-23/marketing/launch-2026-07/copy-product-hunt.md) | Product Hunt kit for the expired campaign | none |
| [copy-reddit.md](archive/2026-11-23/marketing/launch-2026-07/copy-reddit.md) | Reddit copy for the expired campaign | none |
| [copy-x-threads.md](archive/2026-11-23/marketing/launch-2026-07/copy-x-threads.md) | X threads for the expired campaign | none |

### `docs/sales/`, `docs/demo/`, `docs/demos/`, `docs/launch/`, `docs/use-cases/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [sales/alphasigma-architecture-diagrams.md](archive/2026-11-23/sales/alphasigma-architecture-diagrams.md) | Customer-specific design; the quant surfaces are absent from code | none |
| [sales/alphasigma-faq.md](archive/2026-11-23/sales/alphasigma-faq.md) | Customer Q&A from the proposal stage; broken links | none |
| [sales/alphasigma-pro-use-case.md](archive/2026-11-23/sales/alphasigma-pro-use-case.md) | Customer analysis with proposed APIs that are not in code | none |
| [demo/AMAZING_DEMO_DESIGN.md](archive/2026-11-23/demo/AMAZING_DEMO_DESIGN.md) | February 2026 demo design; the demo shipped another way | `docs/demo/DEMO_ACCOUNTS.md`; `apps/web/src/app/dashboard/demo/page.tsx` |
| [demo/DEMO_IMPLEMENTATION_CHECKLIST.md](archive/2026-11-23/demo/DEMO_IMPLEMENTATION_CHECKLIST.md) | Build checklist with 80 boxes open and none checked; features shipped elsewhere | `apps/web/src/lib/context/time-travel-context.tsx`; `docs/demo/DEMO_ACCOUNTS.md` |
| [demos/a16z-speedrun-application.md](archive/2026-11-23/demos/a16z-speedrun-application.md) | Application copy for one expired application; figures stale | none |
| [demos/x-thread.md](archive/2026-11-23/demos/x-thread.md) | One-off thread for an expired campaign | none |
| [launch/2026-06-19-launch-kit.md](archive/2026-11-23/launch/2026-06-19-launch-kit.md) | Dated launch campaign copy for an expired posting plan | none |
| [use-cases/SERVER_SIDE_PROJECTIONS_USE_CASES.md](archive/2026-11-23/use-cases/SERVER_SIDE_PROJECTIONS_USE_CASES.md) | Describes unversioned endpoints and a proposal not in the tree | `docs/use-cases/custom-projections.md`; `/api/v1/projections` in Query Service |

### `docs/x402/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [BATTLE_PLAN.md](archive/2026-11-23/x402/BATTLE_PLAN.md) | November 2025 to February 2026 hackathon plan; campaign expired | `apps/control-plane/docs/X402_SETUP.md` |
| [HACKATHON_CHECKLIST.md](archive/2026-11-23/x402/HACKATHON_CHECKLIST.md) | Hackathon checklist; expired | `apps/control-plane/docs/X402_SETUP.md` |
| [PRD.md](archive/2026-11-23/x402/PRD.md) | November 2025 PRD, shipped in the control-plane x402 middleware | `apps/control-plane/docs/X402_PRODUCTION_PLAN.md` |
| [PROGRESS_TRACKER.md](archive/2026-11-23/x402/PROGRESS_TRACKER.md) | Blank hackathon phase tracker | none |
| [QUICK_REFERENCE.md](archive/2026-11-23/x402/QUICK_REFERENCE.md) | Hackathon command card; commands target a Solana SDK package not in the tree | `apps/control-plane/docs/X402_SETUP.md` |
| [README.md](archive/2026-11-23/x402/README.md) | Hackathon index, self-labelled CURRENT; referenced paths missing | `apps/control-plane/docs/X402_SETUP.md` |
| [SAAS_STRATEGY.md](archive/2026-11-23/x402/SAAS_STRATEGY.md) | November 2025 brainstorm, superseded by the shipped x402 and agent-auth work | `docs/proposals/prd-agent-auth-x402.md` |

### `docs/outreach/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [2026-10-20-omo-matched-no-reload-evaluation.md](archive/2026-11-23/outreach/2026-10-20-omo-matched-no-reload-evaluation.md) | Superseded by the later evaluations; its own note names the skill-only rerun as the causal result | `docs/outreach/2026-10-21-omo-provider-payload-evaluation.md` |
| [2026-10-20-omo-model-selected-evaluation.md](archive/2026-11-23/outreach/2026-10-20-omo-model-selected-evaluation.md) | Its fallback-dependent interpretation is superseded | `docs/outreach/2026-10-21-omo-provider-payload-evaluation.md` |
| [2026-10-20-omo-skill-only-counterbalanced-evaluation.md](archive/2026-11-23/outreach/2026-10-20-omo-skill-only-counterbalanced-evaluation.md) | Superseded by the readiness-checked rerun | `docs/outreach/2026-10-21-omo-provider-payload-evaluation.md` |
| [2026-10-21-omo-readiness-barrier-evaluation.md](archive/2026-11-23/outreach/2026-10-21-omo-readiness-barrier-evaluation.md) | Superseded; fixed-barrier controls reproduced in the provider-payload evaluation | `docs/outreach/2026-10-21-omo-provider-payload-evaluation.md` |

### `docs/seo/`

| Original path | Reason | Superseded by |
|---------------|--------|---------------|
| [2026-08-29-allsource-audit/…/ACTION-PLAN.md](archive/2026-11-23/seo/2026-08-29-allsource-audit/www-all-source-xyz-audit-20260829-015519/ACTION-PLAN.md) | Raw pipeline plan built on a false PageSpeed fallback (performance 35/100 from heuristic LCP and INP) | `docs/seo/2026-08-29-allsource-audit/www-all-source-xyz-audit-20260829-015519/VERIFIED-ACTION-PLAN.md` |
| [2026-08-29-allsource-audit/…/FULL-AUDIT-REPORT.md](archive/2026-11-23/seo/2026-08-29-allsource-audit/www-all-source-xyz-audit-20260829-015519/FULL-AUDIT-REPORT.md) | Raw 78/100 report; performance 35/100 from the heuristic fallback | `docs/seo/2026-08-29-allsource-audit/www-all-source-xyz-audit-20260829-015519/VERIFIED-AUDIT-REPORT.md` |
| [2026-11-07-editorial-review/2026-11-04-event-replay.md](archive/2026-11-23/seo/2026-11-07-editorial-review/2026-11-04-event-replay.md) | Work shipped: the sentence and link are on main | `apps/web/content/why-event-sourcing-2026.mdx:137` |
| [2026-11-07-editorial-review/2026-11-05-version-control-comparison.md](archive/2026-11-23/seo/2026-11-07-editorial-review/2026-11-05-version-control-comparison.md) | Work shipped: the replacement paragraph is on main | `apps/web/content/why-event-sourcing-2026.mdx:61` |
| [2026-11-07-editorial-review/2026-11-06-kafka-comparison.md](archive/2026-11-23/seo/2026-11-07-editorial-review/2026-11-06-kafka-comparison.md) | Work shipped: the replacement Kafka paragraph is on main | `apps/web/content/why-event-sourcing-2026.mdx:57` |
| [2026-11-09-schema-preview.html](archive/2026-11-23/seo/2026-11-09-schema-preview.html) | Preview of a change now on main | `apps/web/src/lib/event-sourcing-patterns.ts:471` |
| [2026-11-11-aws-reference-preview.html](archive/2026-11-23/seo/2026-11-11-aws-reference-preview.html) | Preview of a change now on main | `apps/web/src/lib/event-sourcing-patterns.ts:41` |

---

## Deleted 2026-11-23

Five copies of allstack plans were deleted in the same change that archived their survivors. The verdict record names each survivor.

| Deleted path | Survivor | Basis |
|--------------|----------|-------|
| `docs/plans/allstack-2432f3f6-c468-46ca-99ba-45fa6185deba.md` | [`docs/archive/2026-11-23/plans/2026-10-26-legacy-key-authority-compatibility.md`](archive/2026-11-23/plans/2026-10-26-legacy-key-authority-compatibility.md) | Earlier revision; the dated copy adds the session-introspection and egress-lease constraints |
| `docs/plans/allstack-568bfc54-0b16-4b1a-8043-af1d3d3a5a9e.md` | [`docs/archive/2026-11-23/plans/2026-10-26-hosted-prime-sync-verification.md`](archive/2026-11-23/plans/2026-10-26-hosted-prime-sync-verification.md) | Identical (diff verified) |
| `docs/plans/allstack-66712413-6a19-4587-a359-387994e20530.md` | [`docs/archive/2026-11-23/plans/2026-10-28-prime-toon-control-values.md`](archive/2026-11-23/plans/2026-10-28-prime-toon-control-values.md) | Near-exact duplicate; the dated copy is the accurate one |
| `docs/plans/allstack-b67f2a72-0d88-40b5-bf4d-efdc492f4f99.md` | [`docs/archive/2026-11-23/plans/2026-10-27-chronis-toon-escaping.md`](archive/2026-11-23/plans/2026-10-27-chronis-toon-escaping.md) | Identical (diff verified) |
| `docs/plans/allstack-d8404323-55a8-4137-8e69-639179e39202.md` | [`docs/archive/2026-11-23/plans/2026-10-27-prime-toon-key-quoting.md`](archive/2026-11-23/plans/2026-10-27-prime-toon-key-quoting.md) | Identical (diff verified) |

---

## Other archive folders

### `proposals/archived/` (31 files)

Superseded and shipped proposals and PRDs from 2025 to 2026, including `CORE_REPLICATION_DESIGN.md`, `SERVER_SIDE_PROJECTIONS.md`, `SERVICE_RESPONSIBILITY_REALIGNMENT.md`, `WEBSOCKET_MINT_MIGRATION.md`, `prd-prime-graph-engine.md`, `prd-tiered-context-loading.md` and the other `prd-*.md` files in the folder.

### `roadmaps/archive/` (13 files)

Roadmaps and status reports from October 2025 to February 2026: `2025-10-22_COMPREHENSIVE_ROADMAP.md`, `2026-02-02_CONSOLIDATED_ROADMAP.md`, `query-service-roadmap.md`, `mcp-v2-enhancements.md`, phase and progress reports, and `SAAS_LAUNCH_ROADMAP.md`.

### `launch/archived/` (6 files)

`LAUNCH_CHECKLIST_MANUAL.md`, `LAUNCH_READINESS_ASSESSMENT.md`, `MARKETING_MATERIALS.md`, `REMAINING_TASKS.md`, `SOFT_LAUNCH_CHECKLIST.md`, `TURSO_COMPETITIVE_LAUNCH_PLAN.md`.

### `marketing/archive/` (11 files)

- `2026-06-relaunch/`: the June 2026 relaunch plan, pinned tweets, Product Hunt kit, repurposed posts, X hook templates and the pricing reversal thread.
- `pre-relaunch/`: early alpha announcement, the April 2026 pricing decision, Product Hunt listing, February 2026 social posts and the launch thread.

### `apps/core/docs/archive/` (9 files)

October 2025 Core status files: metrics implementation, Prometheus metrics, status, V1 roadmap, final assessment, honest V1 status, performance report, V1 complete and V1 status.

---

## When to Archive

Documents should be moved to the archive when:

1. **Superseded by newer version**: a document has been replaced by an updated version with significant changes.
2. **Phase completed**: development phase documentation that is no longer actively referenced.
3. **Historical value**: content that may be useful for reference but is no longer current.
4. **Marketing cycles**: campaign materials after the campaign closes.
5. **Session-specific**: development session notes after their content is in the main documentation.

### Archiving Process

1. Move the document to `docs/archive/<YYYY-MM-DD>/`, keeping its original path below that directory.
2. Add the document to the table for its original folder in this index, with the reason and the successor.
3. Update any link in current documentation that pointed at the archived file, and remove the entry from [`docs/README.md`](README.md).

---

## When to Delete

**Never delete documentation without creating a backup first.**

Documents may be considered for deletion only when:

1. **Duplicates**: exact duplicates of a document that is kept, in the live tree or in the archive.
2. **Empty or placeholder**: files with no meaningful content.
3. **Generated artifacts**: auto-generated files that can be recreated.
4. **Sensitive information**: documents containing credentials or secrets (after a secure backup).

### Deletion Process

1. **Verify** the document meets a deletion criterion, and compare it with the survivor.
2. **Create a backup** in a secure location outside the repository.
3. **Record** the deletion and its survivor in the commit message and in the Deleted section of this index.
4. **Remove** references to the deleted file from other documentation.

### What to Never Delete

- Session summaries (historical context)
- Architecture decisions (ADRs)
- Phase completion documentation
- Version history documents (v1, v2, etc.)
- Marketing materials (brand consistency reference)

---

## Documents updated in place

These documents were corrected in place rather than archived. Where the document has since been archived, the table gives its current location.

| Document | Action taken | Current location |
|----------|--------------|------------------|
| `current/C4_ARCHITECTURE_ANALYSIS.md` | Updated: MCP Elixir port, tenant metadata WAL-durable, Gap 1 downgraded | [archive/2026-11-23/current/](archive/2026-11-23/current/C4_ARCHITECTURE_ANALYSIS.md) (archived) |
| `current/TENANT_ARCHITECTURE.md` | Rewritten: event-sourced system streams, stateless QS, no PostgreSQL | [`current/TENANT_ARCHITECTURE.md`](current/TENANT_ARCHITECTURE.md) (live) |
| `ARCHITECTURE_REVIEW.md` | Staleness note: QS stateless since v0.10.0 | [archive/2026-11-23/ARCHITECTURE_REVIEW.md](archive/2026-11-23/ARCHITECTURE_REVIEW.md) (archived) |
| `proposals/SERVICE_RESPONSIBILITY_REALIGNMENT.md` | Status: partially superseded; QS no longer uses PostgreSQL | [`proposals/archived/SERVICE_RESPONSIBILITY_REALIGNMENT.md`](proposals/archived/SERVICE_RESPONSIBILITY_REALIGNMENT.md) (archived earlier) |
| `proposals/better-auth-migration.md` | Staleness note: QS has no PostgreSQL to share | [archive/2026-11-23/proposals/better-auth-migration.md](archive/2026-11-23/proposals/better-auth-migration.md) (archived) |
| `proposals/SERVER_SIDE_PROJECTIONS.md` | Status: implemented (v0.10.5), references ADR-006 | [`proposals/archived/SERVER_SIDE_PROJECTIONS.md`](proposals/archived/SERVER_SIDE_PROJECTIONS.md) (archived earlier) |
| `MCP_SERVER_ENHANCEMENT_PLAN.md` | Status: superseded; the Clojure DSL is removed and MCP is Elixir | [archive/2026-11-23/MCP_SERVER_ENHANCEMENT_PLAN.md](archive/2026-11-23/MCP_SERVER_ENHANCEMENT_PLAN.md) (archived) |
| `docker-images.md` | Updated: image names `chronos-*`, MCP port, version | [`docker-images.md`](docker-images.md) (live) |
| `QUICK_START.md` | Updated: URLs to all-source.xyz, API paths under `/api/v1/` | [`QUICK_START.md`](QUICK_START.md) (live) |
| `deployment/DOCKER.md` | Rewritten: GHCR org, no `DATABASE_URL`, `CORE_URL`, control plane on port 3901 | [`deployment/DOCKER.md`](deployment/DOCKER.md) (live) |
| `launch/SOFT_LAUNCH_CHECKLIST.md` | Rewritten: no PostgreSQL steps, control-plane deployment added | [`launch/archived/SOFT_LAUNCH_CHECKLIST.md`](launch/archived/SOFT_LAUNCH_CHECKLIST.md) (archived earlier) |
| `checklists/NEXT_STEPS.md` | Section 2 rewritten to the current architecture | [archive/2026-11-23/checklists/NEXT_STEPS.md](archive/2026-11-23/checklists/NEXT_STEPS.md) (archived) |

---

## Need / look in

| Need | Look in |
|------|---------|
| Old architecture details | `archive/2026-11-23/current/` and `archive/2025-10-22_*_FULL.md` |
| Test history | `archive/2025-10-21_*` |
| Development phases (Core) | `archive/apps-core-phases/` |
| Marketing content | `archive/2025-11-03-marketing/`, `marketing/archive/`, `archive/2026-11-23/marketing/` |
| v1 documents | `archive/2025-11-04/` |
| Migration history | `archive/2024-11-03/` |
| PostgreSQL cleanup | `archive/2026-02-16-postgres-cleanup/` |
| V1 historical docs | `archive/2026-03-01-v1-historical/` |
| Embedded Core hardening | `archive/2026-03-01-embedded-core-hardening.md` |
| Ralph-TUI execution history | `archive/2026-03-01-ralph-tui-execution-history.md` |
| Architecture decisions | [`adr/README.md`](adr/README.md) (21 ADRs) |

---

*This index is updated whenever documents are archived or deleted.*
