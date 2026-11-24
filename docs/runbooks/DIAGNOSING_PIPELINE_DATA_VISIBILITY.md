# Runbook: "Data is synced but I can't see it" — diagnosing pipeline visibility

**Use when:** an agent/client writes data that is confirmed stored (events ingested, tool calls returned IDs), but a downstream surface (dashboard tab, API, report) shows nothing. The classic shape: *"Prime says it saved 24 nodes, the Memory tab says 'No memory yet'."*

This runbook exists because a single instance of this class took far longer than it should have — the data was fine the whole time; **five independent things in the read/identity path were each capable of producing an empty screen.** The goal here is to reach the real cause in minutes, not hours, and to avoid the specific traps that cost time.

---

## 1. The mental model: every hop re-resolves identity, tenant, and store

A write and a read are **not** symmetric. Draw the actual path and mark, at every hop, three things that can independently differ between the write side and the read side:

```
 writer ──> gateway A ──> Core (store)  <── gateway B <── frontend
   │            │            │                  │            │
 which       which        which              which        which
 identity?   tenant?      partition?         tenant?      identity?
```

The empty screen is almost never "the data is gone." It is almost always **the read path resolves a different (identity → tenant → partition) tuple than the write path.** Enumerate the path first; debug second. In this monorepo the canonical path is:

```
allsource-prime (laptop)
  --sync-to--> control-plane (api.all-source.xyz)  --ingest--> Core (allsource-core)
                                                                  ^
  dashboard --> web /api/events --> Query Service (allsource-query) --read--+
```

Two different gateways (**control-plane** writes, **Query Service** reads) touch the same Core. Anything that makes them resolve a different tenant makes the data invisible without any data loss.

---

## 2. Fastest-path diagnostic ladder (cheap → expensive)

Run these **in order** and stop at the first contradiction. Each step is one command; do not skip ahead to code reading until the cheap probes have localized the hop.

### Step 0 — Trust the empty-state copy, then distrust it
If the surface has a good empty state ("No memory is reaching **this tenant**"), it is telling you the read tenant has zero rows — *not* that no data exists. Treat "this tenant has nothing" and "nothing was written" as **different hypotheses** from the first second.

### Step 1 — Confirm the data exists at the store, via the WRITE path's gateway
```bash
curl -s "$WRITE_GATEWAY/api/v1/events/query?event_type_prefix=prime.&limit=1&order=desc" \
  -H "Authorization: Bearer $KEY" | python3 -m json.tool
```
Look at `total_count` and the **`tenant_id` field on a returned event**. This tells you (a) the data is durable and (b) the exact tenant string it is stored under. Write that string down — it is the source of truth for the rest of the investigation.

### Step 2 — Query the READ path's gateway with the SAME key
```bash
curl -s "$READ_GATEWAY/api/events?event_type_prefix=prime.&limit=1" \
  -H "Authorization: Bearer $KEY"
```
If write-gateway = N and read-gateway = 0 with the **same key**, the divergence is in the read gateway's identity/tenant resolution or in which store it reads. Proceed.

### Step 3 — Write a probe through each gateway, read it back through ALL gateways
This is the single most decisive test. It removes staleness, caching, and lazy-load as variables (the probe is brand new) and isolates the hop:
```bash
PROBE="node:probe:$(openssl rand -hex 3)"   # unique id; do NOT use Math.random in scripts that resume
# write via gateway A
curl -s -XPOST "$GW_A/api/v1/events" -H "Authorization: Bearer $KEY" \
  -H 'Content-Type: application/json' \
  -d "{\"event_type\":\"prime.node.created\",\"entity_id\":\"$PROBE\",\"payload\":{}}"
# read it back via A and via B
curl -s "$GW_A/.../query?entity_id=$PROBE" -H "Authorization: Bearer $KEY"   # expect 1
curl -s "$GW_B/.../events?entity_id=$PROBE" -H "Authorization: Bearer $KEY"  # 0 ⇒ A and B disagree
```
**Then do the reverse** (write via B, read via A and B). If each gateway sees only its own writes, you have narrowed it to "different tenant **or** different store" — but you have **not** yet distinguished them. See the trap in §3.

### Step 4 — Read the STORED `tenant_id` on a probe each gateway wrote
The discriminator that §3's symmetric result cannot give you:
```bash
curl -s "$GW_B/api/events?entity_id=$PROBE_WRITTEN_VIA_B" -H "Authorization: Bearer $KEY" \
  | python3 -c "import json,sys; e=json.load(sys.stdin)['data'][0]; print('stored tenant_id:', e['tenant_id'])"
```
- If gateway B stamped a **different tenant string** than Step 1's truth (e.g. `community` vs `acme-at-gmail-com`) → **same store, wrong tenant resolution in B.** Go to the code path that resolves tenant in B (§5).
- If both stamped the **same** tenant yet still can't see each other → **genuinely different stores.** Compare each service's Core URL (`env | grep -i core`, fly secrets, `/status/services` probe URLs).

