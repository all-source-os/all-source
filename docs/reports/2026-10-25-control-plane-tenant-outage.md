# Control-plane tenant outage: what is fixed and what returns (2026-10-25)

**Verdict: not solved.** Two separate failures hit production on 2026-10-24/25.
The first is fixed at its cause. The second is mitigated: a raised limit gives
it about 16 days before it returns, and a second, slower limit stands behind it.
Both come from the same design property: some reads of a tenant load the
tenant's entire history, and this tenant's history grows about 14 MB a day.

All figures below were measured on production on 2026-10-25 unless marked
otherwise.

## 1. The two failures

| | Failure A: every read through the control plane | Failure B: Longhand control-plane tenant |
|---|---|---|
| Window | 2026-10-24 21:51Z to 2026-10-25 10:03Z | Hidden by A until 10:03Z; still occurring at 11:08Z |
| Symptom | Control-plane edge: 0 × 200, 501 × 503 in 10 minutes | Core 500 `Strict archive read budget exceeded: uncompressed bytes`, about one per second, all for tenant `onboard-control-plane-at-technical-leaders-com` |
| Cause | WAL recovery wrote the first post-restart append into the oldest segment; replay then applied it out of order and invalidated every egress account, so every reserve answered `Unconfigured` (503) | The event feed and read models load the tenant's whole history under a 128 MiB decoded-text limit; this tenant decodes to about 585 MB |
| Fix | #351: `recover_for_append` sorts by sequence and seals the writer past the last entry. Deployed with Core `sha-fe6be491` at 10:02Z | #355 (limit, entity-scoped read models, 503) and Longhand #1702 (back-off). Neither removes the whole-history load from the feed |
| Will it return? | No, from this cause. Boot logs show no `Invalid durable egress event`; the system WAL recovered 569,858 events, 0 corrupted | **Yes**, see section 3 |

Longhand sign-in also failed intermittently after 10:03Z ("We couldn't reach
the account service", reference `aa20bcdf-68fd-4a5e-b383-7bafecaacca5`). The
cause is **not established**: the reference is in the Longhand control-plane
logs, which this investigation could not read (HTTP 401). The candidate is
contention from failure B, whose requests each held Core's single archive
worker (`ALLSOURCE_ARCHIVE_WORKERS=1`) for about a second, back to back.

## 2. Measurements

### Tenant history

Every archive file of `onboard-control-plane-at-technical-leaders-com`, read
with `allsource-repack-archive inspect` (read-only):

| File (time span) | On disk | Declared uncompressed | Rows |
|---|---:|---:|---:|
| 2026-04 snapshot (04-28 to 10-12) | 29,568,604 B | 149,338,384 B | 74,868 |
| 2026-08 snapshot (08-06 to 08-21) | 14,354,971 B | 121,309,602 B | 20,878 |
| 2026-09 snapshot (09-27 to 10-03) | 13,066,457 B | 88,117,594 B | 18,035 |
| 2026-10 snapshot (10-09 to 10-16) | 11,373,381 B | 102,931,934 B | 7,162 |
| 2026-10 snapshot (10-16 to 10-21) | 13,101,684 B | 98,361,129 B | 8,066 |
| 2026-10 snapshot (10-21 to 10-25) | 6,285,934 B | 24,259,500 B | 9,613 |
| 41 flush files (10-25) | ~200 KB | ~245 KB | ~420 |
| **Total** | **~88 MB** | **~585 MB** | **~139,000** |

The row total matches the control plane's own count (`events_used` 138,838 at
10:09:49Z).

**Growth.** The three October snapshots cover 2026-10-09 08:50Z to 10-25 10:05Z
(16.05 days) and hold 225,552,563 declared bytes: **14.05 MB of decoded history
a day** (per window: 13.9, 18.9 and 6.9 MB/day). On disk that is 30.8 MB, or
1.92 MB a day.

**Recent events are larger.** The April snapshot averages 2.0 KB per row; the
October 9–16 snapshot averages 14.4 KB. By count the largest event types are
`mcp_audit.invoked` (20,740), `auth.session.expiry_updated` (13,091),
`classification_queue.row_purged` (11,213), `team_queue.work_item.progressed`
(10,620), `ops.live.observed` (9,419), `seat_usage.reported` (8,893) and
`team_queue.work_item.enqueued` (8,681) (approximate, `quick_stats`). **Bytes
per event type were not measured**; that measurement decides which writers to
change.

### Core machine

shared-cpu-8x, 16 GiB. At 11:08Z: 7,954 MB used, 7,781 MB available. Disk 12%
(1.1 G of 9.7 G).

## 3. Why it returns: two limits

### Limit 1: strict whole-history reads (about 16 days)

The event feed (every page, including the `limit=0` head) and a read-model
commit (which reads the feed head) decode the tenant's entire archive and
refuse past `max_uncompressed_bytes`.

