# Fly cost reduction plan (2026-11-08)

**Goal.** Bring the `allsource` Fly org from **$144.87/month** (machines $133.92, volumes $10.95; [cost report](../reports/2026-11-08-infra-costs-status.html)) to about **$55/month**. Spend must be justified by measured user activity, and today it is not.

**Limits that still hold.**
- No deployment downtime.
- No user denied access unless the owner says so.
- Core changes go in the planned morning window.

## What the spend buys

The figures below come from control-plane `/metrics` on both gateway machines over a 15.3 h window ending 2026-11-08 ~11:00Z, from Core `/proc` and `/metrics`, and from `fly machine list`.

- **Tenants:** 22 registered, and 4 made authenticated requests. All 4 are ours: Longhand (`onboard-control-plane-at-technical-leaders-com`), the consumer-access canary, jbt-fit, and the owner's own account. No external tenant is paying.
- **Longhand's share:** 99.3% of gateway requests (~677k in 15.3 h, about 1.06M a day) and 99.5% of response bytes (8.95 GB, about 14 GB a day). 565k of those requests are `GET /events/query` polls.
- **Writes:** about 2.6k `POST /api/v1/events` a day.
- **Core's CPU:** load average 0.05 on 8 vCPUs. Core runs shared-cpu-8x only because Fly allows at most 2 GB per shared vCPU, so 16 GB of RAM forces 8 CPUs.
- **Core's memory:** RSS 6.47 GiB, peak (`VmHWM`) 12.64 GiB since the 2026-11-06 16:22Z restart.
- **Core's data:**
  - 724 MB of Parquet across all tenants.
  - The `system` tenant holds 429 MB.
  - Longhand holds 120 MB, 81% of it old `team_queue.snapshot.chunk` events that stopped growing on 11-06.
- **Core's cost:** $101.52/month, 70% of the bill. $96 of that is RAM.

The workload is a few hundred MB of data, a few thousand writes a day and one polling client. The 16 GB machine is sized for Core's memory behaviour, not for demand.

## Why Core needs 16 GB today

1. **Hydration costs 16× the archive.** Admission requires `archive_bytes × 16 + 1 GiB` free (`apps/core/src/infrastructure/persistence/hydration_admission.rs:19,22,32-61`); the ratio was measured at 154 MB archive to ~2.2 GB RSS.
   - The `system` tenant would need ~7.9 GB.
   - Longhand needs ~2.9 GB.
   - On 2026-10-24 an 8 GB Core refused Longhand's hydration (`apps/core/fly.toml:120-122`). The refusal returns 503 `archive_hydration_refused`.
2. **Checkpoint verify loads the whole system WAL.** `verify_loaded` calls `read_all_entries()` and collects ~2.95M events into one `Vec<Event>`, then folds twice (`apps/core/src/infrastructure/persistence/system_checkpointer.rs:252-298`). It runs on every checkpoint boot. The fallback full replay does the same (`apps/core/src/infrastructure/persistence/system_store.rs:221-228`). The system WAL is 1.6 GB on disk, mostly a 2.5M-event burst from 2026-10-24 to 10-26.
3. **Whole-tenant decode is capped at 805 MB and peaks at ~8.25× that**, about 6.3 GiB (`apps/core/fly.toml:37-43`). The cap sits that high because the 128 MiB default refused Longhand's 149 MB snapshot.
4. **Steady RSS stays at ~6.5 GiB.**
   - It was the same on 10-25, before verify existed.
   - The cache estimate reads 0.88 GB for 237k resident events, but the runbook measured real usage at roughly 6× the estimate.
   - 5.44 GiB of RSS is in transparent huge pages under mimalloc, so some of it may be freed memory that is never returned to the OS. Not measured.

Which of 2 or 3 produced the 12.64 GiB peak is not known: no Core memory series reaches Prometheus.

## The plan

Ordered so that each step ships on its own and is measured before the next.

### Step 0: measure Core memory (prerequisite for every Core step)

- Export process RSS from Core's `/metrics` and federate it into the bandwidth-alerts Prometheus. Add per-tenant resident bytes as well.
- Add a `CoreMemoryHigh` alert.
- Without this series, no resize decision can be checked against its own result.

### Step 1: cuts with no user impact (about $11–14/month)

| Change | $/month | Evidence | Constraint check |
|---|---|---|---|
| `allsource-auth` 2 × 512 MB → 2 × 256 MB | ~2.50 | RSS 63 MB of 459 MB | Rolling restart over 2 machines, no gap |
| `allsource-prime` 2 × 512 MB → 2 × 256 MB | ~2.50 | RSS 61 MB of 461 MB | Same |
| Destroy the stopped remote builder `fly-builder-sparkling-star-9149` and its 50 GB lhr volume | 0–7.50 | Volume exists; Fly's pricing page says builders are not billed | The invoice line decides. Next `--remote-only` web/admin build starts cold |
| Delete unattached Core volume `vol_vwjoq95l03qzy88r` (10 GB, pre-recovery) | 1.50 | No machine attached, 8 months old | Owner OK, or snapshot first: it is the only pre-recovery copy |

### Step 2: Core to shared-cpu-4x / 8 GB ($50.76/month)

Config work, in one Core morning window, after a rehearsal on `allsource-core-rehearsal` from the latest snapshot.

1. `ALLSOURCE_SYSTEM_CHECKPOINT_VERIFY=false` (`apps/core/fly.toml:84`).
   - The checkpoint has matched the WAL on both runs so far (11-05 rehearsal, 11-06 prod).
   - The kill switch `ALLSOURCE_SYSTEM_CHECKPOINT=false` remains.
