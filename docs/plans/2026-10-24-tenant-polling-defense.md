---
title: "Tenant polling defence and incremental read paths"
status: CURRENT
last_updated: 2026-10-24
---

# Tenant polling defence and incremental read paths

Root cause: [control-plane egress RCA](../reports/2026-10-24-control-plane-egress-rca.md).

The Longhand control plane polls `/api/v1/events/query` at about 35 requests a
second and moves 66 GB a day. Since 15:40Z on 2026-10-24 the control plane's
global 16-slot egress guard throttles it, so Longhand's views go stale while
AllSource is still not protected per tenant. Both ends need fixing:

- **AllSource refuses excessive polling per tenant**, cheaply and fairly, with
  refusals a client can act on.
- **Longhand gets paths that make polling unnecessary**: conditional reads,
  change detection and push, shared projections, and caches on both sides.

This plan builds on #340, #342, #345 and #346 (bounded retained reads, shared
tenant read models, stable feed positions, bounded prefix reads). It does not
replace them.

## Constraints found while designing

| Constraint | Evidence | Consequence |
|---|---|---|
| The AllSource control plane runs on 256 MB and one shared CPU, and the guard buffers each response up to 16 MiB | `apps/control-plane/fly.toml` `[[vm]]`; `egress_guard.go:21` | Raising the 16 global slots risks out-of-memory. Slots change only together with `memory_mb` |
| Longhand is one AllSource tenant carrying every Longhand org | RCA, tenant fingerprint `2732b1a7938f` | Any per-tenant default below Longhand's current rate needs an explicit override, or the 15:40Z outage repeats |
| Core skips rate limiting on `/api/v1/events*`, and the existing limiter requires an `AuthContext` those routes never get | `middleware.rs:41-57,443,453-456` | A Core backstop must be its own layer keyed by the trusted tenant, not the removal of a skip |
| The control plane forwards only `Content-Type` from Core and runs the meter only on 2xx | `delegation.go:238-244,262-266` | ETag, `Retry-After` and 304 need an explicit allowlist and an explicit metering rule |
| The guard holds a 10-second request deadline | `egress_guard.go:107-109` | Long-poll waits must stay at or under 8 seconds on guarded routes |
| `better-auth-allsource` does not trust `order=desc` and reads up to 1000 session events | `better-auth-allsource/src/client.rs:106,117-133,204` | Bound the session window but keep `newest_event` selection and an overflow fallback; never a bare `limit=1` |
| Longhand caches session lookups for 15 seconds and caches no authorisation input | `auth_cache.rs:30,119` | Authorisation caches need event-driven invalidation, with zero stale grace on authorisation decisions |

## Architecture: the data path is a gateway

Today every client read passes through the control plane, which per request
authenticates and mints a JWT, takes one of 16 global slots, makes a
synchronous ledger reserve call to Core (one durable event), proxies the query,
buffers the full response (up to 16 MiB), makes a ledger complete call (a second
durable event), and then writes the body. It does this on a 256 MB VM with a
6.25% CPU baseline. The [production traffic report](../reports/2026-10-24-production-traffic.html)
measures the cost: 81.8% of writes refused by the global slots, 17.5 ledger
events per second into Core's never-checkpointed system log, CPU throttled
35–62% on 2026-10-21, and no way to stream or long-poll because responses are
buffered.

The client data path becomes a gateway. The control plane leaves it.

| Gateway: public, `/api/v1/*` data routes | Control plane: off the data path |
|---|---|
| Stateless and horizontally scalable; streams responses, never buffers them | Admin, billing, onboarding, OAuth, dashboards |
| Verifies API keys from a local cache refreshed in the background; no per-request Core call | Owns tenants, plans and prepaid credit |
| Per-tenant token buckets in memory; refusals carry a computed `Retry-After` | Settles usage asynchronously from the gateway's aggregates |
| Counts request and response bytes per tenant in memory; flushes one aggregate per tenant per minute | Issues budget leases |
| Enforces prepaid hard stops from a budget lease held locally and renewed in bulk, never per request | |
| Long-poll and push become possible because nothing is buffered | |

