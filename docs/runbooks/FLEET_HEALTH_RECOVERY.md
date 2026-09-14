# Fleet Health & Recovery — operator surface catalog

**Use when:** you hit one of the data-visibility or billing-cutover incidents and want
the **named tool that automates the fix** instead of re-deriving it from prose. This is
the short pointer; the design rationale lives in
[`docs/proposals/CONTROL_PLANE_TENANT_HEALTH_RECOVERY.md`](../proposals/CONTROL_PLANE_TENANT_HEALTH_RECOVERY.md),
and the incident narratives live in
[`DIAGNOSING_PIPELINE_DATA_VISIBILITY.md`](./DIAGNOSING_PIPELINE_DATA_VISIBILITY.md) and
[`PRICING_BILLING_CUTOVER.md`](./PRICING_BILLING_CUTOVER.md).

**Status:** P0–P3 **shipped to `main`** (Control Plane API `a02667e`, admin UI `e233bee`,
MCP tools `b6e3c88`). **Control Plane DEPLOYED** — Fly `allsource-control-plane` **v68**;
`/api/v1/admin/fleet/*` + `/api/v1/admin/recovery/*` are live and admin-gated. **Admin UI
DEPLOYED** — Vercel project `allsource-admin` — but **login is not wired cross-origin yet**
(needs the admin custom domain + CORS + OAuth; see the checklist below and
[`CONTROL_PLANE_CORS.md`](./CONTROL_PLANE_CORS.md)). Until that lands the admin pages fall
back to a clearly-marked **fixture**.

**Central invariant (do not undercut it):** "a tenant reports empty data" is a
**read-path / identity symptom, never data loss** — Core IS the durable database
(WAL + Parquet + DashMap). Every signal and recovery action below is built on that;
none of them treats an empty screen as a reason to restore/replay by default.

---

## The three surfaces

The health model + recovery playbook live **once** in the Go Control Plane. The admin web
app and the Elixir MCP server are thin consumers — no parallel scoring logic.

| Surface | Where | Read | Mutate |
|---|---|---|---|
| **Control Plane admin API** | `apps/control-plane` — `internal/interfaces/http/{fleet_health_handler.go,recovery_handler.go}`, under `/api/v1/admin` (AdminAuthMiddleware) | `GET …/fleet/health`, `…/fleet/health/:id`, `…/recovery/diagnose/edition`, `…/recovery/:id/diagnose-identity` | `POST …/recovery/:id/{resync,reconcile-subscription,resolve-dunning,rotate-keys,reprovision,restore}`, `POST …/recovery/batch` |
| **Admin web app** | `apps/admin` — `app/(authenticated)/fleet/page.tsx` + `fleet/[id]/page.tsx`, `components/fleet/{health-chip,recovery-dialog}.tsx`, `lib/fleet-api.ts` | Fleet overview (4 tier counts + worst-N table), per-tenant drill-down (all signals) | Recovery console — guards rendered as real UI (dry-run preview → typed confirm / echoed count → Apply) |
| **Elixir MCP** | `apps/mcp-server-elixir` — `protocol/mcp_tools.ex` | `fleet_health_summary`, `tenant_health_assessment` (gated by `control_plane_enabled`) | `recovery_*` (gated by `ALLSOURCE_SYSTEM_ADMIN`, **off by default**) |

**MCP lives in `mcp-server-elixir`, never `prime-mcp`** — `prime-mcp` is single-tenant by
design; a fleet/cross-tenant tool there would cross a tenant boundary it does not have.

### Auth & the system-admin gate

- **Admin role.** All public auth terminates at the Control Plane. The admin role is minted
  at **`apps/control-plane/auth.go` `roleForEmail()`** — it maps `ADMIN_EMAILS`
  (comma-separated, **case-insensitive**) → `RoleAdmin` at token-mint; every other email
  mints as a developer. The admin middleware (`admin_middleware.go:69`) enforces **only** the
  resulting `role == "admin"` claim — it never re-reads `ADMIN_EMAILS`. One allowlist,
  applied once at mint. Add/remove an operator by editing `ADMIN_EMAILS` on the Control Plane.
- **`ALLSOURCE_SYSTEM_ADMIN` (MCP, mutating tools).** An MCP client merely connected to the
  Control Plane can **read** fleet health, but cannot run any `recovery_*` mutation unless the
  operator explicitly set `ALLSOURCE_SYSTEM_ADMIN=true` on that server instance. Off by default.
  With it unset, `tools/list` omits every `recovery_*` tool and calling one returns a
  "system-admin not enabled" error.
- **Destructive-action guards are server-enforced**, not just UI: every non-read action takes
  `dry_run` (default **on** for Destructive); Destructive single-tenant actions require a typed
  `confirm_tenant_id`; `rotate-keys` + `batch` require an echoed `confirm_token` from a prior
  dry-run; `batch` is capped at `max_tenants` (default 25, ceiling 100) and forbids
  reprovision/restore/rotate. Every mutating apply writes an `admin.recovery.*` audit event
  into Core.

---

## Incident → tool map (the fast paths)

