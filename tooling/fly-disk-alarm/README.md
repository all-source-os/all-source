# fly-disk-alarm

Alarm gate. Fails when any mounted filesystem on a Fly machine in the
`allsource` org is at or above a disk-usage threshold.

```bash
fly auth token > /tmp/fly.token
FLY_API_TOKEN_FILE=/tmp/fly.token cargo run --manifest-path tooling/fly-disk-alarm/Cargo.toml
```

| Exit | Meaning |
|---|---|
| 0 | every mount is under the threshold |
| 1 | at least one is at or above it |
| 2 | usage could not be read at all |

Run on a schedule by `.github/workflows/disk-alarm.yml`, which opens a GitHub
issue labelled `disk-alarm` on exit 1 **or** exit 2. Setup, thresholds and the
response procedure live in `docs/operations/ALERTS.md`.

## Why this is not a Grafana rule

Fly ships a managed Grafana and the obvious move is to click an alert rule into
it. That rule is then configuration that nobody can review in a pull request,
nobody can diff when it changes, and nobody can recreate after an account or
plan change. It also cannot be tested. A gate in the repository is versioned
with the code it protects and runs from CI.

## Why exit 2 exists

The dangerous failure for an alarm is not a false alarm, it is silence. A
missing token, an HTTP error, an unparseable body and an empty result set are
all exit 2, and all open the issue.

An empty result set matters most. `{"status":"success","data":{"result":[]}}`
is a *successful* response that means "no series matched", which reads exactly
like "no volume is full" and is almost always a token scoped to the wrong org
or a renamed metric. Treating it as healthy would make the alarm fail open.

## There is no `fly_volume_*` metric

Checked against the org's metric-name list on 16 September 2026. The only
disk-capacity series Fly publishes are filesystem block counters, so the query
is:

```promql
1 - (fly_instance_filesystem_blocks_avail / fly_instance_filesystem_blocks)
```

An attached volume appears as its mount point — `/app/data` on core, `/data` on
prime. The rootfs overlay `/.fly-upper-layer` is reported too, because a full
rootfs stops a machine just as dead as a full volume.

Fly access tokens are macaroons and authenticate under the `FlyV1` scheme, not
`Bearer`. The gate picks the scheme from the token's shape.
