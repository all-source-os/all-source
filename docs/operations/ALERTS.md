# Alerts

Every alert in this file is defined in the repository. Nothing here depends on
a rule clicked into a vendor dashboard, because that kind of rule cannot be
reviewed in a pull request, cannot be diffed when it changes, and does not
survive an account or plan change.

## Disk usage on Fly volumes

| | |
|---|---|
| Defined in | `.github/workflows/disk-alarm.yml` |
| Gate | `tooling/fly-disk-alarm` |
| Schedule | every 6 hours, plus manual dispatch |
| Threshold | 70% of a volume's capacity |
| Fires as | a GitHub issue labelled `disk-alarm`, plus a red scheduled run |

The gate reads this from Fly's Prometheus endpoint for the `allsource` org:

```promql
1 - (fly_instance_filesystem_blocks_avail / fly_instance_filesystem_blocks)
```

**There is no `fly_volume_*` metric.** Checked against the org's metric-name
list on 16 September 2026: the only disk-capacity series Fly publishes are the
filesystem block counters above. An attached volume shows up as its mount point,
`/app/data` on core and `/data` on prime. The rootfs overlay
`/.fly-upper-layer` is reported too, because a full rootfs stops a machine just
as dead as a full volume.

Baseline on the day the gate landed:

| App | Mount | Used |
|---|---|---|
| allsource-core | /app/data | 23.8% |
| allsource-prime | /data | 6.8% |
| everything else | /.fly-upper-layer | under 7% |

### Setup

The workflow needs one repository secret:

```
FLY_API_TOKEN    a Fly access token scoped to the allsource org
```

Create it with `fly tokens create readonly --name disk-alarm`, then add it under
Settings, Secrets and variables, Actions. Without the secret the gate exits 2
and opens the issue, so a missing token is visible rather than silent.

Fly tokens are macaroons and authenticate under the `FlyV1` scheme, not
`Bearer`. The gate picks the scheme from the token's shape, so paste whichever
form `fly tokens create` gives you.

### Running it by hand

```bash
fly auth token > /tmp/fly.token
FLY_API_TOKEN_FILE=/tmp/fly.token cargo run --manifest-path tooling/fly-disk-alarm/Cargo.toml
FLY_API_TOKEN_FILE=/tmp/fly.token cargo run --manifest-path tooling/fly-disk-alarm/Cargo.toml -- --threshold 0.2 --json
```

`FLY_API_TOKEN_FILE` keeps a live token out of your shell history and out of the
process environment. `FLY_API_TOKEN` works too and is what CI uses.

### Exit codes

| Code | Meaning | Action |
|---|---|---|
| 0 | every volume is under the threshold | none |
| 1 | at least one volume is at or above it | extend the volume, or free space |
| 2 | usage could not be read | fix the token, the org, or the query |

Exit 2 exists because the dangerous failure for an alarm is not a false alarm,
it is silence. A missing token, an HTTP error, an unparseable body and an empty
result set are all exit 2 and all open the issue. An empty result set in
particular is never treated as "no volume is full": it usually means the token
is scoped to the wrong org, or that Fly renamed the metric.

### Responding

1. Read the report in the issue. It lists every mount with its usage, worst
   first, and marks the ones over the line.
2. For a **volume** mount (`/app/data`, `/data`): find the volume with
   `fly volumes list -a <app>`, then
   `fly volumes extend <volume_id> --size <GB> -a <app>`. Fly volumes cannot be
   shrunk again, so extend by what the growth rate justifies, not by the
   largest number available.
3. For the **rootfs** mount (`/.fly-upper-layer`): this is the image plus
   whatever the process has written outside its volume. It is usually a log or
   a temp file that should have been written to the volume, or not at all.
   Redeploying resets it, which buys time but does not fix the writer.
4. For `allsource-core`, check whether compaction and the cold-tier archive are
   keeping up before extending. A volume that refills immediately is a retention
   problem, not a capacity one.
5. Close the issue once usage is back under the threshold. The next scheduled
   run opens a fresh one if it is not.

### Why 70%

It leaves room to extend a volume without racing the writer, and Fly volumes
cannot be shrunk, so tripping early is cheaper than tripping late.