### Step 5 — Only now read code, and read the *deployed* config, not just the source
Resolve the tenant assignment in the offending gateway top-down: auth pipeline → tenant-context plug → controller. Critically, check **runtime configuration / edition flags**, not only the happy-path code:
```bash
fly ssh console -a <app> -C "/bin/sh -lc 'env | grep -iE \"edition|tenant|core\"'"
```
A single env var (an "edition", a "mode", a "default tenant") can route every request down a branch that ignores the authenticated identity entirely.

---

## 3. Traps that cost time on this class of bug (read before you theorize)

1. **Declaring success off the wrong read path.** Verifying via the *write* gateway and calling it fixed is the cardinal error. Always verify through the **exact** path the user's surface uses (same gateway, ideally same session/cookie, not just an API key). If the user can see the screen and you're hitting a different endpoint, you have not verified anything.
2. **A symmetric "each sees only its own writes" result does NOT prove two stores.** It is equally explained by one store with two different tenant scopes. You must read the **stored `tenant_id`** (§4) to tell them apart. Two of this session's hours died in this ambiguity.
3. **Subagent / hypothesis hand-off without re-reading the data shape.** An investigator concluded "the DTO `id` is a random UUID" from source; the *actual* deployed endpoint returned `id = <slug>`. Verify a hypothesis against a real response body before acting on it, especially for serialization shape.
4. **Assuming a deploy shipped because the command exited 0.** A `fly deploy` reported success but created **no new release** (cache/no-op); the old code kept running and the symptom persisted. Confirm a *new release/version number* and, when in doubt, redeploy with `--no-cache`. Then re-test.
5. **Auth-skipped data planes resolve tenant from the request, not the token.** Core's `/api/v1/events*` is in `AUTH_SKIP_PREFIXES`: the Bearer token is ignored and the `?tenant_id=` **query param wins**. So "who the key is" matters only insofar as the *gateway* turns it into the right param. Don't assume the token scopes the query.
6. **Identity can be correct on one endpoint and wrong on another.** `/api/auth/me` returned the right tenant (it reads the JWT claim) while `/api/events` used the wrong one (it ran through a tenant-context plug with a different branch). Never generalize tenant resolution from one endpoint to the whole service.
7. **MCP launchers pass args verbatim.** `~`/`$HOME`/`${HOME}` in a `--data-dir` are **not** shell-expanded by Claude Desktop/DXT; a config value can send the process to a junk path. Inspect the *running* process args (`ps`), not the config file's intent.

---

## 4. Command cookbook (this stack)

```bash
# The key a local MCP/DXT is actually using (decoded, redacted), incl. its tenant claim:
PID=$(pgrep -f "allsource-prime" | head -1)
KEY=$(ps -o command= -p "$PID" | grep -oE -- '--api-key [A-Za-z0-9._-]+' | awk '{print $2}')
python3 -c "import json,base64,sys; p='$KEY'.split('.')[1]; p+='='*(-len(p)%4); print(json.loads(base64.urlsafe_b64decode(p)))"

# Where prime actually writes on disk + whether sync ran (cursor exists only after a successful push):
ls -la ~/.prime/memory; cat ~/.prime/memory/.prime_sync_cursor.json 2>/dev/null

# Which Core each service points at (run per app):
fly ssh console -a allsource-query        -C "/bin/sh -lc 'env | grep -i core'"
curl -s https://api.all-source.xyz/api/v1/status/services        # control-plane's view of backends
curl -s https://allsource-query.fly.dev/health                   # QS version + core node health

# Confirm a deploy actually shipped:
fly releases -a <app> | head -3                                  # a NEW version row must appear
```

Note: the control-plane container is distroless — `fly ssh -C` with `printenv`/`sh` fails. Use `/status/services` and `/health` to infer its backend instead.

---

## 5. Worked example — the five layered causes behind one empty Memory tab

All five were real; the first four were fixed and the screen was *still* empty, which is why the discipline above matters.

