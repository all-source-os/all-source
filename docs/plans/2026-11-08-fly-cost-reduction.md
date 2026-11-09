# Fly cost reduction plan (2026-11-08)

**Goal.** Bring the `allsource` Fly org from **$144.87/month** (machines $133.92, volumes $10.95; [cost report](../reports/2026-11-08-infra-costs-status.html)) to about **$55/month**. Spend must be justified by measured user activity, and today it is not.

**Limits that still hold.**
- No deployment downtime.
- No user denied access unless the owner says so.
- Core changes go in the planned morning window.

**Owner decisions, 2026-11-08: all yes.**
- Core restart in the morning window is acceptable.
- Checkpoint verify may go off.
- Longhand's old `team_queue.snapshot.chunk` events may be compacted.
- System retention with a cold tier is approved, as in step 3.4.

## Status

- **Step 1: done.** `allsource-auth` and `allsource-prime` are 2 × 256 MB. The remote builder app is destroyed. Unattached volume `vol_vwjoq95l03qzy88r` was snapshotted as `vs_9Z9kew82zK3sjAkgx0eaxzw`, then destroyed. `apps/web` runs with `min_machines_running = 0` and suspend.
- **Step 0: partly done in code.** Core exports `allsource_system_resident_events`, `allsource_system_resident_bytes` and `allsource_process_resident_bytes` (60 s refresh). Per-tenant resident bytes are not done: the owner has not yet approved tenant ids as a Prometheus label.
- **Step 2: in progress.** See "Step 2" below for what this change set ships and what stays gated on the rehearsal.

## Measured Core memory growth (2026-11-08)

Core memory used (`mem_total - mem_available`), from Prometheus:

| Window | Observed | Slope |
|---|---|---|
| 2026-10-28 → 11-06 ~22:00Z | 10.53 → 12.07 GiB | ~0.16 GiB/day |
| 2026-11-06 restart | drops to 6.87 GiB | — |
| 2026-11-08 | 7.38 GiB and climbing | ~0.43 GiB/day in the 15-min series since restart |

The restart time is 16:21Z on 2026-11-06 (the Prometheus series), not ~22:00Z as the midnight samples suggested. A boot curve of 11.84 → 6.52 GB over ~4 minutes shows that most of the verify transient is returned after boot. Growth stays **sand** until the step-2 gauges show what grows. The "6 KB per event" figure is an inference, not a measurement, and is not used.

Rejected explanations:
- Cache of 2 GiB bounds RSS. Enforcement ran only on hydration (`apps/core/src/store.rs` `ensure_tenant_loaded`), so the cache did not bound RSS.
- `MALLOC_ARENA_MAX=2` bounds RSS. Every Core runtime ships a static musl binary built on `rust:*-alpine` (`apps/core/Dockerfile:24`), so there is no glibc for it to configure, and Rust allocations go through mimalloc (`apps/core/src/main.rs`).

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

Config and code work in one Core morning window. The items below marked as shipped go first. The rest waits for a rehearsal on `allsource-core-rehearsal` from the latest snapshot.

Shipped by this change set (code and config only, no deploy):

1. **Checkpoint verify off.** `ALLSOURCE_SYSTEM_CHECKPOINT_VERIFY=false` (`apps/core/fly.toml:83`). Removes the whole-WAL load at boot. Confidence: **rock** as config. The kill switch `ALLSOURCE_SYSTEM_CHECKPOINT=false` remains.
   - **Detection lost.** With verify off, a checkpoint is accepted on its CRC32 alone (`system_checkpoint.rs` read). Nothing at boot folds the WAL and compares it with the checkpoint, so a fold bug that writes a valid CRC is served until someone notices.
   - **Recovery copy.** The WAL is never pruned. Recovery from a bad checkpoint is `ALLSOURCE_SYSTEM_CHECKPOINT=false` with a full WAL replay (`system_bootstrap.rs` `load_checkpoint`), which needs the 16 GB machine.
   - **Compensating check, not built.** An offline verify on a copy of the data directory, run on a machine large enough for the full replay, before each resize.
