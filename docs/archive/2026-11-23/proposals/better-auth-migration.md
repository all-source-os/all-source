# Better Auth Migration — Using AllSource as Auth Backend

> **Status**: Proposal (revised 2026-03-24)
> **Replaces**: Previous proposal that assumed PostgreSQL + TypeScript better-auth
> **Key change**: Uses `better-auth` (Rust) + `better-auth-allsource` adapter — no Postgres needed

---

## What Already Exists

### `crates/better-auth-allsource` — COMPLETE (v0.14.4, published)

A Rust crate implementing all 10 `DatabaseAdapter` sub-traits for `better-auth-rs`:

| Trait | Operations | Status |
|-------|-----------|--------|
| UserOps | create, get (by id/email/username), update, delete, list/search | Done |
| SessionOps | create, get, update expiry, delete, cleanup expired | Done |
| AccountOps | create, get (by provider), update, delete | Done |
| VerificationOps | create, get, consume, delete, cleanup expired | Done |
| OrganizationOps | create, get (by id/slug), update, delete (cascading) | Done |
| MemberOps | create, get, update role, delete, list, count | Done |
| InvitationOps | create, get, update status, list | Done |
| TwoFactorOps | create, get, update backup codes, delete | Done |
| ApiKeyOps | create, get (by id/hash), update, delete, cleanup expired | Done |
| PasskeyOps | create, get (by id/credential_id), update counter/name, delete | Done |

**Architecture**: HTTP-based, writes to Core (`/api/v1/events`), reads from Query Service (`/api/v1/events/query`). Event-sourced — every auth mutation is an immutable event with full audit trail.

**Event types**: `UserCreated`, `SessionCreated`, `AccountCreated`, etc. Entity IDs: `auth-user:{id}`, `auth-session:{token}`, etc.

### What does NOT exist yet

- No auth **server** running `better-auth` — the adapter exists but nothing uses it
- No auth **endpoints** exposed (login, register, OAuth callback, session, JWKS)
- No web app integration — frontend still uses Control Plane auth via proxy
- No JWT/JWKS integration with Query Service

---

## Revised Architecture

```
CURRENT (broken demo, fragile proxy chain):
  Browser → Next.js proxy → Query Service (no auth routes)
                           → Control Plane (Go, has auth but proxy routing is fragile)

PROPOSED:
  Browser → Next.js → Auth Service (Rust, better-auth + allsource adapter)
                              ↓
                         AllSource Core (events: auth-user:*, auth-session:*, etc.)
                              ↓
  Query Service validates JWT via JWKS ← Auth Service exposes /.well-known/jwks.json
```

The Auth Service is a new Rust binary (`apps/auth/`) that runs `better-auth` with the AllSource adapter. It handles all auth flows and stores everything as events in Core.

**Why a separate service instead of embedding in Core?**
- Core is the database — adding auth HTTP handlers to it violates SRP
- The auth service can scale independently
- Follows the existing monorepo isolation pattern (each app is standalone)

**Alternative: embed in Prime MCP server.**
Prime already has HTTP mode and connects to Core. Adding auth routes to `allsource-prime --mode http` could avoid a new service. Trade-off: coupling auth to the agent memory server.

---

## Implementation Phases

### Phase 1: Auth Service Binary — `apps/auth/` (NEW)

**Effort: ~4 hours** (the hard work is done in the adapter crate)

Create a new Rust binary that wires `better-auth` + `better-auth-allsource`:

```rust
use better_auth::{BetterAuth, Config};
use better_auth_allsource::AllsourceAuthAdapter;

let adapter = AllsourceAuthAdapter::new(
    &core_url,    // "http://allsource-core.internal:3900"
    &qs_url,      // "http://allsource-query.internal:3902"
    &api_key,
);

let auth = BetterAuth::builder()
    .database(adapter)
    .secret(&env::var("AUTH_SECRET")?)
    .base_url(&env::var("AUTH_BASE_URL")?)
    // OAuth providers
    .google_oauth(google_id, google_secret)
    .github_oauth(github_id, github_secret)
    // Plugins
    .jwt()        // Enable JWT + JWKS endpoint
    .email()      // Enable email/password
    .build()
    .await?;
```

**Endpoints exposed by better-auth**:
- `POST /api/auth/sign-up/email` — email registration
- `POST /api/auth/sign-in/email` — email login
- `POST /api/auth/sign-in/social` — OAuth initiate
- `GET /api/auth/callback/:provider` — OAuth callback
- `GET /api/auth/session` — get current session
- `POST /api/auth/sign-out` — logout
- `POST /api/auth/forget-password` — password reset request
- `POST /api/auth/reset-password` — password reset
- `POST /api/auth/verify-email` — email verification
- `GET /.well-known/jwks.json` — JWKS for JWT validation

**Deliverables**:
- [ ] `apps/auth/Cargo.toml` — depends on `better-auth`, `better-auth-allsource`, `axum`, `tokio`
- [ ] `apps/auth/src/main.rs` — CLI args (core-url, qs-url, port, secrets), starts axum server
- [ ] `apps/auth/Dockerfile` — standalone build
- [ ] `apps/auth/fly.toml` — Fly.io deployment config
- [ ] Excluded from root workspace (per monorepo rules)

### Phase 2: Web App Integration (~3 hours)

Replace the Control Plane auth proxy with direct calls to the Auth Service.

