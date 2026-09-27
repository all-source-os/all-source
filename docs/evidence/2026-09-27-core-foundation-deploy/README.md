# Core foundation deployment

Date: 27 September 2026. Target: existing `allsource-core` Fly application in
the `allsource` organization, region `iad`.

## Scope and source

Build source is the clean git export of
`a49d8c43359b99f198d8e814a7b0c80acf8ebb63`. Only root Cargo manifests,
`.dockerignore`, `apps/core`, and the existing workspace manifest stubs enter the
build context. The Core build inputs match the later review commit; uncommitted
Prime, web analytics and outreach work is excluded.

This deploys the Core foundation, including stored event-version correction and
conditional config records used by scoped connections and pending reviews. It
does not deploy Query Service, Control Plane, web or MCP, enable customer
connections, issue customer credentials, or establish human approval.

The existing `runtime-alpine` Docker target, UID 1000, entrypoint, migration flag,
internal-only network, 4 GiB machine and mounted 10 GiB volume are preserved.
The distroless CI image is not substituted for the production runtime.

CI for the source revision passed:

- [CI 36313047825](https://github.com/all-source-os/all-source/actions/runs/36313047825)
- [Docker Build 36313047853](https://github.com/all-source-os/all-source/actions/runs/36313047853)
- [Container CI 36313769845](https://github.com/all-source-os/all-source/actions/runs/36313769845)
- [Security Scanning 36313047923](https://github.com/all-source-os/all-source/actions/runs/36313047923),
  including Core container and Rust dependency checks

The subsequent customer review commit had a CI formatter disagreement between
Elixir versions. Alias-only correction
`03b7e1efb6550dad3aa0bdfdcaa0ba62ca39b2ba` passed
[CI 36315219978](https://github.com/all-source-os/all-source/actions/runs/36315219978),
including formatting, compile, Credo, Dialyzer and Query Service tests. This
correction does not alter the Core image being deployed. Its
[Docker Build 36315219992](https://github.com/all-source-os/all-source/actions/runs/36315219992)
also passed.

## Baseline and recovery

Before deployment, machine `7817667a276368` was running release 42, Core 0.25.0,
with mounted volume `vol_vwjoq95l03qzy88r` at `/app/data`. Actual `df -h /app/data`
reported 2.0 GiB used, 7.3 GiB available, 21% used. Query Service readiness,
Control Plane readiness and web health all passed at 11:19 UTC.

Previous image:

```text
registry.fly.io/allsource-core:deployment-01M2NE6WV5HAS9MHHNHGW3QYQY
sha256:61c157fa45f858a5b1dd109d3a16e102aa7d9f4f0e59696112caea889f8d436e
```

A fresh recovery snapshot completed before the production update:
`vs_2gnkX3Dew5RsxoJzVDgQNQ2`, created at `2026-09-27T11:16:32Z`, status
`created`, retention five days. Fly reports digest
`985b16942a8a65dac4a717984bc2f2ebcaa16031382b37cb3f2f23687807e777`.
No snapshot restoration or production data deletion is part of this deployment.

The scheduled Disk Alarm failure at run 36314886645 is a missing repository
`FLY_API_TOKEN`, not a measured full disk. Direct authenticated volume inspection
above supplies the actual observation. Monitoring credential setup remains open;
this deployment neither creates nor expands that credential.

The older security scan 36312168080 failed while building Control Plane because
the Alpine package index TLS fetch failed; its missing SARIF upload followed that
build failure. The later source-revision security scan linked above completed
successfully. No TLS check or security gate was disabled to work around it.

## Deployment verification

The initial image built and pushed successfully:

```text
registry.fly.io/allsource-core:core-a49d8c43-20260927
sha256:6bf11236280218b9ef009db62c0db9f5fe7348b7b339629841021511a52c6f68
```

The builder reported a cleanup deadline after the image push. The subsequent
deploy independently resolved this exact digest and installed it as release 43
(`rel_6rx0yxvjn15nzpjo`). Image labels matched source revision `a49d8c43` and
version 0.25.1. The existing machine, volume, UID and VM size were preserved.

**Release 43 failed post-deploy readiness and was rolled back.** Fly initially
reported a passing startup check, but subsequent actual Core health requests
timed out after ten seconds. Query Service and Control Plane readiness requests
also timed out. Web health remained available. The update is not a successful
production release.

Startup logs show migration completed with zero files changed, system WAL
recovered 27,799 events with zero corrupted entries, metadata caches rebuilt and
HTTP listening at 11:39:08 UTC. At 11:39:09, Core began loading the cold `system`
tenant subtree, containing 219,184 Parquet files; a request hit the 30-second
timeout and health checks stalled. Machine memory at the observed failure was
99 MiB used with 3,634 MiB available, so this observation was not an OOM report.

Rollback restored the previous image digest as release 44
(`rel_8ld7y4e7lweeznwo`), keeping the same volume. Direct Core health returned
0.25.0 and healthy event-sourced system metadata. Query Service backend/WebSocket
readiness and Control Plane Core readiness passed again at 11:41 UTC. Fly's
health check was passing. No volume snapshot was restored and no production data
was intentionally removed.

The regression is in `dcf09678`: its unconditional call to `ensure_tenant_loaded`
was added to the HTTP append path even when callers supplied no `expected_version`.
Two local cold-archive tests failed because an ordinary append hydrated the tenant.
The minimal repair calls hydration only when `expected_version` is present.
Ordinary appends leave the tenant cold; a later query still sees both archived and
new events, and a later conditional append still rejects a stale version.

All 43 focused tests passed after the repair: six acknowledged-version cases,
25 optimistic-concurrency cases, seven WAL durability cases, three conditional
config cases and two embedded cold-boot cases. Formatting and strict all-target,
all-feature Clippy passed. The full all-feature Core library suite passed 1,990
tests with five ignored. The rebuilt enterprise binary passed 11 actual HTTP
tests across agent-run ordering, typed capture and pending evidence reviews,
including hard restart, competing writers and acknowledgement recovery.

Separate follow-up `t-e1d5cf` retains the incomplete/corrupt archive and unbounded
conditional-hydration concerns. The loader currently skips unreadable files, and
its lock timeout does not bound archive traversal. This lazy-append repair does
not claim those broader conditional-write guarantees or activate customer capture.
`source-sha256.txt` identifies the repaired Core source/tests/build inputs;
`local-binary-sha256.txt` identifies the local HTTP proof binary.

Build command, from the clean export:

```text
flyctl deploy . --config apps/core/fly.toml --dockerfile apps/core/Dockerfile \
  --build-target runtime-alpine --remote-only --build-only --push --ha=false \
  --build-arg VERSION=0.25.1 \
  --build-arg REVISION=a49d8c43359b99f198d8e814a7b0c80acf8ebb63 \
  --build-arg BUILDTIME=2026-09-27T11:14:44Z \
  --image-label core-a49d8c43-20260927
```

If the new image fails recovery, the application rollback uses the previous
image digest and the same machine/volume/configuration. Restoring a volume is a
separate, potentially data-losing operation and is not an automatic rollback.

```text
flyctl deploy . --config apps/core/fly.toml \
  --image registry.fly.io/allsource-core@sha256:61c157fa45f858a5b1dd109d3a16e102aa7d9f4f0e59696112caea889f8d436e \
  --ha=false --update-only --only-machines 7817667a276368 --no-public-ips \
  --wait-timeout 5m
```

## Limits

Health and image checks establish the deployed foundation and service recovery.
They do not prove a customer's MCP connection, source consent, proposal display,
human approval, replay execution, failover or full backup restoration. Existing
local actual-Core HTTP and restart proofs are recorded separately in the
[run evidence contract](../../plans/2026-09-27-agent-run-evidence-contract.md) and
[pending review evidence](../2026-09-27-customer-evidence-reviews/README.md).
Dependent customer delivery beads remain open.