2. **Folded events leave memory at each checkpoint.** `SystemMetadataStore::release_folded` runs after a checkpoint is durable (`apps/core/src/infrastructure/persistence/system_store.rs`, `system_checkpointer.rs` `commit`). Before this, resident folded events only grew. Confidence: **rock** on mechanics; the memory magnitude is **sand** until the gauges are read.
   - This change set does not claim a steady-state RSS reduction. The tenant cache holds 0.82 GiB of the ~6.75 GiB anonymous memory; the rest is not attributed.
   - Once folded events are released, the checkpoint is the only in-memory copy of folded tenant state (item 1 applies).
   - Non-tenant domains (`audit`, `config`, `schema`, `policy`, `consumer`, `auth`) are not folded, so they stay resident and grow with `ACK_UPDATED` and audit events. Their growth rate is not measured.
3. **Gauges for the rehearsal.** `allsource_system_resident_events`, `allsource_system_resident_bytes` and `allsource_process_resident_bytes` (`apps/core/src/infrastructure/observability/metrics.rs`). Confidence: **rock**.
4. **Cache budget enforced on a 60 s timer.** `EventStore::enforce_cache_budget_now`, driven from `spawn_memory_upkeep` in `apps/core/src/main.rs`. The timer evicts only while the byte budget is exceeded. It does not consult kernel headroom: on a machine under the reserve, evicting frees nothing the kernel can see, and a headroom loop would empty the cache every tick. Headroom eviction stays on the hydration path, where the loaded tenant is exempt. Lowering `ALLSOURCE_CACHE_BYTES` now takes effect without a hydration. Eviction now also returns capacity (`shrink_to_fit`). Confidence: **sand**; an active tenant can be evicted and then rehydrated, which costs admission.
Rehearsal of this change set, 2026-11-09 00:50Z: image `ghcr.io/all-source-os/allsource-core-alpine@sha256:0492cdb0d482d0f40e15297aff024666050c7ecbda7812d5e3f43cf7fa6c44d2` on `allsource-core-rehearsal` (16 GB, prod `fly.toml`), volume restored from prod snapshot `vs_6M90bpMKMyJ0hXJPYXY5R`.
- Boot: stage 1 in 530 ms, ready 0.6 s after process start, `Boot system checkpoint written at 2999771`, no whole-WAL load.
- After the first memory tick: `allsource_system_resident_events` 12,590 (the 61 folded tenant events were released; the rest are config events), `allsource_system_resident_bytes` 30,788,874, `allsource_process_resident_bytes` 121,786,368. Core VmHWM 247 MB, against 13.26 GB at prod's last boot with verify on.
- Not exercised: client traffic, so no tenant hydrated and the growth slope was not observable. The slope check is the production gauges after deploy.

Still gated on the rehearsal (not changed by this change set):

