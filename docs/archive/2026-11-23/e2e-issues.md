# E2E Test Suite — Issues Found

Issues discovered while writing the Dashboard E2E test suite (beads 2xz.5–2xz.18) and backend integration tests (beads 1d0.2–1d0.6).

| # | Page | Element | Description | Status | Fix Location |
|---|------|---------|-------------|--------|--------------|
| 1 | All dashboard pages | TenantContext plug | QS returns `tenant_not_found` for demo accounts when Core loses in-memory tenant data (restart, race condition). Root cause: Core stores tenants in DashMap (in-memory), lost on restart. QS had no fallback. | Fixed | `apps/query-service/lib/query_service_ex_web/plugs/tenant_context.ex` — added lazy auto-provisioning via `auto_provision_tenant/1` |
| 2 | All dashboard pages | RustCoreClient | No `create_tenant/3` function existed to provision tenants from QS into Core. | Fixed | `apps/query-service/lib/query_service_ex/infrastructure/adapters/rust_core_client.ex` — added `create_tenant/3` with 409-conflict handling |
| 3 | Settings > Profile | Email field | CP JWT didn't include `email` claim — QS `/api/auth/me` returned `email: null`. | Fixed (prior session) | `apps/control-plane/auth.go` — Claims struct includes Email and Name |
| 4 | Events page | Live feed | WebSocket URL was hardcoded to `ws://localhost:3902` — never worked in production. `NEXT_PUBLIC_WS_URL` was not set in deployment. | Fixed | `apps/web/src/hooks/use-websocket.ts` — auto-derives WS URL from `NEXT_PUBLIC_API_URL` (`https:` → `wss:`, `http:` → `ws:`) |
| 5 | API Keys page | Key table | API key CRUD requires tenant to be provisioned in Core first. With auto-provisioning fix (#1), this now works. | Fixed | Same as #1 |
| 6 | Demo Zone | Seed data | Demo seeding depends on CP `demo/start` creating tenant in Core. If Core was restarted, seeding would fail silently. Auto-provisioning (#1) mitigates this. | Fixed | Same as #1 |
| 7 | Replay page | Start Replay | Replay shows "No replays yet" for new accounts. Projections (`entity_snapshots`, `event_counters`) exist as Core auto-registers them — replay targets are available. | Known/Acceptable | N/A — correct empty state for new accounts, projections exist as targets |
| 8 | Billing page | Plan cards | Billing page renders client-side with static plan data — no backend dependency issues found. | No Issue | N/A |
| 9 | Team page | Invite modal | Team invite sends request to QS which requires tenant context. Fixed by #1. | Fixed | Same as #1 |
| 10 | Audit Log | Filter pills | Audit log was empty for demo accounts — no entries seeded during demo provisioning. | Fixed | `apps/control-plane/onboard.go` — `buildDemoAuditEntries()` seeds 6 realistic audit events during `demo/start` |

## Summary

- **8 issues fixed** — tenant sync, WebSocket URL, JWT claims, audit log seeding
- **1 known/acceptable** — replay empty state (correct behavior, projection targets exist)
- **1 no issue** — billing page has no backend dependency problems
- **127 E2E tests** across 17 spec files covering all dashboard pages
