# Gateway config strip outage (2026-11-06)

`allsource-control-plane` (`api.all-source.xyz`) served nothing from
**2026-11-06 23:31:29Z to 2026-11-07 00:03:56Z (32.5 minutes)**. The deployer
handed `flyctl` an empty `fly.toml`, and the only gateway machine came back with
no services, no environment, no checks and no metrics.

## Timeline (UTC)

| Time | Event |
|---|---|
| 2026-11-06 22:36:51 | Tenant `consumer-access-canary` created with two read-only keys (one with no expiry, one expiring 2027-11-06), staged as `CONSUMER_CANARY_CREDENTIALS` on the gateway. |
| 2026-11-06 23:31:29 | Deploy runs with a 0-byte `fly.toml`. `flyctl` reports "Configuration is valid", defaults to the `rolling` strategy and replaces the only machine's config. The machine has no `[[services]]`, no `[env]`, no `[checks]`, no `[metrics]`. Every request to `api.all-source.xyz` fails. |
| shortly after | A rollback is attempted with the same file-writing method. It produces the same empty config and repeats the strip. |
| 2026-11-07 00:03:56 | Service restored by `fly machine update --port ... --env ...` on the machine, re-adding the listener and environment by hand. |
| 2026-11-07 00:53:52–00:53:59 | Full config restored by a verified redeploy from a correctly written `fly.toml`. About 6 s without service during the swap. |

## Cause

The deploy script wrote the config for a given revision with

```bash
git show <rev>:<path> --output=<file>
```

For a blob, `git show` ignores `--output` as a destination for the content: it
prints the blob to stdout and leaves `<file>` created and empty. `flyctl`
accepted the 0-byte file as a valid configuration with every section absent,
fell back to its defaults (strategy `rolling`, no services), and applied that to
the running machine. Nothing in the pipeline compared the file to the repo copy
or checked its size.

The rollback reused the same command, so it rewrote the same empty file and
stripped the machine a second time.

## Signals that were there and were missed

- **`rolling` in the deploy output.** The app's `fly.toml` says `bluegreen`. A
  deploy that announces any other strategy is not reading the intended file.
- **An empty `fly checks list`.** The gateway always has a `consumer_access`
  check and a health check. No checks after a deploy means no config.
- **A client probe that finished before the swap.** A post-deploy probe ran
  and passed against the old machine, before the replacement took over, so it
  reported success for a machine that was about to stop serving.

## Rule

1. Write a config blob from git with a redirect, never `--output`:
   `git show <rev>:apps/control-plane/fly.toml > /tmp/fly.toml`.
2. Before deploying, check the file is non-empty and contains a key line:
   `test -s /tmp/fly.toml` and `grep -q '^app = "allsource-control-plane"' /tmp/fly.toml`
   and `grep -q 'strategy = "bluegreen"' /tmp/fly.toml`. Stop if either fails.
3. Right after the deploy, read the result back:
   `fly services list -a allsource-control-plane` must list the HTTP service, and
   `fly machine status -d <machine-id> -a allsource-control-plane` must show the
   services, env, checks and metrics sections.
4. If either read-back is empty, revert immediately from a file that passed
   step 2, never with the method that produced the bad deploy.

The read-back step is in the gateway deploy runbook:
[`docs/runbooks/CONSUMER_ACCESS_GATE.md`](../runbooks/CONSUMER_ACCESS_GATE.md#read-the-config-back-after-every-deploy).

## Cost

32.5 minutes in which every request to `api.all-source.xyz` went unanswered,
for every tenant including Longhand's control plane, plus about 6 seconds during
the verified redeploy at 00:53:52–00:53:59Z. Request and error counts for the
window are not part of this report.

## Related state

- The canary tenant `consumer-access-canary` and its two read-only keys were
  created at 22:36:51Z, before the outage, and are the current
  `CONSUMER_CANARY_CREDENTIALS` on the gateway. The expiring key stops working on
  2027-11-06; see the consumer access gate runbook for its replacement.
- The decision that follows from the same week's auth failures:
  [Tenant clients authenticate with registered ES256 keys](../decisions/2026-11-07-tenant-clients-registered-es256-keys.md).

## What this report does not establish

- The exact time the rollback started and ended; only that it repeated the strip.
- Per-tenant request or error counts during the window.
