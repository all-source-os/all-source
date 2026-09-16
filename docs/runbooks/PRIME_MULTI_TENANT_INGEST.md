# Prime multi-tenant ingest: one writer, many readers

How to run Prime when several tenants' nodes are ingested into one graph
store, and how to tell when you have accidentally run two writers.

## The constraint

A Prime data directory is **single-writer**. Prime takes an exclusive advisory
lock on `prime.lock` at open. The process holding it is the writer. Any other
process opening the same directory becomes a **read-only replica**: it replays
the WAL and Parquet so it can serve reads of the shared memory, but it never
truncates the WAL and rejects every write.

This is not a tuning preference. Before the lock existed (issue #201), a second
process booting on a live data directory unlinked the WAL inode out from under
the writer. The writer kept appending to an inode with no name, so those writes
never reached disk and were invisible to every later process, and new instances
came up with an empty graph.

## The shape

Run exactly one writer, and give everything else a network path to it.

| Role | How | Writes? |
|---|---|---|
| Writer | one process with `--mode http` on the data dir | yes |
| Local agent clients | `--sync-to <writer URL> --api-key <key>` | via the writer |
| Extra readers | open the same data dir | no, replica |
| Hosted | stateless over Core, no data dir at all | n/a |

Do not point N agent processes at one data directory and expect them to share a
writable memory. They will not: the first one wins the lock and the rest go
read-only. Point them at the writer's HTTP endpoint instead.

The hosted `allsource-prime` app is not in this picture. It runs stateless over
Core with per-tenant scoping, and its `prime_data` volume is mounted but unused.

## Tenant isolation is a query-time filter, not a store boundary

One store holds every tenant's nodes. Isolation comes from `properties.tenant_id`
stamped by the writer and filtered at read time. Both reads honour it:

```bash
GET  /api/v1/prime/graph?tenant_id=<tenant>
POST /api/v1/prime/recall   {"tenant_id": "<tenant>", ...}
```

The `prime_recall` MCP tool takes the same `tenant_id` argument.

Omitting it returns the whole store, which is correct **only** where the
deployment is the tenant boundary — one Prime per tenant. In the shape this
runbook describes, omitting it is a cross-tenant read.

## Telling a writer from a replica

`GET /health` says which one you have:

```json
{"status": "ok", "writable": true}
{"status": "degraded", "writable": false, "reason": "another process owns the Prime data dir; ..."}
```

Both return HTTP 200. A replica is up and serving reads, and failing the Fly
health check would turn a diagnosable problem into a restart loop.

**`degraded` means ingest is silently doing nothing.** It is the state to alert
on. The writer also logs a warning at open naming the data dir.

## When health says degraded

1. Find the other process: `lsof <data-dir>/prime.lock`, or on Fly,
   `fly machines list -a allsource-prime` and check for more than one machine on
   the volume.
2. Decide which process should be the writer. Usually the long-lived service,
   not the ad-hoc one.
3. Stop the other one. The OS releases the lock on exit, including on a crash.
4. Restart the intended writer and confirm `"writable": true`.
5. Check what was lost. A replica **rejects** writes rather than dropping them
   silently, so the caller saw errors. Re-run the ingest for the affected
   window.

## One caveat

On a filesystem without advisory locking, Prime logs a warning and proceeds as
a writer without a lock, so a lone process is never bricked. On such a
filesystem the lock is not protecting you and two writers will corrupt the log.
Local development on macOS and Linux, and Fly volumes, all support locking.