| Incident (runbook) | Read / diagnose | Fix |
|---|---|---|
| Edition=community trap — [data-visibility §5 #5](./DIAGNOSING_PIPELINE_DATA_VISIBILITY.md#5-worked-example--the-five-layered-causes-behind-one-empty-memory-tab) | `GET …/recovery/diagnose/edition` (MCP `recovery_diagnose_edition`) | **Operator-executed:** `ALLSOURCE_EDITION=enterprise` on `allsource-query` + confirm a **new** Fly release. The endpoint returns the command + a verify probe; it changes nothing itself. |
| API-key role-string drift `service_account` vs `serviceaccount` — [§5 #1](./DIAGNOSING_PIPELINE_DATA_VISIBILITY.md) | `check_key_role` (the `api_key_validity` signal on `…/fleet/health/:id`) | Guarded→Destructive `rotate_keys` — `POST …/recovery/:id/rotate-keys` (re-mints canonical `serviceaccount`; dry-run + echoed `confirm_token`). |
| Wrong-tenant session / JWT `tenant_id` mismatch — [§7](./DIAGNOSING_PIPELINE_DATA_VISIBILITY.md#7-2026-06-22-recurrence) | `GET …/recovery/:id/diagnose-identity` (MCP `recovery_diagnose_identity`) — compares JWT vs stored vs QS-resolved tenant | **Re-login** with the account whose `TenantSlug(email)` equals the data tenant. **Does NOT auto-merge tenants** — moving data is out of scope. |
| Empty-read / `404 page not found` — [§8](./DIAGNOSING_PIPELINE_DATA_VISIBILITY.md#8-2026-06-22-round-2) | `empty_read_symptom_rate` signal on `…/fleet/health/:id` (MCP `tenant_health_assessment`) — **read-path/identity symptom, never data loss** | Fix at the layer it occupies (proxy target / edition / client unwrap), per the runbook's Definition of Done. The signal points you back into §8; it is not a restore trigger. |
| Retired-tier backfill across the cohort — [billing-cutover #5](./PRICING_BILLING_CUTOVER.md) | `recovery_batch` dry-run (affected list + count + `confirm_token`) | `POST …/recovery/batch` with `action: "reconcile-subscription"`, `filter` scoped to retired tiers; echo `confirm_token` + type the count. Per-tenant: `reconcile_subscription`. |
| Dunning / past-due drift — [billing-cutover §6](./PRICING_BILLING_CUTOVER.md) | dunning list | `resolve_dunning` — `POST …/recovery/:id/resolve-dunning` (Guarded; preview the action taken). |

---

## Deploy + fixture-removal checklist

The admin fleet pages currently render a **fixture** (a banner reading
`FIXTURE — endpoint unreachable`) when the Control Plane fleet endpoint does not answer.
That is intentional pre-deploy scaffolding so the layout can be verified. **Do these in
order, then remove the fixture:**

1. **Control Plane** (`apps/control-plane`) — ✅ **DONE.** Deployed to Fly `allsource-control-plane`
   **v68** (`fly deploy apps/control-plane --app allsource-control-plane`). `/api/v1/admin/fleet/*`
   + `/api/v1/admin/recovery/*` answer live (401 unauthenticated = wired + gated). Confirm a
   **new** Fly release on each redeploy (a `fly deploy` that creates no release is a no-op —
   data-visibility trap #4).
2. **MCP server** (`apps/mcp-server-elixir`) — **NOT a Fly deploy.** It is a **stdio MCP server**
   (no `fly.toml`), consumed by an MCP client, not a hosted HTTP service. To use it: set
   `ALLSOURCE_CONTROL_URL` + `ALLSOURCE_ADMIN_JWT` (so it reaches `/api/v1/admin/*`) and register
   it in your MCP client config. Set `ALLSOURCE_SYSTEM_ADMIN=true` **only** where `recovery_*`
   mutations should be allowed (off by default).
3. **Admin app** (`apps/admin`) — ✅ **deployed** to Fly app `allsource-admin`, with
   `NEXT_PUBLIC_API_URL=https://api.all-source.xyz` and private Control Plane routing through
   `http://allsource-control-plane.internal:3901`. Cross-origin login requires: (a) custom domain
   **`admin.all-source.xyz`** on the Fly app so it shares `.all-source.xyz` with the API cookie;
   (b) **CORS** — `admin.all-source.xyz` in Control Plane `ALLOWED_FRONTEND_URLS`; and (c) **OAuth** —
   admin domain in Google/GitHub authorized redirect URIs. See
   [`CONTROL_PLANE_CORS.md`](./CONTROL_PLANE_CORS.md) and [`FLY_FRONTENDS.md`](./FLY_FRONTENDS.md).
4. **Verify live**, then **remove the fixture fallback** from `fleet/page.tsx` and
   `fleet/[id]/page.tsx` (the `FIXTURE_FLEET` / `fixtureTenantHealth` blocks + the
   `usingFixture` banners). Once the endpoints answer, the fixture is dead weight and must
   not ship to a screenshot that could be mistaken for live data.