| # | Layer | Symptom it produced | Real fix |
|---|-------|---------------------|----------|
| 1 | UI minted API keys with role `service_account` (underscore) vs the canonical `serviceaccount` | every key 403'd on read+write at the gateway | `api_key_controller.ex` mint the canonical string |
| 2 | DXT `--data-dir ${HOME}/...` not expanded; cwd `/` | writes never persisted | prime expands `~`/`$HOME`/`${HOME}` (`0.21.6`) |
| 3 | Dashboard fetched latest-200 then client-filtered `prime.*` | prime events drowned by other types | server-side `event_type_prefix` filter |
| 4 | **Red herring**: `tenant_context.tenant_id/1` read `id` not `tenant_id` | *looked* like the bug; was a no-op because the real tenant endpoint returns `id = <slug>` | harmless hardening only |
| 5 | `ALLSOURCE_EDITION=community` on the hosted Query Service | `TenantContext` hardcoded every request to the `community` tenant; prime synced under the real slug → dashboard read `community` → empty | set `ALLSOURCE_EDITION=enterprise` on `allsource-query` |
| 6 | **The actual final blocker — a frontend double-unwrap.** `apiClient.request()` unwraps `{data:X}→X`; the events body `{count, data:[...]}` became the bare array, then `useEvents` did `.data` again → `undefined` → `events=[]` | the API returned 71 rows the whole time; only the *rendered page* was empty — invisible to every API-level test | `useEvents` treats the value as the already-unwrapped array |

The fast path would have reached the truth in: Step 1 (data exists under `acme-at-gmail-com`), Step 3+4 (QS *writes* land under `community`), Step 5 (`env | grep edition` → `community`) — then, crucially, **load the rendered page with the user's real session** and watch it stay empty while the network tab shows 71 rows, which points straight at the client. Everything else was time spent reading through the wrong path, assuming a deploy shipped, and (twice) declaring victory from an API call instead of the screen. The single highest-leverage habit: **verify through the user's exact rendered path, with their session, every time — an API that returns rows proves nothing about what the component renders.**