2. `ALLSOURCE_CACHE_BYTES` 2 GiB → 512 MiB.
3. `swap_size_mb` on Core so that a transient decode peak pages instead of OOM-killing. Steady state must stay under 8 GB.
4. Rehearse:
   - Boot, then hydrate each tenant in turn, recording the RSS step for each.
   - Run one strict feed decode of Longhand.
   - Pass only if peak RSS stays under ~7 GiB with no `archive_hydration_refused`.
5. Resize `[[vm]]` to `cpus = 4`, `memory_mb = 8192` and update the stale "Do not downsize" note at `apps/core/fly.toml:128` with the rehearsal evidence.

Confidence: **sand**. It becomes a rock once the rehearsal numbers exist. The resize is a Core restart (6.5–81 s not ready), which is the accepted morning-window exception.

### Step 3: make Core fit 4 GB (another $25.38/month, $76.14 in total)

Code work in `apps/core`, each change shipped through the Core deploy window. This is the step that answers "we should not use that much RAM".

1. **Fold the system WAL as a stream.** Fold segment by segment in verify and in the fallback replay, instead of `read_all_entries()` into one `Vec` (`system_checkpointer.rs:257`, `system_store.rs:221-228,354-362`).
   - The peak then no longer grows with history, and a fallback boot cannot OOM-loop on a small machine.
2. **Shrink the resident multiplier.**
   - 16× the archive size in RAM is the defect.
   - Measure where the bytes go per event (payload representation, per-event indexes, `entity_versions`), then cut the largest.
   - Target is under 4× before sizing to it.
3. **Per-tenant compaction / delete-by-type.**
   - Lets Longhand drop the 468 MB of superseded `team_queue.snapshot.chunk` events. Longhand's archive goes from ~120 MB to ~23 MB.
   - The strict decode cap can then return to 128 MiB without refusing Longhand's feed.
   - Needs the tenant owner's OK on the data.
4. **System retention with a cold tier.**
   - Set `ALLSOURCE_COLD_STORAGE_URL`, then `ALLSOURCE_RETENTION_SYSTEM_DAYS=30`.
   - The 429 MB `system` tenant shrinks to what is actually read.
5. **Allocator.** Test `MIMALLOC_PURGE_DELAY=0` / large-page settings against the THP-backed RSS. This is a hypothesis to rehearse, not a fix to assume.

Confidence: **water** for 4 GB until 1 and 2 are measured. A 2 GB Core ($88.83 saved) is not planned. Even after the chunk cleanup, Longhand would need ~1.4 GB at today's multiplier.

### Step 4: cut the demand that drives it (Longhand side, no Core window)

- **Long-poll instead of polling.** Longhand makes ~10 `events/query` calls per second. Moving its hot loops to `GET /api/v1/views/{view}/changes?wait` or the event feed:
  - cuts gateway egress (16 GB/day, ~$9.63/month at $0.02/GB, billed on top of the $144.87);
  - cuts Core's transient allocations.
- **Private route (Flycast).** Saves the same egress, but only after the gateway refuses the plain-HTTP port from public addresses. A Fly service port binds on every IP, so without that guard keys could cross the internet in cleartext. Cross-org Flycast DNS and billing are not documented and need a probe first.

### Step 5: fewer apps (owner decisions)

- **Query service:** about $3.19. Near-zero user traffic. It sits on the web sign-in check (`apps/web/src/app/api/auth/session/route.ts:9-15`) and holds one websocket. Routing `/api/auth/me` to auth or the gateway would let it go.
- **Hosted Prime:** about $6.68. Core already carries the Prime modules. If the hosted MCP is a thin front, it can be served by Core or the gateway.
- **Web:** about $3.50 with `min_machines_running = 0`. Visitors then wait for a cold start. `suspend` may cut that wait to a few hundred ms (not checked).

### Rejected

- **Auth, Prime or gateway down to one machine:** $2–4 each. A crash then denies access, and the gateway outage of 2026-11-06 happened on one machine.
- **Replacing bandwidth-alerts with Grafana:** $2.34. Fly's Grafana has no alerting. It would mean porting 11 rules to an external Grafana and losing the only availability alerting meanwhile.
- **Core autostop:** Core has no services and callers use `.internal`, so Fly cannot start it on demand.

## Where the bill lands

| After | Core | Everything else | Total |
|---|---|---|---|
| Today | $104.52 (machine + 2 volumes) | $40.35 | $144.87 |
| Step 1 | $103.02 | ~$28–31 | ~$131–134 |
| Step 2 | $52.26 | ~$28–31 | ~$80–83 |
| Step 3 | $26.88 | ~$28–31 | ~$55–58 |
| Step 5 (all) | $26.88 | ~$15–18 | ~$42–45 |

## Open questions for the owner

1. Is the 6.5–81 s Core restart in the morning window acceptable for the resize? Every Core step depends on it.
2. May checkpoint verify go off now (matched twice), or stay on until the streaming fold lands?
3. May Longhand's old `team_queue.snapshot.chunk` events be compacted once Core supports it?
4. Can `vol_vwjoq95l03qzy88r` be deleted, or should it be snapshotted first?
5. Does the invoice bill the builder's 50 GB volume? The billed machine total is $7.44 above list prices, unattributed.
6. Is a cold start on the marketing site acceptable?
7. Is a separate hosted Prime still needed?

## Not measured

- When the 12.64 GiB peak happened, and which tenants are resident.
- Live heap versus allocator-retained memory.
- Prime and auth request counts: neither logs requests, and Prime's `/metrics` returns 404.
- Invoice line items.
- The contents of the unattached volume.
- Swap behaviour on Fly.
- Cross-org Flycast DNS and billing.