Both roles come from the existing Go codebase in two modes (`data` and `admin`),
deployed as two Fly apps. `api.all-source.xyz` moves to the gateway, which
forwards non-data routes to the control plane over the private network. The
per-response ledger in `egress_guard.go` is removed; its billing guarantee is
kept by the lease: a tenant can overrun its prepaid balance by at most one
lease, and the lease size is the knob.

## Layers

| Layer | Side | Mechanism |
|---|---|---|
| Per-tenant admission | AllSource control plane | Token buckets keyed by the authenticated tenant, checked before the ledger reserve so a refusal costs no Core round trip. A data lane and a cheaper metadata lane (registered metadata route templates, and reads the server answered 304), a byte bucket, and an in-flight cap. A fair-share rule inside the global 16 slots. In enforce mode, per-tenant refusals carry a computed `Retry-After` and a `scope` of `tenant` or `global_fair_share`, and responses carry `RateLimit-Policy` and `RateLimit`. Global-pool and ledger refusals keep their status and `Retry-After: 30` in every mode |
| Header passthrough and conditional reads | AllSource control plane | Forward `If-None-Match` and `Accept-Encoding` to Core. Pass back `ETag`, `Retry-After` and `RateLimit-*`. Meter 304s explicitly as a conditional count with 0 bytes |
| Compression and ledger lease | AllSource control plane | gzip after buffering, charged at compressed length. A per-tenant byte lease replaces the per-response reserve and complete calls |
| Change detection | AllSource Core | Per-tenant prefix heads to depth 3 and bounded per-tenant entity heads. `GET /event-feed/heads`. A pre-scan ETag on `/events/query` with a 304 when nothing under the query's prefix changed. Epoch bump on rewrites (tombstones, retention, repack) |
| Push | AllSource Core | `/event-feed?wait=<=8` long-poll on positions from #345 |
| Core backstop | AllSource Core | A dedicated per-tenant limiter at twice the control plane's numbers, and a per-tenant in-flight cap in the response and read-model pools |
| Client hygiene | `allsource` SDK, `better-auth-allsource` | A `RateLimited` error, `Retry-After` honoured with a not-before fast fail, a breaker that ignores 429, a single half-open probe, gzip, `if_none_match`. Bounded session reads with overflow fallback |
| Stop amplifying | Longhand control plane | A governor over every AllSource transport that pauses globally on 429, with priority classes auth > user action > board > background. A Redis leader lease for `schedule_tick`, `queue_lifeline` and `hosted_scheduler`. Queue bindings and teams read once per pass, not once per tenant. A request-scoped RBAC memo |
| Shared projection and revisions | Longhand control plane | A `longhand-directory-v1` read model (membership, bindings, teams, skills) on #342. Revisions published to Redis after each checkpoint. An RBAC memo over it that bypasses when the checkpoint is older than 30 seconds. Session sign-out invalidation over Redis, then a longer session TTL |
| Client protocol | Longhand control plane → desktop | `ETag` and `X-Poll-Interval` on every poll endpoint, a `/me/changes` long-poll doorbell, and per-user poll and action buckets (shadow first) |
| Desktop | Longhand desktop | Honour 429 and `X-Poll-Interval`, persist ETags, pull only on a doorbell, and fall back to today's cadences when the listener is down |

The durable control-plane store stays unbuilt. It is the most expensive option
and the least verified. It is reconsidered only if, after the phases below,
repeat reads remain that a validated in-memory cache cannot cover.

## Phases

Each phase ships on its own. Per-tenant enforcement never lands before Longhand
has a path that stays under it.

