# Core memory and compaction

How AllSource Core bounds its own memory, what refuses work when it cannot,
and how to tell whether compaction and retention are actually running.

Written after Core was OOM-killed in production on 2026-10-12 (gh#321). The
short version: compaction had never run since the feature shipped, so
fragmentation and retention both went unenforced, and nothing refused a
tenant load that could not fit.

## The invariant

> Nothing becomes resident that the budget accounts for, and no single
> request allocates memory proportional to a tenant's history.

Every mechanism below exists to hold one half of that. A fix that does not
name it gets re-broken through the next door: gh#321 was first fixed for
conditional writes in 0.25.4 and recurred through tenant hydration.

## Is compaction running?

```bash
fly ssh console -a allsource-core -C "curl -s localhost:3900/api/v1/compaction/stats"
```

`stats.total_compactions` is the number that matters. **If it is `0` and the
process has been up longer than `compaction_interval_seconds`, compaction is
not running** — which was true in production for the life of the feature,
because the config, the strategy and this endpoint all shipped without a
scheduler to call them.

**These stats are in-memory and reset on restart**, so `0` on a
freshly-started process means nothing either way. Check uptime first
(`ps -o etime`), and read the file counts below for the durable evidence — a
tenant sitting at a handful of `snapshot.*` files has been compacted whatever
the counter says.

The loop lives in `apps/core/src/main.rs` and logs on start:

```
✅ Background compaction loop started (interval: 3600s)
```

No such line means no sweep. A completed sweep logs
`Background compaction sweep complete` with `files_compacted`.

### Retention rides on the sweep

`ALLSOURCE_RETENTION_SYSTEM_DAYS` is applied *inside* `compact_tenant`, not on
its own cadence. No sweep means no retention, however the TTL is configured.
That is how the `system` tenant reached 240,602 Parquet files for 1,910 MB
with a 30-day TTL set.

Three things about this variable that the name does not tell you:

- **It reaches the `system` tenant and nothing else.** `RetentionConfig`'s
  `default_ttl` is `None` and `system` is the only override `from_env_vars`
  ever writes, so every customer tenant keeps events forever regardless of
  what this is set to.
- **`0` means keep forever**, not "expire everything now". The literal
  reading would delete the tenant's whole history, which is never what
  turning the number down is asking for.
- **Unsetting it does not disable retention.** The 30-day default for
  `system` lives in `RetentionConfig::default()`, so the explicit `0` is the
  only way to turn it off from the environment.

It is currently `0` in `apps/core/fly.toml`. Compaction ran for the first
time on 2026-10-12 and this TTL has still never been applied, so enabling it
deletes heartbeat history that has never been subject to it — around 1.9 GB,
outright, because no cold-tier archive is configured. **Turning it on is a
separate, deliberate change**: set it to `30`, and consider configuring
`ALLSOURCE_COLD_STORAGE_URL` first so the dropped events are archived rather
than discarded (`docs/operations/COLD_TIER.md`).

### Plan-tier retention is defined but not enforced

`apps/control-plane/internal/domain/entities/subscription.go` gives each tier
a `RetentionDays` — 7 (Free), 14 (Indie), 90 (Studio), 365 (Scale), `-1`
forever (Enterprise). **Nothing applies it.** Core is the only component that
could expire a tenant's events and it has no view of the tenant's plan, so
every customer tenant today behaves as unlimited.

That is the safe direction to be wrong in, and it is why no customer's data
was at risk when the sweep was switched on. Closing the gap means Core
deleting customer events on a schedule, so it needs its own design — at
minimum a per-tenant TTL carried in tenant metadata, a cold tier configured
first, and a dry-run mode that reports what a sweep would drop.

## Why file count matters more than bytes

Get the file **count** before reasoning about any per-request archive read.
Measured in production: ~0.205 ms to touch one file, so 20,000 files cost
~4.2 s before anything is decoded. The strict read budget allows 4 s.

```bash
fly ssh console -a allsource-core -C \
  "sh -c 'for d in /app/data/storage/*/; do n=\$(find \$d -name \"*.parquet\" | wc -l); s=\$(du -sm \$d | cut -f1); echo \"\$n files \${s}MB \$d\"; done'"
```

`max_files` in `ArchiveReadLimits` is 50,000. A tenant past it cannot be read
strictly at all, so conditional writes to it fail until compaction brings it
back under.

## What refuses work, and what each refusal means

| Response | Meaning | Action |
|---|---|---|
| `503 archive_index_incomplete` + `Retry-After: 1` | A version resolve ran out of budget. It persisted what it folded, so the next attempt resolves strictly fewer files. | Retry. `remaining_files` must strictly decrease; `stopped_by` names the limit. |
| `503 archive_hydration_refused` + `Retry-After: 30` | The tenant's archive does not fit in the memory the process has left. Nothing was allocated. | Retry later, or shrink the tenant. A tight retry does not help — see below. |
| `409 version_conflict` | Ordinary OCC. | Caller re-reads and retries. |

**A client that treats a 503 as terminal turns all of this back into a failed
publish.** That is the one link verified from neither side.

`archive_hydration_refused` carries `needed_bytes` and `available_bytes`. It
clears when another tenant is evicted or when compaction and retention shrink
this one, neither of which a fast retry brings closer — hence 30s rather
than 1s.

An entity-scoped query (`?entity_id=…`) does **not** get refused: it falls
back to reading that entity's events straight out of the archive, decoding
only row groups whose `entity_id` statistics admit it. That is what lets a
conditional publish succeed against a tenant too large to be resident.

## Memory accounting: two numbers, only one of them real

| Number | Where | Trust |
|---|---|---|
| `allsource_core_cache_bytes` | `/metrics` | **Estimate only.** Counts a serialized payload plus 256 bytes per event. Read 554 MB while the process held 3.3 GB. |
| cgroup `memory.current` / `memory.max` | kernel | Ground truth. What admission and eviction key off. |

```bash
fly ssh console -a allsource-core -C "sh -c 'free -m; ps -o pid,rss,etime,comm -A | grep allsource'"
```

The estimate is kept because it is per-tenant and so can **order** eviction
victims. It does not decide **whether** to evict; measured headroom does.
`ALLSOURCE_CACHE_BYTES` is therefore a soft hint, and `fly.toml` has said so
since before the outage: "this soft cache limit alone cannot prevent OOM
during a cold load."

On a platform that reports no limit — a developer machine, CI on macOS — the
probe returns nothing and admission allows everything. Fail-open is
deliberate: a probe that cannot measure must not stop every load.

## Tunables

| Variable | Default | What it bounds |
|---|---|---|
| `ALLSOURCE_SNAPSHOT_INTERVAL_SECONDS` | 3600 | Sweep cadence. Clamped to 60…86400 by the loop. |
| `ALLSOURCE_RETENTION_SYSTEM_DAYS` | 30 | TTL for the `system` tenant. Applied inside the sweep. |
| `ALLSOURCE_COMPACTION_INPUT_BYTES` | 32 MiB | Compressed input per snapshot. **This is the term that bounds a sweep's peak memory** (~16x resident). |
| `ALLSOURCE_MEMORY_RESERVE_BYTES` | 1 GiB | Headroom kept free. Admission refuses a load that would eat into it. |
| `ALLSOURCE_RESIDENT_BYTE_RATIO` | 16 | Resident bytes per archive byte. Measured, not derived — re-measure before changing. |
| `ALLSOURCE_CACHE_BYTES` | 2 GiB | Estimate-based eviction target. Soft. |
| `ALLSOURCE_ARCHIVE_WORKERS` | 1 | Concurrent strict cold loaders. |

Raising `ALLSOURCE_RESIDENT_BYTE_RATIO` makes admission stricter; lowering it
admits loads that may not fit. If you lower it, measure RSS before and after
a cold load of a known-size tenant and put the number in a commit message.

### Machine size

`shared-cpu-2x` caps at 4 GiB. 8 GiB needs `shared-cpu-4x` — the CPU bump is
a requirement of the memory, not a separate choice. `apps/core/fly.toml` must
match the running machine, or the next deploy silently downsizes it.

## Sweep behaviour worth knowing before you watch one

- **Groups, not tenants.** `compact_tenant` splits candidates into groups
  bounded by compressed bytes and file count, and compacts each separately.
  A tenant's size sets how many snapshots a pass writes, never how much
  memory it needs.
- **Per-group atomicity.** A group's snapshot is renamed into place before
  any of its own inputs is deleted. A crash between groups leaves earlier
  groups compacted and later ones untouched.
- **One bad file no longer blocks its tenant.** The failing group is skipped
  and reported; its siblings still compact. The sweep still returns the
  error, so a partial sweep cannot be read as a clean one.
- **Snapshots merge upward.** A snapshot under `small_file_threshold`
  (10 MB) is a candidate again, so passes consolidate their own output
  instead of leaving one snapshot per pass.
- **A sweep stops at `max_pass_seconds`** (240s) and resumes next tick. The
  first pass over a long-uncompacted tenant has as many groups as the tenant
  has, so expect several ticks before it converges.

## First sweep after enabling it

Expect the backlog to take multiple ticks. Watch:

```bash
fly logs -a allsource-core | grep -E "compaction|refusing to hydrate"
```

- `compacting tenant` with a `groups` count — the plan for that tenant.
- `tenant compaction group complete` — per group.
- `compaction sweep hit its time budget; resuming next tick` — normal on a
  backlog, not an error.
- `refusing to hydrate tenant` — admission working; read `needed_bytes`
  against `available_bytes`.

Re-run the file-count command above between ticks; the count should fall
monotonically for every tenant that has candidates.

## Known limits

- A tenant whose **retained** history genuinely exceeds the machine stays
  refused for queries that are not entity-scoped. That is the honest answer
  for a read model that holds a tenant in memory; compaction and retention
  move the line, they do not remove it.
- `max_file_bytes` (32 MiB) bounds a single strictly-read file. Raising
  `ALLSOURCE_COMPACTION_INPUT_BYTES` far above it produces snapshots that the
  strict path then refuses — raise both together or neither.
- A crash between a version manifest's temp write and its rename leaves a
  `_version_manifest.json.<pid>.tmp` that `cleanup_partial_writes` does not
  sweep. Bounded to one per pid and never read.
- Freeing Rust allocations does not reliably return pages to the kernel, so
  eviction may not move `memory.current` as much as the estimate suggests.

## Related

- `docs/proposals/SUSTAINABLE_DATA_STRATEGY.md` — Steps 1–6, including the
  compaction and retention design this runbook operates.
- `docs/evidence/2026-09-27-isolated-archive-capacity/` — the row-count
  evidence behind the strict limits.
- `docs/runbooks/DIAGNOSING_PIPELINE_DATA_VISIBILITY.md` — when data is
  present but a dashboard shows none.
