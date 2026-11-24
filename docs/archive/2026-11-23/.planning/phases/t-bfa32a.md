# Phase plan — Replay Studio impact analysis and SDK workflow

Objective: turn Replay Studio from a rebuild button into an inspectable, automatable projection-recovery workflow.

Context:

- `@docs/plans/2026-08-14-replay-studio-analysis-sdk-design.md`
- `@apps/query-service/lib/query_service_ex_web/controllers/replay_controller.ex`
- `@apps/query-service/lib/query_service_ex/projections/tenant_projections.ex`
- `@apps/web/src/app/dashboard/tools/replay/page.tsx`
- `@sdks/typescript/src/client.ts`
- `@sdks/rust/src/client.rs`
- `@sdks/python-client/src/allsource_client/client.py`
- `@sdks/go/allsource.go`

## Task 1 — Tenant-safe preview contract

Files: Query Service replay controller/router, replay-analysis module, controller tests.

Action: add bounded read-only preview that reports total scope, sample composition, affected entities, event window, projection state, and safety checks. Reuse authenticated tenant boundary. Never expose Core global replay.

Verify: focused ExUnit tests cover valid preview, disabled target, empty history, and tenant propagation.

Done: preview returns stable typed JSON and performs no replay mutation.

## Task 2 — Typed SDK workflow

Files: TypeScript, Rust, Python sync/async, and Go SDK clients/types/tests/readmes.

Action: expose analyze, start, list, get, and cancel operations through Query Service `/api/replay` endpoints. Keep language naming idiomatic and response envelopes hidden.

Verify: each SDK's unit tests assert method, path, request body, and decoded response.

Done: supported SDKs offer same replay workflow without raw HTTP.

## Task 3 — Replay Studio impact console

Files: replay page, web API client/hook, tests.

Action: require analysis before rebuild; render event composition, sampled entities, event window, safety checks, run throughput, and SDK integration snippet. Preserve current dashboard visual system and keyboard behavior.

Verify: TypeScript, focused web tests, production build, desktop/mobile ProofShot.

Done: operator can inspect impact, start rebuild, monitor progress, and copy SDK workflow from one page.

## Overall verification

- Query Service focused and full tests where practical.
- TypeScript, Rust, Python, and Go SDK tests.
- Web focused tests, TypeScript check, production build.
- No tenant-isolation regression.
- Commit and push `main`; confirm Vercel web deployment.