| Phase | Repo | Changes | Confidence |
|---|---|---|---|
| P1 | Longhand | Governor with `Retry-After` pause and priorities; Redis leader lease for the three background loops; bindings and teams hoisted to once per pass; request-scoped RBAC memo; serve-stale for non-authorisation reads with `X-Longhand-Stale` | rock |
| P2 | AllSource SDK and `better-auth-allsource`, then a Longhand bump | SDK 0.25 rate-limit handling and gzip; bounded session read with overflow fallback | rock |
| P3 | AllSource control plane | Per-tenant admission in shadow; computed `Retry-After` and refusal scopes; `/api/v1/tenants/me/limits`; tenant and namespace labels on byte metrics; `[metrics]` scraping; header passthrough with explicit 304 metering | rock |
| P4 | AllSource control plane | gzip charged at compressed length; per-tenant ledger lease | sand |
| P5 | AllSource control plane | Enforce admission with a Longhand override set to its shadow p99 × 1.5; fair-share rule active | rock |
| P6 | AllSource Core | Prefix and entity heads, pre-scan ETag/304, epoch bump on rewrites, dedicated per-tenant limiter, `wait<=8` long-poll | sand |
| P7 | Longhand | Directory read model with parity shadow; Redis revisions; RBAC memo over it; session invalidation then 60-second TTL; skill catalog from the model | sand |
| P8 | Longhand | Board revisions and ETags; `/me/changes`; per-user buckets in shadow | sand |
| P9 | Longhand desktop | Rate-limit handling, persisted ETags, doorbell-driven pulls, batched lease renewals | rock |
| P10 | Both | Ratchet the Longhand override down after each landed phase; enforce per-user limits once most seats run the new desktop | sand |

## Acceptance

- **P1:** with three tenants, two machines and Redis, one machine ticks and
  bindings are read once per pass; with Redis down, exactly one deterministic
  leader ticks; a 429 with `Retry-After: 5` stops every transport for 5 seconds;
  one authorised request resolves membership once.
- **P2:** a 429 with `Retry-After: 3` makes exactly one attempt and fails fast for
  3 seconds without opening the breaker; the bounded session read returns the
  same session as the 1000-event read over recorded entities, including a
  tombstone as newest, and takes the overflow path when Core ignores ordering.
- **P3:** shadow mode changes no behaviour and populates would-refuse counters
  per tenant; a per-tenant refusal's `Retry-After` is never the constant 30; a Core 304 reaches the
  client with its `ETag` and is metered as 0 bytes.
- **P5:** a tenant at 200 requests a second is refused with `scope=tenant` and
  zero ledger calls, while a tenant at 5 requests a second gets 100% 2xx; the
  Longhand tenant sees about zero refusals at its override; enabling enforcement
  without the override map loaded fails a test.
- **P6:** another tenant's ETag never produces a 304; an append under an
  unrelated prefix gives a 304 and a matching append a 200; a tombstone,
  retention rewrite or restart gives a 200; the 304 path performs zero scans.
- **P7–P9:** an unchanged revision costs zero AllSource requests; a membership
  revocation on one Longhand machine is enforced on another within the
  projector and pub/sub bound; a claim on one paired desktop renders on another
  within the doorbell latency, through the real UI.
- **End state:** `allsource-control-plane` edge output under 5 GB a day; the
  Longhand tenant under 5 requests a second with about zero refusals under the
  default policy for 7 days; `X-Longhand-Stale` on under 1% of responses.

## Today

The global guard stays as the bound until P1 and P3 land. Raising its slots
risks out-of-memory on the 256 MB VM, and shortening `Retry-After` by hand
reopens the unbounded load. P1, P2 and P3 do not depend on each other and can
proceed together.

A longer schedule-tick interval is not part of this plan. The tick's binding
re-reads measure at 1.35–5.4 GB a day (32 live orgs × 29,269 B × 1,440 passes,
for one to four machines), and reading the bindings once per pass in P1 removes
31 of every 32 of them without delaying schedules. See the
[production traffic report](../reports/2026-10-24-production-traffic.html).

## Decisions for the owner

1. The override multiplier (shadow p99 × 1.5) and how fast it ratchets down.
2. Billing: compressed bytes, and 304s as zero bytes plus a conditional count.
3. Raising the AllSource control plane's `memory_mb`, which more global slots or
   any response cache requires.
4. Security sign-off on a 60-second session TTL once cross-machine sign-out
   invalidation is in place.
5. Whether Core's `/events*` routes are reachable only through the control
   plane's private network, which the Core limiter's trust in `tenant_id`
   depends on.