- Before #355: 128 MiB. The tenant is about 585 MB, so these calls always failed.
- After #355: 768 MiB (805,306,368 B) through
  `ALLSOURCE_STRICT_ARCHIVE_MAX_DECODED_BYTES`. Headroom 220 MB.
- **At 14 MB a day the limit is reached in about 16 days, around 2026-11-10.**
- Caveat: the budget charges decoded JSON text, not the declared Arrow size.
  The 2026-04 file alone (149 MB declared) exceeded 134 MB of charge, so the two
  are close, but the exact date can move by days either way.

Who it hits today: the shared queue projector only. User reads of the
projection are switched off (`LONGHAND_SHARED_QUEUE_PROJECTION=false`), so no
user screen depends on it. Enabling the shared projection would put this limit
on the user path.

There is a second risk inside this limit. A strict decode peaks near 8.25× the
decoded text (`archive_budget.rs`, dhat measurement). At 585 MB that is about
4.8 GB of transient heap; at the 805 MB limit, about 6.6 GB, against 7.8 GB
available today. Core's memory admission (`admit_hydration`) estimates
on-disk bytes × 16 + 1 GiB reserve = about 2.5 GB for this tenant, so it would
not refuse a decode that large. Near the end of the 16 days, a cold strict load
could exhaust memory and restart Core for every tenant. This is an estimate
from a synthetic measurement, not an observation.

### Limit 2: ordinary reads load the whole tenant (months)

A normal query for this tenant (`store.rs` `query` → `ensure_tenant_loaded`)
makes the whole tenant resident before answering, under memory admission
instead of the strict budget.

- On 2026-10-24 at 21:40Z, on the then 8 GiB machine, admission refused this
  tenant: needed 2,474,832,720 B, 1,971,437,568 B available. Core was moved to
  16 GiB that day.
- Today: estimated need ~2.5 GB, 7.8 GB available, 5.3 GB headroom. The
  estimate grows 16 × 1.92 MB = 31 MB a day, so **about 170 days** if nothing
  else changes. Available memory also shrinks as other tenants grow.
- When it is reached, every read of this tenant answers 503
  `archive_hydration_refused`. That is a full outage for Longhand, not only the
  projector.

## 4. What each change did

| Change | Status | Effect | Does not |
|---|---|---|---|
| #351 WAL append order | Deployed (`sha-fe6be491`) | Ends failure A at its cause | — |
| #352/#353 gzip, fastest level, no gzip on internal hops | Deployed | Smaller responses to clients that ask; release gate 0.851 | Change the outage |
| #354 build only changed images | Merged | Docker Build rebuilds only images whose sources changed | Change the outage |
| #355 A: strict limit 768 MiB, 60 s | PR open | Feed and read models load for this tenant | Remove the whole-history load; buys ~16 days |
| #355 B: budget overrun → 503 + `Retry-After` | PR open | Clients back off (300 s for size, 5 s for time) instead of retrying a 500 | Make the request succeed |
| #355 C: read model restores from its own entity | PR open | A read-model GET no longer depends on tenant size | Fix a read-model commit, which still reads the feed head |
| Longhand #1702 projector back-off | Merged 11:05:54Z; control-plane deploy queued | At most one request per back-off window (up to 300 s) while Core fails | Make the projector succeed |

## 5. What removes the limits

Ranked by how much of the risk each removes.

1. **Feed without the whole history (Core, `sand`).** Persist the feed epoch and
   per-file positions so the head is metadata and a page decodes only the files
   it emits. The archive is rewritten in four places, and each must keep
   positions consistent: flush (`flush_storage`/`checkpoint` →
   `write_atomic_parquet` → `record_flushed_versions`), compaction
   (`compact_group`), repack/rollback, and retention. This removes limit 1 and
   the OOM risk above. Moves to `rock` once each rewrite site has a test that
   positions survive it.
2. **Cut the growth (Longhand, `sand`).** 14 MB a day of decoded history drives
   both limits. Measure bytes per event type first, then move high-volume,
   short-lived types (audit, session-expiry, liveness, usage reports) out of
   this tenant or give them retention. Halving the write rate doubles both
   runways.
3. **Ordinary reads without the whole tenant (Core, `water`).** Answer
   event-type and entity queries from the archive selection paths
   (`visit_retained_entity`, `visit_retained_prefix`) rather than full
   residency. This removes limit 2. Needs a spike to classify which query
   shapes can be served this way.
4. **Admission estimate (Core, `rock`).** Base `admit_hydration` on declared
   uncompressed bytes instead of on-disk bytes × 16. This tenant compresses
   6.6×, so the current estimate is too low for strict decodes.

## 6. Not verified

- The cause of the intermittent sign-in failures (Longhand control-plane logs
  were unreadable).
- Whether the projector is the only caller of the failing route. Inferred from
  code and from one error per projector tick; no request path is logged.
- Bytes per event type.
- The resident size of this tenant in Core's memory; only admission's estimate
  is known.
- The exact decoded-text charge for the whole tenant (declared bytes used as the
  proxy).
