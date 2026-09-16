# Regional Independence — proposal

**Status:** NOT PLANNED as of 2026-09-16 — Steps 3–5 closed won't-do (#288). Steps 1
and 2 shipped in de2e991 and stay: the tenant entity carries `HomeRegion` and the
delegation client routes by it, so setting `CORE_SERVICE_URL_<REGION>` activates
regional routing whenever a second region is actually wanted. What was dropped is
standing up and paying for that region. Reopen this only against a customer
requirement for in-region reads or an iad-outage tolerance, not as an
architecture goal.

**Original status:** draft, 2026-04-27
**Trigger:** AllSource Core runs single-region (iad). For non-US tenants every read pays trans-Atlantic latency, and any iad outage takes the whole platform down. Sustainable Data Strategy (Steps 1–6, shipped) bounds memory and disk growth on a single node; this proposal addresses the *availability* and *locality* axes that single-node work didn't touch.

## Implementation status

| Item | Status |
|---|---|
| Single-region leader-follower replication (WAL shipping) | ✅ Shipped, see `apps/core/src/infrastructure/replication/` |
| CRDT appendix (design note for multi-region merge) | ✅ Captured in archived `CORE_REPLICATION_DESIGN.md` Appendix A |
| Step 1 — Tenant `home_region` attribute | ⬜ Not started |
| Step 2 — Routing rule in Control Plane | ⬜ Not started |
| Step 3 — Cross-region read replicas | ⬜ Not started |
| Step 4 — Failover policy + runbook | ⬜ Not started |
| Step 5 — Per-tenant `replication_mode` opt-in (CRDT) | ⬜ Not started |

## The shift in mental model

Today (single-region):
- One Core leader in iad, optional in-region followers.
- Every client connects to the same gateway, every write is local to iad.
- A region outage = total platform outage.
- Cross-Atlantic latency = the floor for non-US tenant performance.

Target (tenant-pinned active-active):
- Each tenant has a **home region** declared at create time. Writes for that tenant are routed to its home; reads served from the *nearest* region with a replica.
- Most regions hold async-replicated read-only copies of every tenant.
- A region outage takes only the tenants homed there into write-unavailable mode; everyone else keeps reading from local replicas, and tenants homed elsewhere keep reading and writing.
- For tenants that explicitly opt into CRDT mode, writes are accepted in *every* region with async merge — full active-active at the cost of relaxed per-entity OCC.

The default keeps strict single-writer semantics (the per-entity `version` counter still works); CRDT mode is opt-in for workloads where availability matters more than serializability.

## Approaches considered

### 1. Tenant-pinned active-active (recommended default)

Each tenant has one home region; reads served everywhere via async replication.

