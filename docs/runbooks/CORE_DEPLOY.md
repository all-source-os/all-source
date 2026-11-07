# Deploying Core

Core (`allsource-core`) is one Fly machine with one volume
(`allsource_data`, mounted at `/app/data`). A deploy stops that machine and
starts the new image on the same volume. Until its boot finishes, an image
with the boot gate (`c76c4316` and later, so `019b7e57`) answers every
request, `/health` included, with `503 {"error":"not_ready"}` and
`Retry-After: 1`; an older image (`16dd13eb`, in prod 2026-11-01 to
2026-11-06) refuses the connection. Every Core deploy is therefore a client-visible
window. This runbook keeps that window known, short and reversible; it does
not remove it.

The gateway (`allsource-control-plane`), `allsource-query`, `allsource-prime`
and `allsource-auth` reach Core at `http://allsource-core.internal:3900`.
Longhand's control plane reaches it through the gateway
(`https://api.all-source.xyz`). Never start a second machine in the
`allsource-core` app: `.internal` resolves to every started machine, so real
traffic would reach it.

Gateway deploys follow
[`CONSUMER_ACCESS_GATE.md`](CONSUMER_ACCESS_GATE.md#read-the-config-back-after-every-deploy),
including the config read-back after every deploy. The same rule applies to a
Core `fly.toml` taken from git: write it with a redirect, check its size and its
`app = "allsource-core"` line before deploying, and read
`fly machine status -d` back afterwards.

## What a boot costs

| Boot | Work | Measured |
|---|---|---|
| Image without system checkpoints (`16dd13eb`) | Full system WAL replay | 2026-11-01 in prod: 45 s stage 1 + 4m47s stage 2; clients refused 17:32:20 → 17:38:05 (5m45s). 2026-11-05 rehearsal, 2,931,768 events: 49.2 s stage 1 + 5m41s stage 2, ready after 6m30s |
| First boot of an image with checkpoints (`019b7e57`) | Full replay, then a checkpoint written at boot | 2026-11-05 rehearsal, same events: 49.6 s stage 1 + 29.6 s stage 2, ready after 79.3 s; checkpoint written 106 ms later. 2026-11-06 in prod, 2,960,884 events: old process exited 16:20:13, connections refused until the gate opened at 16:20:18.8, `not_ready` until ready at 16:21:34.4 (50.2 s stage 1), so 81.4 s in all |
| Later boots (`019b7e57`) | Newest checkpoint + WAL tail ([SYSTEM_CHECKPOINTS.md](./SYSTEM_CHECKPOINTS.md)) | 2026-11-05 rehearsal: 270 ms stage 1 + 26 ms stage 2. Verify replayed 2,919,522 tenant events and logged a match 100 s after ready. 2026-11-06 in prod (the verify restart): process exited 16:22:01, ready 16:22:07.5 (250 ms stage 1), so 6.5 s, most of it the machine restart; verify matched over 2,948,521 tenant events 98 s after ready |

The 2026-11-06 window as clients saw it, with `7e08966d` in front:

- Longhand's status page (`/api/v1/ops/status`, probed every 60 s) recorded
  one `queue_read` incident, `core_read_failed`, 16:21:17 → 16:21:56.
- The gateway answered reads with 503 `egress_budget_unavailable` (reason
  `ledger_unavailable`) and its own heartbeat and usage writes got 503
  `not_ready` until 16:21:32.
- After the verify restart the gateway refused reads with 429 `pool_full`
  from 16:22:16 to 16:22:22, and one usage increment hit its deadline at
  16:22:19.
- The gateway's log buffer started at 16:20:55, so the first 42 s of the
  window are not in this count. No 401 appears in the buffer, but whether
  the gateway logs a refused `ask_` key was not checked, so the number
  refused is unknown.

The rehearsal ran each image on a volume restored from the prod snapshot of
2026-11-05 09:28, on the same guest (shared, 8 CPUs, 16 GiB), in the separate
app `allsource-core-rehearsal` with no services. A full replay on `019b7e57`
is the 79 s row, so any later boot that falls back to the WAL (no valid
checkpoint, or a verify rejection) costs about that, not 6m30s.

## What clients see during the window

| Client | Behaviour |
|---|---|
| Gateway without `corehold` (`7e08966d`, live since the 2026-10-26 rollback) | Writes: 502 while Core refuses the connection, then the forwarded 503 `not_ready` while a gated image boots. Reads: 502 or the forwarded 503 while the tenant's egress lease holds, then 503 `egress_budget_unavailable` (reason `ledger_unavailable`, `Retry-After: 30`). An `ask_` key not checked in the last 120 s: **401** (`call Core /me: …` while refused, `invalid api key (Core /me HTTP 503 …)` while booting). JWT keys are checked locally and keep working |
| Gateway with `corehold` (main, `c76c4316`) | Holds each Core call up to 6 s, then 503 `core_unavailable`, or the forwarded 503 `not_ready` when Core was booting. An unreachable `ask_` key check answers 503 `auth_unavailable`; a managed key in enforce mode answers 503 `authority_unavailable`. Never 401 |
| Longhand control plane (SDK ≥ 0.27.0, through the gateway) | Behind `corehold`: resends on `not_ready`/`core_unavailable` for 20 s, then two ordinary retries, then `CoreRestarting` ("The data store is restarting"). Behind `7e08966d`: the 502/503/401 above come straight through, the SDK does not treat them as a restart, and the circuit breaker opens after 5 failures |
| `allsource-query` | Holds each Core call up to 6 s on a refused dial or `not_ready`, then fails it. Its usage reporter buffers increments in memory and drops them after 3 failed attempts (2 s, 4 s, 8 s back-off) |
| `allsource-prime`, `allsource-auth` | Fail their Core calls for the window |

Apart from query-service's usage buffer, no client queues a refused write for
later; it returns an error to its caller.

**Deploy the gateway from main first.** With `7e08966d` in front, an `ask_`
key that was not used in the 120 s before the window is refused with 401 for
the whole window, which a customer reads as a revoked key. The gateway deploy
is blocked until `CONSUMER_CANARY_CREDENTIALS` is set
([CONSUMER_ACCESS_GATE.md](./CONSUMER_ACCESS_GATE.md)). Going ahead without it
is a decision for the operator, made before the window.

## Before the window

1. List what changes: `git log --oneline <running>..<candidate>`. For each
   change to a client contract (for example the Prime API), decide whether
   that client deploys before or after Core.
2. The candidate commit has green CI, Security Scanning and Docker Build.
   Record its digest:
   `docker buildx imagetools inspect ghcr.io/all-source-os/allsource-core-alpine:sha-<7>`.
   Fly pulls this ghcr image by digest; the running machine and the 2026-11-05
   rehearsal both did. Promoting it to `registry.fly.io/allsource-core`
   ([FLY_IMAGE_PROMOTION.md](./FLY_IMAGE_PROMOTION.md)) also works; then record
   that image's digest instead, since the copy can have a different one.
3. Record the running image for rollback:
   `fly machines list -a allsource-core --json` → `config.image` and
   `image_ref.digest`, plus the machine id and `config.restart`.
4. Compare `config.env` from the same output with `apps/core/fly.toml`
   `[env]`. `fly deploy` replaces the machine config with the file, so a
   setting only on the machine is lost. Secrets (`fly secrets list`) are not in
   the file and survive the deploy.
5. Read the client-side baseline you will compare against for the last hour:
   gateway 2xx/4xx/5xx by route, and Longhand control-plane Core errors.
6. Snapshot the volume right before the window (daily snapshots are kept 5
   days): `fly volumes list -a allsource-core` gives the `allsource_data` id;
   `fly volumes snapshots create <vol_id> -a allsource-core`; then
   `fly volumes snapshots list <vol_id> -a allsource-core --json` until the new
   snapshot is `created`. Record its id.

## The window

```bash
fly deploy -a allsource-core --config apps/core/fly.toml \
  --image ghcr.io/all-source-os/allsource-core-alpine@sha256:<digest>
```

- Use `fly deploy` with the committed config, not `fly machine update`: only
  `fly deploy` applies `kill_timeout = 65` and `[deploy] wait_timeout = "15m"`.
- Pin the image by digest.
- Follow `fly logs -a allsource-core`. A first boot of `019b7e57` prints, in
  order: `🚦 Listening on …; answering not_ready until the bootstrap finishes`
  (before the replay), `Stage 1: system store opened in …`,
  `System store recovered: … checkpoint None`, one `Stage 2: <name> cache
  rebuilt in …` per cache, `🚀 AllSource Core ready on …` (the gate opens),
  `Boot system checkpoint written at <seq>`, and
  `System checkpoint verify skipped: this boot replayed the WAL`. A
  `16dd13eb` boot prints `🚀 AllSource Core listening on …` once it is ready.
- If `fly deploy` reports failure when the 15 m wait runs out, the machine may
  still be booting. Do not re-run the deploy or restart the machine: either
  starts the replay from zero. Follow the logs to the ready line, and roll back
  only if the boot errors or the process exits.
- Confirm the machine runs the candidate: `fly machines list -a allsource-core
  --json` shows `image_ref.digest` = the recorded digest and state `started`,
  and `fly checks list -a allsource-core` shows `health` passing.

### Verify the checkpoint inside the window

The first boot replays the WAL, so verify does not run on it. The first boot
that loads a checkpoint, and so runs verify, is the next restart. Make that
restart part of the window rather than leaving it to whichever restart comes
next:

```bash
fly machine restart <machine id> -a allsource-core
```

Expect a boot of under a second, `System store recovered: … checkpoint
Some(<seq>)`, and, within about 100 s,
`System checkpoint <seq> matches a full replay of <n> tenant events`. The
verify replay runs while Core serves; watch memory on the Fly metrics for the
app until it finishes.

If it logs `differs from a full replay` instead, Core moves every checkpoint to
`checkpoint/rejected/` and exits with code 70 (`Exiting so the restart rebuilds
system state without checkpoint <seq>`). The machine's restart policy
(`on-failure`, recorded in step 3) starts it again, and that boot replays the
WAL: about 80 s of `not_ready`. Check it came back with `fly machines list`;
if it stays stopped, `fly machine start <machine id> -a allsource-core`. Keep
the tenants the log names ([SYSTEM_CHECKPOINTS.md](./SYSTEM_CHECKPOINTS.md) §
Rolling out). If the next checkpoint boot differs again, deploy with
`ALLSOURCE_SYSTEM_CHECKPOINT=false` (the kill switch) and commit that change to
`apps/core/fly.toml`. `System checkpoint verify failed to run` does not exit;
treat it as no verification.

### Done

Done means clients are served again, read from the client side: gateway 2xx
back to baseline, Longhand Core errors back to baseline, and one canary read
per credential type ([CONSUMER_ACCESS_GATE.md](./CONSUMER_ACCESS_GATE.md) §
Limits: the gate does not run for a Core deploy). Report the window as start,
end and refused requests, never as the machine swap time.

## Rolling back

```bash
fly deploy -a allsource-core --config apps/core/fly.toml \
  --image <config.image recorded in step 3, pinned to image_ref.digest>
```

- Use the current `fly.toml`, so the rollback gets the same 15 m wait.
- A rollback is another full boot: an image without checkpoints replays the
  whole WAL (6m30s in the 2026-11-05 rehearsal) and refuses connections until
  it is ready. The rehearsal booted `16dd13eb` on a volume `019b7e57` had
  written to and checkpointed, and it recovered every system event.
- `16dd13eb` predates `cf75ee44`. Rolling back to it from `019b7e57` reads the
  data but drops `valid_from`/`valid_to`: events it flushes or compacts are
  rewritten without valid time, permanently. Decide before the window who may
  approve that.
- Restoring the pre-window snapshot is the last resort: it loses every write
  after the snapshot. `fly volumes create allsource_data --snapshot-id <id>
  -a allsource-core --region iad --size 10` creates the volume (wait for state
  `created`; if the machine cannot start on that host, `fly volumes fork`
  moves it to one with capacity), then the machine is moved onto it.

## After the window

- Longhand's shared projection rebuilds after every Core restart, and after
  Core evicts the tenant from its cache: the feed epoch lives in a per-tenant
  in-memory index unless `ALLSOURCE_FEED_DURABLE_LOG` names the tenant (it is
  unset in `apps/core/fly.toml`).
- A compaction sweep in which tenants fail still records itself as the last
  compaction, so a running Core does not sweep again for at least a full
  interval; a restart schedules the next sweep for the rest of the interval,
  and never sooner than 120 s.

## Files on the volume must belong to uid 1000

Core runs as uid 1000. Anything run on the volume as root (`fly ssh console`
runs as root) leaves files Core cannot open. The 2026-10-13 archive recovery
left 16 of them in `storage/decebal-dobrica-at-gmail-com/2026-05/` and
`storage/default/2026-02/` (`.archive-mutation.lock`, a repair receipt and
the repacked parts), and every compaction sweep failed for those two tenants
with `Archive repack: I/O failed: Permission denied` until they were chowned
on 2026-11-06; the next sweep compacted both tenants without an error. Reads
were not affected. After any root work on the volume,
check and fix ownership:

```bash
fly ssh console -a allsource-core -C "find /app/data ! -user 1000 -print"
fly ssh console -a allsource-core -C "find <dir> ! -user 1000 -exec chown 1000:1000 {} +"
```

`/app/data/lost+found` belongs to the filesystem and stays root-owned.
