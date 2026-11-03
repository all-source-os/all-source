# Core restarts without refused clients

Status: proposed. Confidence: sand. The phases below are ordered so each one
ships on its own and lowers the refused-request window measured from the
client side.

## The job

An operator deploys a new `allsource-core` image to production, and no client
of the gateways sees a refused or failed request. Downtime is counted at the
client: a refused read, a 401 for a valid key, a 503 from the egress ledger and
a 502 from delegation all count, whatever the machine swap looked like.

## What happens today

Measured on 2026-11-01, deploying `16dd13eb` (v0.27.0) with
`fly machine update --image` on the single Core machine:

| Time (UTC) | Event |
|---|---|
| 17:32:20 | Old process gets SIGINT, shuts down cleanly |
| 17:32:31 | v0.27.0 starts; data WAL recovers 0 events |
| 17:33:16 | Stage 1: 2,852,970 system events recovered from the system WAL (45 s) |
| 17:33:18 | Stage 2: system streams replayed into repository caches |
| 17:38:05 | Stage 4 done, listener bound on `:3900` |
| 17:38:01 | Last gateway refusal: `egress refused status=503 reason=ledger_unavailable` |
| 17:38:25 | Longhand's tenant reads served again |

Clients were refused for 5 m 45 s. Longhand's `/health` counted 30 failed
queries, 2 failed canary reads and 9 failed board reads across its two
machines. On 2026-10-26 the same restart took about 6 minutes.

## Why every restart costs minutes

- **The system WAL is never pruned or snapshotted.** `system_store.rs` only
  creates and recovers its WAL (`:86`, `:92`); the only `truncate` trims a query
  result (`:274`). Every boot parses all system events (stage 1) and folds them
  into five repository caches (stage 2). The cost grows with system history.
- **Core binds only after the whole bootstrap.** `main.rs` runs
  `SystemBootstrap::try_initialize` before `serve_v1` binds the listener, so
  for the whole replay the port refuses connections.
- **The gateways turn a short Core outage into client errors.**
  - API-key auth: a cache miss calls Core `/api/v1/auth/me`; a dial error or any
    non-200 becomes "invalid api key" (`auth.go:305-316`), so `ask_` keys get
    401 within the 120 s cache TTL.
  - Egress ledger: the lease book closes leases every 30 s and the next acquire
    needs Core; failure is `503 egress_budget_unavailable` with
    `Retry-After: 30`.
  - Delegation: a dial error is a `502 delegation upstream`, with no retry and
    no dial timeout on the HTTP client.
- **Some Core state does not survive any restart.**
  - Webhook subscriptions are in-memory only (`webhook.rs:81`).
  - The feed epoch is random per process (`store_feed.rs:87`), so every feed
    cursor gets `409` after a restart unless `ALLSOURCE_FEED_DURABLE_LOG` is on.

## Ruled out: leader/follower replication

Core ships a WAL shipper, a receiver, `promote` and `allsource-sentinel`.
They cannot back a cutover as they stand:

- The shipper streams only the live broadcast channel. A fresh follower gets
  either no history, or a Parquet snapshot loaded from a flat directory in
  which every event is restamped to tenant `default`.
- System streams (tenants, keys, config, audit) live in `__system` and are not
  replicated at all.
- `promote` flips the role before the receiver stops and there is no fencing,
  so an old leader that comes back is a second leader.
- Leader WAL sequences reset on every boot, which stalls repointed followers
  silently.

Making replication carry production is a separate design, not a restart fix.

## Phases

Each phase is one or more commits on this branch, verified before the next.

### Phase 1: the gateways hold through a short outage (rock)

Gateways first, because a Core restart of any length is refused today even
after it is fast.

- One shared Core-hold helper in the control plane, used by delegation,
  `ValidateAPIKey`, the egress lease exchange and the key authority.
  - It retries only when no Core handler can have run: a dial error
    (connection refused, DNS failure, dial timeout) or a `503` whose body is
    `not_ready`. Never on a reset, an EOF or another 5xx for a write, because
    ingest has no idempotency key and a resend would duplicate the event.
  - Deadline D, bounded below the egress request deadline (10 s today) and the
    16 egress response slots. Held requests must not starve the slot pool.
  - Core unreachable maps to `503` with `Retry-After`, never to 401.
- A `net.Dialer{Timeout: 1s}` on the delegation client.
- Narrow the three retrying resty clients in `core_client.go` (`:448`, `:479`,
  `:530`) to the same rule. Today they retry POST on any error or 5xx.