> ### 🛠 Resolve with — Control Plane fleet/recovery tooling (catalog: [`FLEET_HEALTH_RECOVERY.md`](./FLEET_HEALTH_RECOVERY.md))
> Two of the §5 incidents now have a named diagnose/fix surface in the admin Control Plane (commits a02667e / e233bee / b6e3c88 — **shipped to `main` and deployed** to `allsource-control-plane` v87 on 2026-06-27; the fleet API first deployed at v68). These automate the *detection + the exact remediation command* — they never move data.
>
> - **Edition=community trap (row #5).** Fast path: `GET /api/v1/admin/recovery/diagnose/edition` (MCP: `recovery_diagnose_edition`). It detects the trap (QS edition `community` while ≥ 2 non-`community` tenants hold data) and returns the copy-paste fix + a post-change verification probe. The fix is still **operator-executed**, exactly as above: set `ALLSOURCE_EDITION=enterprise` on `allsource-query`, then confirm a **NEW** Fly release (trap #4 — a `fly deploy` that creates no release is a no-op). The endpoint changes nothing itself; it just hands you the command and the verify step.
> - **API-key role-string drift, `service_account` vs `serviceaccount` (row #1).** Read check: the `check_key_role` diagnosis asserts the canonical `serviceaccount` string on a representative key (surfaced as the `api_key_validity` signal on `GET /api/v1/admin/fleet/health/:id`). If drifted, the Guarded `rotate_keys` action — `POST /api/v1/admin/recovery/:id/rotate-keys` (MCP: `recovery_rotate_keys`) — re-mints with the canonical role. It is **Destructive** (current keys stop working): dry-run first, then echo the dry-run's `confirm_token`.

---

## 6. Definition of done

- The probe written through the **user's exact read path** comes back with a count > 0.
- The stored `tenant_id` on that probe matches the tenant the user is logged into.
- A **new** release/version is confirmed live (not assumed), and the symptom is re-tested against it.
- The fix is stated at the layer it actually lives (config/edition vs code), not the first plausible code path.

---

## 7. 2026-06-22 recurrence — the double-unwrap was never fully killed, plus a tenant/logout trap

Same "no data anywhere" report, three compounding causes — none was data loss (Core held **7,725 streams + 141 event-types + 500+ events** under `decebal-dobrica-at-gmail-com`, tier `studio`, the whole time):

1. **Wrong-tenant session.** The user's browser was pinned to an empty auto-provisioned tenant, not their data tenant. Proof that beat all guessing: `/api/tenant` returned a full record (`id`, tier, quota) for `decebal-dobrica-at-gmail-com` but `tenant?.id` rendered `—` in Settings → the session was elsewhere. `AuthPipeline` copies `tenant_id` **verbatim from the JWT claim** (`auth_pipeline.ex:108`); `TenantContext` then **silently auto-provisions an empty tenant + returns 200** when that id is unknown (`tenant_context.ex:141`) — a mismatch masquerades as "no data, no error." Ground truth came from prod logs: `fly logs -a allsource-query | grep UserSocket` printed `user_id` + `tenant_id` for the live session. Re-login with the Google account whose `TenantSlug(email)` equals the data tenant resolved it; no data was moved.
2. **Logout was a no-op**, so the user couldn't escape the wrong session. `auth-store.logout()` only cleared localStorage; the `auth_token` cookie survived → next `/api/auth/session` re-hydrated. Fixed: explicit expired Set-Cookie matching the callback's attributes (`session/route.ts` DELETE) + hard `window.location` reload + `localStorage.removeItem("auth-storage")` in the header.
3. **The double-unwrap from §5 #6 was only patched in `useEvents`.** Five more consumers still did `data?.data` on the already-unwrapped array and rendered empty **even on the correct tenant**: `useEventsByEntity`, `useStreams`, `useEventTypes`, `useReplays`, and `use-notification-preferences`. Hooks that did a single `response.data` (`pipelines/page.tsx`, `use-dashboard-stats`) were always fine. **Lesson: when you find a response-shape bug, grep the whole client for every consumer of that shape — `rg "data\?\.data"` — and fix them as a set, not the one that was reported.** Settings Tenant ID now falls back to `user?.tenant_id` (the JWT claim) so the user can always read their live tenant even when `/api/tenant` is empty.

> ### 🛠 Resolve with — tenant/JWT divergence diagnosis (catalog: [`FLEET_HEALTH_RECOVERY.md`](./FLEET_HEALTH_RECOVERY.md))
> Cause-1 (wrong-tenant session / JWT `tenant_id` mismatch) now has a **read-only** diagnosis: `GET /api/v1/admin/recovery/:id/diagnose-identity` (MCP: `recovery_diagnose_identity`). It compares the three tenants that must agree — (a) what the caller's JWT claims, (b) what the data is stored under in Core, (c) what the Query Service resolves — and reports the divergence plus the fix: **re-login** with the account whose `TenantSlug(email)` equals the data tenant. This replaces the manual `fly logs -a allsource-query | grep UserSocket` hunt with a single call. **It does NOT auto-merge tenants** — moving/merging tenant data is explicitly out of scope (the runbook fix is a re-login, never a data move; Core held the data correctly the whole time).

---

## 8. 2026-06-22 (round 2) — still blank after §7 shipped: the Next data proxy pointed at the wrong backend (404, not blank)

§7's three fixes were real and **confirmed live** (see "how the build was verified" below) — yet the dashboard was *still* empty. The remaining cause was a layer nobody had probed: the **Next.js catch-all data proxy targeted the Control Plane, which does not serve the read API**, so every data call 404'd before any component or hook ran. The hook fixes couldn't matter — the data never arrived.

**Root cause.** `apps/web/src/app/api/[...path]/route.ts` (the same-origin proxy for `/api/events`, `/api/streams`, `/api/event-types`, `/api/tenant`, `/api/auth/me`, `/api/billing/*`, …) resolved its backend from `NEXT_PUBLIC_API_URL`, which in Vercel prod = `https://api.all-source.xyz` — the **branded gateway / Control Plane**. The CP routes only a narrow slice of `/api/v1/*` (e.g. `/api/v1/events/query`, `/api/v1/prime/graph`) and serves **none** of the non-v1 dashboard surface. So the browser's `/api/events` etc. all returned a plain-text **`404 page not found`** (Go's default — a CP fingerprint; the Query Service's Phoenix 404 is a JSON `{"error":{"code":"not_found",…,"correlation_id":…}}`). The proxy faithfully relayed the 404 → SWR `error` → `events=[]` → "No events yet" / "No memory is reaching this tenant" on every view.

This was already a *known* hazard for auth: `api/auth/session/route.ts` and `api/auth/callback/route.ts` had been special-cased months earlier (their header comments spell it out: *"NEXT_PUBLIC_API_URL points at the branded gateway … which routes /api/v1/events to the QS but NOT /api/auth/*"*) to resolve `QUERY_SERVICE_URL || allsource-query.fly.dev` directly. The **data** proxies (`api/[...path]` and `api/v1/[...path]`) were never given the same treatment.

**How confirmed (the decisive probes).** Send the user's exact session — recover the Debug service-account JWT (tenant `decebal-dobrica-at-gmail-com`) and replay the browser's own transport:
```bash
# the dashboard's EXACT proxied path — same-origin, auth_token cookie
curl -s --cookie "auth_token=$JWT" 'https://www.all-source.xyz/api/events?limit=3'   # → 404 page not found
curl -s --cookie "auth_token=$JWT" 'https://www.all-source.xyz/api/streams?limit=3'  # → 404 page not found
# now hit each backend DIRECTLY with the same JWT to localize the 404:
curl -s -H "Authorization: Bearer $JWT" 'https://api.all-source.xyz/api/events?limit=3'        # → 404 page not found (CP)
curl -s -H "Authorization: Bearer $JWT" 'https://allsource-query.fly.dev/api/events?limit=3'   # → 200, count=3, rows (QS)
curl -s -H "Authorization: Bearer $JWT" 'https://allsource-query.fly.dev/api/streams?limit=3'  # → 200, total=7731 (QS)
```
The proxy's 404 body was **byte-identical to the Control Plane's** and different from the QS's — proving the proxy forwarded to `api.all-source.xyz`. The QS, hit directly, returned the real data (streams total **7,731**, event-types total **144**). `total=7731 vs 7725` because data kept growing — the data was there the whole time.

**Verifying the §7 build actually shipped, with Vercel API access blocked** (CLI authed to a personal scope, not the team that owns the prod project). Detect freshness client-side instead: the `/dashboard` route 307-redirects to `/login` server-side (`src/proxy.ts` middleware, cookie-gated only), so fetch it **with the JWT as the `auth_token` cookie** to get the real, authenticated HTML + its content-hashed chunk list, then grep the live JS for the post-fix fingerprints:
```bash
curl -s --cookie "auth_token=$JWT" 'https://www.all-source.xyz/dashboard/settings' -o s.html   # 200, age:0
# download every /_next/static/chunks/*.js it references, then:
grep -l 'removeItem("auth-storage")' chunks/*.js   # FIX 1 (header logout) present
grep -o 'A?.id||e?.tenant_id||"—"' chunks/*.js      # FIX 2 (settings tenant fallback) present
grep -o 'Array.isArray(h)?h:h?.data??\[\]' chunks/*.js  # FIX 3 (useEvents unwrap) present
```
All three were in the deployed bundle → §7 shipped; the blocker was elsewhere. **Trap avoided:** "still blank after a fix" is *not* automatically "the deploy failed" — prove freshness by fingerprinting the live chunk, not by assuming.

**Fix.** Point the non-v1 data proxy at the Query Service the same way the auth routes do — `QUERY_SERVICE_URL || (NODE_ENV==="production" ? "https://allsource-query.fly.dev" : "http://localhost:3902")` — in `api/[...path]/route.ts`. This is durable (no dependency on a Vercel env var aiming at the right host) and ships through git, no Vercel access needed. **Left `api/v1/[...path]` on the branded gateway on purpose**: the CP correctly serves the only two `/api/v1/*` calls the client makes (`/api/v1/prime/graph`, `/api/v1/agents/claim` trial flow); repointing it would break the trial/agent path. Verified the CP serves **zero** non-v1 routes (including billing), so moving all of non-v1 to the QS loses nothing and fixes everything.

**Lesson (add to §3):** before blaming the component layer for "rows but blank," confirm the proxy is even reaching a backend that *serves the route*. A same-origin `/api/*` proxy can 404 silently because it forwards to the wrong service — the network tab shows `404`, not rows, and that single status distinguishes "wrong backend" (this round) from "right backend, component drops the rows" (§7 #3). Two gateways in this stack (`api.all-source.xyz` = Control Plane, `allsource-query.fly.dev` = Query Service) have **disjoint** path surfaces; the dashboard read API lives **only** on the Query Service.

> ### 🛠 Resolve with — empty-read / 404 symptom signal (catalog: [`FLEET_HEALTH_RECOVERY.md`](./FLEET_HEALTH_RECOVERY.md))
> This empty-read/404 class is surfaced fleet-wide as the **`empty_read_symptom_rate`** signal on `GET /api/v1/admin/fleet/health/:id` (MCP: `tenant_health_assessment`): the rate of authenticated reads returning 0 rows or `404 page not found` for a tenant that **demonstrably has Core data**. It is deliberately framed as a **read-path / identity symptom class, never data loss** — Core IS the durable database (WAL + Parquet + DashMap), so a non-zero rate points you at exactly the layers this section debugs (wrong-backend proxy, wrong-tenant resolution, double-unwrap), not at the store. The signal is a *pointer back into this runbook*; the fix still lives at the layer it actually occupies (proxy target, edition, or client unwrap), per the Definition of Done.