| | |
|---|---|
| Write path | gateway → home-region Core (one cross-region hop if client isn't in the home region) |
| Read path | gateway → nearest replica (always in-region) |
| Consistency | Strong per-entity OCC preserved (single writer per tenant) |
| Failure mode | Home-region down → that tenant's writes fail until failover; reads still work from any replica |
| Fit | Most workloads. Anything that uses `expected_version` for OCC. |

The current leader-follower replication is already this pattern, just collapsed to one region. Lifting it to multi-region is mostly **a routing change in Control Plane** plus running follower Core instances in each region; the WAL shipper code needs no changes.

### 2. CRDT active-active (per-tenant opt-in)

Every region accepts writes; events merge async via the G-Set + UUID dedup pattern in the CRDT appendix.

| | |
|---|---|
| Write path | gateway → local Core (always in-region) |
| Read path | gateway → local Core |
| Consistency | Eventual; per-entity `version` becomes ambiguous; conflicts resolved by HLC + last-writer-wins |
| Failure mode | Region down → other regions keep accepting writes for *their* clients; merge resumes when the partition heals |
| Fit | High-volume telemetry, activity logs, anything where availability >> strict ordering |

Inappropriate as a global default: most code today reads `version` and acts on it; flipping that semantic globally would silently change behavior. Right shape: a per-tenant `replication_mode` flag that defaults to `pinned` and switches a tenant into CRDT mode after it's been audited.

### 3. Sharded by tenant (no replication)

Each region runs Core for *its* tenants only; no cross-region copies.

Rejected as primary design: zero DR benefit. Useful as a building block (the routing layer needs to know "tenant X lives in region Y") but not a complete answer.

### 4. Multi-master Raft consensus

Spanner-style strict serializability across regions.

Rejected: write latency = quorum-across-regions = hundreds of milliseconds; operational complexity is enormous; not justified by current or near-term workload requirements. Revisit only if a regulated workload (banking, healthcare) demands it and is willing to absorb the latency.

## Concrete steps, ordered for ROI

### Step 1 — Tenant `home_region` attribute

Add a `home_region: String` field to the tenant record (Control Plane's tenant repository). Default for existing tenants: `"iad"`. New tenants: pick at create time, default to a region inferred from the creator's IP / country.

**Acceptance criteria:**
- Migration backfills `home_region = "iad"` for every existing tenant.
- Tenant create API accepts an optional `home_region` parameter, validates it against an allowlist.
- The attribute is exposed via the existing tenant API and through the dashboard.

**Why first:** every later step branches on this. Without a home region, "route writes to home" has no anchor.

### Step 2 — Routing rule in Control Plane

The Control Plane today proxies every request to the iad Core. Update the proxy to:

1. Resolve the tenant from the request (already done for auth).
2. If the request is a write (POST/PUT/PATCH/DELETE on an event endpoint), forward to `core.<home_region>.allsource.internal`.
3. If the request is a read (GET), forward to `core.<nearest_region>.allsource.internal`.

The `<nearest_region>` resolution is initially trivial: the same region the Control Plane instance is running in (assuming one CP per region). A geo-IP fallback can land later.

**Acceptance criteria:**
- A write for a tenant homed in `iad` from a Control Plane instance in `lhr` lands on the iad Core, not the lhr replica.
- A read for any tenant from a Control Plane instance in `lhr` lands on the lhr replica (cross-region read latency = 0).
- Latency budget: the routing decision adds <1 ms to existing request handling.

### Step 3 — Cross-region read replicas

Run additional Core *follower* instances in target regions (e.g. lhr, ord). Each follower subscribes to the iad leader's WAL via the existing replication stack; cross-region network is the only new variable.

**Acceptance criteria:**
- One follower in lhr, one in iad (existing); both reach steady-state replication lag <2 s under normal traffic.
- A read query against the lhr follower returns the same events as the iad leader within `lag` seconds.
- Smoke test: `curl https://api-eu.all-source.xyz/api/v1/events/query` returns local-region latency.

### Step 4 — Failover policy + runbook

Define what "iad is down" means operationally:

- **Detection:** existing Fly health checks; supplement with a sentinel that flags >5 min of write failures.
- **Action:** promote a follower to leader. Existing replication stack supports this (see `wal_shipper.rs` follower → leader promotion path).
- **Recovery:** when iad comes back, the new leader re-syncs to the old; manual decision on whether to fail back.
- **Runbook lives in `docs/runbooks/REGIONAL_FAILOVER.md`** (to be written as part of this step, not before).

This is the step most likely to surface "things we hadn't thought of." Worth landing as a tabletop exercise on staging before any production cutover.

### Step 5 — Per-tenant `replication_mode` opt-in

Add `replication_mode: Pinned | Crdt` to the tenant record (defaults to `Pinned`). When `Crdt`:

- Writes are accepted at any regional Core, not just the home.
- The home region's WAL shipper merges incoming WAL streams from peer regions (G-Set union; UUID dedup; HLC sort).
- Per-entity `version` is no longer monotonic — the tenant has agreed to relaxed semantics in exchange for AP-bias availability.

This step builds on the foundation in Steps 1–4 but is **decoupled in time**. Ship it when there's a real customer asking for it; the design space (HLC vs vector clock, projection rebuild cost, conflict semantics) needs more thought than this proposal contains.

## Sequencing

Steps 1 and 2 are tightly coupled: the routing rule needs the home_region attribute to function. Land them in one PR if practical, otherwise back-to-back.

Step 3 can land in parallel with Steps 1–2 (no code dependency — it's a Fly + secrets exercise).

Step 4 must come *after* Step 3 (no failover without followers).

Step 5 is a separate track. Don't block on it.

A reasonable cadence: Steps 1–3 over ~1 week, Step 4 over ~3 days (mostly the runbook + chaos exercise), Step 5 deferred.

## Decisions implied by this proposal

| Decision | Rationale |
|---|---|
| Tenant-pinned, not CRDT-by-default | Most workloads use OCC; flipping semantics globally would silently break them. |
| Routing in Control Plane, not in Core | Core stays a generic event store; routing is a deployment-topology concern that belongs at the gateway. |
| Async replication (existing WAL shipper), not sync | Sync replication across regions adds 100s of ms of write latency. Async lag <2 s is acceptable for the workloads we have. |
| Failover is a manual decision, not automatic | Automatic failover during a transient network blip causes split-brain. A 5-min sentinel + human go-ahead is the safer default until we have more operational data. |
| No PostgreSQL for replication coordination | Same as the original replication design — Core's WAL is the source of truth for events; metadata coordination uses the existing Control Plane stack. |

## Open questions

- **Geo-routing for reads:** Step 2's "nearest region" heuristic uses the Control Plane instance's region. Real geo-IP routing (CDN-style) is a Step 3.5 nice-to-have; do we need it before customers complain?
- **Tenant migration between regions:** if a customer wants to change their home region (compliance, latency), what's the cost? Probably "stop writes, drain WAL, rsync Parquet, switch attribute, resume writes" — but the runbook for this is unwritten.
- **CRDT projections:** the appendix notes that projection rebuild after merge is the hard part. Step 5 needs to either solve this or limit CRDT mode to tenants that don't run projections.