- Query Service: put `Tesla.Middleware.Timeout` inside `Retry` (or raise it to
  cover D), and narrow `should_retry` on writes. Move `/api/health/ready` out of
  `[[http_service.checks]]` into a top-level `[checks]` so a Core outage does
  not drop the Query Service from Fly routing.
- Prime's delegation (`ProxyPrime`) uses the same helper.

Verification: a Go integration test that stops a real Core binary behind the
control plane under load for less than D and asserts zero non-2xx across all
six refusal classes (401, 502, 503 ledger, 503 authority, 504, 429 pool full).

#### As built

- `internal/infrastructure/corehold` holds a call for D = 6 s and shares one
  availability state per process. The first call that finds Core unreachable
  marks it down; later billable requests wait in `corehold.WaitUp` before they
  take an egress response slot, so a restart parks requests outside the
  16-slot pool instead of filling it. A waiter goes through as a probe when no
  call has tried Core for a second, so the state cannot stay down after every
  holder gave up.
- Delegation resends idempotent methods after a transport error and any method
  after a refused dial or `not_ready`. Lease waiters wait `D + 3 s`, longer than
  the holder's hold.
- The control plane's resty clients retry 12 times from 50 ms doubling to
  500 ms, about D in total, and resend a write only under the same rule.
- In the Query Service, the Tesla client and the five Req-based Core stores use
  the same rule. A tenant read that gets no answer while verifying an API key
  is a `503 core_unavailable` with `Retry-After`, not a 401.

#### Known limits

- With tenant admission in `enforce` mode, held reads still count as in flight,
  so a busy tenant can hit its own in-flight cap during a restart. Production
  runs admission in `shadow`.
- The Query Service key authority still folds a failed Core read into
  `:invalid_key`. Production leaves `ALLSOURCE_KEY_AUTHORITY_MODE` unset, which
  is `shadow`: refused keys are still served except revocations. In `enforce`
  mode a restart would answer 401.
- The Req-based stores halt their body on a 503, so they hold only on a refused
  dial, not on `not_ready`, until phase 2 makes Core send it.
- The real-Core load test above is not written yet.

### Phase 2: Core answers `not_ready` instead of refusing (rock)

- Bind the listener first and serve a not-ready router from a spawned task
  while the bootstrap runs in `spawn_blocking`; flip a ready flag when stage 4
  completes. Every route, `/internal/*` included, answers
  `503 {"error":"not_ready"}` with `Retry-After: 1` until then.
- `/health` returns 503 until ready and stays O(1) afterwards (today it counts
  two streams and all events per call).
- `apps/core/fly.toml`: `kill_timeout` of at least 65 s (strict archive reads
  run up to 60 s) and a `[deploy]` `wait_timeout` longer than the measured
  replay, so a slow first boot is not reported as a failed release and rolled
  back into a second outage.
- Install the SIGTERM handler at the top of `main`, so a stop during boot is
  clean.

#### As built

- `main` binds the port before anything else and serves
  `infrastructure::web::boot_gate::BootGate`. Until `serve_v1` opens it, every
  route, `/health` and `/internal/*` included, answers `503 not_ready` with
  `Retry-After: 1`; afterwards each request goes to the real router.
- The bootstrap still runs on `main`'s thread; the gate is served by the
  runtime's workers. A SIGTERM or Ctrl+C before the gate opens exits the
  process at once, since no handler has run and the bootstrap cannot be
  interrupted.
- `apps/core/fly.toml` sets `kill_timeout = 65` and `[deploy] wait_timeout =
  "15m"`. Both apply to `fly deploy`; `fly machine update` ignores them.
- `/health` still counts system streams on every call; making it O(1) is open.

With phases 1 and 2, a restart shorter than D is invisible to clients. Longer
restarts still refuse, but with 503 and `Retry-After`, never 401.

### Phase 3: boot in seconds (sand)

#### What the replay is

On 2026-11-01 Core's `/health` reported 2,852,971 system events, 2,841,079 of
them in the tenant stream. Most come from
`POST /api/v1/tenants/{id}/usage/increment`, which the Query Service's
`UsageReporter` calls: each call appends a tenant `UPDATED` event that carries
the tenant's whole metadata blob. The stream grows with traffic and never
shrinks, so every boot replays more than the last.