5. `MALLOC_ARENA_MAX=2` removed from `apps/core/Dockerfile` (PR #407): the image has no glibc.
6. `ALLSOURCE_CACHE_BYTES` 2 GiB → 512 MiB (`apps/core/fly.toml:33`).
7. `ALLSOURCE_STRICT_ARCHIVE_MAX_DECODED_BYTES` 805 MB → 512 MB (`apps/core/fly.toml:43`). Confirm the 149,338,384 B Longhand snapshot and the system tenant decode within it first.
8. `swap_size_mb` on Core. Key support in `fly.toml` not verified. Confidence: **sand**.
9. `[[vm]]` to `cpus = 4`, `memory_mb = 8192`. The stale "Do not downsize" note stays until the rehearsal evidence exists. The `[[vm]]` block stays at 16 GB in this change set.

Rehearsal, required before items 6 to 9:

- Clone Core to a separate test app from the same image at 8192 MB, and restore the volume snapshot.
- Record boot `VmHWM`, then steady RSS over 1 h.
- Hydrate the largest tenant and check admission refusal for each tenant.
- Pass criteria: RSS plateau below 6 GiB with cache 512 MiB, and no admission refusal for any tenant.

Admission risk, open: the 429 MB `system` tenant needs ~7.9 GB free under `archive × 16 + 1 GiB` (`hydration_admission.rs:19-22`). Not verified whether the `_system` tenant passes through hydration admission at all. If it does, 8 GB is blocked until the floor drops.

Blocker, projected and not measured on 8 GB. Used memory is ~7.15 GiB (Prometheus, 2026-11-08). On an 8 GiB machine that leaves ~0.7 GiB available, under the 1 GiB reserve (`apps/core/src/infrastructure/observability/memory.rs` `has_headroom`). Consequences:
- Headroom stays false, so the post-hydration eviction (`store.rs` `enforce_cache_budget`) evicts every other tenant. Eviction does not return RSS to the kernel reliably, so this repeats per hydration.
- Admission refuses every tenant. Longhand (120 MB archive) needs ~2.9 GiB free.
- Step 2 is not shippable as a resize until the rehearsal shows steady RSS below the reserve line, or the reserve and ratio are changed with evidence.

Other open items, not changed here:
- Admission is one pre-check against a ratio measured on one sample. Nothing re-checks during decode, so a tenant admitted near the threshold can overshoot 8 GB mid-load.
- Strict whole-tenant decode (`ALLSOURCE_STRICT_ARCHIVE_MAX_DECODED_BYTES`, 805 MB) charges uncompressed payload bytes (`archive_budget.rs` `decoded_payload`), not heap. No admission check runs on that path (`store.rs` strict override).
- `CoreMemoryHigh` (`tooling/monitor-stack/rules/memory.yml`) fires at 85% used. At 8 GiB and current usage it fires continuously. That is the same state as the blocker above, not a new fault.

Confidence for the resize as a whole: **sand**, a rock once the rehearsal numbers exist. The resize is a Core restart, which is the accepted morning-window exception.

### Rollback

- **Config items (verify, cache bytes, decode cap, `[[vm]]`):** set the value back in `apps/core/fly.toml` and deploy the Core window. Verify back on at 8 GB loads the full system WAL in one `Vec` (`system_checkpointer.rs` `verify_loaded`) and is not safe there. Roll back to 16 GB first.
- **Code items (release_folded, shrink_to_fit, timer eviction, gauges):** these are not config flips. Rollback is an image revert to the previous Core release, followed by the Core deploy window.
- **Checkpoint damage:** `ALLSOURCE_SYSTEM_CHECKPOINT=false` boots with a full WAL replay, which needs 16 GB. Checkpoint files move to `rejected/` on a verify failure; none are deleted.

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

Answered 2026-11-08, all yes:
1. Core restart in the morning window for the resize.
2. Checkpoint verify off.
3. Compaction of Longhand's old `team_queue.snapshot.chunk` events.
4. Volume `vol_vwjoq95l03qzy88r`: snapshotted as `vs_9Z9kew82zK3sjAkgx0eaxzw`, then destroyed.

Still open:
5. Does the invoice bill the builder's 50 GB volume? The billed machine total is $7.44 above list prices, unattributed.
6. Is a cold start on the marketing site acceptable? Step 1 set `apps/web` to `min_machines_running = 0` with suspend.
7. Is a separate hosted Prime still needed?
8. Per-tenant resident bytes as a Prometheus series with a `tenant` label. The `[metrics]` comment in `apps/core/fly.toml` says series carry no tenant label. Needs the owner's OK before tenant ids enter the org Prometheus.

## Not measured

- When the 12.64 GiB peak happened, and which tenants are resident.
- Live heap versus allocator-retained memory.
- Prime and auth request counts: neither logs requests, and Prime's `/metrics` returns 404.
- Invoice line items.
- The contents of the unattached volume.
- Swap behaviour on Fly.
- Cross-org Flycast DNS and billing.