**Files to modify**:
- `apps/web/src/app/api/v1/auth/[...path]/route.ts` — proxy to Auth Service instead of Control Plane
- `apps/web/src/app/api/v1/demo/start/route.ts` — proxy to Auth Service
- `apps/web/src/app/(auth)/login/page.tsx` — use better-auth API paths
- `apps/web/src/app/(auth)/signup/page.tsx` — same
- `apps/web/src/app/(auth)/forgot-password/page.tsx` — same
- `apps/web/src/app/(auth)/reset-password/page.tsx` — same
- `apps/web/fly.toml` — change `CONTROL_PLANE_INTERNAL_URL` to `AUTH_SERVICE_URL`

**Key change**: The auth proxy in the web app just changes its upstream URL. The better-auth API paths are similar to what the frontend already uses.

**Demo login**: The Auth Service replaces the Control Plane's `DemoStartHandler`. The adapter creates a demo user event in Core — same effect, event-sourced.

### Phase 3: Query Service JWKS Validation (~4 hours)

Replace Guardian JWT validation with JWKS-based validation from the Auth Service.

**Files to modify**:
- `apps/query-service/mix.exs` — add `jose` dependency
- New plug: `BetterAuthJwt` — fetches JWKS, validates Bearer tokens
- `apps/query-service/lib/query_service_ex_web/router.ex` — replace Guardian pipeline
- Config: `AUTH_JWKS_URL` env var pointing to `http://auth.internal:3903/.well-known/jwks.json`

**JWKS flow**:
1. Auth Service exposes `/.well-known/jwks.json` (built into better-auth JWT plugin)
2. Query Service fetches and caches the JWKS (ETS, refresh hourly)
3. On each request: extract Bearer token → verify with cached JWKS → extract claims → assign user
4. On first auth: upsert user in QS from JWT claims (email, name)

### Phase 4: Data Migration (~2 hours)

Migrate existing users from Control Plane's Core-backed store to the Auth Service's event format.

**Current**: Control Plane stores users as events in Core with entity_id format specific to Go's auth implementation.
**Target**: Auth Service stores users as `auth-user:{id}` events.

**Migration script**:
1. Query Core for all existing user events from Control Plane
2. For each user, create equivalent `auth-user:{id}` events via the Auth Service
3. For OAuth users, create `auth-account:{id}` events linking provider + user
4. Verify: all existing users can log in via the new Auth Service

### Phase 5: Cleanup (~2 hours)

- Remove auth handlers from Control Plane (Go) — `DemoStartHandler`, `LoginHandler`, `RegisterHandler`
- Remove Ueberauth/Guardian from Query Service (if still present)
- Remove `CONTROL_PLANE_INTERNAL_URL` from web app fly.toml
- Add `AUTH_SERVICE_URL` to web app fly.toml
- Update `check-versions.sh` to include `apps/auth/`
- Update `set-version` in Makefile

### Phase 6: Deploy (~1 hour)

1. Deploy Auth Service to Fly.io: `fly apps create allsource-auth`
2. Set secrets: `AUTH_SECRET`, `GOOGLE_CLIENT_ID/SECRET`, `GITHUB_CLIENT_ID/SECRET`
3. Run data migration script
4. Update web app env vars: `AUTH_SERVICE_URL = http://allsource-auth.internal:3903`
5. Redeploy web app
6. Verify: email login, OAuth login, demo login, session, JWT validation in QS

---

## What Can Be Automated (by Claude)

| Phase | Task | Automatable? |
|-------|------|-------------|
| 1 | Auth Service scaffold + Cargo.toml | Yes |
| 1 | main.rs wiring better-auth + adapter | Yes |
| 1 | Dockerfile + fly.toml | Yes |
| 2 | Update web app proxy routes | Yes |
| 2 | Update frontend auth pages | Yes |
| 3 | Elixir JWKS plug | Yes (but needs testing) |
| 4 | Migration script | Yes |
| 5 | Cleanup | Yes |
| 6 | Fly.io deploy | Needs human (secrets, DNS) |

**Estimated total: ~16 hours of code work, most automatable.**

---

## Comparison: This Proposal vs Previous

| Aspect | Previous (TypeScript) | This (Rust + AllSource) |
|--------|----------------------|------------------------|
| Database | PostgreSQL (new infra) | AllSource Core (existing) |
| Auth framework | better-auth (TS, npm) | better-auth (Rust, crates.io) |
| Adapter | Prisma/Kysely | `better-auth-allsource` (DONE) |
| New service | None (embedded in Next.js) | `apps/auth/` Rust binary |
| Infrastructure | Need Postgres instance | None — uses existing Core |
| Data model | SQL tables | Event-sourced (immutable, auditable) |
| Effort | ~30 hours | ~16 hours |
| Risk | High (Elixir JOSE integration) | Medium (adapter is proven) |

---

## Benefits of AllSource-Native Auth

1. **No new infrastructure** — auth events stored in the same Core that stores everything else
2. **Full audit trail** — every login, session, password change is an immutable event
3. **Time-travel** — "who was logged in on March 15th?" is a Core query
4. **Dogfooding** — AllSource's own auth proves the event store works for operational data
5. **Single backup** — Core's WAL + Parquet backs up auth alongside business events

---

## Decision Needed

1. **New service (`apps/auth/`)** vs **embed in Prime MCP HTTP mode** — separate service is cleaner but adds one more Fly.io app
2. **Migration timing** — do this before or after the v0.17.0 release?
3. **OAuth providers** — Google + GitHub confirmed, add any others?