`tests/system_replay_cost.rs` (ignored; run by hand in release) writes a
synthetic tenant stream and times recovery and the tenant rebuild. On an M-series
laptop, 300,000 `UPDATED` events with 1 KiB of metadata recover in 0.57 s and
rebuild in 0.40 to 0.52 s, about 1.3 to 1.7 µs per event. Scaled to 2.85 M events
that is about 5 s each; production measured 45 s and 4 m 47 s. Recovery's gap
fits a shared CPU and larger events. The rebuild's gap, about 57x, does not:
production's events carry egress and query-usage receipts the synthetic ones do
not.

#### Egress replay: measured and fixed

`EgressAccount::apply` clones the whole account before each transition so a
refused command leaves it untouched. The clone includes every live settle
receipt, kept for an hour and capped at 4,096 per tenant, so a gateway that
settles once a second makes every replayed egress event copy a map of thousands
of entries. `SYSTEM_REPLAY_SETTLES` adds that traffic to the synthetic stream
(valid UUID operation ids; the test asserts the replayed ledger's used bytes):

| Settles, one tenant | Rebuild with the clone | Rebuild in place |
|---|---|---|
| 20,000 | 4.61 s (about 230 µs each) | 0.136 s |
| 100,000 | not run | 0.733 s (about 7.3 µs each) |

Replay events were accepted once when written, and a replay error already
invalidated the account, so the rebuild now calls `EgressAccount::replay`,
which transitions in place and invalidates on error. `tests/egress_budget.rs`
checks that `replay` and `apply` reach the same results and snapshot over a
lease and settle chain, and that a failed replay invalidates the account.

At about 230 µs each, 1.2 M settles would account for the whole 4 m 47 s. How
many of production's 2.84 M events are egress settles is not measured yet; the
per-kind counts below answer it.

#### What the next boot says

Stage 2 logs each repository's rebuild time, and the tenant rebuild logs the
count and time per event kind (lifecycle, egress, query_usage, metadata). The
next production boot says whether anything is left to fix, before any snapshot
is built:

- If egress still dominates, give `EgressAccount` an expiry-ordered index so
  expiring entries costs O(log n) per event instead of three scans.
- If metadata dominates, stop writing the whole metadata blob on every usage
  increment: a usage event that carries only the counter keeps the stream small
  and the replay cheap.
- Only if neither is enough, the snapshot below.

#### Snapshot, if still needed

A system-store snapshot so boot loads folded state and replays only the tail.

- One mutex around the WAL append and the in-memory push, so a snapshot never
  covers a sequence whose event is not folded yet.
- The snapshot is a fold of the event list under that mutex, with
  `covered_seq` set to the highest sequence it includes, written to
  `__system/snapshot/` with tmp, fsync, rename and directory fsync. The old
  binary ignores the directory (it lists only `wal-*.log`).
- Boot skips a segment only when a per-segment max-sequence sidecar, written
  at seal time, is at or below `covered_seq`. A segment's file name is only a
  lower bound: older segments can be appended to after a restart (`wal.rs:462`).
- On boot, set the WAL sequence to `max(recovered, covered_seq)` before the
  first append. Without it, a pruned WAL restarts at 0 and the next boot drops
  every write since the snapshot.
- Pruning stays behind `ALLSOURCE_SYSTEM_WAL_PRUNE`, default off, so rolling
  back to an image without the snapshot stays lossless. Turning it on is a
  later, separate release, and that release becomes the rollback floor.
- Test: snapshot, prune, restart, write, restart, and the write is present;
  kill during snapshot write, restart, state intact.

The first deploy carrying phase 3 still replays everything once to build the
snapshot. Avoiding that window needs an offline tool that builds the snapshot
from the volume before the new image boots; it is water and not in this plan.

#### Snapshot: as built

Built without waiting for the per-kind boot logs, because no change to the
replay loop can make a boot that reads 2.85 M events fit inside the 6 s
gateway hold. Operator view: `docs/runbooks/SYSTEM_CHECKPOINTS.md`.

- `persistence/system_checkpoint.rs`: one file per checkpoint, JSON body plus a
  footer with length and CRC-32, written tmp, fsync, rename, directory fsync.
  The two newest valid files are kept; a file that fails its footer or is in
  an unknown format is skipped and never deleted.
- What it holds: the tenant stream folded (tenants, query-usage state, egress
  accounts), every other system event verbatim, per-domain event counts, and
  each WAL segment with the length of it that is covered.
- `SystemMetadataStore` holds one lock across the WAL append and the resident
  push, so a capture reads a sequence, segment lengths and a resident length
  that agree. Resident events are append-only, so the copy after the lock is
  released is still exact.
- Boot skips a covered segment whose length is unchanged, reads a longer one
  from its covered length, and reads an unlisted one whole. A covered segment
  that is shorter, or an entry at or below `covered_seq` outside the covered
  bytes, discards the checkpoint and replays the whole WAL. The next sequence
  is above both, so a pruned or skipped WAL never restarts at 0.
- Checkpoints are written at boot from the rebuilt state (before any request),
  every 300 s by folding the tenant events since the last one onto it, and on a
  graceful stop. Periodic folds use events only, never the live cache, so
  runtime-only writes such as usage windows never reach a checkpoint.
- `ALLSOURCE_SYSTEM_CHECKPOINT_VERIFY` replays the whole WAL in the background
  after a checkpoint boot. On a difference it stops this process writing
  checkpoints, moves every checkpoint aside and exits, so the restart replays
  the WAL. It is on in `apps/core/fly.toml`.
- The WAL is never pruned. With pruning, every fallback (a contradiction, an
  unreadable or rejected checkpoint, a fold format change) would replay a
  truncated WAL. Without it, any doubt about a checkpoint is answered by
  replaying the WAL, and rolling back to an image without checkpoints is
  lossless.
- Segment lengths are recorded after every segment and the WAL directory are
  fsynced. A failed WAL append stops checkpoints until restart, and a capture
  refuses while the WAL's sequence is ahead of the last resident event. A
  rotation failure after a durable write no longer reports the write as
  failed.
- A contradiction between WAL and checkpoint moves every checkpoint aside and
  starts the next sequence above every checkpoint name on disk.
- The tenant fold records every event type and payload key its binary
  ignored, and is used only if it re-encodes to the same JSON through the
  loading binary's types and lists nothing that binary reads.
- `/health`'s system counts are per-domain counters, no longer a scan of every
  event per call.
- Measured with `tests/system_replay_cost.rs`, debug build (the build machine
  had no disk for a release build), 105,060 events (100,000 usage updates with
  256-byte metadata, 5,000 egress settles): full replay recovered in 2.35 s and
  rebuilt tenants in 0.61 s; the boot from the checkpoint recovered in 14 ms
  and rebuilt in 18 ms with no tenant event resident, and the egress state was
  identical. Writing the checkpoint took 46 ms. A boot from a checkpoint reads
  only the WAL after it, so its cost does not grow with history.
- `tests/system_checkpoint.rs`: checkpoint boot equals a full replay after
  mixed traffic (usage, metadata, egress leases and settles, query admissions,
  config); writes after checkpoint boots survive and no WAL sequence repeats;
  a corrupt newest checkpoint falls back to the older one; no valid checkpoint
  replays the whole WAL; a torn temporary file is ignored and removed; a WAL
  shorter than the checkpoint saw is replayed whole; writes from an image
  without checkpoints reach the next checkpoint boot; writes racing repeated
  checkpoints are all kept; a fold that does not round-trip, or that ignored a
  key this binary reads, is replayed from the WAL; ignored event types and
  keys are recorded; verify accepts a true checkpoint, rejects a forged one,
  and the rejected checkpointer writes nothing more.

### Phase 4: state that survives a restart (sand)

- Persist webhook subscriptions as a `_system` stream, folded at boot.
- Turn on `ALLSOURCE_FEED_DURABLE_LOG` after measuring its disk and first-read
  cost at production size. `build_durable_feed` holds the store's write lock
  across a disk write, so the first read per tenant stalls ingest for all.
- Accept a client event id or `Idempotency-Key` on ingest and dedupe it, so the
  gateways may later retry a reset write safely.

## Rehearsal before production

Each phase is rehearsed on a separate Fly app restored from a Core volume
snapshot, never as a second machine in `allsource-core`: any machine there
answers `allsource-core.internal` and would take production traffic.

- Copy the gateway `fly.toml` files with Core URLs rewritten and check
  `fly ssh console -C env` before any load; the production files point at
  `allsource-core.internal`.
- Copy `ALLSOURCE_JWT_SECRET` and the bootstrap key to the rehearsal apps, or
  every delegated call is 401 and the measurement means nothing.
- Use synthetic tenants only, and block webhook egress on the rehearsal Core.
- Measure restart time and every refusal class under load through the
  rehearsal control plane.

## What this does not cover

- The gateway deploys themselves, which wait on `CONSUMER_CANARY_CREDENTIALS`
  (`docs/runbooks/CONSUMER_ACCESS_GATE.md`).
- `allsource-prime`: its volume makes every deploy a restart, and its image is
  not anonymously pullable while the image-promotion workflow has no
  `FLY_IMAGE_PROMOTION_TOKEN`.
- A second Core serving traffic. That needs the replication redesign above.
